//! Saga Audit Trail
//!
//! This module provides comprehensive audit logging for saga execution,
//! tracking all lifecycle events for debugging, compliance, and observability.
//! Thread-safe implementation (US-15).

use crate::saga::{SagaId, SagaType};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// Audit event types for saga execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SagaAuditEventType {
    Started,
    StepStarted,
    StepCompleted,
    StepFailed,
    CompensationStarted,
    CompensationCompleted,
    CompensationFailed,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    RateLimited,
}

/// A single audit entry in the saga audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaAuditEntry {
    pub id: Uuid,
    pub timestamp: SystemTime,
    pub saga_id: SagaId,
    pub saga_type: SagaType,
    pub event_type: SagaAuditEventType,
    pub step_name: Option<String>,
    pub step_index: Option<usize>,
    pub attempt: Option<u32>,
    pub will_compensate: bool,
    pub duration_ms: Option<u64>,
    pub error_message: Option<String>,
    pub metadata: serde_json::Value,
}

impl SagaAuditEntry {
    pub fn new(saga_id: SagaId, saga_type: SagaType, event_type: SagaAuditEventType) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: SystemTime::now(),
            saga_id,
            saga_type,
            event_type,
            step_name: None,
            step_index: None,
            attempt: None,
            will_compensate: false,
            duration_ms: None,
            error_message: None,
            metadata: serde_json::json!({}),
        }
    }

    pub fn with_step(mut self, step_name: &str, step_index: usize) -> Self {
        self.step_name = Some(step_name.to_string());
        self.step_index = Some(step_index);
        self
    }

    pub fn with_attempt(mut self, attempt: u32) -> Self {
        self.attempt = Some(attempt);
        self
    }

    pub fn with_compensation(mut self, will_compensate: bool) -> Self {
        self.will_compensate = will_compensate;
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration_ms = Some(duration.as_millis() as u64);
        self
    }

    pub fn with_error(mut self, error: &str) -> Self {
        self.error_message = Some(error.to_string());
        self
    }

    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata[key] = value;
        self
    }
}

/// Saga Audit Trail - thread-safe collection of audit entries (US-15)
#[derive(Debug, Clone, Default)]
pub struct SagaAuditTrail {
    entries: Arc<Mutex<Vec<SagaAuditEntry>>>,
}

impl SagaAuditTrail {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Record an audit entry
    pub fn record(&self, entry: SagaAuditEntry) {
        self.entries.lock().unwrap().push(entry);
    }

    /// Get all entries (cloned)
    pub fn get_all(&self) -> Vec<SagaAuditEntry> {
        self.entries.lock().unwrap().clone()
    }

    /// Export audit trail to JSON
    pub fn to_json(&self) -> serde_json::Value {
        let entries = self.entries.lock().unwrap();
        serde_json::json!({
            "entries": entries.clone(),
            "total_entries": entries.len(),
        })
    }
}

/// Trait for audit trail consumers
pub trait SagaAuditConsumer: Send + Sync + Debug {
    fn consume(&self, entry: &SagaAuditEntry);
}

/// In-memory audit consumer that logs to tracing
#[derive(Debug, Default)]
pub struct TracingAuditConsumer;

impl SagaAuditConsumer for TracingAuditConsumer {
    fn consume(&self, entry: &SagaAuditEntry) {
        tracing::info!(
            saga_id = %entry.saga_id,
            saga_type = ?entry.saga_type,
            event_type = ?entry.event_type,
            step = entry.step_name.as_deref().unwrap_or("N/A"),
            step_idx = entry.step_index.unwrap_or(0),
            attempt = entry.attempt.unwrap_or(0),
            will_compensate = entry.will_compensate,
            duration_ms = entry.duration_ms.unwrap_or(0),
            error = entry.error_message.as_deref().unwrap_or("N/A"),
            "Saga audit event"
        );
    }
}

/// Audit consumer that writes to a Vec (for testing)
#[derive(Debug, Default)]
pub struct VecAuditConsumer {
    entries: std::sync::Mutex<Vec<SagaAuditEntry>>,
}

impl VecAuditConsumer {
    pub fn new() -> Self {
        Self {
            entries: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn get_all(&self) -> Vec<SagaAuditEntry> {
        self.entries.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}

impl SagaAuditConsumer for VecAuditConsumer {
    fn consume(&self, entry: &SagaAuditEntry) {
        self.entries.lock().unwrap().push(entry.clone());
    }
}

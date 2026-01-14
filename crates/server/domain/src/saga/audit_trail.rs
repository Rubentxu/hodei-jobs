//! Saga Audit Trail
//!
//! This module provides comprehensive audit logging for saga execution,
//! tracking all lifecycle events for debugging, compliance, and observability.

use crate::saga::{SagaContext, SagaExecutionResult, SagaId, SagaState, SagaType};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// Audit event types for saga execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SagaAuditEventType {
    /// Saga was initiated
    Started,
    /// A step started execution
    StepStarted,
    /// A step completed successfully
    StepCompleted,
    /// A step failed
    StepFailed,
    /// Compensation started for a step
    CompensationStarted,
    /// Compensation completed for a step
    CompensationCompleted,
    /// Compensation failed
    CompensationFailed,
    /// Saga completed successfully
    Completed,
    /// Saga failed (after compensation)
    Failed,
    /// Saga was cancelled
    Cancelled,
    /// Saga timed out
    TimedOut,
    /// Rate limit exceeded
    RateLimited,
}

/// A single audit entry in the saga audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaAuditEntry {
    /// Unique identifier for this audit entry
    pub id: Uuid,
    /// Timestamp of the event
    pub timestamp: SystemTime,
    /// Saga identifier
    pub saga_id: SagaId,
    /// Saga type
    pub saga_type: SagaType,
    /// Event type
    pub event_type: SagaAuditEventType,
    /// Step name (if applicable)
    pub step_name: Option<String>,
    /// Step index (if applicable)
    pub step_index: Option<usize>,
    /// Attempt number (for retries)
    pub attempt: Option<u32>,
    /// Whether compensation was triggered
    pub will_compensate: bool,
    /// Duration of the step/event
    pub duration_ms: Option<u64>,
    /// Error message (if failed)
    pub error_message: Option<String>,
    /// Additional metadata
    pub metadata: serde_json::Value,
}

impl SagaAuditEntry {
    /// Create a new audit entry
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

    /// Set step information
    pub fn with_step(mut self, step_name: &str, step_index: usize) -> Self {
        self.step_name = Some(step_name.to_string());
        self.step_index = Some(step_index);
        self
    }

    /// Set attempt number
    pub fn with_attempt(mut self, attempt: u32) -> Self {
        self.attempt = Some(attempt);
        self
    }

    /// Set compensation flag
    pub fn with_compensation(mut self, will_compensate: bool) -> Self {
        self.will_compensate = will_compensate;
        self
    }

    /// Set duration
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration_ms = Some(duration.as_millis() as u64);
        self
    }

    /// Set error message
    pub fn with_error(mut self, error: &str) -> Self {
        self.error_message = Some(error.to_string());
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata[key] = value;
        self
    }
}

/// Saga Audit Trail - collects and manages audit entries for a saga execution
#[derive(Debug, Default)]
pub struct SagaAuditTrail {
    entries: Vec<SagaAuditEntry>,
}

impl SagaAuditTrail {
    /// Create a new empty audit trail
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Record an audit entry
    pub fn record(&mut self, entry: SagaAuditEntry) {
        self.entries.push(entry);
    }

    /// Record saga started event
    pub fn record_started(&mut self, saga_id: SagaId, saga_type: SagaType) {
        self.record(SagaAuditEntry::new(
            saga_id,
            saga_type,
            SagaAuditEventType::Started,
        ));
    }

    /// Record step started event
    pub fn record_step_started(
        &mut self,
        saga_id: SagaId,
        saga_type: SagaType,
        step_name: &str,
        step_index: usize,
    ) {
        self.record(
            SagaAuditEntry::new(saga_id, saga_type, SagaAuditEventType::StepStarted)
                .with_step(step_name, step_index),
        );
    }

    /// Record step completed event
    pub fn record_step_completed(
        &mut self,
        saga_id: SagaId,
        saga_type: SagaType,
        step_name: &str,
        step_index: usize,
        duration: Duration,
    ) {
        self.record(
            SagaAuditEntry::new(saga_id, saga_type, SagaAuditEventType::StepCompleted)
                .with_step(step_name, step_index)
                .with_duration(duration),
        );
    }

    /// Record step failed event
    pub fn record_step_failed(
        &mut self,
        saga_id: SagaId,
        saga_type: SagaType,
        step_name: &str,
        step_index: usize,
        error: &str,
        will_compensate: bool,
    ) {
        self.record(
            SagaAuditEntry::new(saga_id, saga_type, SagaAuditEventType::StepFailed)
                .with_step(step_name, step_index)
                .with_error(error)
                .with_compensation(will_compensate),
        );
    }

    /// Record compensation started event
    pub fn record_compensation_started(
        &mut self,
        saga_id: SagaId,
        saga_type: SagaType,
        step_name: &str,
        step_index: usize,
        attempt: u32,
    ) {
        self.record(
            SagaAuditEntry::new(saga_id, saga_type, SagaAuditEventType::CompensationStarted)
                .with_step(step_name, step_index)
                .with_attempt(attempt),
        );
    }

    /// Record compensation completed event
    pub fn record_compensation_completed(
        &mut self,
        saga_id: SagaId,
        saga_type: SagaType,
        step_name: &str,
        step_index: usize,
        duration: Duration,
    ) {
        self.record(
            SagaAuditEntry::new(
                saga_id,
                saga_type,
                SagaAuditEventType::CompensationCompleted,
            )
            .with_step(step_name, step_index)
            .with_duration(duration),
        );
    }

    /// Record saga completed event
    pub fn record_completed(
        &mut self,
        saga_id: SagaId,
        saga_type: SagaType,
        result: &SagaExecutionResult,
        total_duration: Duration,
    ) {
        self.record(
            SagaAuditEntry::new(saga_id, saga_type, SagaAuditEventType::Completed)
                .with_duration(total_duration)
                .with_metadata("steps_executed", serde_json::json!(result.steps_executed))
                .with_metadata(
                    "compensations_executed",
                    serde_json::json!(result.compensations_executed),
                ),
        );
    }

    /// Record saga failed event
    pub fn record_failed(
        &mut self,
        saga_id: SagaId,
        saga_type: SagaType,
        result: &SagaExecutionResult,
        total_duration: Duration,
        error: &str,
    ) {
        self.record(
            SagaAuditEntry::new(saga_id, saga_type, SagaAuditEventType::Failed)
                .with_duration(total_duration)
                .with_error(error)
                .with_metadata("steps_executed", serde_json::json!(result.steps_executed))
                .with_metadata(
                    "compensations_executed",
                    serde_json::json!(result.compensations_executed),
                ),
        );
    }

    /// Record rate limited event
    pub fn record_rate_limited(&mut self, saga_id: SagaId, saga_type: SagaType) {
        self.record(SagaAuditEntry::new(
            saga_id,
            saga_type,
            SagaAuditEventType::RateLimited,
        ));
    }

    /// Get all entries
    pub fn entries(&self) -> &[SagaAuditEntry] {
        &self.entries
    }

    /// Get entries by event type
    pub fn entries_by_type(&self, event_type: SagaAuditEventType) -> Vec<&SagaAuditEntry> {
        self.entries
            .iter()
            .filter(|e| e.event_type == event_type)
            .collect()
    }

    /// Get step entries
    pub fn step_entries(&self) -> Vec<&SagaAuditEntry> {
        self.entries
            .iter()
            .filter(|e| {
                e.event_type == SagaAuditEventType::StepStarted
                    || e.event_type == SagaAuditEventType::StepCompleted
                    || e.event_type == SagaAuditEventType::StepFailed
            })
            .collect()
    }

    /// Get compensation entries
    pub fn compensation_entries(&self) -> Vec<&SagaAuditEntry> {
        self.entries
            .iter()
            .filter(|e| {
                e.event_type == SagaAuditEventType::CompensationStarted
                    || e.event_type == SagaAuditEventType::CompensationCompleted
                    || e.event_type == SagaAuditEventType::CompensationFailed
            })
            .collect()
    }

    /// Check if saga started
    pub fn has_started(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.event_type == SagaAuditEventType::Started)
    }

    /// Check if saga completed
    pub fn has_completed(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.event_type == SagaAuditEventType::Completed)
    }

    /// Check if saga failed
    pub fn has_failed(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.event_type == SagaAuditEventType::Failed)
    }

    /// Get total number of steps executed
    pub fn steps_executed(&self) -> usize {
        self.step_entries()
            .iter()
            .filter(|e| e.event_type == SagaAuditEventType::StepCompleted)
            .count()
    }

    /// Get total number of compensations executed
    pub fn compensations_executed(&self) -> usize {
        self.compensation_entries()
            .iter()
            .filter(|e| e.event_type == SagaAuditEventType::CompensationCompleted)
            .count()
    }

    /// Calculate total duration from start to end
    pub fn total_duration(&self) -> Option<Duration> {
        let started = self
            .entries
            .iter()
            .find(|e| e.event_type == SagaAuditEventType::Started)?;
        let completed = self.entries.iter().find(|e| {
            e.event_type == SagaAuditEventType::Completed
                || e.event_type == SagaAuditEventType::Failed
        })?;

        let start = started.timestamp;
        let end = completed.timestamp;
        end.duration_since(start).ok()
    }

    /// Export audit trail to JSON
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "entries": self.entries,
            "summary": {
                "total_entries": self.entries.len(),
                "steps_executed": self.steps_executed(),
                "compensations_executed": self.compensations_executed(),
                "has_started": self.has_started(),
                "has_completed": self.has_completed(),
                "has_failed": self.has_failed(),
                "total_duration_ms": self.total_duration().map(|d| d.as_millis() as u64),
            }
        })
    }
}

/// Trait for audit trail consumers
pub trait SagaAuditConsumer: Send + Sync {
    /// Consume an audit entry
    fn consume(&self, entry: &SagaAuditEntry);

    /// Consume multiple audit entries
    fn consume_batch(&self, entries: &[SagaAuditEntry]) {
        for entry in entries {
            self.consume(entry);
        }
    }
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
            step_name = entry.step_name.as_deref().unwrap_or("N/A"),
            step_index = entry.step_index.unwrap_or(0),
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
    /// Create a new vector audit consumer
    pub fn new() -> Self {
        Self {
            entries: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Get all entries
    pub fn entries(&self) -> Vec<SagaAuditEntry> {
        self.entries.lock().unwrap().clone()
    }

    /// Clear all entries
    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }
}

impl SagaAuditConsumer for VecAuditConsumer {
    fn consume(&self, entry: &SagaAuditEntry) {
        self.entries.lock().unwrap().push(entry.clone());
    }
}


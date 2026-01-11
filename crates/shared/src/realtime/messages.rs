//! Realtime Event Types for WebSocket Protocol
//!
//! This module provides the shared types used for WebSocket communication
//! between the server and clients (Leptos WASM frontend).

use serde::{Deserialize, Serialize};

/// Server message envelope sent to WebSocket clients
///
/// Design optimized for bandwidth:
/// - Tag "t" (type) for quick message type identification
/// - Content "d" for message payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "d")]
pub enum ServerMessage {
    /// Response to a client command (e.g., successful subscription)
    #[serde(rename = "ack")]
    Ack { id: String, status: String },

    /// Single event (low frequency scenarios)
    #[serde(rename = "evt")]
    Event { event: ClientEvent },

    /// Batch of events (high frequency optimization)
    ///
    /// This is the CRITICAL optimization to prevent UI freezes.
    /// Instead of sending 500 individual WebSocket frames per second,
    /// we send 1 batch frame containing 500 events every 200ms.
    #[serde(rename = "batch")]
    Batch { events: Vec<ClientEvent> },

    /// System error message
    #[serde(rename = "err")]
    Error { code: String, msg: String },
}

/// Client event projected from DomainEvent
///
/// This is the payload that clients receive. It has been:
/// - **Filtered**: Internal events excluded (~10/40+ total)
/// - **Projected**: Internal fields removed (correlation_id, actor)
/// - **Sanitized**: Sensitive data removed (commands with passwords)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientEvent {
    /// Event type identifier (e.g., "job.status_changed", "worker.heartbeat")
    pub event_type: String,
    /// Event version for schema evolution
    pub event_version: u32,
    /// ID of the aggregate this event belongs to (job_id or worker_id)
    pub aggregate_id: String,
    /// Event-specific payload
    pub payload: ClientEventPayload,
    /// Unix timestamp in milliseconds
    pub occurred_at: i64,
}

impl ClientEvent {
    /// Returns the topics this event should be broadcast to.
    pub fn topics(&self) -> Vec<String> {
        let mut topics = Vec::new();
        topics.push(format!("agg:{}", self.aggregate_id));

        match self.event_type.as_str() {
            "job.created"
            | "job.status_changed"
            | "job.cancelled"
            | "job.retried"
            | "job.assigned"
            | "job.accepted"
            | "job.execution_error"
            | "job.dispatch_failed" => {
                topics.push("jobs:all".to_string());
            }
            "worker.ready"
            | "worker.heartbeat"
            | "worker.terminated"
            | "worker.disconnected"
            | "worker.reconnected" => {
                topics.push("workers:all".to_string());
            }
            "provider.registered"
            | "provider.updated"
            | "provider.health_changed"
            | "provider.recovered"
            | "provider.auto_scaling_triggered"
            | "provider.selected"
            | "provider.execution_error" => {
                topics.push("providers:all".to_string());
            }
            _ => {}
        }

        topics
    }
}

/// Client event payloads
///
/// Only includes fields relevant for UI display.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum ClientEventPayload {
    // ======================================================================
    // Job Events
    // ======================================================================
    #[serde(rename = "job.created")]
    JobCreated { job_id: String, timestamp: i64 },

    #[serde(rename = "job.status_changed")]
    JobStatusChanged {
        job_id: String,
        old_status: String,
        new_status: String,
        timestamp: i64,
    },

    #[serde(rename = "job.cancelled")]
    JobCancelled {
        job_id: String,
        reason: Option<String>,
        timestamp: i64,
    },

    #[serde(rename = "job.retried")]
    JobRetried {
        job_id: String,
        attempt: u32,
        max_attempts: u32,
        timestamp: i64,
    },

    #[serde(rename = "job.assigned")]
    JobAssigned {
        job_id: String,
        worker_id: String,
        timestamp: i64,
    },

    #[serde(rename = "job.accepted")]
    JobAccepted {
        job_id: String,
        worker_id: String,
        timestamp: i64,
    },

    #[serde(rename = "job.execution_error")]
    JobExecutionError {
        job_id: String,
        worker_id: String,
        failure_reason: String,
        command: String,
        timestamp: i64,
    },

    #[serde(rename = "job.dispatch_failed")]
    JobDispatchFailed {
        job_id: String,
        worker_id: String,
        failure_reason: String,
        retry_count: u32,
        timestamp: i64,
    },

    // ======================================================================
    // Worker Events
    // ======================================================================
    #[serde(rename = "worker.ready")]
    WorkerReady {
        worker_id: String,
        provider_id: String,
        current_job_id: Option<String>,
        timestamp: i64,
    },

    #[serde(rename = "worker.heartbeat")]
    WorkerHeartbeat {
        worker_id: String,
        state: String,
        cpu: Option<f32>,
        memory_mb: Option<u64>,
        active_jobs: u32,
        timestamp: i64,
    },

    #[serde(rename = "worker.terminated")]
    WorkerTerminated {
        worker_id: String,
        provider_id: String,
        reason: String,
        timestamp: i64,
    },

    #[serde(rename = "worker.disconnected")]
    WorkerDisconnected { worker_id: String, timestamp: i64 },

    #[serde(rename = "worker.reconnected")]
    WorkerReconnected { worker_id: String, timestamp: i64 },

    // ======================================================================
    // Provider Events
    // ======================================================================
    #[serde(rename = "provider.registered")]
    ProviderRegistered {
        provider_id: String,
        provider_type: String,
        config_summary: String,
        timestamp: i64,
    },

    #[serde(rename = "provider.updated")]
    ProviderUpdated {
        provider_id: String,
        changes: Option<String>,
        timestamp: i64,
    },

    #[serde(rename = "provider.health_changed")]
    ProviderHealthChanged {
        provider_id: String,
        old_status: String,
        new_status: String,
        timestamp: i64,
    },

    #[serde(rename = "provider.recovered")]
    ProviderRecovered {
        provider_id: String,
        previous_status: String,
        timestamp: i64,
    },

    #[serde(rename = "provider.auto_scaling_triggered")]
    AutoScalingTriggered {
        provider_id: String,
        reason: String,
        timestamp: i64,
    },

    #[serde(rename = "provider.selected")]
    ProviderSelected {
        job_id: String,
        provider_id: String,
        provider_type: String,
        effective_cost: f64,
        timestamp: i64,
    },

    #[serde(rename = "provider.execution_error")]
    ProviderExecutionError {
        provider_id: String,
        worker_id: String,
        error_type: String,
        message: String,
        timestamp: i64,
    },

    // ======================================================================
    // Template Events
    // ======================================================================
    #[serde(rename = "template.created")]
    TemplateCreated {
        template_id: String,
        template_name: String,
        version: u32,
        created_by: Option<String>,
        spec_summary: String,
        timestamp: i64,
    },

    #[serde(rename = "template.updated")]
    TemplateUpdated {
        template_id: String,
        template_name: String,
        old_version: u32,
        new_version: u32,
        changes: Option<String>,
        timestamp: i64,
    },

    #[serde(rename = "template.disabled")]
    TemplateDisabled {
        template_id: String,
        template_name: String,
        version: u32,
        timestamp: i64,
    },

    #[serde(rename = "template.run_created")]
    TemplateRunCreated {
        template_id: String,
        template_name: String,
        execution_id: String,
        job_id: Option<String>,
        job_name: String,
        triggered_by: String,
        timestamp: i64,
    },

    #[serde(rename = "template.execution_recorded")]
    TemplateExecutionRecorded {
        execution_id: String,
        template_id: String,
        job_id: Option<String>,
        status: String,
        exit_code: Option<i32>,
        duration_ms: Option<u64>,
        timestamp: i64,
    },

    // ======================================================================
    // Scheduled Job Events
    // ======================================================================
    #[serde(rename = "scheduled_job.created")]
    ScheduledJobCreated {
        scheduled_job_id: String,
        name: String,
        template_id: String,
        created_by: Option<String>,
        timestamp: i64,
    },

    #[serde(rename = "scheduled_job.triggered")]
    ScheduledJobTriggered {
        scheduled_job_id: String,
        name: String,
        template_id: String,
        execution_id: String,
        job_id: Option<String>,
        scheduled_for: i64,
        triggered_at: i64,
        timestamp: i64,
    },

    #[serde(rename = "scheduled_job.missed")]
    ScheduledJobMissed {
        scheduled_job_id: String,
        name: String,
        scheduled_for: i64,
        detected_at: i64,
        reason: String,
        timestamp: i64,
    },

    #[serde(rename = "scheduled_job.error")]
    ScheduledJobError {
        scheduled_job_id: String,
        name: String,
        template_id: String,
        execution_id: String,
        error_message: String,
        timestamp: i64,
    },
}

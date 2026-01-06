//! Outbox Event Model
//!
//! Domain model for outbox events used in the Transactional Outbox Pattern.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Status of an outbox event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutboxStatus {
    /// Event has been created but not yet published
    Pending,
    /// Event has been successfully published to the event bus
    Published,
    /// Event publication failed and will be retried
    Failed,
}

/// Error types for outbox operations
#[derive(Debug, thiserror::Error)]
pub enum OutboxError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Duplicate idempotency key: {0}")]
    DuplicateIdempotencyKey(String),

    #[error("Event not found: {0}")]
    NotFound(Uuid),

    #[error("Infrastructure error: {message}")]
    InfrastructureError { message: String },

    #[error("Event bus error: {0}")]
    EventBus(String),
}

/// Type of aggregate for an outbox event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregateType {
    Job,
    Worker,
    Provider,
}

impl std::fmt::Display for AggregateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AggregateType::Job => write!(f, "JOB"),
            AggregateType::Worker => write!(f, "WORKER"),
            AggregateType::Provider => write!(f, "PROVIDER"),
        }
    }
}

/// An outbox event ready to be inserted into the database
#[derive(Debug, Clone)]
pub struct OutboxEventInsert {
    pub aggregate_id: Uuid,
    pub aggregate_type: AggregateType,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub metadata: Option<serde_json::Value>,
    pub idempotency_key: Option<String>,
}

impl OutboxEventInsert {
    /// Create a new outbox event for insertion
    pub fn new(
        aggregate_id: Uuid,
        aggregate_type: AggregateType,
        event_type: String,
        payload: serde_json::Value,
        metadata: Option<serde_json::Value>,
        idempotency_key: Option<String>,
    ) -> Self {
        Self {
            aggregate_id,
            aggregate_type,
            event_type,
            payload,
            metadata,
            idempotency_key,
        }
    }

    /// Create a job-related event
    pub fn for_job(
        job_id: Uuid,
        event_type: String,
        payload: serde_json::Value,
        metadata: Option<serde_json::Value>,
        idempotency_key: Option<String>,
    ) -> Self {
        Self::new(
            job_id,
            AggregateType::Job,
            event_type,
            payload,
            metadata,
            idempotency_key,
        )
    }

    /// Create a worker-related event
    pub fn for_worker(
        worker_id: Uuid,
        event_type: String,
        payload: serde_json::Value,
        metadata: Option<serde_json::Value>,
        idempotency_key: Option<String>,
    ) -> Self {
        Self::new(
            worker_id,
            AggregateType::Worker,
            event_type,
            payload,
            metadata,
            idempotency_key,
        )
    }
}

/// A view of an outbox event from the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEventView {
    pub id: Uuid,
    pub aggregate_id: Uuid,
    pub aggregate_type: AggregateType,
    pub event_type: String,
    pub event_version: i32,
    pub payload: serde_json::Value,
    pub metadata: Option<serde_json::Value>,
    pub idempotency_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub status: OutboxStatus,
    pub retry_count: i32,
    pub last_error: Option<String>,
}

impl OutboxEventView {
    /// Check if the event is still pending (can be retried)
    pub fn is_pending(&self) -> bool {
        matches!(self.status, OutboxStatus::Pending)
    }

    /// Check if the event has been published successfully
    pub fn is_published(&self) -> bool {
        matches!(self.status, OutboxStatus::Published)
    }

    /// Check if the event has failed and exceeded max retries
    pub fn has_failed(&self, max_retries: i32) -> bool {
        matches!(self.status, OutboxStatus::Failed) && self.retry_count >= max_retries
    }

    /// Get the age of the event
    pub fn age(&self) -> chrono::Duration {
        Utc::now().signed_duration_since(self.created_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outbox_status_display() {
        assert_eq!(AggregateType::Job.to_string(), "JOB");
        assert_eq!(AggregateType::Worker.to_string(), "WORKER");
        assert_eq!(AggregateType::Provider.to_string(), "PROVIDER");
    }

    #[test]
    fn test_outbox_event_insert_creation() {
        let event = OutboxEventInsert::for_job(
            Uuid::new_v4(),
            "JobAssigned".to_string(),
            serde_json::json!({"test": "data"}),
            None,
            Some("key-123".to_string()),
        );

        assert_eq!(event.aggregate_type, AggregateType::Job);
        assert_eq!(event.event_type, "JobAssigned");
        assert_eq!(event.idempotency_key, Some("key-123".to_string()));
    }

    #[test]
    fn test_outbox_event_view_status_checks() {
        let event = OutboxEventView {
            id: Uuid::new_v4(),
            aggregate_id: Uuid::new_v4(),
            aggregate_type: AggregateType::Job,
            event_type: "JobAssigned".to_string(),
            event_version: 1,
            payload: serde_json::json!({"test": "data"}),
            metadata: None,
            idempotency_key: None,
            created_at: Utc::now(),
            published_at: None,
            status: OutboxStatus::Pending,
            retry_count: 0,
            last_error: None,
        };

        assert!(event.is_pending());
        assert!(!event.is_published());
        assert!(!event.has_failed(3));
    }

    #[test]
    fn test_outbox_event_view_published() {
        let event = OutboxEventView {
            id: Uuid::new_v4(),
            aggregate_id: Uuid::new_v4(),
            aggregate_type: AggregateType::Worker,
            event_type: "WorkerReady".to_string(),
            event_version: 1,
            payload: serde_json::json!({"test": "data"}),
            metadata: None,
            idempotency_key: None,
            created_at: Utc::now(),
            published_at: Some(Utc::now()),
            status: OutboxStatus::Published,
            retry_count: 0,
            last_error: None,
        };

        assert!(event.is_published());
        assert!(!event.is_pending());
    }

    #[test]
    fn test_outbox_event_view_failed() {
        let event = OutboxEventView {
            id: Uuid::new_v4(),
            aggregate_id: Uuid::new_v4(),
            aggregate_type: AggregateType::Provider,
            event_type: "ProviderRegistered".to_string(),
            event_version: 1,
            payload: serde_json::json!({"test": "data"}),
            metadata: None,
            idempotency_key: None,
            created_at: Utc::now(),
            published_at: None,
            status: OutboxStatus::Failed,
            retry_count: 3,
            last_error: Some("Connection timeout".to_string()),
        };

        assert!(!event.is_pending());
        assert!(event.has_failed(3));
    }
}

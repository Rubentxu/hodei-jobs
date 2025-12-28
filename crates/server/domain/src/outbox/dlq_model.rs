//! Dead Letter Queue (DLQ) Model for Outbox Events
//!
//! This module provides the domain model for storing events that have repeatedly
//! failed to publish and need manual intervention.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Status of a DLQ entry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DlqStatus {
    /// Entry is waiting for manual resolution
    Pending,
    /// Entry has been resolved by an operator
    Resolved,
    /// Entry was re-queued for reprocessing
    Requeued,
}

/// Error types for DLQ operations
#[derive(Debug, thiserror::Error)]
pub enum DlqError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Entry not found: {0}")]
    NotFound(Uuid),

    #[error("Infrastructure error: {message}")]
    InfrastructureError { message: String },

    #[error("Invalid operation: {message}")]
    InvalidOperation { message: String },
}

/// A Dead Letter Queue entry representing a failed event
///
/// This struct represents an event that has exceeded the maximum number of
/// retry attempts and has been moved to the DLQ for manual investigation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqEntry {
    /// Unique identifier for this DLQ entry
    pub id: Uuid,

    /// The original event ID from the outbox table
    pub original_event_id: Uuid,

    /// The aggregate ID (job_id, worker_id, etc.)
    pub aggregate_id: Uuid,

    /// Type of aggregate (Job, Worker, Provider)
    pub aggregate_type: String,

    /// The type of event that failed
    pub event_type: String,

    /// The original event payload
    pub payload: serde_json::Value,

    /// Event metadata (correlation_id, actor, etc.)
    pub metadata: Option<serde_json::Value>,

    /// The error message from the last failed publish attempt
    pub error_message: String,

    /// Total number of publish attempts made
    pub retry_count: i32,

    /// When the original event was created
    pub original_created_at: DateTime<Utc>,

    /// When this entry was moved to the DLQ
    pub moved_at: DateTime<Utc>,

    /// When the entry was resolved (None if pending)
    pub resolved_at: Option<DateTime<Utc>>,

    /// Notes from the operator who resolved this entry
    pub resolution_notes: Option<String>,

    /// Who resolved this entry
    pub resolved_by: Option<String>,
}

impl DlqEntry {
    /// Create a new DLQ entry from an outbox event
    pub fn from_outbox_event(event: &super::OutboxEventView, error_message: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            original_event_id: event.id,
            aggregate_id: event.aggregate_id,
            aggregate_type: event.aggregate_type.to_string(),
            event_type: event.event_type.clone(),
            payload: event.payload.clone(),
            metadata: event.metadata.clone(),
            error_message,
            retry_count: event.retry_count,
            original_created_at: event.created_at,
            moved_at: Utc::now(),
            resolved_at: None,
            resolution_notes: None,
            resolved_by: None,
        }
    }

    /// Check if the entry is pending resolution
    pub fn is_pending(&self) -> bool {
        self.resolved_at.is_none()
    }

    /// Get the time spent in DLQ
    pub fn time_in_dlq(&self) -> chrono::Duration {
        Utc::now().signed_duration_since(self.moved_at)
    }

    /// Get the total time from event creation to now
    pub fn total_age(&self) -> chrono::Duration {
        Utc::now().signed_duration_since(self.original_created_at)
    }
}

/// View of a DLQ entry for listing purposes (lighter than full entry)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqEntrySummary {
    pub id: Uuid,
    pub original_event_id: Uuid,
    pub aggregate_id: Uuid,
    pub aggregate_type: String,
    pub event_type: String,
    pub retry_count: i32,
    pub error_message: String,
    pub moved_at: DateTime<Utc>,
    pub is_resolved: bool,
}

impl From<&DlqEntry> for DlqEntrySummary {
    fn from(entry: &DlqEntry) -> Self {
        Self {
            id: entry.id,
            original_event_id: entry.original_event_id,
            aggregate_id: entry.aggregate_id,
            aggregate_type: entry.aggregate_type.clone(),
            event_type: entry.event_type.clone(),
            retry_count: entry.retry_count,
            error_message: entry.error_message.clone(),
            moved_at: entry.moved_at,
            is_resolved: entry.resolved_at.is_some(),
        }
    }
}

/// Statistics about the DLQ
#[derive(Debug, Clone, Default)]
pub struct DlqStats {
    pub pending_count: u64,
    pub resolved_count: u64,
    pub oldest_pending_age_seconds: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::{OutboxEventView, OutboxStatus};

    #[test]
    fn test_dlq_entry_from_outbox_event() {
        let outbox_event = OutboxEventView {
            id: Uuid::new_v4(),
            aggregate_id: Uuid::new_v4(),
            aggregate_type: crate::outbox::AggregateType::Job,
            event_type: "JobAssigned".to_string(),
            event_version: 1,
            payload: serde_json::json!({"test": "data"}),
            metadata: Some(serde_json::json!({"correlation_id": "test-123"})),
            idempotency_key: None,
            created_at: Utc::now() - chrono::Duration::hours(1),
            published_at: None,
            status: OutboxStatus::Failed,
            retry_count: 5,
            last_error: Some("Connection timeout".to_string()),
        };

        let dlq_entry = DlqEntry::from_outbox_event(
            &outbox_event,
            "Connection timeout after 5 retries".to_string(),
        );

        assert_eq!(dlq_entry.original_event_id, outbox_event.id);
        assert_eq!(dlq_entry.aggregate_id, outbox_event.aggregate_id);
        assert_eq!(dlq_entry.event_type, "JobAssigned");
        assert_eq!(dlq_entry.retry_count, 5);
        assert!(dlq_entry.is_pending());
        assert!(dlq_entry.time_in_dlq().num_seconds() >= 0);
    }

    #[test]
    fn test_dlq_entry_is_pending() {
        let entry = DlqEntry {
            id: Uuid::new_v4(),
            original_event_id: Uuid::new_v4(),
            aggregate_id: Uuid::new_v4(),
            aggregate_type: "JOB".to_string(),
            event_type: "TestEvent".to_string(),
            payload: serde_json::json!({}),
            metadata: None,
            error_message: "Test error".to_string(),
            retry_count: 3,
            original_created_at: Utc::now(),
            moved_at: Utc::now(),
            resolved_at: None,
            resolution_notes: None,
            resolved_by: None,
        };

        assert!(entry.is_pending());
    }

    #[test]
    fn test_dlq_entry_resolved() {
        let entry = DlqEntry {
            id: Uuid::new_v4(),
            original_event_id: Uuid::new_v4(),
            aggregate_id: Uuid::new_v4(),
            aggregate_type: "JOB".to_string(),
            event_type: "TestEvent".to_string(),
            payload: serde_json::json!({}),
            metadata: None,
            error_message: "Test error".to_string(),
            retry_count: 3,
            original_created_at: Utc::now(),
            moved_at: Utc::now(),
            resolved_at: Some(Utc::now()),
            resolution_notes: Some("Fixed upstream issue".to_string()),
            resolved_by: Some("admin@example.com".to_string()),
        };

        assert!(!entry.is_pending());
        assert_eq!(
            entry.resolution_notes,
            Some("Fixed upstream issue".to_string())
        );
    }

    #[test]
    fn test_dlq_entry_summary() {
        let entry = DlqEntry {
            id: Uuid::new_v4(),
            original_event_id: Uuid::new_v4(),
            aggregate_id: Uuid::new_v4(),
            aggregate_type: "WORKER".to_string(),
            event_type: "WorkerTerminated".to_string(),
            payload: serde_json::json!({}),
            metadata: None,
            error_message: "Connection lost".to_string(),
            retry_count: 2,
            original_created_at: Utc::now(),
            moved_at: Utc::now(),
            resolved_at: None,
            resolution_notes: None,
            resolved_by: None,
        };

        let summary = DlqEntrySummary::from(&entry);

        assert_eq!(summary.id, entry.id);
        assert_eq!(summary.event_type, "WorkerTerminated");
        assert!(!summary.is_resolved);
    }
}

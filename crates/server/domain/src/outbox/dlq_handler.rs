//! Dead Letter Queue Handler
//!
//! This module provides the DLQ handler that moves failed events from the
//! outbox to the Dead Letter Queue when they exceed the maximum retry count.

use std::sync::Arc;
use tracing::{debug, error, info};
use uuid::Uuid;

use super::OutboxEventView;
use super::dlq_model::{DlqEntry, DlqStats};
use super::dlq_repository::DlqRepository;

/// Configuration for DLQ behavior
#[derive(Debug, Clone)]
pub struct DlqHandlerConfig {
    /// Maximum retries before moving to DLQ
    pub max_retries: i32,

    /// Whether to log when events enter DLQ
    pub log_on_dlq: bool,
}

impl Default for DlqHandlerConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            log_on_dlq: true,
        }
    }
}

/// Handler that moves failed events to the Dead Letter Queue
///
/// This struct is responsible for:
/// 1. Detecting events that have exceeded the retry limit
/// 2. Moving them to the DLQ with full error context
/// 3. Providing statistics about DLQ contents
pub struct DlqHandler<R> {
    dlq_repository: Arc<R>,
    config: DlqHandlerConfig,
}

impl<R> DlqHandler<R>
where
    R: DlqRepository + Send + Sync,
{
    /// Create a new DLQ handler
    pub fn new(dlq_repository: Arc<R>, config: DlqHandlerConfig) -> Self {
        Self {
            dlq_repository,
            config,
        }
    }

    /// Create a DLQ handler with default configuration
    pub fn with_defaults(dlq_repository: Arc<R>) -> Self {
        Self::new(dlq_repository, DlqHandlerConfig::default())
    }

    /// Handle a failed event by moving it to the DLQ
    ///
    /// # Arguments
    /// * `event` - The failed outbox event
    /// * `error_message` - The error from the last publish attempt
    ///
    /// # Returns
    /// Result indicating success or failure
    pub async fn handle_failed_event(
        &self,
        event: &OutboxEventView,
        error_message: &str,
    ) -> Result<(), <R as DlqRepository>::Error> {
        // Check if the event has exceeded the retry limit
        if event.retry_count < self.config.max_retries {
            debug!(
                event_id = event.id.to_string(),
                retry_count = event.retry_count,
                max_retries = self.config.max_retries,
                "Event not yet at max retries, will retry later"
            );
            return Ok(());
        }

        // Create DLQ entry
        let dlq_entry = DlqEntry::from_outbox_event(event, error_message.to_string());

        // Insert into DLQ
        self.dlq_repository.insert(&dlq_entry).await?;

        if self.config.log_on_dlq {
            error!(
                event_id = %event.id,
                event_type = event.event_type,
                aggregate_id = %event.aggregate_id,
                retry_count = event.retry_count,
                error = error_message,
                "Event moved to Dead Letter Queue"
            );
        }

        Ok(())
    }

    /// Process a batch of failed events
    ///
    /// # Arguments
    /// * `events` - List of failed events with their error messages
    ///
    /// # Returns
    /// Number of events moved to DLQ
    pub async fn process_failed_events(
        &self,
        events: Vec<(OutboxEventView, String)>,
    ) -> Result<u64, <R as DlqRepository>::Error> {
        let mut moved_count = 0u64;

        for (event, error_msg) in events {
            if event.retry_count >= self.config.max_retries {
                if let Err(e) = self.handle_failed_event(&event, &error_msg).await {
                    error!(
                        event_id = %event.id,
                        error = %e,
                        "Failed to move event to DLQ"
                    );
                } else {
                    moved_count += 1;
                }
            }
        }

        if moved_count > 0 {
            info!(
                count = moved_count,
                "Moved failed events to Dead Letter Queue"
            );
        }

        Ok(moved_count)
    }

    /// Get DLQ statistics
    pub async fn get_stats(&self) -> Result<DlqStats, <R as DlqRepository>::Error> {
        self.dlq_repository.get_stats().await
    }

    /// Resolve a DLQ entry
    pub async fn resolve(
        &self,
        id: Uuid,
        notes: &str,
        resolved_by: &str,
    ) -> Result<(), <R as DlqRepository>::Error> {
        self.dlq_repository.resolve(id, notes, resolved_by).await?;
        info!(dlq_entry_id = %id, resolved_by = resolved_by, "DLQ entry resolved");
        Ok(())
    }

    /// Re-queue a DLQ entry for reprocessing
    pub async fn requeue(&self, id: Uuid) -> Result<Option<Uuid>, <R as DlqRepository>::Error> {
        let event_id = self.dlq_repository.requeue(id).await?;
        if let Some(eid) = &event_id {
            info!(dlq_entry_id = %id, original_event_id = %eid, "DLQ entry requeued for reprocessing");
        }
        Ok(event_id)
    }

    /// Get pending DLQ entries
    pub async fn get_pending(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<DlqEntry>, <R as DlqRepository>::Error> {
        self.dlq_repository.get_pending(limit, offset).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::dlq_repository::mocks::InMemoryDlqRepository;
    use crate::outbox::{OutboxEventView, OutboxStatus};
    use chrono::Utc;

    #[tokio::test]
    async fn test_handle_failed_event_under_max_retries() {
        let repo = InMemoryDlqRepository::new();
        let handler = DlqHandler::with_defaults(Arc::new(repo));

        let event = OutboxEventView {
            id: Uuid::new_v4(),
            aggregate_id: Uuid::new_v4(),
            aggregate_type: crate::outbox::AggregateType::Job,
            event_type: "TestEvent".to_string(),
            event_version: 1,
            payload: serde_json::json!({"test": "data"}),
            metadata: None,
            idempotency_key: None,
            created_at: Utc::now(),
            published_at: None,
            status: OutboxStatus::Failed,
            retry_count: 2,
            last_error: Some("Connection timeout".to_string()),
        };

        // Should not move to DLQ because retry_count (2) < max_retries (5)
        handler
            .handle_failed_event(&event, "Connection timeout")
            .await
            .unwrap();

        // Entry should NOT be in DLQ
        let stats = handler.get_stats().await.unwrap();
        assert_eq!(stats.pending_count, 0);
    }

    #[tokio::test]
    async fn test_handle_failed_event_at_max_retries() {
        let repo = InMemoryDlqRepository::new();
        let repo_clone = repo.clone();
        let handler = DlqHandler::with_defaults(Arc::new(repo));

        let event = OutboxEventView {
            id: Uuid::new_v4(),
            aggregate_id: Uuid::new_v4(),
            aggregate_type: crate::outbox::AggregateType::Worker,
            event_type: "WorkerFailed".to_string(),
            event_version: 1,
            payload: serde_json::json!({"error": "persistent"}),
            metadata: None,
            idempotency_key: None,
            created_at: Utc::now(),
            published_at: None,
            status: OutboxStatus::Failed,
            retry_count: 5, // At max_retries
            last_error: Some("Permanent failure".to_string()),
        };

        // Should move to DLQ
        handler
            .handle_failed_event(&event, "Permanent failure")
            .await
            .unwrap();

        // Entry should be in DLQ
        let stats = handler.get_stats().await.unwrap();
        assert_eq!(stats.pending_count, 1);

        let dlq_entry = repo_clone.get_by_event_id(event.id).await.unwrap().unwrap();
        assert_eq!(dlq_entry.error_message, "Permanent failure");
        assert!(dlq_entry.is_pending());
    }

    #[tokio::test]
    async fn test_process_failed_events_batch() {
        let repo = InMemoryDlqRepository::new();
        let handler = DlqHandler::with_defaults(Arc::new(repo));

        let events = vec![
            (
                OutboxEventView {
                    id: Uuid::new_v4(),
                    aggregate_id: Uuid::new_v4(),
                    aggregate_type: crate::outbox::AggregateType::Job,
                    event_type: "Event1".to_string(),
                    event_version: 1,
                    payload: serde_json::json!({}),
                    metadata: None,
                    idempotency_key: None,
                    created_at: Utc::now(),
                    published_at: None,
                    status: OutboxStatus::Failed,
                    retry_count: 5,
                    last_error: None,
                },
                "Error 1".to_string(),
            ),
            (
                OutboxEventView {
                    id: Uuid::new_v4(),
                    aggregate_id: Uuid::new_v4(),
                    aggregate_type: crate::outbox::AggregateType::Job,
                    event_type: "Event2".to_string(),
                    event_version: 1,
                    payload: serde_json::json!({}),
                    metadata: None,
                    idempotency_key: None,
                    created_at: Utc::now(),
                    published_at: None,
                    status: OutboxStatus::Failed,
                    retry_count: 3, // Under max, should not be moved
                    last_error: None,
                },
                "Error 2".to_string(),
            ),
        ];

        let moved = handler.process_failed_events(events).await.unwrap();
        assert_eq!(moved, 1); // Only Event1 should be moved

        let stats = handler.get_stats().await.unwrap();
        assert_eq!(stats.pending_count, 1);
    }

    #[tokio::test]
    async fn test_resolve_dlq_entry() {
        let event = DlqEntry {
            id: Uuid::new_v4(),
            original_event_id: Uuid::new_v4(),
            aggregate_id: Uuid::new_v4(),
            aggregate_type: "JOB".to_string(),
            event_type: "TestEvent".to_string(),
            payload: serde_json::json!({}),
            metadata: None,
            error_message: "Error".to_string(),
            retry_count: 5,
            original_created_at: Utc::now(),
            moved_at: Utc::now(),
            resolved_at: None,
            resolution_notes: None,
            resolved_by: None,
        };

        let repo = InMemoryDlqRepository::with_entries(vec![event.clone()]);
        let handler = DlqHandler::with_defaults(Arc::new(repo));

        handler
            .resolve(event.id, "Fixed upstream issue", "admin@test.com")
            .await
            .unwrap();

        let stats = handler.get_stats().await.unwrap();
        assert_eq!(stats.pending_count, 0);
        assert_eq!(stats.resolved_count, 1);
    }

    #[tokio::test]
    async fn test_requeue_dlq_entry() {
        let event = DlqEntry {
            id: Uuid::new_v4(),
            original_event_id: Uuid::new_v4(),
            aggregate_id: Uuid::new_v4(),
            aggregate_type: "PROVIDER".to_string(),
            event_type: "ProviderEvent".to_string(),
            payload: serde_json::json!({"key": "value"}),
            metadata: None,
            error_message: "Temp error".to_string(),
            retry_count: 5,
            original_created_at: Utc::now(),
            moved_at: Utc::now(),
            resolved_at: None,
            resolution_notes: None,
            resolved_by: None,
        };

        let repo = InMemoryDlqRepository::with_entries(vec![event.clone()]);
        let handler = DlqHandler::with_defaults(Arc::new(repo));

        let original_event_id = handler.requeue(event.id).await.unwrap().unwrap();
        assert_eq!(original_event_id, event.original_event_id);

        let stats = handler.get_stats().await.unwrap();
        assert_eq!(stats.pending_count, 0);
    }
}

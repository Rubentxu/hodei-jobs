//! Outbox Repository Trait
//!
//! Abstraction for outbox event persistence operations.

use crate::outbox::{OutboxError, OutboxEventInsert, OutboxEventView};
use async_trait::async_trait;
use sqlx::PgTransaction;
use uuid::Uuid;

/// Repository for outbox event persistence
///
/// Provides atomic operations for storing and retrieving outbox events
/// used in the Transactional Outbox Pattern.
#[async_trait::async_trait]
pub trait OutboxRepository {
    /// Insert events into the outbox table
    ///
    /// This should be called within a database transaction to ensure
    /// atomicity with the main business operation.
    ///
    /// # Arguments
    /// * `events` - Slice of events to insert
    ///
    /// # Returns
    /// * `Result<(), OutboxError>` - Success or error
    ///
    /// # Errors
    /// * Returns an error if:
    ///   - Database operation fails
    ///   - Duplicate idempotency key is detected
    ///   - Transaction rollback occurs
    async fn insert_events(&self, events: &[OutboxEventInsert]) -> Result<(), OutboxError>;

    /// Retrieve pending events for publication
    ///
    /// Uses `FOR UPDATE SKIP LOCKED` to avoid contention in multi-instance deployments.
    /// Events are ordered by `created_at` to ensure chronological order.
    ///
    /// # Arguments
    /// * `limit` - Maximum number of events to retrieve
    /// * `max_retries` - Maximum retry count to include (events above this are skipped)
    ///
    /// # Returns
    /// * `Result<Vec<OutboxEventView>, OutboxError>` - List of pending events
    async fn get_pending_events(
        &self,
        limit: usize,
        max_retries: i32,
    ) -> Result<Vec<OutboxEventView>, OutboxError>;

    /// Mark events as published
    ///
    /// Updates the status and sets the published_at timestamp.
    ///
    /// # Arguments
    /// * `event_ids` - IDs of events to mark as published
    ///
    /// # Returns
    /// * `Result<(), OutboxError>` - Success or error
    async fn mark_published(&self, event_ids: &[Uuid]) -> Result<(), OutboxError>;

    /// Mark an event as failed
    ///
    /// Increments the retry count and stores the error message.
    /// The event will remain pending for future retry attempts.
    ///
    /// # Arguments
    /// * `event_id` - ID of the event to mark as failed
    /// * `error` - Error message describing the failure
    ///
    /// # Returns
    /// * `Result<(), OutboxError>` - Success or error
    async fn mark_failed(&self, event_id: &Uuid, error: &str) -> Result<(), OutboxError>;

    /// Record a failure for retry
    ///
    /// Increments the retry count and stores the error message, but keeps the status as PENDING.
    ///
    /// # Arguments
    /// * `event_id` - ID of the event
    /// * `error` - Error message
    ///
    /// # Returns
    /// * `Result<(), OutboxError>` - Success or error
    async fn record_failure_retry(&self, event_id: &Uuid, error: &str) -> Result<(), OutboxError>;

    /// Check if an idempotency key already exists
    ///
    /// Used to ensure idempotency when inserting events.
    ///
    /// # Arguments
    /// * `idempotency_key` - The idempotency key to check
    ///
    /// # Returns
    /// * `Result<bool, OutboxError>` - True if exists, false otherwise
    async fn exists_by_idempotency_key(&self, idempotency_key: &str) -> Result<bool, OutboxError>;

    /// Count pending events
    ///
    /// Useful for monitoring and alerting.
    ///
    /// # Returns
    /// * `Result<u64, OutboxError>` - Count of pending events
    async fn count_pending(&self) -> Result<u64, OutboxError>;

    /// Get statistics about outbox events
    ///
    /// Returns counts by status for monitoring purposes.
    ///
    /// # Returns
    /// * `Result<OutboxStats, OutboxError>` - Statistics
    async fn get_stats(&self) -> Result<OutboxStats, OutboxError>;

    /// Delete published events older than the specified duration
    ///
    /// This is used for cleanup of old events that have been successfully processed.
    /// Only published events are deleted to preserve failed events for investigation.
    ///
    /// # Arguments
    /// * `older_than` - Delete events older than this duration
    ///
    /// # Returns
    /// * `Result<u64, OutboxError>` - Number of events deleted
    async fn cleanup_published_events(
        &self,
        older_than: std::time::Duration,
    ) -> Result<u64, OutboxError>;

    /// Delete failed events that have exceeded max retry attempts
    ///
    /// Events that have failed too many times and are beyond recovery.
    ///
    /// # Arguments
    /// * `max_retries` - Delete events with retry_count >= this value
    /// * `older_than` - Only delete if also older than this duration
    ///
    /// # Returns
    /// * `Result<u64, OutboxError>` - Number of events deleted
    async fn cleanup_failed_events(
        &self,
        max_retries: i32,
        older_than: std::time::Duration,
    ) -> Result<u64, OutboxError>;

    /// Archive old events to a separate table (optional implementation)
    ///
    /// Moves events to an archive table instead of deleting them.
    /// Default implementation just deletes.
    ///
    /// # Arguments
    /// * `older_than` - Archive events older than this duration
    ///
    /// # Returns
    /// * `Result<u64, OutboxError>` - Number of events archived
    async fn archive_old_events(
        &self,
        older_than: std::time::Duration,
    ) -> Result<u64, OutboxError> {
        // Default: just cleanup published events
        self.cleanup_published_events(older_than).await
    }

    /// Find an event by its ID
    ///
    /// Used for reactive processing when receiving NOTIFY events.
    ///
    /// # Arguments
    /// * `id` - The event ID to find
    ///
    /// # Returns
    /// * `Result<Option<OutboxEventView>, OutboxError>` - The event if found
    async fn find_by_id(&self, id: Uuid) -> Result<Option<OutboxEventView>, OutboxError>;
}

/// Outbox Repository with Transaction Support
///
/// Extends the base OutboxRepository with transaction-aware operations
/// for the Transactional Outbox Pattern (EPIC-43 Sprint 1).
#[async_trait::async_trait]
pub trait OutboxRepositoryTx {
    /// Error type for repository operations
    type Error: From<OutboxError> + std::fmt::Debug + Send + Sync;

    /// Insert events into the outbox within an existing transaction.
    ///
    /// This is the core method for the Transactional Outbox Pattern.
    /// Call this method within the same transaction as your entity persistence.
    ///
    /// # Arguments
    /// * `tx` - The database transaction to use
    /// * `events` - Events to insert
    ///
    /// # Returns
    /// * `std::result::Result<(), OutboxError>` - Success or error
    ///
    /// # Errors
    /// * Transaction errors, duplicate idempotency keys, etc.
    async fn insert_events_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        events: &[OutboxEventInsert],
    ) -> std::result::Result<(), OutboxError>;

    /// Check if an idempotency key exists within a transaction.
    ///
    /// Useful for checking if an event has already been processed
    /// before inserting within the same transaction.
    ///
    /// # Arguments
    /// * `tx` - The database transaction to use
    /// * `idempotency_key` - The key to check
    ///
    /// # Returns
    /// * `std::result::Result<bool, OutboxError>` - True if exists
    async fn exists_by_idempotency_key_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        idempotency_key: &str,
    ) -> std::result::Result<bool, OutboxError>;
}

/// Statistics about outbox events
#[derive(Debug, Clone)]
pub struct OutboxStats {
    pub pending_count: u64,
    pub published_count: u64,
    pub failed_count: u64,
    pub oldest_pending_age_seconds: Option<i64>,
}

impl OutboxStats {
    /// Calculate the total number of events
    pub fn total(&self) -> u64 {
        self.pending_count + self.published_count + self.failed_count
    }

    /// Check if there are any pending events
    pub fn has_pending(&self) -> bool {
        self.pending_count > 0
    }

    /// Get the error rate as a percentage
    pub fn error_rate(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            (self.failed_count as f64 / total as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    // Mock implementation for testing
    struct MockOutboxRepository {
        events: std::sync::Mutex<Vec<OutboxEventView>>,
    }

    impl MockOutboxRepository {
        fn new() -> Self {
            Self {
                events: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl OutboxRepository for MockOutboxRepository {
        async fn insert_events(&self, events: &[OutboxEventInsert]) -> Result<(), OutboxError> {
            let mut vec = self.events.lock().unwrap();
            for event in events {
                if let Some(ref key) = event.idempotency_key {
                    if vec.iter().any(|e| e.idempotency_key.as_ref() == Some(key)) {
                        return Err(OutboxError::DuplicateIdempotencyKey(key.clone()));
                    }
                }
                vec.push(OutboxEventView {
                    id: Uuid::new_v4(),
                    aggregate_id: event.aggregate_id,
                    aggregate_type: event.aggregate_type.clone(),
                    event_type: event.event_type.clone(),
                    event_version: 1,
                    payload: event.payload.clone(),
                    metadata: event.metadata.clone(),
                    idempotency_key: event.idempotency_key.clone(),
                    created_at: chrono::Utc::now(),
                    published_at: None,
                    status: crate::outbox::OutboxStatus::Pending,
                    retry_count: 0,
                    last_error: None,
                });
            }
            Ok(())
        }

        async fn get_pending_events(
            &self,
            limit: usize,
            _max_retries: i32,
        ) -> Result<Vec<OutboxEventView>, OutboxError> {
            let vec = self.events.lock().unwrap();
            Ok(vec
                .iter()
                .filter(|e| matches!(e.status, crate::outbox::OutboxStatus::Pending))
                .take(limit)
                .cloned()
                .collect())
        }

        async fn mark_published(&self, event_ids: &[Uuid]) -> Result<(), OutboxError> {
            let mut vec = self.events.lock().unwrap();
            for id in event_ids {
                if let Some(event) = vec.iter_mut().find(|e| &e.id == id) {
                    event.status = crate::outbox::OutboxStatus::Published;
                    event.published_at = Some(chrono::Utc::now());
                }
            }
            Ok(())
        }

        async fn mark_failed(&self, event_id: &Uuid, error: &str) -> Result<(), OutboxError> {
            let mut vec = self.events.lock().unwrap();
            if let Some(event) = vec.iter_mut().find(|e| &e.id == event_id) {
                event.status = crate::outbox::OutboxStatus::Failed;
                event.retry_count += 1;
                event.last_error = Some(error.to_string());
            }
            Ok(())
        }

        async fn record_failure_retry(&self, event_id: &Uuid, error: &str) -> Result<(), OutboxError> {
            let mut vec = self.events.lock().unwrap();
            if let Some(event) = vec.iter_mut().find(|e| &e.id == event_id) {
                // Keep status as PENDING (or whatever it was)
                event.retry_count += 1;
                event.last_error = Some(error.to_string());
            }
            Ok(())
        }

        async fn exists_by_idempotency_key(&self, key: &str) -> Result<bool, OutboxError> {
            let vec = self.events.lock().unwrap();
            Ok(vec
                .iter()
                .any(|e| e.idempotency_key.as_deref() == Some(key)))
        }

        async fn count_pending(&self) -> Result<u64, OutboxError> {
            let vec = self.events.lock().unwrap();
            Ok(vec.iter().filter(|e| e.is_pending()).count() as u64)
        }

        async fn get_stats(&self) -> Result<OutboxStats, OutboxError> {
            let vec = self.events.lock().unwrap();
            let pending_count = vec.iter().filter(|e| e.is_pending()).count() as u64;
            let published_count = vec.iter().filter(|e| e.is_published()).count() as u64;
            let failed_count = vec
                .iter()
                .filter(|e| matches!(e.status, crate::outbox::OutboxStatus::Failed))
                .count() as u64;

            let oldest_pending_age_seconds = vec
                .iter()
                .filter(|e| e.is_pending())
                .map(|e| e.age().num_seconds())
                .max();

            Ok(OutboxStats {
                pending_count,
                published_count,
                failed_count,
                oldest_pending_age_seconds,
            })
        }

        async fn cleanup_published_events(
            &self,
            older_than: std::time::Duration,
        ) -> Result<u64, OutboxError> {
            let mut vec = self.events.lock().unwrap();
            let now = chrono::Utc::now();
            let threshold = now - chrono::Duration::from_std(older_than).unwrap_or_default();

            let before_count = vec.len();
            vec.retain(|e| !(e.is_published() && e.created_at < threshold));
            let deleted = (before_count - vec.len()) as u64;
            Ok(deleted)
        }

        async fn cleanup_failed_events(
            &self,
            max_retries: i32,
            older_than: std::time::Duration,
        ) -> Result<u64, OutboxError> {
            let mut vec = self.events.lock().unwrap();
            let now = chrono::Utc::now();
            let threshold = now - chrono::Duration::from_std(older_than).unwrap_or_default();

            let before_count = vec.len();
            vec.retain(|e| {
                !(matches!(e.status, crate::outbox::OutboxStatus::Failed)
                    && e.retry_count >= max_retries
                    && e.created_at < threshold)
            });
            let deleted = (before_count - vec.len()) as u64;
            Ok(deleted)
        }

        async fn find_by_id(&self, id: uuid::Uuid) -> Result<Option<OutboxEventView>, OutboxError> {
            let vec = self.events.lock().unwrap();
            Ok(vec.iter().find(|e| e.id == id).cloned())
        }
    }

    #[tokio::test]
    async fn test_insert_events() {
        let repo = MockOutboxRepository::new();

        let events = vec![OutboxEventInsert::for_job(
            Uuid::new_v4(),
            "JobAssigned".to_string(),
            serde_json::json!({"job_id": "123", "worker_id": "456"}),
            None,
            Some("assign-123-456".to_string()),
        )];

        let result = repo.insert_events(&events).await;
        assert!(result.is_ok());

        let pending = repo.get_pending_events(10, 3).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event_type, "JobAssigned");
    }

    #[tokio::test]
    async fn test_idempotency_check() {
        let repo = MockOutboxRepository::new();

        let key = "test-key-123";
        assert!(!repo.exists_by_idempotency_key(key).await.unwrap());

        let event = OutboxEventInsert::for_job(
            Uuid::new_v4(),
            "JobCreated".to_string(),
            serde_json::json!({"test": "data"}),
            None,
            Some(key.to_string()),
        );
        repo.insert_events(&[event]).await.unwrap();

        assert!(repo.exists_by_idempotency_key(key).await.unwrap());
    }

    #[tokio::test]
    async fn test_mark_published() {
        let repo = MockOutboxRepository::new();

        let event = OutboxEventInsert::for_job(
            Uuid::new_v4(),
            "JobAssigned".to_string(),
            serde_json::json!({"test": "data"}),
            None,
            None,
        );
        repo.insert_events(&[event]).await.unwrap();

        let pending = repo.get_pending_events(10, 3).await.unwrap();
        let event_id = pending[0].id;

        assert!(pending[0].is_pending());

        repo.mark_published(&[event_id]).await.unwrap();

        let published = repo.get_pending_events(10, 3).await.unwrap();
        assert!(published.is_empty());
    }

    #[tokio::test]
    async fn test_mark_failed() {
        let repo = MockOutboxRepository::new();

        let event = OutboxEventInsert::for_job(
            Uuid::new_v4(),
            "JobAssigned".to_string(),
            serde_json::json!({"test": "data"}),
            None,
            None,
        );
        repo.insert_events(&[event]).await.unwrap();

        let pending = repo.get_pending_events(10, 3).await.unwrap();
        let event_id = pending[0].id;

        repo.mark_failed(&event_id, "Network error").await.unwrap();

        let pending = repo.get_pending_events(10, 3).await.unwrap();
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn test_stats() {
        let repo = MockOutboxRepository::new();

        // Insert some events
        for i in 0..5 {
            let event = OutboxEventInsert::for_job(
                Uuid::new_v4(),
                format!("JobCreated{}", i),
                serde_json::json!({"id": i}),
                None,
                None,
            );
            repo.insert_events(&[event]).await.unwrap();
        }

        let stats = repo.get_stats().await.unwrap();
        assert_eq!(stats.pending_count, 5);
        assert_eq!(stats.published_count, 0);
        assert_eq!(stats.failed_count, 0);
        assert_eq!(stats.total(), 5);
    }

    #[tokio::test]
    async fn test_duplicate_idempotency_key() {
        let repo = MockOutboxRepository::new();

        let key = "duplicate-key";
        let event1 = OutboxEventInsert::for_job(
            Uuid::new_v4(),
            "JobCreated".to_string(),
            serde_json::json!({"test": "data1"}),
            None,
            Some(key.to_string()),
        );
        let event2 = OutboxEventInsert::for_job(
            Uuid::new_v4(),
            "JobCreated".to_string(),
            serde_json::json!({"test": "data2"}),
            None,
            Some(key.to_string()),
        );

        repo.insert_events(&[event1]).await.unwrap();
        let result = repo.insert_events(&[event2]).await;

        assert!(matches!(
            result,
            Err(OutboxError::DuplicateIdempotencyKey(ref k)) if k == key
        ));
    }
}

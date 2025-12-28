//! Dead Letter Queue Repository Interface
//!
//! This module defines the repository interface for DLQ operations.

use async_trait::async_trait;
use uuid::Uuid;

use super::DlqEntry;
use super::DlqStats;

/// Repository interface for DLQ operations
///
/// This trait defines the operations that must be supported for managing
/// Dead Letter Queue entries. Implementations can use SQL, NoSQL, or other
/// storage backends.
#[async_trait]
pub trait DlqRepository {
    /// The error type for repository operations
    type Error: std::fmt::Display + Send + Sync;

    /// Insert an entry into the DLQ
    ///
    /// # Arguments
    /// * `entry` - The DLQ entry to insert
    ///
    /// # Returns
    /// Result indicating success or failure
    async fn insert(&self, entry: &DlqEntry) -> Result<(), Self::Error>;

    /// Get a DLQ entry by its ID
    ///
    /// # Arguments
    /// * `id` - The DLQ entry ID
    ///
    /// # Returns
    /// Some(DlqEntry) if found, None otherwise
    async fn get_by_id(&self, id: Uuid) -> Result<Option<DlqEntry>, Self::Error>;

    /// Get a DLQ entry by the original event ID
    ///
    /// # Arguments
    /// * `event_id` - The original outbox event ID
    ///
    /// # Returns
    /// Some(DlqEntry) if found, None otherwise
    async fn get_by_event_id(&self, event_id: Uuid) -> Result<Option<DlqEntry>, Self::Error>;

    /// Get all pending DLQ entries
    ///
    /// # Arguments
    /// * `limit` - Maximum number of entries to return
    /// * `offset` - Number of entries to skip
    ///
    /// # Returns
    /// List of pending DLQ entries
    async fn get_pending(&self, limit: usize, offset: usize) -> Result<Vec<DlqEntry>, Self::Error>;

    /// Mark a DLQ entry as resolved
    ///
    /// # Arguments
    /// * `id` - The DLQ entry ID
    /// * `notes` - Resolution notes
    /// * `resolved_by` - Who resolved the entry
    ///
    /// # Returns
    /// Result indicating success or failure
    async fn resolve(&self, id: Uuid, notes: &str, resolved_by: &str) -> Result<(), Self::Error>;

    /// Re-queue a DLQ entry for reprocessing
    ///
    /// This moves the entry back to the outbox table for another attempt.
    ///
    /// # Arguments
    /// * `id` - The DLQ entry ID
    ///
    /// # Returns
    /// Result containing the original event data if successful
    async fn requeue(&self, id: Uuid) -> Result<Option<Uuid>, Self::Error>;

    /// Get DLQ statistics
    async fn get_stats(&self) -> Result<DlqStats, Self::Error>;

    /// Delete a DLQ entry
    ///
    /// # Arguments
    /// * `id` - The DLQ entry ID
    ///
    /// # Returns
    /// Result indicating success or failure
    async fn delete(&self, id: Uuid) -> Result<(), Self::Error>;

    /// Cleanup old resolved entries
    ///
    /// # Arguments
    /// * `older_than` - Remove entries older than this duration
    ///
    /// # Returns
    /// Number of entries deleted
    async fn cleanup_resolved(&self, older_than: std::time::Duration) -> Result<u64, Self::Error>;
}

#[cfg(test)]
pub mod mocks {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// In-memory implementation of DlqRepository for testing
    #[derive(Clone)]
    pub struct InMemoryDlqRepository {
        entries: Arc<Mutex<HashMap<Uuid, DlqEntry>>>,
    }

    impl InMemoryDlqRepository {
        pub fn new() -> Self {
            Self {
                entries: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        pub fn with_entries(entries: Vec<DlqEntry>) -> Self {
            Self {
                entries: Arc::new(Mutex::new(entries.into_iter().map(|e| (e.id, e)).collect())),
            }
        }
    }

    #[async_trait]
    impl DlqRepository for InMemoryDlqRepository {
        type Error = String;

        async fn insert(&self, entry: &DlqEntry) -> Result<(), Self::Error> {
            let mut entries = self.entries.lock().unwrap();
            entries.insert(entry.id, entry.clone());
            Ok(())
        }

        async fn get_by_id(&self, id: Uuid) -> Result<Option<DlqEntry>, Self::Error> {
            let entries = self.entries.lock().unwrap();
            Ok(entries.get(&id).cloned())
        }

        async fn get_by_event_id(&self, event_id: Uuid) -> Result<Option<DlqEntry>, Self::Error> {
            let entries = self.entries.lock().unwrap();
            Ok(entries
                .values()
                .find(|e| e.original_event_id == event_id)
                .cloned())
        }

        async fn get_pending(
            &self,
            limit: usize,
            offset: usize,
        ) -> Result<Vec<DlqEntry>, Self::Error> {
            let entries = self.entries.lock().unwrap();
            let pending: Vec<DlqEntry> = entries
                .values()
                .filter(|e| e.is_pending())
                .cloned()
                .skip(offset)
                .take(limit)
                .collect();
            Ok(pending)
        }

        async fn resolve(
            &self,
            id: Uuid,
            notes: &str,
            resolved_by: &str,
        ) -> Result<(), Self::Error> {
            let mut entries = self.entries.lock().unwrap();
            if let Some(entry) = entries.get_mut(&id) {
                entry.resolved_at = Some(Utc::now());
                entry.resolution_notes = Some(notes.to_string());
                entry.resolved_by = Some(resolved_by.to_string());
                Ok(())
            } else {
                Err(format!("DLQ entry {} not found", id))
            }
        }

        async fn requeue(&self, id: Uuid) -> Result<Option<Uuid>, Self::Error> {
            let mut entries = self.entries.lock().unwrap();
            if let Some(entry) = entries.get(&id) {
                let event_id = entry.original_event_id;
                entries.remove(&id);
                Ok(Some(event_id))
            } else {
                Err(format!("DLQ entry {} not found", id))
            }
        }

        async fn get_stats(&self) -> Result<DlqStats, Self::Error> {
            let entries = self.entries.lock().unwrap();
            let pending: Vec<&DlqEntry> = entries.values().filter(|e| e.is_pending()).collect();
            let oldest = pending
                .iter()
                .min_by_key(|e| e.moved_at)
                .map(|e| e.moved_at.timestamp());

            Ok(DlqStats {
                pending_count: pending.len() as u64,
                resolved_count: (entries.len() - pending.len()) as u64,
                oldest_pending_age_seconds: oldest,
            })
        }

        async fn delete(&self, id: Uuid) -> Result<(), Self::Error> {
            let mut entries = self.entries.lock().unwrap();
            entries.remove(&id);
            Ok(())
        }

        async fn cleanup_resolved(
            &self,
            _older_than: std::time::Duration,
        ) -> Result<u64, Self::Error> {
            let mut entries = self.entries.lock().unwrap();
            let before_count = entries.len();
            entries.retain(|_, e| e.is_pending());
            Ok((before_count - entries.len()) as u64)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use chrono::Utc;

        #[tokio::test]
        async fn test_insert_and_get() {
            let repo = InMemoryDlqRepository::new();

            let entry = DlqEntry {
                id: Uuid::new_v4(),
                original_event_id: Uuid::new_v4(),
                aggregate_id: Uuid::new_v4(),
                aggregate_type: "JOB".to_string(),
                event_type: "TestEvent".to_string(),
                payload: serde_json::json!({"test": "data"}),
                metadata: None,
                error_message: "Test error".to_string(),
                retry_count: 3,
                original_created_at: Utc::now(),
                moved_at: Utc::now(),
                resolved_at: None,
                resolution_notes: None,
                resolved_by: None,
            };

            repo.insert(&entry).await.unwrap();

            let retrieved = repo.get_by_id(entry.id).await.unwrap();
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap().event_type, "TestEvent");
        }

        #[tokio::test]
        async fn test_resolve() {
            let entry = DlqEntry {
                id: Uuid::new_v4(),
                original_event_id: Uuid::new_v4(),
                aggregate_id: Uuid::new_v4(),
                aggregate_type: "WORKER".to_string(),
                event_type: "WorkerFailed".to_string(),
                payload: serde_json::json!({}),
                metadata: None,
                error_message: "Connection lost".to_string(),
                retry_count: 5,
                original_created_at: Utc::now(),
                moved_at: Utc::now(),
                resolved_at: None,
                resolution_notes: None,
                resolved_by: None,
            };

            let repo = InMemoryDlqRepository::with_entries(vec![entry.clone()]);

            repo.resolve(entry.id, "Fixed upstream", "admin@test.com")
                .await
                .unwrap();

            let retrieved = repo.get_by_id(entry.id).await.unwrap().unwrap();
            assert!(!retrieved.is_pending());
            assert_eq!(
                retrieved.resolution_notes,
                Some("Fixed upstream".to_string())
            );
        }

        #[tokio::test]
        async fn test_get_pending() {
            let pending = DlqEntry {
                id: Uuid::new_v4(),
                original_event_id: Uuid::new_v4(),
                aggregate_id: Uuid::new_v4(),
                aggregate_type: "JOB".to_string(),
                event_type: "PendingEvent".to_string(),
                payload: serde_json::json!({}),
                metadata: None,
                error_message: "Error".to_string(),
                retry_count: 1,
                original_created_at: Utc::now(),
                moved_at: Utc::now(),
                resolved_at: None,
                resolution_notes: None,
                resolved_by: None,
            };

            let resolved = DlqEntry {
                id: Uuid::new_v4(),
                original_event_id: Uuid::new_v4(),
                aggregate_id: Uuid::new_v4(),
                aggregate_type: "JOB".to_string(),
                event_type: "ResolvedEvent".to_string(),
                payload: serde_json::json!({}),
                metadata: None,
                error_message: "Error".to_string(),
                retry_count: 1,
                original_created_at: Utc::now(),
                moved_at: Utc::now(),
                resolved_at: Some(Utc::now()),
                resolution_notes: Some("Done".to_string()),
                resolved_by: Some("admin".to_string()),
            };

            let repo = InMemoryDlqRepository::with_entries(vec![pending, resolved]);

            let pending_entries = repo.get_pending(10, 0).await.unwrap();
            assert_eq!(pending_entries.len(), 1);
            assert_eq!(pending_entries[0].event_type, "PendingEvent");
        }

        #[tokio::test]
        async fn test_requeue() {
            let entry = DlqEntry {
                id: Uuid::new_v4(),
                original_event_id: Uuid::new_v4(),
                aggregate_id: Uuid::new_v4(),
                aggregate_type: "PROVIDER".to_string(),
                event_type: "ProviderEvent".to_string(),
                payload: serde_json::json!({"key": "value"}),
                metadata: None,
                error_message: "Temp error".to_string(),
                retry_count: 2,
                original_created_at: Utc::now(),
                moved_at: Utc::now(),
                resolved_at: None,
                resolution_notes: None,
                resolved_by: None,
            };

            let repo = InMemoryDlqRepository::with_entries(vec![entry.clone()]);

            let event_id = repo.requeue(entry.id).await.unwrap().unwrap();
            assert_eq!(event_id, entry.original_event_id);

            // Entry should be removed
            let retrieved = repo.get_by_id(entry.id).await.unwrap();
            assert!(retrieved.is_none());
        }
    }
}

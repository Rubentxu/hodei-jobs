//! # SQLite EventStore Implementation
//!
//! This module provides [`SqliteEventStore`] for local event persistence
//! using SQLite as the backend.

use async_trait::async_trait;
use chrono::Utc;
use saga_engine_core::event::{HistoryEvent, SagaId};
use saga_engine_core::port::event_store::{EventStore, EventStoreError};
use sqlx::SqlitePool;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

/// Configuration for [`SqliteEventStore`].
#[derive(Debug, Clone)]
pub struct SqliteEventStoreConfig {
    /// Create a snapshot every N events.
    pub snapshot_every: u64,
    /// Enable foreign key constraints.
    pub foreign_keys: bool,
    /// Busy timeout in milliseconds.
    pub busy_timeout_ms: u32,
}

impl Default for SqliteEventStoreConfig {
    fn default() -> Self {
        Self {
            snapshot_every: 100,
            foreign_keys: true,
            busy_timeout_ms: 5000,
        }
    }
}

/// SQLite EventStore implementation.
///
/// This implementation provides:
/// - ACID transactions for event appends
/// - Optimistic locking for concurrent access
/// - Automatic snapshot creation
/// - Zero-config file creation or in-memory mode
///
/// # Examples
///
/// ## File-based storage
///
/// ```ignore
/// use saga_engine_sqlite::SqliteEventStore;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let event_store = SqliteEventStore::new("/tmp/my-app.db").await?;
///     Ok(())
/// }
/// ```
///
/// ## In-memory storage (for testing)
///
/// ```rust
/// use saga_engine_sqlite::SqliteEventStore;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let event_store = SqliteEventStore::in_memory().await?;
///     Ok(())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct SqliteEventStore {
    pool: Arc<SqlitePool>,
    config: SqliteEventStoreConfig,
}

impl SqliteEventStore {
    /// Create a new SQLite EventStore with file-based persistence.
    ///
    /// Creates the database file if it doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the SQLite database file.
    ///
    /// # Errors
    ///
    /// Returns [`SqliteEventStoreError::Backend`] if the database cannot be opened.
    pub async fn new(path: impl AsRef<Path>) -> Result<Self, SqliteEventStoreError> {
        let pool = SqlitePool::connect(&format!("sqlite:{}", path.as_ref().display()))
            .await
            .map_err(SqliteEventStoreError::Backend)?;

        Self::init_pool(pool, &SqliteEventStoreConfig::default()).await
    }

    /// Create a new in-memory SQLite EventStore.
    ///
    /// Useful for testing. The database is destroyed when the connection is dropped.
    ///
    /// # Errors
    ///
    /// Returns [`SqliteEventStoreError::Backend`] if the in-memory database cannot be created.
    pub async fn in_memory() -> Result<Self, SqliteEventStoreError> {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .map_err(SqliteEventStoreError::Backend)?;

        Self::init_pool(pool, &SqliteEventStoreConfig::default()).await
    }

    /// Create a new builder.
    pub fn builder() -> SqliteEventStoreBuilder {
        SqliteEventStoreBuilder::new()
    }

    async fn init_pool(
        pool: SqlitePool,
        config: &SqliteEventStoreConfig,
    ) -> Result<Self, SqliteEventStoreError> {
        // Enable foreign keys if configured
        if config.foreign_keys {
            sqlx::query("PRAGMA foreign_keys = ON")
                .execute(&pool)
                .await
                .map_err(|e| SqliteEventStoreError::Backend(e))?;
        }

        // Set busy timeout
        sqlx::query(&format!(
            "PRAGMA busy_timeout = {}",
            config.busy_timeout_ms
        ))
        .execute(&pool)
        .await
        .map_err(|e| SqliteEventStoreError::Backend(e))?;

        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(SqliteEventStoreError::Migration)?;

        Ok(Self {
            pool: Arc::new(pool),
            config: config.clone(),
        })
    }
}

#[async_trait]
impl EventStore for SqliteEventStore {
    type Error = SqliteEventStoreError;

    async fn append_event(
        &self,
        saga_id: &SagaId,
        expected_next_event_id: u64,
        event: &HistoryEvent,
    ) -> Result<u64, EventStoreError<Self::Error>> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(SqliteEventStoreError::Backend)?;

        // Verify optimistic lock
        let current_id: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(MAX(event_id), 0) FROM saga_events
            WHERE saga_id = ?
            "#,
        )
        .bind(&saga_id.0)
        .fetch_one(&mut *tx)
        .await
        .map_err(SqliteEventStoreError::Backend)?;

        let current_id = current_id as u64;

        if current_id != expected_next_event_id {
            return Err(EventStoreError::conflict(expected_next_event_id, current_id));
        }

        // Serialize event data
        let event_data = serde_json::to_vec(event)
            .map_err(SqliteEventStoreError::Serialization)?;

        // Insert event
        let event_id = expected_next_event_id + 1;
        let event_data_for_snapshot = event_data.clone();
        sqlx::query(
            r#"
            INSERT INTO saga_events
            (saga_id, event_id, event_type, event_category, event_data, event_version,
             is_reset_point, is_retry, parent_event_id, task_queue, trace_id, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&saga_id.0)
        .bind(event_id as i64)
        .bind(event.event_type.as_str())
        .bind(event.category.as_str())
        .bind(event_data)
        .bind(event.event_version as i64)
        .bind(event.is_reset_point as i64)
        .bind(event.is_retry as i64)
        .bind(event.parent_event_id.map(|e| e.0 as i64))
        .bind(event.task_queue.as_deref())
        .bind(event.trace_id.as_deref())
        .bind(Utc::now().timestamp())
        .execute(&mut *tx)
        .await
        .map_err(SqliteEventStoreError::Backend)?;

        tx.commit()
            .await
            .map_err(SqliteEventStoreError::Backend)?;

        // Create snapshot if needed
        if event_id % self.config.snapshot_every == 0 {
            self.save_snapshot(saga_id, event_id, &event_data_for_snapshot)
                .await
                .map_err(|e| EventStoreError::Backend(e))?;
        }

        Ok(event_id)
    }

    async fn get_history(
        &self,
        saga_id: &SagaId,
    ) -> Result<Vec<HistoryEvent>, Self::Error> {
        let events: Vec<HistoryEvent> = sqlx::query_as::<_, SqliteEventRow>(
            r#"
            SELECT event_data FROM saga_events
            WHERE saga_id = ?
            ORDER BY event_id ASC
            "#,
        )
        .bind(&saga_id.0)
        .fetch_all(&*self.pool)
        .await
        .map_err(SqliteEventStoreError::Backend)?
        .into_iter()
        .map(|row| {
            serde_json::from_slice(&row.event_data)
                .map_err(SqliteEventStoreError::Serialization)
        })
        .collect::<Result<_, _>>()?;

        if events.is_empty() {
            return Err(SqliteEventStoreError::NotFound {
                saga_id: saga_id.clone(),
            });
        }

        Ok(events)
    }

    async fn get_history_from(
        &self,
        saga_id: &SagaId,
        from_event_id: u64,
    ) -> Result<Vec<HistoryEvent>, Self::Error> {
        let events: Vec<HistoryEvent> = sqlx::query_as::<_, SqliteEventRow>(
            r#"
            SELECT event_data FROM saga_events
            WHERE saga_id = ? AND event_id > ?
            ORDER BY event_id ASC
            "#,
        )
        .bind(&saga_id.0)
        .bind(from_event_id as i64)
        .fetch_all(&*self.pool)
        .await
        .map_err(SqliteEventStoreError::Backend)?
        .into_iter()
        .map(|row| {
            serde_json::from_slice(&row.event_data)
                .map_err(SqliteEventStoreError::Serialization)
        })
        .collect::<Result<_, _>>()?;

        Ok(events)
    }

    async fn save_snapshot(
        &self,
        saga_id: &SagaId,
        event_id: u64,
        state: &[u8],
    ) -> Result<(), Self::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO saga_snapshots
            (saga_id, event_id, snapshot_data, created_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(&saga_id.0)
        .bind(event_id as i64)
        .bind(state)
        .bind(Utc::now().timestamp())
        .execute(&*self.pool)
        .await
        .map_err(SqliteEventStoreError::Backend)?;

        Ok(())
    }

    async fn get_latest_snapshot(
        &self,
        saga_id: &SagaId,
    ) -> Result<Option<(u64, Vec<u8>)>, Self::Error> {
        let row = sqlx::query_as::<_, SqliteSnapshotRow>(
            r#"
            SELECT event_id, snapshot_data FROM saga_snapshots
            WHERE saga_id = ?
            "#,
        )
        .bind(&saga_id.0)
        .fetch_optional(&*self.pool)
        .await
        .map_err(SqliteEventStoreError::Backend)?;

        Ok(row.map(|r| (r.event_id as u64, r.snapshot_data)))
    }

    async fn get_current_event_id(&self, saga_id: &SagaId) -> Result<u64, Self::Error> {
        let id: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT COALESCE(MAX(event_id), 0) FROM saga_events
            WHERE saga_id = ?
            "#,
        )
        .bind(&saga_id.0)
        .fetch_one(&*self.pool)
        .await
        .map_err(SqliteEventStoreError::Backend)?;

        Ok(id.unwrap_or(0) as u64)
    }

    async fn saga_exists(&self, saga_id: &SagaId) -> Result<bool, Self::Error> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM saga_events
            WHERE saga_id = ?
            "#,
        )
        .bind(&saga_id.0)
        .fetch_one(&*self.pool)
        .await
        .map_err(SqliteEventStoreError::Backend)?;

        Ok(count > 0)
    }

    async fn snapshot_count(&self, saga_id: &SagaId) -> Result<u64, Self::Error> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM saga_snapshots
            WHERE saga_id = ?
            "#,
        )
        .bind(&saga_id.0)
        .fetch_one(&*self.pool)
        .await
        .map_err(SqliteEventStoreError::Backend)?;

        Ok(count as u64)
    }
}

/// Internal row type for event deserialization.
#[derive(sqlx::FromRow)]
struct SqliteEventRow {
    event_data: Vec<u8>,
}

/// Internal row type for snapshot deserialization.
#[derive(sqlx::FromRow)]
struct SqliteSnapshotRow {
    event_id: i64,
    snapshot_data: Vec<u8>,
}

/// Builder for [`SqliteEventStore`].
#[derive(Debug, Default)]
pub struct SqliteEventStoreBuilder {
    config: SqliteEventStoreConfig,
    storage: StorageType,
}

#[derive(Debug)]
enum StorageType {
    File(std::path::PathBuf),
    Memory,
}

impl Default for StorageType {
    fn default() -> Self {
        StorageType::Memory
    }
}

impl SqliteEventStoreBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            config: SqliteEventStoreConfig::default(),
            storage: StorageType::Memory,
        }
    }

    /// Set snapshot frequency.
    pub fn snapshot_every(mut self, events: u64) -> Self {
        self.config.snapshot_every = events;
        self
    }

    /// Enable or disable foreign keys.
    pub fn foreign_keys(mut self, enabled: bool) -> Self {
        self.config.foreign_keys = enabled;
        self
    }

    /// Set busy timeout in milliseconds.
    pub fn busy_timeout(mut self, ms: u32) -> Self {
        self.config.busy_timeout_ms = ms;
        self
    }

    /// Set file-based storage path.
    pub fn path(mut self, path: impl AsRef<Path>) -> Self {
        self.storage = StorageType::File(path.as_ref().to_path_buf());
        self
    }

    /// Set in-memory storage.
    pub fn in_memory(mut self) -> Self {
        self.storage = StorageType::Memory;
        self
    }

    /// Build the [`SqliteEventStore`].
    pub async fn build(self) -> Result<SqliteEventStore, SqliteEventStoreError> {
        match self.storage {
            StorageType::File(path) => {
                let pool = SqlitePool::connect(&format!("sqlite:{}", path.display()))
                    .await
                    .map_err(SqliteEventStoreError::Backend)?;
                SqliteEventStore::init_pool(pool, &self.config).await
            }
            StorageType::Memory => {
                let pool = SqlitePool::connect("sqlite::memory:")
                    .await
                    .map_err(SqliteEventStoreError::Backend)?;
                SqliteEventStore::init_pool(pool, &self.config).await
            }
        }
    }
}

/// Errors from [`SqliteEventStore`] operations.
#[derive(Debug, Error)]
pub enum SqliteEventStoreError {
    /// Backend-specific error.
    #[error("Backend error: {0}")]
    Backend(#[from] sqlx::Error),

    /// Failed to run migrations.
    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Saga not found.
    #[error("Saga not found: {saga_id}")]
    NotFound { saga_id: SagaId },

    /// Missing path for file-based storage.
    #[error("Path is required for file-based storage")]
    MissingPath,

    /// Optimistic lock error.
    #[error("Optimistic lock error: expected {expected}, got {actual}")]
    OptimisticLock { expected: u64, actual: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use saga_engine_core::event::{EventCategory, EventType, HistoryEvent};
    use serde_json::json;

    fn make_test_event(saga_id: &SagaId, event_num: u64) -> HistoryEvent {
        HistoryEvent::builder()
            .event_id(saga_engine_core::event::EventId(event_num))
            .saga_id(saga_id.clone())
            .event_type(EventType::WorkflowExecutionStarted)
            .category(EventCategory::Workflow)
            .payload(json!({"event_num": event_num}))
            .build()
    }

    #[tokio::test]
    async fn test_in_memory_event_store() {
        let store = SqliteEventStore::in_memory().await.unwrap();

        let saga_id = SagaId::new();
        let event = make_test_event(&saga_id, 1);

        let event_id = store.append_event(&saga_id, 0, &event).await.unwrap();
        assert_eq!(event_id, 1);

        let history = store.get_history(&saga_id).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].event_id.0, 1);
    }

    #[tokio::test]
    async fn test_optimistic_lock() {
        let store = SqliteEventStore::in_memory().await.unwrap();

        let saga_id = SagaId::new();
        let event1 = make_test_event(&saga_id, 1);
        let event2 = make_test_event(&saga_id, 2);

        // First append succeeds
        store.append_event(&saga_id, 0, &event1).await.unwrap();

        // Second append with wrong expected ID fails
        let result = store.append_event(&saga_id, 0, &event2).await;
        assert!(result.is_err());

        // Correct expected ID succeeds
        let event_id = store.append_event(&saga_id, 1, &event2).await.unwrap();
        assert_eq!(event_id, 2);
    }

    #[tokio::test]
    async fn test_snapshots() {
        let store = SqliteEventStore::builder()
            .in_memory()
            .snapshot_every(2)
            .build()
            .await
            .unwrap();

        let saga_id = SagaId::new();

        // Add events - snapshot should be created at event 2
        for i in 1..=3 {
            let event = make_test_event(&saga_id, i);
            store.append_event(&saga_id, i - 1, &event).await.unwrap();
        }

        let snapshot = store.get_latest_snapshot(&saga_id).await.unwrap();
        assert!(snapshot.is_some());
        let (event_id, _) = snapshot.unwrap();
        assert_eq!(event_id, 2);
    }

    #[tokio::test]
    async fn test_saga_not_found() {
        let store = SqliteEventStore::in_memory().await.unwrap();
        let saga_id = SagaId::new();

        let result = store.get_history(&saga_id).await;
        assert!(result.is_err());
    }
}

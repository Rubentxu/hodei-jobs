//! # Local Saga Engine Builder
//!
//! This module provides the [`LocalSagaEngine`] builder for creating
//! local saga engine instances with SQLite persistence.

pub mod task_queue;
pub mod timer_store;
mod errors;

#[cfg(feature = "sqlite")]
pub use self::builder::LocalSagaEngine;

    #[cfg(feature = "sqlite")]
    mod builder {
        use super::task_queue::{TokioTaskQueue, TokioTaskQueueConfig};
        use super::timer_store::InMemoryTimerStore;
        use num_cpus::get;
        use saga_engine_core::SagaEngine;
        use saga_engine_sqlite::{SqliteEventStore, SqliteEventStoreError, SqliteTimerStoreError};
        use std::path::Path;
        use std::sync::Arc;
        use thiserror::Error;

    /// Builder for local saga engine with presets for different use cases.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use saga_engine_local::LocalSagaEngine;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // CLI preset
    ///     let engine = LocalSagaEngine::cli("./my-tool.db").await?;
    ///     Ok(())
    /// }
    /// ```
    #[derive(Debug)]
    pub struct LocalSagaEngine;

    impl LocalSagaEngine {
        /// Preset for CLI tools.
        ///
        /// - SQLite for persistence
        /// - 2 workers
        /// - Snapshots every 100 events
        pub async fn cli(db_path: impl AsRef<Path>) -> Result<SagaEngine<SqliteEventStore, TokioTaskQueue, InMemoryTimerStore>, LocalSagaEngineError> {
            Self::builder()
                .with_sqlite(db_path)
                .with_workers(2)
                .with_snapshot_every(100)
                .build()
                .await
        }

        /// Preset for desktop applications.
        ///
        /// - SQLite for persistence
        /// - Workers = CPU cores
        /// - Snapshots every 50 events
        pub async fn desktop(db_path: impl AsRef<Path>) -> Result<SagaEngine<SqliteEventStore, TokioTaskQueue, InMemoryTimerStore>, LocalSagaEngineError> {
            Self::builder()
                .with_sqlite(db_path)
                .with_workers(get())
                .with_snapshot_every(50)
                .build()
                .await
        }

        /// Preset for testing.
        ///
        /// - In-memory SQLite
        /// - 1 worker
        /// - No snapshots
        pub async fn testing() -> Result<SagaEngine<SqliteEventStore, TokioTaskQueue, InMemoryTimerStore>, LocalSagaEngineError> {
            Self::builder()
                .with_in_memory_sqlite()
                .with_workers(1)
                .with_snapshot_every(u64::MAX)
                .build()
                .await
        }

        /// Preset for embedded systems.
        ///
        /// - SQLite for persistence
        /// - 1 worker (low resource usage)
        /// - Snapshots every 200 events
        pub async fn embedded(db_path: impl AsRef<Path>) -> Result<SagaEngine<SqliteEventStore, TokioTaskQueue, InMemoryTimerStore>, LocalSagaEngineError> {
            Self::builder()
                .with_sqlite(db_path)
                .with_workers(1)
                .with_snapshot_every(200)
                .build()
                .await
        }

        /// Create a builder for custom configuration.
        pub fn builder() -> LocalSagaEngineBuilder {
            LocalSagaEngineBuilder::new()
        }
    }

    /// Builder for custom local saga engine configuration.
    #[derive(Debug, Default)]
    pub struct LocalSagaEngineBuilder {
        event_store: Option<EventStoreConfig>,
        task_queue: Option<TokioTaskQueueConfig>,
        workers: usize,
        snapshot_every: u64,
    }

    #[derive(Debug)]
    enum EventStoreConfig {
        Sqlite(std::path::PathBuf),
        InMemory,
    }

    impl LocalSagaEngineBuilder {
        /// Create a new builder.
        pub fn new() -> Self {
            Self {
                event_store: None,
                task_queue: None,
                workers: get(),
                snapshot_every: 100,
            }
        }

        /// Use SQLite for event storage.
        pub fn with_sqlite(mut self, path: impl AsRef<Path>) -> Self {
            self.event_store = Some(EventStoreConfig::Sqlite(path.as_ref().to_path_buf()));
            self
        }

        /// Use in-memory SQLite (for testing).
        pub fn with_in_memory_sqlite(mut self) -> Self {
            self.event_store = Some(EventStoreConfig::InMemory);
            self
        }

        /// Configure the task queue.
        pub fn with_task_queue(mut self, config: TokioTaskQueueConfig) -> Self {
            self.task_queue = Some(config);
            self
        }

        /// Set the number of workers.
        pub fn with_workers(mut self, workers: usize) -> Self {
            self.workers = workers;
            self
        }

        /// Set snapshot frequency.
        pub fn with_snapshot_every(mut self, events: u64) -> Self {
            self.snapshot_every = events;
            self
        }

        /// Build the saga engine.
        pub async fn build(self) -> Result<SagaEngine<SqliteEventStore, TokioTaskQueue, InMemoryTimerStore>, LocalSagaEngineError> {
            let event_store = match self.event_store.unwrap_or(EventStoreConfig::InMemory) {
                EventStoreConfig::Sqlite(path) => {
                    SqliteEventStore::builder()
                        .path(path)
                        .snapshot_every(self.snapshot_every)
                        .build()
                        .await?
                }
                EventStoreConfig::InMemory => {
                    SqliteEventStore::builder()
                        .in_memory()
                        .snapshot_every(self.snapshot_every)
                        .build()
                        .await?
                }
            };

            let task_queue = if let Some(config) = self.task_queue {
                TokioTaskQueue::with_config(config)
            } else {
                TokioTaskQueue::with_workers(self.workers)
            };

            let timer_store = InMemoryTimerStore::new();

            Ok(SagaEngine::new(
                saga_engine_core::SagaEngineConfig::default(),
                Arc::new(event_store),
                Arc::new(task_queue),
                Arc::new(timer_store),
            ))
        }
    }

    /// Errors from local saga engine building.
    #[derive(Debug, Error)]
    pub enum LocalSagaEngineError {
        #[error("Event store error: {0}")]
        EventStore(#[from] SqliteEventStoreError),

        #[error("Timer store error: {0}")]
        TimerStore(#[from] SqliteTimerStoreError),
    }
}

//! # Saga Engine Configuration
//!
//! Configuration types for Saga Engine including reactive mode settings.
//! The Saga Engine uses PostgreSQL LISTEN/NOTIFY by default for reactive
//! event processing without polling.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Reactive mode configuration for the Saga Engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactiveMode {
    /// Fully reactive mode using PostgreSQL LISTEN/NOTIFY
    Enabled,
    /// Legacy polling mode (deprecated, will be removed)
    #[deprecated(since = "0.87.0", note = "Use ReactiveMode::Enabled instead")]
    Polling,
}

impl Default for ReactiveMode {
    fn default() -> Self {
        ReactiveMode::Enabled
    }
}

impl ReactiveMode {
    /// Check if reactive mode is enabled
    pub fn is_enabled(&self) -> bool {
        matches!(self, ReactiveMode::Enabled)
    }

    /// Check if polling mode is enabled (for deprecation warnings)
    #[allow(deprecated)]
    pub fn is_polling(&self) -> bool {
        matches!(self, ReactiveMode::Polling)
    }
}

/// Configuration for the SagaEngine.
#[derive(Debug, Clone)]
pub struct SagaEngineConfig {
    /// Reactive mode: use LISTEN/NOTIFY instead of polling
    pub reactive_mode: ReactiveMode,
    /// Maximum events before forcing a snapshot.
    pub max_events_before_snapshot: u64,
    /// Default activity timeout.
    pub default_activity_timeout: Duration,
    /// Task queue for workflow tasks.
    pub workflow_task_queue: String,
    /// Worker ID for sharding (in reactive mode)
    pub worker_id: u64,
    /// Total number of workers (for sharding)
    pub total_shards: u64,
    /// Enable automatic compensation on workflow failure
    pub auto_compensation: bool,
}

impl Default for SagaEngineConfig {
    fn default() -> Self {
        Self {
            reactive_mode: ReactiveMode::Enabled,
            max_events_before_snapshot: 100,
            default_activity_timeout: Duration::from_secs(300),
            workflow_task_queue: "saga-workflows".to_string(),
            worker_id: 0,
            total_shards: 1,
            auto_compensation: true,
        }
    }
}

impl SagaEngineConfig {
    /// Create a new configuration with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set reactive mode
    pub fn with_reactive_mode(mut self, mode: ReactiveMode) -> Self {
        self.reactive_mode = mode;
        self
    }

    /// Enable reactive mode
    pub fn with_reactive(mut self) -> Self {
        self.reactive_mode = ReactiveMode::Enabled;
        self
    }

    /// Set maximum events before snapshot
    pub fn with_max_events_before_snapshot(mut self, n: u64) -> Self {
        self.max_events_before_snapshot = n;
        self
    }

    /// Set activity timeout
    pub fn with_activity_timeout(mut self, timeout: Duration) -> Self {
        self.default_activity_timeout = timeout;
        self
    }

    /// Set worker ID for sharding
    pub fn with_worker_id(mut self, worker_id: u64) -> Self {
        self.worker_id = worker_id;
        self
    }

    /// Set total shards for distribution
    pub fn with_total_shards(mut self, shards: u64) -> Self {
        self.total_shards = shards;
        self
    }

    /// Enable/disable automatic compensation
    pub fn with_auto_compensation(mut self, enabled: bool) -> Self {
        self.auto_compensation = enabled;
        self
    }

    /// Set workflow task queue
    pub fn with_workflow_task_queue(mut self, queue: impl Into<String>) -> Self {
        self.workflow_task_queue = queue.into();
        self
    }
}

/// Worker configuration for reactive workers
#[derive(Debug, Clone)]
pub struct ReactiveWorkerConfig {
    /// Worker ID for sharding
    pub worker_id: u64,
    /// Total number of workers (for sharding)
    pub total_shards: u64,
    /// Consumer name for task queue
    pub consumer_name: String,
    /// Queue group for load balancing
    pub queue_group: String,
    /// Maximum concurrent tasks
    pub max_concurrent: u64,
}

impl Default for ReactiveWorkerConfig {
    fn default() -> Self {
        Self {
            worker_id: 0,
            total_shards: 1,
            consumer_name: "saga-reactive-worker".to_string(),
            queue_group: "saga-workers".to_string(),
            max_concurrent: 10,
        }
    }
}

impl ReactiveWorkerConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_worker_id(mut self, worker_id: u64) -> Self {
        self.worker_id = worker_id;
        self
    }

    pub fn with_total_shards(mut self, shards: u64) -> Self {
        self.total_shards = shards;
        self
    }

    pub fn with_max_concurrent(mut self, n: u64) -> Self {
        self.max_concurrent = n;
        self
    }
}

/// Timer scheduler configuration for reactive mode
#[derive(Debug, Clone)]
pub struct ReactiveTimerSchedulerConfig {
    /// Worker ID for sharding
    pub worker_id: u64,
    /// Total number of workers (for sharding)
    pub total_shards: u64,
    /// Maximum timers to process per batch
    pub max_batch_size: u64,
}

impl Default for ReactiveTimerSchedulerConfig {
    fn default() -> Self {
        Self {
            worker_id: 0,
            total_shards: 1,
            max_batch_size: 100,
        }
    }
}

impl ReactiveTimerSchedulerConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_worker_id(mut self, worker_id: u64) -> Self {
        self.worker_id = worker_id;
        self
    }

    pub fn with_total_shards(mut self, shards: u64) -> Self {
        self.total_shards = shards;
        self
    }
}

/// Configuration for PostgreSQL connections
#[derive(Debug, Clone)]
pub struct PostgresConfig {
    /// Database connection URL
    pub database_url: String,
    /// Maximum connections in pool
    pub max_connections: u32,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Enable LISTEN/NOTIFY
    pub notify_enabled: bool,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            database_url: "postgresql://localhost/saga".to_string(),
            max_connections: 10,
            connection_timeout: Duration::from_secs(30),
            notify_enabled: true,
        }
    }
}

impl PostgresConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_database_url(mut self, url: impl Into<String>) -> Self {
        self.database_url = url.into();
        self
    }

    pub fn with_max_connections(mut self, n: u32) -> Self {
        self.max_connections = n;
        self
    }
}

/// Environment-based configuration loader
#[derive(Debug, Clone)]
pub struct EnvConfig;

impl EnvConfig {
    /// Load SagaEngineConfig from environment variables
    pub fn load_saga_engine_config() -> SagaEngineConfig {
        SagaEngineConfig::new()
            .with_worker_id(
                std::env::var("SAGA_WORKER_ID")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
            )
            .with_total_shards(
                std::env::var("SAGA_TOTAL_SHARDS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1),
            )
            .with_workflow_task_queue(
                std::env::var("SAGA_WORKFLOW_QUEUE")
                    .unwrap_or_else(|_| "saga-workflows".to_string()),
            )
            .with_auto_compensation(
                std::env::var("SAGA_AUTO_COMPENSATION")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(true),
            )
    }

    /// Load PostgreSQL config from environment
    pub fn load_postgres_config() -> PostgresConfig {
        PostgresConfig::new()
            .with_database_url(
                std::env::var("DATABASE_URL")
                    .unwrap_or_else(|_| "postgresql://localhost/saga".to_string()),
            )
            .with_max_connections(
                std::env::var("SAGA_DB_MAX_CONNECTIONS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(10),
            )
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_reactive_mode_defaults() {
        let mode = ReactiveMode::default();
        assert!(mode.is_enabled());
    }

    #[test]
    #[allow(deprecated)]
    fn test_reactive_mode_polling() {
        let mode = ReactiveMode::Polling;
        assert!(!mode.is_enabled());
        assert!(mode.is_polling());
    }

    #[test]
    fn test_saga_engine_config_defaults() {
        let config = SagaEngineConfig::default();
        assert!(config.reactive_mode.is_enabled());
        assert_eq!(config.max_events_before_snapshot, 100);
        assert_eq!(config.default_activity_timeout, Duration::from_secs(300));
        assert_eq!(config.worker_id, 0);
        assert_eq!(config.total_shards, 1);
        assert!(config.auto_compensation);
    }

    #[test]
    fn test_saga_engine_config_builder() {
        let config = SagaEngineConfig::new()
            .with_worker_id(5)
            .with_total_shards(10)
            .with_max_events_before_snapshot(50)
            .with_auto_compensation(false);

        assert_eq!(config.worker_id, 5);
        assert_eq!(config.total_shards, 10);
        assert_eq!(config.max_events_before_snapshot, 50);
        assert!(!config.auto_compensation);
    }

    #[test]
    fn test_reactive_worker_config_defaults() {
        let config = ReactiveWorkerConfig::default();
        assert_eq!(config.worker_id, 0);
        assert_eq!(config.total_shards, 1);
        assert_eq!(config.max_concurrent, 10);
    }

    #[test]
    fn test_reactive_timer_config_defaults() {
        let config = ReactiveTimerSchedulerConfig::default();
        assert_eq!(config.worker_id, 0);
        assert_eq!(config.total_shards, 1);
        assert_eq!(config.max_batch_size, 100);
    }

    #[test]
    fn test_postgres_config_defaults() {
        let config = PostgresConfig::default();
        assert!(config.notify_enabled);
        assert_eq!(config.max_connections, 10);
    }

    #[test]
    fn test_env_config_load() {
        // Set environment variables
        unsafe {
            std::env::set_var("SAGA_WORKER_ID", "3");
            std::env::set_var("SAGA_TOTAL_SHARDS", "8");
            std::env::set_var("SAGA_AUTO_COMPENSATION", "false");
        }

        let config = EnvConfig::load_saga_engine_config();

        assert_eq!(config.worker_id, 3);
        assert_eq!(config.total_shards, 8);
        assert!(!config.auto_compensation);

        // Clean up
        unsafe {
            std::env::remove_var("SAGA_WORKER_ID");
            std::env::remove_var("SAGA_TOTAL_SHARDS");
            std::env::remove_var("SAGA_AUTO_COMPENSATION");
        }
    }
}

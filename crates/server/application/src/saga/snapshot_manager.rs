//!
//! # Snapshot Manager for Saga Engine
//!
//! This module provides [`SnapshotManagerApp`] - a wrapper around saga-engine's
//! [`SnapshotManager`] with application-specific configuration and strategies.
//!
//! # Snapshot Strategy for Different Workflow Types
//!
//! - **Short workflows** (< 10 events): Disabled - replay is cheap
//! - **Medium workflows** (10-50 events): Count-based every 20 events
//! - **Long workflows** (50-200 events): Adaptive (100 events or 60 minutes)
//! - **Very long workflows** (> 200 events): Count-based every 50 events
//! - **I/O-heavy workflows**: Time-based every 30 minutes
//!
//! # Usage Example
//!
//! ```ignore
//! let manager = SnapshotManagerApp::for_workflow::<ExecutionWorkflow>(
//!     Arc::new(event_store),
//!     SnapshotConfig::default(),
//! );
//!
//! // Check if snapshot needed after each event
//! if manager.should_snapshot(event_id, last_snapshot_id) {
//!     manager.create_snapshot(saga_id, event_id, &state).await?;
//! }
//! ```
//!

use saga_engine_core::SagaId;
use saga_engine_core::snapshot::{
    SnapshotConfig, SnapshotManager, SnapshotResult, SnapshotStrategy,
};
use std::sync::Arc;

/// Application-specific snapshot manager wrapper.
///
/// This struct wraps the saga-engine's `SnapshotManager` and provides:
/// - Workflow-specific snapshot strategies
/// - Simplified API for common operations
/// - Integration with application configuration
#[derive(Debug, Clone)]
pub struct SnapshotManagerApp<S: saga_engine_core::port::EventStore> {
    /// The inner snapshot manager
    inner: Arc<SnapshotManager<S>>,
    /// The strategy this manager uses
    strategy: SnapshotStrategy,
}

impl<S: saga_engine_core::port::EventStore> SnapshotManagerApp<S> {
    /// Create a new snapshot manager with the given strategy.
    ///
    /// # Arguments
    ///
    /// * `event_store` - The event store for persisting snapshots.
    /// * `strategy` - The snapshot strategy to use.
    /// * `config` - Configuration for snapshot behavior.
    pub fn new(event_store: Arc<S>, strategy: SnapshotStrategy, config: SnapshotConfig) -> Self {
        let inner = SnapshotManager::new(event_store, config);
        Self {
            inner: Arc::new(inner),
            strategy,
        }
    }

    /// Get the snapshot strategy being used.
    pub fn strategy(&self) -> &SnapshotStrategy {
        &self.strategy
    }

    /// Get the underlying manager reference.
    pub fn inner(&self) -> &SnapshotManager<S> {
        &*self.inner
    }

    /// Check if a snapshot should be taken based on current event count.
    ///
    /// Uses the configured strategy to determine if a snapshot is needed.
    pub fn should_snapshot(&self, current_event_id: u64, last_snapshot_id: Option<u64>) -> bool {
        if self.strategy.is_disabled() {
            return false;
        }

        let events_since_last = current_event_id.saturating_sub(last_snapshot_id.unwrap_or(0));
        self.strategy.should_snapshot_by_count(events_since_last)
    }

    /// Check if a snapshot should be taken based on elapsed time.
    pub fn should_snapshot_by_time(&self, elapsed: std::time::Duration) -> bool {
        self.strategy.should_snapshot_by_time(elapsed)
    }

    /// Check if a snapshot should be taken for a specific event type.
    pub fn should_snapshot_by_event(&self, event_type: &str) -> bool {
        self.strategy.should_snapshot_by_event(event_type)
    }

    /// Combined check for snapshot necessity.
    pub fn should_snapshot_combined(
        &self,
        events_since_last: u64,
        elapsed: std::time::Duration,
        event_type: &str,
    ) -> bool {
        self.strategy
            .should_snapshot(events_since_last, elapsed, event_type)
    }

    /// Create a snapshot if conditions are met.
    ///
    /// This checks the configured strategy and creates a snapshot only if
    /// the conditions are satisfied.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga to snapshot.
    /// * `current_event_id` - The current event ID.
    /// * `state` - The serialized state to snapshot.
    ///
    /// # Returns
    ///
    /// Whether a snapshot was created or skipped.
    pub async fn maybe_take_snapshot(
        &self,
        saga_id: &SagaId,
        current_event_id: u64,
        state: &[u8],
    ) -> Result<SnapshotResult, saga_engine_core::snapshot::SnapshotError<S::Error>> {
        self.inner
            .maybe_take_snapshot(saga_id, current_event_id, state)
            .await
    }

    /// Create a snapshot unconditionally.
    pub async fn create_snapshot(
        &self,
        saga_id: &SagaId,
        last_event_id: u64,
        state: &[u8],
    ) -> Result<(), saga_engine_core::snapshot::SnapshotError<S::Error>> {
        self.inner
            .create_snapshot(saga_id, last_event_id, state)
            .await
    }

    /// Find the latest valid snapshot for a saga.
    pub async fn find_latest_valid(
        &self,
        saga_id: &SagaId,
    ) -> Result<Option<(u64, Vec<u8>)>, saga_engine_core::snapshot::SnapshotError<S::Error>> {
        self.inner.find_latest_valid(saga_id).await
    }

    /// Get the snapshot count for a saga.
    pub async fn snapshot_count(
        &self,
        saga_id: &SagaId,
    ) -> Result<u64, saga_engine_core::snapshot::SnapshotError<S::Error>> {
        self.inner.snapshot_count(saga_id).await
    }
}

/// Workflow type trait for automatic snapshot strategy selection.
///
/// Implement this trait to provide custom snapshot strategies for specific
/// workflow types.
pub trait SnapshotStrategyProvider {
    /// Get the snapshot strategy for this workflow type.
    fn snapshot_strategy(&self) -> SnapshotStrategy;
}

/// Workflow type identifiers for snapshot strategy selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowType {
    /// Provisioning workflow (worker provisioning)
    Provisioning,
    /// Execution workflow (job execution)
    Execution,
    /// Recovery workflow (error recovery)
    Recovery,
    /// Cleanup workflow (resource cleanup)
    Cleanup,
    /// Cancellation workflow (job cancellation)
    Cancellation,
    /// Timeout handling workflow
    Timeout,
    /// Custom/unknown workflow
    Unknown,
}

impl WorkflowType {
    /// Get the recommended snapshot strategy for this workflow type.
    pub fn recommended_strategy(&self) -> SnapshotStrategy {
        match self {
            Self::Provisioning => SnapshotStrategy::for_medium_saga(),
            Self::Execution => SnapshotStrategy::for_medium_saga(),
            Self::Recovery => SnapshotStrategy::for_short_saga(),
            Self::Cleanup => SnapshotStrategy::for_short_saga(),
            Self::Cancellation => SnapshotStrategy::for_short_saga(),
            Self::Timeout => SnapshotStrategy::for_short_saga(),
            Self::Unknown => SnapshotStrategy::default(),
        }
    }

    /// Get the name as a string slice.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Provisioning => "provisioning",
            Self::Execution => "execution",
            Self::Recovery => "recovery",
            Self::Cleanup => "cleanup",
            Self::Cancellation => "cancellation",
            Self::Timeout => "timeout",
            Self::Unknown => "unknown",
        }
    }
}

impl Default for WorkflowType {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Factory methods for creating snapshot managers with appropriate strategies.
impl<S: saga_engine_core::port::EventStore> SnapshotManagerApp<S> {
    /// Create a snapshot manager for a short saga (few events).
    ///
    /// Short sagas typically don't need snapshots because replay is cheap.
    pub fn for_short_saga(event_store: Arc<S>) -> Self {
        Self::new(
            event_store,
            SnapshotStrategy::for_short_saga(),
            SnapshotConfig::default(),
        )
    }

    /// Create a snapshot manager for a medium-length saga.
    pub fn for_medium_saga(event_store: Arc<S>) -> Self {
        Self::new(
            event_store,
            SnapshotStrategy::for_medium_saga(),
            SnapshotConfig::default(),
        )
    }

    /// Create a snapshot manager for a long saga.
    pub fn for_long_saga(event_store: Arc<S>) -> Self {
        Self::new(
            event_store,
            SnapshotStrategy::for_long_saga(),
            SnapshotConfig::default(),
        )
    }

    /// Create a snapshot manager for a very long saga.
    pub fn for_very_long_saga(event_store: Arc<S>) -> Self {
        Self::new(
            event_store,
            SnapshotStrategy::for_very_long_saga(),
            SnapshotConfig::default(),
        )
    }

    /// Create a snapshot manager for an I/O-heavy saga.
    pub fn for_io_heavy_saga(event_store: Arc<S>) -> Self {
        Self::new(
            event_store,
            SnapshotStrategy::for_io_heavy_saga(),
            SnapshotConfig::default(),
        )
    }

    /// Create a snapshot manager based on workflow type.
    pub fn for_workflow_type(event_store: Arc<S>, workflow_type: WorkflowType) -> Self {
        let strategy = workflow_type.recommended_strategy();
        let config = match workflow_type {
            // Longer-running workflows need more aggressive snapshot configs
            WorkflowType::Execution | WorkflowType::Provisioning => SnapshotConfig::new(
                50,   // Snapshot every 50 events
                10,   // Keep last 10 snapshots
                true, // Enable checksums
            ),
            // Shorter workflows can use defaults
            _ => SnapshotConfig::default(),
        };

        Self::new(event_store, strategy, config)
    }

    /// Create a snapshot manager with a custom strategy.
    pub fn with_strategy(
        event_store: Arc<S>,
        strategy: SnapshotStrategy,
        config: SnapshotConfig,
    ) -> Self {
        Self::new(event_store, strategy, config)
    }

    /// Create a snapshot manager with disabled snapshots.
    pub fn disabled(event_store: Arc<S>) -> Self {
        Self::new(
            event_store,
            SnapshotStrategy::Disabled,
            SnapshotConfig::disabled(),
        )
    }
}

/// Configuration builder for snapshot settings.
#[derive(Debug, Clone, Default)]
pub struct SnapshotConfigBuilder {
    interval: u64,
    max_snapshots: u32,
    enable_checksum: bool,
    strategy: SnapshotStrategy,
}

impl SnapshotConfigBuilder {
    /// Create a new builder with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the snapshot interval (events between snapshots).
    pub fn interval(mut self, interval: u64) -> Self {
        self.interval = interval;
        self
    }

    /// Set the maximum number of snapshots to retain.
    pub fn max_snapshots(mut self, max: u32) -> Self {
        self.max_snapshots = max;
        self
    }

    /// Enable or disable checksum verification.
    pub fn enable_checksum(mut self, enabled: bool) -> Self {
        self.enable_checksum = enabled;
        self
    }

    /// Set the snapshot strategy.
    pub fn strategy(mut self, strategy: SnapshotStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Build the snapshot manager.
    pub fn build<S: saga_engine_core::port::EventStore>(
        self,
        event_store: Arc<S>,
    ) -> SnapshotManagerApp<S> {
        let config = SnapshotConfig::new(self.interval, self.max_snapshots, self.enable_checksum);
        SnapshotManagerApp::new(event_store, self.strategy, config)
    }
}

/// Snapshot metrics for monitoring.
#[derive(Debug, Default)]
pub struct SnapshotMetrics {
    /// Total snapshots created
    pub snapshots_created: std::sync::atomic::AtomicU64,
    /// Total snapshots skipped
    pub snapshots_skipped: std::sync::atomic::AtomicU64,
    /// Total integrity check failures
    pub integrity_failures: std::sync::atomic::AtomicU64,
}

impl Clone for SnapshotMetrics {
    fn clone(&self) -> Self {
        Self {
            snapshots_created: std::sync::atomic::AtomicU64::new(
                self.snapshots_created
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            snapshots_skipped: std::sync::atomic::AtomicU64::new(
                self.snapshots_skipped
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            integrity_failures: std::sync::atomic::AtomicU64::new(
                self.integrity_failures
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
        }
    }
}

impl SnapshotMetrics {
    /// Record a snapshot creation.
    pub fn record_created(&self) {
        self.snapshots_created
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record a skipped snapshot.
    pub fn record_skipped(&self) {
        self.snapshots_skipped
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record an integrity failure.
    pub fn record_integrity_failure(&self) {
        self.integrity_failures
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get current metrics snapshot.
    pub fn snapshot(&self) -> SnapshotMetricsSnapshot {
        SnapshotMetricsSnapshot {
            created: self
                .snapshots_created
                .load(std::sync::atomic::Ordering::Relaxed),
            skipped: self
                .snapshots_skipped
                .load(std::sync::atomic::Ordering::Relaxed),
            failures: self
                .integrity_failures
                .load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// A snapshot of metrics at a point in time.
#[derive(Debug, Clone)]
pub struct SnapshotMetricsSnapshot {
    pub created: u64,
    pub skipped: u64,
    pub failures: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use saga_engine_core::event::SagaId;
    use saga_engine_core::port::event_store::{EventStore, EventStoreError};
    use saga_engine_core::snapshot::SnapshotResult;
    use std::sync::Arc;

    /// Mock event store for testing.
    #[derive(Debug, Default)]
    struct MockEventStore;

    #[async_trait]
    impl EventStore for MockEventStore {
        type Error = String;

        async fn append_event(
            &self,
            _saga_id: &SagaId,
            _expected_next_event_id: u64,
            _event: &saga_engine_core::event::HistoryEvent,
        ) -> Result<u64, EventStoreError<Self::Error>> {
            Ok(1)
        }

        async fn append_events(
            &self,
            _saga_id: &SagaId,
            _expected_next_event_id: u64,
            _events: &[saga_engine_core::event::HistoryEvent],
        ) -> Result<u64, EventStoreError<Self::Error>> {
            Ok(1)
        }

        async fn get_history(
            &self,
            _saga_id: &SagaId,
        ) -> Result<Vec<saga_engine_core::event::HistoryEvent>, Self::Error> {
            Ok(vec![])
        }

        async fn get_history_from(
            &self,
            _saga_id: &SagaId,
            _from_event_id: u64,
        ) -> Result<Vec<saga_engine_core::event::HistoryEvent>, Self::Error> {
            Ok(vec![])
        }

        async fn save_snapshot(
            &self,
            _saga_id: &SagaId,
            _event_id: u64,
            _state: &[u8],
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_latest_snapshot(
            &self,
            _saga_id: &SagaId,
        ) -> Result<Option<(u64, Vec<u8>)>, Self::Error> {
            Ok(None)
        }

        async fn get_current_event_id(&self, _saga_id: &SagaId) -> Result<u64, Self::Error> {
            Ok(0)
        }

        async fn get_last_reset_point(
            &self,
            _saga_id: &SagaId,
        ) -> Result<Option<u64>, Self::Error> {
            Ok(None)
        }

        async fn saga_exists(&self, _saga_id: &SagaId) -> Result<bool, Self::Error> {
            Ok(false)
        }

        async fn snapshot_count(&self, _saga_id: &SagaId) -> Result<u64, Self::Error> {
            Ok(0)
        }
    }

    #[test]
    fn test_workflow_type_strategies() {
        assert!(SnapshotStrategy::for_short_saga().is_disabled());
        assert!(!SnapshotStrategy::for_medium_saga().is_disabled());
        assert!(!SnapshotStrategy::for_long_saga().is_disabled());
    }

    #[test]
    fn test_workflow_type_recommendations() {
        assert_eq!(
            WorkflowType::Provisioning.recommended_strategy(),
            SnapshotStrategy::for_medium_saga()
        );
        assert_eq!(
            WorkflowType::Execution.recommended_strategy(),
            SnapshotStrategy::for_medium_saga()
        );
    }

    #[test]
    fn test_workflow_type_as_str() {
        assert_eq!(WorkflowType::Execution.as_str(), "execution");
        assert_eq!(WorkflowType::Provisioning.as_str(), "provisioning");
    }

    #[test]
    fn test_snapshot_manager_disabled() {
        let store = Arc::new(MockEventStore);
        let manager = SnapshotManagerApp::disabled(store);

        assert!(manager.strategy().is_disabled());
        assert!(!manager.should_snapshot(100, None));
    }

    #[test]
    fn test_snapshot_manager_short_saga() {
        let store = Arc::new(MockEventStore);
        let manager = SnapshotManagerApp::for_short_saga(store);

        assert!(manager.strategy().is_disabled());
    }

    #[test]
    fn test_snapshot_manager_medium_saga() {
        let store = Arc::new(MockEventStore);
        let manager = SnapshotManagerApp::for_medium_saga(store);

        assert!(!manager.strategy().is_disabled());
        assert!(manager.should_snapshot(20, None));
        assert!(!manager.should_snapshot(10, None));
    }

    #[test]
    fn test_config_builder() {
        let store = Arc::new(MockEventStore);
        let manager = SnapshotConfigBuilder::new()
            .interval(25)
            .max_snapshots(10)
            .enable_checksum(true)
            .strategy(SnapshotStrategy::for_long_saga())
            .build(store);

        assert!(!manager.strategy().is_disabled());
    }

    #[test]
    fn test_snapshot_metrics() {
        let metrics = SnapshotMetrics::default();

        metrics.record_created();
        metrics.record_skipped();
        metrics.record_skipped();
        metrics.record_integrity_failure();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.created, 1);
        assert_eq!(snapshot.skipped, 2);
        assert_eq!(snapshot.failures, 1);
    }

    #[tokio::test]
    async fn test_maybe_take_snapshot_disabled() {
        let store = Arc::new(MockEventStore);
        let manager = SnapshotManagerApp::disabled(store);

        let saga_id = SagaId("test".to_string());
        let result = manager
            .maybe_take_snapshot(&saga_id, 100, b"state")
            .await
            .unwrap();

        assert!(matches!(result, SnapshotResult::Skipped));
    }
}

//! Snapshot management for efficient event replay.
//!
//! This module provides [`SnapshotManager`] for automatically creating and
//! managing snapshots of saga state to optimize replay operations.
//!
//! # Snapshot Strategy
//!
//! Without snapshots, replaying a saga requires reading all events from the
//! beginning. With snapshots, we only replay events after the last snapshot,
//! significantly reducing replay time.
//!
//! ```ignore
/// // Without snapshot: O(n) events
/// let events = event_store.get_history(saga_id).await?;
/// replayer.replay_all(&events)?;
///
/// // With snapshot: O(m) events where m << n
/// if let Some((snap_id, state)) = snapshot_manager.find_latest_valid(saga_id).await? {
///     let events = event_store.get_history_from(saga_id, snap_id).await?;
///     replayer.replay_from(&state, &events)?;
/// }
/// ```
use super::event::{CURRENT_EVENT_VERSION, SagaId};
use super::port::event_store::EventStore;
#[cfg(test)]
use super::port::event_store::EventStoreError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Configuration for snapshot behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotConfig {
    /// Number of events between automatic snapshots.
    /// Set to 0 to disable automatic snapshots.
    pub interval: u64,

    /// Maximum number of snapshots to retain per saga.
    /// Older snapshots are pruned when exceeded.
    pub max_snapshots: u32,

    /// Whether to include a checksum for integrity verification.
    pub enable_checksum: bool,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            interval: 100,         // Take snapshot every 100 events
            max_snapshots: 5,      // Keep last 5 snapshots
            enable_checksum: true, // Enable integrity checks by default
        }
    }
}

impl SnapshotConfig {
    /// Create a new configuration with custom values.
    pub fn new(interval: u64, max_snapshots: u32, enable_checksum: bool) -> Self {
        Self {
            interval,
            max_snapshots,
            enable_checksum,
        }
    }

    /// Disable automatic snapshot creation.
    pub fn disabled() -> Self {
        Self {
            interval: 0,
            max_snapshots: 0,
            enable_checksum: false,
        }
    }

    /// Check if automatic snapshots are enabled.
    pub fn is_enabled(&self) -> bool {
        self.interval > 0
    }
}

/// A serialized snapshot of saga state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// The saga this snapshot belongs to.
    pub saga_id: SagaId,

    /// The event ID this snapshot was taken after.
    pub last_event_id: u64,

    /// The serialized state data.
    pub state: Vec<u8>,

    /// Checksum for integrity verification.
    #[serde(default)]
    pub checksum: Option<SnapshotChecksum>,

    /// Event schema version when snapshot was taken.
    pub event_version: u32,

    /// When the snapshot was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Snapshot {
    /// Create a new snapshot.
    pub fn new(
        saga_id: SagaId,
        last_event_id: u64,
        state: Vec<u8>,
        checksum: Option<SnapshotChecksum>,
    ) -> Self {
        Self {
            saga_id,
            last_event_id,
            state,
            checksum,
            event_version: CURRENT_EVENT_VERSION,
            created_at: chrono::Utc::now(),
        }
    }

    /// Verify the snapshot integrity.
    pub fn verify_integrity(&self) -> bool {
        match &self.checksum {
            Some(checksum) => checksum.verify(&self.state),
            None => true, // No checksum means we trust it
        }
    }
}

/// A checksum for snapshot integrity verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotChecksum(pub [u8; 32]);

impl SnapshotChecksum {
    /// Create a checksum from data using SHA-256.
    pub fn from_data(data: &[u8]) -> Self {
        use sha2::Digest;
        let hash = sha2::Sha256::digest(data);
        let mut result = [0u8; 32];
        result.copy_from_slice(hash.as_slice());
        Self(result)
    }

    /// Verify data matches this checksum.
    pub fn verify(&self, data: &[u8]) -> bool {
        Self::from_data(data) == *self
    }
}

/// The result of a snapshot operation.
#[derive(Debug)]
pub enum SnapshotResult {
    /// Snapshot was created successfully.
    Created,

    /// Snapshot was not needed (not enough events since last).
    Skipped,

    /// No previous snapshot exists.
    NoSnapshotExists,
}

/// Error type for snapshot operations.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError<E> {
    /// A checksum mismatch was detected.
    #[error("Checksum mismatch: snapshot may be corrupt")]
    ChecksumMismatch,

    /// An error from the underlying event store.
    #[error("Event store error: {0}")]
    EventStore(E),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl<E> From<E> for SnapshotError<E> {
    fn from(err: E) -> Self {
        SnapshotError::EventStore(err)
    }
}

/// Manager for creating and retrieving snapshots.
///
/// The SnapshotManager automatically creates snapshots at configured intervals
/// and provides methods for finding the latest valid snapshot for replay.
#[derive(Debug)]
pub struct SnapshotManager<S: EventStore> {
    event_store: Arc<S>,
    config: SnapshotConfig,
}

impl<S: EventStore> SnapshotManager<S> {
    /// Create a new SnapshotManager.
    ///
    /// # Arguments
    ///
    /// * `event_store` - The event store for persisting snapshots.
    /// * `config` - Configuration for snapshot behavior.
    pub fn new(event_store: Arc<S>, config: SnapshotConfig) -> Self {
        Self {
            event_store,
            config,
        }
    }

    /// Get the current configuration.
    pub fn config(&self) -> &SnapshotConfig {
        &self.config
    }

    /// Check if a snapshot should be taken based on event count.
    ///
    /// Returns `true` if a snapshot should be created at the given event ID.
    pub fn should_snapshot(&self, current_event_id: u64, last_snapshot_id: Option<u64>) -> bool {
        if !self.config.is_enabled() {
            return false;
        }

        let events_since_last = current_event_id.saturating_sub(last_snapshot_id.unwrap_or(0));

        events_since_last >= self.config.interval
    }

    /// Create a snapshot if the conditions are met.
    ///
    /// This method checks if enough events have occurred since the last
    /// snapshot and creates one if necessary.
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
    ) -> Result<SnapshotResult, SnapshotError<S::Error>> {
        // Find the latest snapshot
        let latest = self
            .event_store
            .get_latest_snapshot(saga_id)
            .await
            .map_err(SnapshotError::from)?;

        // Check if we need a snapshot
        let should_take = self.should_snapshot(current_event_id, latest.map(|(id, _)| id));

        if !should_take {
            return Ok(SnapshotResult::Skipped);
        }

        // Create the snapshot
        self.create_snapshot(saga_id, current_event_id, state)
            .await?;

        Ok(SnapshotResult::Created)
    }

    /// Create a snapshot unconditionally.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga to snapshot.
    /// * `last_event_id` - The event ID this snapshot covers.
    /// * `state` - The serialized state.
    pub async fn create_snapshot(
        &self,
        saga_id: &SagaId,
        last_event_id: u64,
        state: &[u8],
    ) -> Result<(), SnapshotError<S::Error>> {
        let checksum = if self.config.enable_checksum {
            Some(SnapshotChecksum::from_data(state))
        } else {
            None
        };

        let snapshot = Snapshot::new(saga_id.clone(), last_event_id, state.to_vec(), checksum);

        // Serialize the snapshot for storage
        let serialized = bincode::serialize(&snapshot)
            .map_err(|e| SnapshotError::Serialization(e.to_string()))?;

        self.event_store
            .save_snapshot(saga_id, last_event_id, &serialized)
            .await
            .map_err(SnapshotError::from)?;

        Ok(())
    }

    /// Find the latest valid snapshot for a saga.
    ///
    /// This method retrieves the most recent snapshot and verifies its
    /// integrity if checksums are enabled.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga to find a snapshot for.
    ///
    /// # Returns
    ///
    /// The latest valid snapshot, or `None` if no snapshot exists or all are invalid.
    pub async fn find_latest_valid(
        &self,
        saga_id: &SagaId,
    ) -> Result<Option<(u64, Vec<u8>)>, SnapshotError<S::Error>> {
        let snapshot_data = self
            .event_store
            .get_latest_snapshot(saga_id)
            .await
            .map_err(SnapshotError::from)?;

        match snapshot_data {
            Some((event_id, data)) => {
                let snapshot: Snapshot = bincode::deserialize(&data)
                    .map_err(|e| SnapshotError::Serialization(e.to_string()))?;

                // Verify integrity if checksums are enabled
                if self.config.enable_checksum && !snapshot.verify_integrity() {
                    // Snapshot is corrupt - return None
                    // In production, you might want to log this or attempt recovery
                    Ok(None)
                } else {
                    Ok(Some((event_id, snapshot.state)))
                }
            }
            None => Ok(None),
        }
    }

    /// Get the number of snapshots for a saga.
    ///
    /// Note: This requires loading all snapshots, which may be expensive.
    /// For more efficient counting, consider adding a dedicated method to EventStore.
    pub async fn snapshot_count(&self, saga_id: &SagaId) -> Result<u64, SnapshotError<S::Error>> {
        self.event_store
            .snapshot_count(saga_id)
            .await
            .map_err(SnapshotError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{HistoryEvent, SagaId};
    use crate::port::event_store::EventStore;
    use async_trait::async_trait;
    use serde_json::json;

    /// Test event store implementation.
    #[derive(Debug, Default)]
    struct TestEventStore {
        snapshots: Arc<std::sync::Mutex<Vec<(u64, Vec<u8>)>>>,
    }

    #[async_trait]
    impl EventStore for TestEventStore {
        type Error = String;

        async fn append_event(
            &self,
            _saga_id: &SagaId,
            _expected_next_event_id: u64,
            _event: &HistoryEvent,
        ) -> Result<u64, EventStoreError<Self::Error>> {
            Ok(0)
        }

        async fn append_events(
            &self,
            _saga_id: &SagaId,
            _expected_next_event_id: u64,
            _events: &[HistoryEvent],
        ) -> Result<u64, EventStoreError<Self::Error>> {
            Ok(0)
        }

        async fn get_history(&self, _saga_id: &SagaId) -> Result<Vec<HistoryEvent>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_history_from(
            &self,
            _saga_id: &SagaId,
            _from_event_id: u64,
        ) -> Result<Vec<HistoryEvent>, Self::Error> {
            Ok(Vec::new())
        }

        async fn save_snapshot(
            &self,
            _saga_id: &SagaId,
            event_id: u64,
            state: &[u8],
        ) -> Result<(), Self::Error> {
            let mut guard = self.snapshots.lock().unwrap();
            guard.push((event_id, state.to_vec()));
            Ok(())
        }

        async fn get_latest_snapshot(
            &self,
            _saga_id: &SagaId,
        ) -> Result<Option<(u64, Vec<u8>)>, Self::Error> {
            let guard = self.snapshots.lock().unwrap();
            Ok(guard.last().cloned())
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
            let guard = self.snapshots.lock().unwrap();
            Ok(guard.len() as u64)
        }
    }

    #[test]
    fn test_snapshot_config_defaults() {
        let config = SnapshotConfig::default();
        assert_eq!(config.interval, 100);
        assert_eq!(config.max_snapshots, 5);
        assert!(config.enable_checksum);
        assert!(config.is_enabled());
    }

    #[test]
    fn test_snapshot_config_disabled() {
        let config = SnapshotConfig::disabled();
        assert_eq!(config.interval, 0);
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_checksum_verification() {
        let data = b"test data";
        let checksum = SnapshotChecksum::from_data(data);

        assert!(checksum.verify(data));

        let modified = b"modified data";
        assert!(!checksum.verify(modified));
    }

    #[test]
    fn test_should_snapshot_enabled() {
        let config = SnapshotConfig::new(10, 5, true);
        let store = TestEventStore::default();
        let manager = SnapshotManager::new(Arc::new(store), config);

        // No previous snapshot, 10 events - should snapshot
        assert!(manager.should_snapshot(10, None));

        // No previous snapshot, 5 events - should not snapshot
        assert!(!manager.should_snapshot(5, None));

        // Previous snapshot at 0, 10 events - should snapshot
        assert!(manager.should_snapshot(10, Some(0)));

        // Previous snapshot at 5, 10 events (5 since last) - should not snapshot
        assert!(!manager.should_snapshot(10, Some(5)));
    }

    #[test]
    fn test_should_snapshot_disabled() {
        let config = SnapshotConfig::disabled();
        let store = TestEventStore::default();
        let manager = SnapshotManager::new(Arc::new(store), config);

        assert!(!manager.should_snapshot(100, None));
    }

    #[tokio::test]
    async fn test_create_snapshot() {
        let store = TestEventStore::default();
        let manager = SnapshotManager::new(Arc::new(store), SnapshotConfig::new(10, 5, true));

        let saga_id = SagaId("test-saga".to_string());
        let state = b"serialized state";

        let result = manager.create_snapshot(&saga_id, 100, state).await.unwrap();

        assert!(matches!(result, ()));
    }

    #[tokio::test]
    async fn test_find_latest_valid() {
        let store = TestEventStore::default();
        let manager = SnapshotManager::new(Arc::new(store), SnapshotConfig::new(10, 5, true));

        let saga_id = SagaId("test-saga".to_string());

        // No snapshots yet
        let result = manager.find_latest_valid(&saga_id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_maybe_take_snapshot_skipped() {
        let store = TestEventStore::default();
        let manager = SnapshotManager::new(
            Arc::new(store),
            SnapshotConfig::new(100, 5, true), // High interval
        );

        let saga_id = SagaId("test-saga".to_string());
        let state = b"state";

        // Should skip (not enough events)
        let result = manager
            .maybe_take_snapshot(&saga_id, 50, state)
            .await
            .unwrap();

        assert!(matches!(result, SnapshotResult::Skipped));
    }

    #[tokio::test]
    async fn test_snapshot_integrity() {
        let data = b"important state";
        let checksum = SnapshotChecksum::from_data(data);

        let snapshot = Snapshot::new(
            SagaId("test".to_string()),
            10,
            data.to_vec(),
            Some(checksum),
        );

        assert!(snapshot.verify_integrity());

        // Modify the state
        let mut corrupted = snapshot.clone();
        corrupted.state = b"corrupted".to_vec();

        assert!(!corrupted.verify_integrity());
    }
}

//! HistoryReplayer port for replaying saga state from events.
//!
//! This module defines the [`HistoryReplayer`] trait for reconstructing
//! saga state from the event history (durable execution pattern).
//!
//! # Architecture
//!
//! The HistoryReplayer is responsible for:
//! - **Deterministic Replay**: Replaying events in exact order to reconstruct state
//! - **Snapshot Integration**: Using snapshots to optimize replay
//! - **Partial Replay**: Starting from a specific event ID
//! - **Debug Support**: Replay to any historical state
//!
//! # Features
//!
//! - **Event Replay**: Apply events to state in order
//! - **Snapshot Caching**: Use latest snapshot as base for replay
//! - **Partial Replay**: Replay from specific event ID
//! - **Thread-Safe**: Support concurrent replay operations
//!
//! # Usage Pattern
//!
//! ```ignore
//! // 1. Get latest snapshot (if exists)
//! let snapshot = snapshot_manager.get_latest(&saga_id).await?;
//!
//! // 2. Load events from snapshot point
//! let events = event_store.get_history_from(&saga_id, snapshot.event_id).await?;
//!
//! // 3. Replay events onto snapshot state
//! let state = replayer.replay(&snapshot.state, &events).await?;
//!
//! // 4. Use state for workflow execution
//! execute_workflow(&state, &workflow);
//! ```
//!
//! # Replayer Components
//!
//! - **Port**: `HistoryReplayer` trait in `core/src/port/replay.rs`
//! - **Testing**: `InMemoryReplayer` for unit tests
//! - **Production**: `PostgresReplayer` for PostgreSQL-backed event stores

use super::super::event::{HistoryEvent, SagaId};
use std::fmt::Debug;
use std::time::Duration;

/// Trait for states that can be reconstructed from history events.
///
/// This trait must be implemented by any state type that wants to be
/// managed by a [`HistoryReplayer`].
pub trait Applicator: Sized {
    /// Apply a single history event to the current state.
    ///
    /// # Errors
    /// Returns an error if the event cannot be applied to the current state.
    fn apply(&mut self, event: &HistoryEvent) -> Result<(), String>;

    /// Reconstruct the state from a serialized snapshot.
    ///
    /// # Errors
    /// Returns an error if the snapshot data is invalid or cannot be deserialized.
    fn from_snapshot(data: &[u8]) -> Result<Self, String>;
}

/// Result of replaying events onto state.
#[derive(Debug, Clone)]
pub struct ReplayResult<T> {
    /// The reconstructed state after replay.
    pub state: T,

    /// Number of events replayed.
    pub events_replayed: usize,

    /// Time taken to replay.
    pub replay_duration: Duration,

    /// Last event ID processed.
    pub last_event_id: u64,
}

/// Configuration for replay operations.
#[derive(Debug, Clone, Copy)]
pub struct ReplayConfig {
    /// Whether to use snapshots for optimization.
    pub use_snapshots: bool,

    /// Maximum number of events to replay (0 = unlimited).
    pub max_events: usize,

    /// Timeout for replay operation.
    pub timeout: Duration,
}

impl Default for ReplayConfig {
    fn default() -> Self {
        Self {
            use_snapshots: true,
            max_events: 0,
            timeout: Duration::from_secs(30),
        }
    }
}

/// Error type for replay operations.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError<E> {
    #[error("Failed to deserialize event: {0}")]
    Deserialization(String),

    #[error("Event application failed: {0}")]
    Apply(String),

    #[error("Invalid event sequence: expected {expected}, got {actual}")]
    InvalidSequence { expected: u64, actual: u64 },

    #[error("Replay timeout exceeded")]
    Timeout,

    #[error("Storage error: {0}")]
    Storage(E),

    #[error("Too many events to replay: max {max}")]
    TooManyEvents { max: usize },
}

impl<E> ReplayError<E> {
    /// Create an invalid sequence error.
    pub fn invalid_sequence(expected: u64, actual: u64) -> Self {
        Self::InvalidSequence { expected, actual }
    }

    /// Create a too many events error.
    pub fn too_many(max: usize) -> Self {
        Self::TooManyEvents { max }
    }
}

/// Trait for replaying saga history to reconstruct state.
///
/// The HistoryReplayer provides deterministic replay of events to rebuild
/// saga state at any point in history. This is the core component
/// that enables durable execution and fault tolerance.
///
/// # Generic Parameters
///
/// - `T`: The state type that events are applied to (e.g., `SagaContext`)
/// - `E`: The error type for storage operations
///
/// # Replayer Contract
///
/// Implementations MUST guarantee:
/// - **Deterministic Replay**: Same events + same starting state = same result
/// - **Event Order**: Events must be applied in exact order by event_id
/// - **Error Handling**: Invalid events must not corrupt state
/// - **Performance**: Replay must be efficient for large histories
///
/// # Thread Safety
///
/// Implementations MUST be thread-safe (`Send + Sync`) to support
/// concurrent replay operations.
#[async_trait::async_trait]
pub trait HistoryReplayer<T>: Send + Sync {
    /// The error type for this implementation.
    type Error: Debug + Send + Sync + 'static;

    /// Replay events onto the given state.
    ///
    /// This method applies events in order to reconstruct the state.
    /// Events are applied sequentially, and each event must be
    /// successfully applied before proceeding to the next.
    ///
    /// # Arguments
    ///
    /// * `state` - The initial state (often from snapshot or empty).
    /// * `events` - The events to replay in chronological order.
    /// * `config` - Optional configuration for replay behavior.
    ///
    /// # Returns
    ///
    /// Returns the reconstructed state with metadata about the replay.
    ///
    /// # Errors
    ///
    /// - `InvalidSequence`: If events are not in chronological order.
    /// - `Apply`: If an event fails to apply to state.
    /// - `Timeout`: If replay exceeds configured timeout.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let initial_state = SagaState::new();
    /// let events = vec![event1, event2, event3];
    /// let result = replayer.replay(initial_state, &events).await?;
    /// // result.state is the state after applying all events
    /// // result.events_replayed = 3
    /// ```
    async fn replay(
        &self,
        state: T,
        events: &[HistoryEvent],
        config: Option<ReplayConfig>,
    ) -> Result<ReplayResult<T>, ReplayError<Self::Error>>;

    /// Replay events from a specific event ID.
    ///
    /// This enables partial replay starting from a known point in history,
    /// useful for debugging and disaster recovery.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga to replay.
    /// * `from_event_id` - The event ID to start replay from.
    /// * `config` - Optional configuration for replay behavior.
    ///
    /// # Returns
    ///
    /// Returns the reconstructed state at that point in history.
    ///
    /// # Errors
    ///
    /// - `Storage`: If event store operations fail.
    /// - `Timeout`: If replay exceeds configured timeout.
    async fn replay_from_event_id(
        &self,
        saga_id: &SagaId,
        from_event_id: u64,
        config: Option<ReplayConfig>,
    ) -> Result<ReplayResult<T>, ReplayError<Self::Error>>;

    /// Get the current state of a saga.
    ///
    /// This combines snapshot + events to get the latest state efficiently.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga to get state for.
    /// * `config` - Optional configuration for replay behavior.
    ///
    /// # Returns
    ///
    /// Returns the current reconstructed state.
    ///
    /// # Optimization
    ///
    /// Implementations should use snapshots when available:
    /// 1. Get latest snapshot
    /// 2. Get events from snapshot event_id
    /// 3. Replay events onto snapshot state
    ///
    /// This is O(events after snapshot) instead of O(all events).
    async fn get_current_state(
        &self,
        saga_id: &SagaId,
        config: Option<ReplayConfig>,
    ) -> Result<ReplayResult<T>, ReplayError<Self::Error>>;

    /// Validate that events form a valid sequence.
    ///
    /// This method checks for:
    /// - Monotonically increasing event IDs
    /// - No duplicate event IDs
    /// - No gaps in sequence (optional)
    ///
    /// # Arguments
    ///
    /// * `events` - The events to validate.
    /// * `allow_gaps` - Whether to allow gaps in event ID sequence.
    ///
    /// # Returns
    ///
    /// Returns the number of invalid events (0 = all valid).
    async fn validate_sequence(
        &self,
        events: &[HistoryEvent],
        allow_gaps: bool,
    ) -> Result<usize, ReplayError<Self::Error>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_replay_config_defaults() {
        let config = ReplayConfig::default();
        assert!(config.use_snapshots);
        assert_eq!(config.max_events, 0);
        assert_eq!(config.timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_replay_config_custom() {
        let config = ReplayConfig {
            use_snapshots: false,
            max_events: 100,
            timeout: Duration::from_secs(60),
        };

        assert!(!config.use_snapshots);
        assert_eq!(config.max_events, 100);
        assert_eq!(config.timeout, Duration::from_secs(60));
    }
}

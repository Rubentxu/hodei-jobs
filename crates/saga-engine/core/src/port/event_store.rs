//! EventStore port trait definition.
//!
//! This module defines the [`EventStore`] trait that backends must implement
//! to provide event storage capabilities for the saga engine.

use super::super::event::{EventId, HistoryEvent, SagaId};
use async_trait::async_trait;
use std::fmt::Debug;

/// Errors that can occur when operating on the event store.
#[derive(Debug, thiserror::Error)]
pub enum EventStoreError<E> {
    /// Conflict error - optimistic locking detected version mismatch.
    #[error("Conflict: expected event_id {expected}, but current is {actual}")]
    Conflict {
        /// The event ID we expected to be next.
        expected: u64,
        /// The actual current event ID.
        actual: u64,
    },

    /// The requested saga was not found.
    #[error("Saga not found: {saga_id}")]
    NotFound {
        /// The saga ID that was not found.
        saga_id: SagaId,
    },

    /// The saga has been closed and cannot accept more events.
    #[error("Saga closed: {saga_id}")]
    SagaClosed {
        /// The saga ID that is closed.
        saga_id: SagaId,
    },

    /// Backend-specific error.
    #[error("Backend error: {0}")]
    Backend(E),

    /// Codec/serialization error.
    #[error("Codec error: {0}")]
    Codec(String),
}

impl<E> EventStoreError<E> {
    /// Create a conflict error.
    pub fn conflict(expected: u64, actual: u64) -> Self {
        Self::Conflict { expected, actual }
    }

    /// Create a not found error.
    pub fn not_found(saga_id: SagaId) -> Self {
        Self::NotFound { saga_id }
    }

    /// Create a saga closed error.
    pub fn saga_closed(saga_id: SagaId) -> Self {
        Self::SagaClosed { saga_id }
    }

    /// Check if this is a conflict error.
    pub fn is_conflict(&self) -> bool {
        matches!(self, Self::Conflict { .. })
    }
}

impl<E> From<E> for EventStoreError<E> {
    fn from(err: E) -> Self {
        EventStoreError::Backend(err)
    }
}

/// Trait for event storage operations.
///
/// The EventStore is responsible for persisting and retrieving events
/// from the saga history. Implementations must provide:
/// - Append-only event storage
/// - Optimistic locking for concurrent access
/// - History retrieval for replay
/// - Snapshot storage and retrieval
///
/// # Concurrency Model
///
/// Implementations should use optimistic locking:
/// 1. `append_event` requires `expected_next_event_id`
/// 2. If current event_id != expected, return `EventStoreError::Conflict`
/// 3. Caller retries with correct version
///
/// This allows multiple workers to append to the same saga
/// without coordination, while maintaining consistency.
#[async_trait::async_trait]
pub trait EventStore: Send + Sync {
    /// The error type for this implementation.
    type Error: Debug + Send + Sync + 'static;

    /// Append a single event to the saga history.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga to append to.
    /// * `expected_next_event_id` - The event ID we expect to be next.
    /// * `event` - The event to append.
    ///
    /// # Returns
    ///
    /// The event ID of the appended event.
    ///
    /// # Errors
    ///
    /// - `EventStoreError::Conflict` if the current event_id doesn't match
    ///   `expected_next_event_id`.
    /// - `EventStoreError::SagaClosed` if the saga has been closed.
    async fn append_event(
        &self,
        saga_id: &SagaId,
        expected_next_event_id: u64,
        event: &HistoryEvent,
    ) -> Result<u64, EventStoreError<Self::Error>>;

    /// Append multiple events atomically.
    ///
    /// This is more efficient than calling `append_event` multiple times
    /// when adding multiple events at once.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga to append to.
    /// * `expected_next_event_id` - The event ID we expect to be next.
    /// * `events` - The events to append.
    ///
    /// # Returns
    ///
    /// The event ID of the last appended event.
    async fn append_events(
        &self,
        saga_id: &SagaId,
        expected_next_event_id: u64,
        events: &[HistoryEvent],
    ) -> Result<u64, EventStoreError<Self::Error>> {
        let mut current = expected_next_event_id;
        for event in events {
            current = self.append_event(saga_id, current, event).await?;
        }
        Ok(current)
    }

    /// Get the complete event history for a saga.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga to get history for.
    ///
    /// # Returns
    ///
    /// A vector of all events in order, or error if saga not found.
    async fn get_history(&self, saga_id: &SagaId) -> Result<Vec<HistoryEvent>, Self::Error>;

    /// Get event history starting from a specific event ID.
    ///
    /// This is used for incremental replay after a snapshot.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga to get history for.
    /// * `from_event_id` - The event ID to start from (exclusive).
    ///
    /// # Returns
    ///
    /// A vector of events from `from_event_id + 1` to the end.
    async fn get_history_from(
        &self,
        saga_id: &SagaId,
        from_event_id: u64,
    ) -> Result<Vec<HistoryEvent>, Self::Error>;

    /// Save a snapshot of the saga state.
    ///
    /// Snapshots are used to speed up replay by avoiding reprocessing
    /// all events from the beginning.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga to snapshot.
    /// * `event_id` - The event ID this snapshot was taken after.
    /// * `state` - The serialized state data.
    async fn save_snapshot(
        &self,
        saga_id: &SagaId,
        event_id: u64,
        state: &[u8],
    ) -> Result<(), Self::Error>;

    /// Get the latest snapshot for a saga.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga to get snapshot for.
    ///
    /// # Returns
    ///
    /// `Some((event_id, state_bytes))` if a snapshot exists, `None` otherwise.
    async fn get_latest_snapshot(
        &self,
        saga_id: &SagaId,
    ) -> Result<Option<(u64, Vec<u8>)>, Self::Error>;

    /// Get the current event ID for a saga (for optimistic locking).
    ///
    /// Returns the ID of the last event, or 0 if no events exist.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga to check.
    ///
    /// # Returns
    ///
    /// The current event ID.
    async fn get_current_event_id(&self, saga_id: &SagaId) -> Result<u64, Self::Error>;

    /// Check if a saga exists.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga to check.
    ///
    /// # Returns
    ///
    /// `true` if the saga has any events.
    async fn saga_exists(&self, saga_id: &SagaId) -> Result<bool, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventCategory, EventType, HistoryEvent};
    use serde_json::json;

    /// Test helper to create a test event.
    fn make_test_event(saga_id: &SagaId, event_num: u64) -> HistoryEvent {
        HistoryEvent::builder()
            .saga_id(saga_id.clone())
            .event_type(EventType::WorkflowExecutionStarted)
            .category(EventCategory::Workflow)
            .payload(json!({"event_num": event_num}))
            .build()
    }

    // Integration tests are provided by saga-engine-testing crate
    // which implements InMemoryEventStore for testing.
}

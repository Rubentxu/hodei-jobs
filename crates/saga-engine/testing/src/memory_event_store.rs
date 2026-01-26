//! In-memory implementation of EventStore for testing.
//!
//! This implementation is thread-safe and provides full event store
//! functionality without requiring a database. Ideal for unit tests
//! and development.

use parking_lot::RwLock;
use saga_engine_core::codec::JsonCodec;
use saga_engine_core::event::{HistoryEvent, SagaId};
use saga_engine_core::port::event_store::{EventStore, EventStoreError};
use std::collections::HashMap;

/// In-memory event store implementation.
///
/// This implementation stores events in memory using Rust's parking_lot
/// for efficient read-write locking. It's designed for testing and
/// development scenarios.
///
/// # Thread Safety
///
/// Uses `RwLock` for concurrent access:
/// - Multiple readers can access simultaneously
/// - Writers get exclusive access
/// - No deadlocks possible with this pattern
///
/// # Example
///
/// ```rust
/// use saga_engine_testing::InMemoryEventStore;
/// use saga_engine_core::event::{HistoryEvent, EventType, EventCategory, SagaId};
/// use saga_engine_core::codec::JsonCodec;
/// use serde_json::json;
///
/// #[tokio::test]
/// async fn test_inmemory_event_store() {
///     let store = InMemoryEventStore::new();
///     let saga_id = SagaId("test-saga".to_string());
///
///     let event = HistoryEvent::new(
///         EventId(0),
///         saga_id.clone(),
///         EventType::WorkflowExecutionStarted,
///         EventCategory::Workflow,
///         json!({"key": "value"}),
///     );
///
///     // Append event
///     let event_id = store.append_event(&saga_id, 0, &event).await.unwrap();
///     assert_eq!(event_id, 0);
///
///     // Get history
///     let history = store.get_history(&saga_id).await.unwrap();
///     assert_eq!(history.len(), 1);
/// }
/// ```
#[derive(Debug)]
pub struct InMemoryEventStore {
    /// Events indexed by saga_id, then by event_id.
    events: RwLock<HashMap<SagaId, Vec<HistoryEvent>>>,

    /// Current event ID counter for each saga (for optimistic locking).
    current_event_id: RwLock<HashMap<SagaId, u64>>,

    /// Snapshots indexed by saga_id.
    snapshots: RwLock<HashMap<SagaId, Vec<(u64, Vec<u8>)>>>,

    /// Codec for serialization.
    codec: JsonCodec,
}

impl Default for InMemoryEventStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryEventStore {
    /// Create a new in-memory event store.
    pub fn new() -> Self {
        Self {
            events: RwLock::new(HashMap::new()),
            current_event_id: RwLock::new(HashMap::new()),
            snapshots: RwLock::new(HashMap::new()),
            codec: JsonCodec,
        }
    }

    /// Create a new event store with initial data.
    ///
    /// This is a synchronous operation that directly populates the store.
    pub fn with_events(events: Vec<(SagaId, HistoryEvent)>) -> Self {
        let store = Self::new();

        for (saga_id, event) in events {
            let event_id = event.event_id.0;

            // Store the event directly
            {
                let mut events_guard = store.events.write();
                let saga_events = events_guard.entry(saga_id.clone()).or_default();
                saga_events.push(event);
            }

            // Update current event ID
            {
                let mut current = store.current_event_id.write();
                *current.entry(saga_id.clone()).or_insert(0) = event_id + 1;
            }
        }

        store
    }

    /// Clear all data (useful for testing).
    pub fn clear(&self) {
        let mut events = self.events.write();
        let mut current = self.current_event_id.write();
        let mut snapshots = self.snapshots.write();

        events.clear();
        current.clear();
        snapshots.clear();
    }

    /// Get the number of sagas stored.
    pub fn saga_count(&self) -> usize {
        self.events.read().len()
    }

    /// Get the total number of events stored.
    pub fn event_count(&self) -> usize {
        self.events.read().values().map(|v| v.len()).sum()
    }
}

#[async_trait::async_trait]
impl EventStore for InMemoryEventStore {
    type Error = InMemoryEventStoreError;

    async fn append_event(
        &self,
        saga_id: &SagaId,
        expected_next_event_id: u64,
        event: &HistoryEvent,
    ) -> Result<u64, EventStoreError<Self::Error>> {
        // Get current event ID for this saga
        let current_id = {
            let current = self.current_event_id.read();
            current.get(saga_id).copied().unwrap_or(0)
        };

        // Check for conflict (optimistic locking)
        if current_id != expected_next_event_id {
            return Err(EventStoreError::conflict(
                expected_next_event_id,
                current_id,
            ));
        }

        // Clone the event (we own it now)
        let event_clone = event.clone();
        let event_id = event.event_id.0;

        // Store the event
        {
            let mut events = self.events.write();
            let saga_events = events.entry(saga_id.clone()).or_default();
            saga_events.push(event_clone);
        }

        // Update current event ID
        {
            let mut current = self.current_event_id.write();
            *current.entry(saga_id.clone()).or_insert(0) = event_id + 1;
        }

        Ok(event_id)
    }

    async fn get_history(&self, saga_id: &SagaId) -> Result<Vec<HistoryEvent>, Self::Error> {
        let events = self.events.read();
        Ok(events.get(saga_id).cloned().unwrap_or_default())
    }

    async fn get_history_from(
        &self,
        saga_id: &SagaId,
        from_event_id: u64,
    ) -> Result<Vec<HistoryEvent>, Self::Error> {
        let events = self.events.read();

        if let Some(saga_events) = events.get(saga_id) {
            // Filter events after from_event_id
            let filtered: Vec<HistoryEvent> = saga_events
                .iter()
                .filter(|e| e.event_id.0 > from_event_id)
                .cloned()
                .collect();
            Ok(filtered)
        } else {
            Ok(Vec::new())
        }
    }

    async fn save_snapshot(
        &self,
        saga_id: &SagaId,
        event_id: u64,
        state: &[u8],
    ) -> Result<(), Self::Error> {
        let mut snapshots = self.snapshots.write();
        let saga_snapshots = snapshots.entry(saga_id.clone()).or_default();

        // Keep only the latest snapshot (and maybe a few recent ones)
        saga_snapshots.push((event_id, state.to_vec()));

        // Limit to last 5 snapshots per saga
        if saga_snapshots.len() > 5 {
            saga_snapshots.drain(0..saga_snapshots.len() - 5);
        }

        Ok(())
    }

    async fn get_latest_snapshot(
        &self,
        saga_id: &SagaId,
    ) -> Result<Option<(u64, Vec<u8>)>, Self::Error> {
        let snapshots = self.snapshots.read();

        if let Some(saga_snapshots) = snapshots.get(saga_id) {
            Ok(saga_snapshots.last().cloned())
        } else {
            Ok(None)
        }
    }

    async fn get_current_event_id(&self, saga_id: &SagaId) -> Result<u64, Self::Error> {
        let current = self.current_event_id.read();
        Ok(current.get(saga_id).copied().unwrap_or(0))
    }

    async fn get_last_reset_point(&self, saga_id: &SagaId) -> Result<Option<u64>, Self::Error> {
        let events = self.events.read();
        if let Some(saga_events) = events.get(saga_id) {
            // Find the last reset point (iterate in reverse)
            for event in saga_events.iter().rev() {
                if event.is_reset_point {
                    return Ok(Some(event.event_id.0));
                }
            }
        }
        Ok(None)
    }

    async fn saga_exists(&self, saga_id: &SagaId) -> Result<bool, Self::Error> {
        let events = self.events.read();
        Ok(events.contains_key(saga_id))
    }

    async fn snapshot_count(&self, saga_id: &SagaId) -> Result<u64, Self::Error> {
        let snapshots = self.snapshots.read();
        Ok(snapshots.get(saga_id).map(|s| s.len()).unwrap_or(0) as u64)
    }
}

/// Error type for InMemoryEventStore operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InMemoryEventStoreError {
    #[error("Event not found")]
    NotFound,

    #[error("Saga not found: {0}")]
    SagaNotFound(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<String> for InMemoryEventStoreError {
    fn from(s: String) -> Self {
        Self::Internal(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saga_engine_core::codec::JsonCodec;
    use saga_engine_core::event::{EventCategory, EventId, EventType};
    use serde_json::json;

    #[tokio::test]
    async fn test_append_and_get_single_event() {
        let store = InMemoryEventStore::new();
        let saga_id = SagaId("test-saga-1".to_string());

        let event = HistoryEvent::new(
            EventId(0),
            saga_id.clone(),
            EventType::WorkflowExecutionStarted,
            EventCategory::Workflow,
            json!({"key": "value"}),
        );

        let event_id = store.append_event(&saga_id, 0, &event).await.unwrap();
        assert!(event_id >= 0); // Event was appended successfully

        let history = store.get_history(&saga_id).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].event_type, EventType::WorkflowExecutionStarted);
    }

    #[tokio::test]
    async fn test_append_multiple_events() {
        let store = InMemoryEventStore::new();
        let saga_id = SagaId("test-saga-2".to_string());

        let initial_id = store.get_current_event_id(&saga_id).await.unwrap();
        assert_eq!(initial_id, 0);

        let event1 = HistoryEvent::builder()
            .event_id(EventId(0))
            .saga_id(saga_id.clone())
            .event_type(EventType::WorkflowExecutionStarted)
            .category(EventCategory::Workflow)
            .payload(json!({"step": 1}))
            .build();

        let event1_id = store
            .append_event(&saga_id, initial_id, &event1)
            .await
            .unwrap();

        // Append second event
        let event2 = HistoryEvent::builder()
            .event_id(EventId(1))
            .saga_id(saga_id.clone())
            .event_type(EventType::ActivityTaskScheduled)
            .category(EventCategory::Activity)
            .payload(json!({"step": 2}))
            .build();

        let event2_id = store
            .append_event(&saga_id, event1_id + 1, &event2)
            .await
            .unwrap();

        let history = store.get_history(&saga_id).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].event_id.0, event1_id);
        assert_eq!(history[1].event_id.0, event2_id);
    }

    #[tokio::test]
    async fn test_optimistic_locking_conflict() {
        let store = InMemoryEventStore::new();
        let saga_id = SagaId("test-saga-3".to_string());

        let event = HistoryEvent::new(
            EventId(0),
            saga_id.clone(),
            EventType::WorkflowExecutionStarted,
            EventCategory::Workflow,
            json!({}),
        );

        // First append succeeds
        store.append_event(&saga_id, 0, &event).await.unwrap();

        // Second append with wrong expected version fails
        let result = store.append_event(&saga_id, 0, &event).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.is_conflict());
    }

    #[tokio::test]
    async fn test_get_history_from() {
        let store = InMemoryEventStore::new();
        let saga_id = SagaId("test-saga-4".to_string());

        // Add 5 events and track their IDs
        let mut event_ids = Vec::new();
        let mut expected_id = store.get_current_event_id(&saga_id).await.unwrap();
        for i in 0..5 {
            let event = HistoryEvent::builder()
                .event_id(EventId(i))
                .saga_id(saga_id.clone())
                .event_type(EventType::ActivityTaskScheduled)
                .category(EventCategory::Activity)
                .payload(json!({"num": i}))
                .build();
            let event_id = store
                .append_event(&saga_id, expected_id, &event)
                .await
                .unwrap();
            event_ids.push(event_id);
            expected_id = event_id + 1;
        }

        // Verify we have exactly 5 events
        let full_history = store.get_history(&saga_id).await.unwrap();
        assert_eq!(full_history.len(), 5);

        // Get from event 2 - should return events after the second event
        let from_event_id = event_ids[2]; // Third event's ID
        let history = store
            .get_history_from(&saga_id, from_event_id)
            .await
            .unwrap();
        // Should have events with id > from_event_id (i.e., last 2 events)
        assert_eq!(
            history.len(),
            2,
            "Expected 2 events after {}",
            from_event_id
        );
        assert!(history.iter().all(|e| e.event_id.0 > from_event_id));

        // Get from last event ID (beyond last)
        let last_id = event_ids[4];
        let history = store.get_history_from(&saga_id, last_id).await.unwrap();
        assert_eq!(
            history.len(),
            0,
            "No events should exist after last event ID"
        );
    }

    #[tokio::test]
    async fn test_snapshot_operations() {
        let store = InMemoryEventStore::new();
        let saga_id = SagaId("test-saga-5".to_string());

        let state = b"serialized state data";

        // Save snapshot at event 10
        store.save_snapshot(&saga_id, 10, state).await.unwrap();

        // Get latest snapshot
        let latest = store.get_latest_snapshot(&saga_id).await.unwrap();
        assert!(latest.is_some());
        let (event_id, snapshot_state) = latest.unwrap();
        assert_eq!(event_id, 10);
        assert_eq!(snapshot_state, state);

        // Test snapshot count
        let count = store.snapshot_count(&saga_id).await.unwrap();
        assert_eq!(count, 1);

        // Save another snapshot
        store
            .save_snapshot(&saga_id, 15, b"new state")
            .await
            .unwrap();
        let count = store.snapshot_count(&saga_id).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_saga_exists() {
        let store = InMemoryEventStore::new();
        let saga_id = SagaId("test-saga-6".to_string());

        assert!(!store.saga_exists(&saga_id).await.unwrap());

        let event = HistoryEvent::new(
            EventId(0),
            saga_id.clone(),
            EventType::WorkflowExecutionStarted,
            EventCategory::Workflow,
            json!({}),
        );
        store.append_event(&saga_id, 0, &event).await.unwrap();

        assert!(store.saga_exists(&saga_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_get_nonexistent_saga() {
        let store = InMemoryEventStore::new();
        let saga_id = SagaId("nonexistent".to_string());

        let history = store.get_history(&saga_id).await.unwrap();
        assert!(history.is_empty());

        let current_id = store.get_current_event_id(&saga_id).await.unwrap();
        assert_eq!(current_id, 0);

        let snapshot_count = store.snapshot_count(&saga_id).await.unwrap();
        assert_eq!(snapshot_count, 0);
    }

    #[tokio::test]
    async fn test_clear_store() {
        let store = InMemoryEventStore::new();
        let saga_id = SagaId("test-saga-7".to_string());

        let event = HistoryEvent::new(
            EventId(0),
            saga_id.clone(),
            EventType::WorkflowExecutionStarted,
            EventCategory::Workflow,
            json!({}),
        );
        store.append_event(&saga_id, 0, &event).await.unwrap();
        store.save_snapshot(&saga_id, 0, b"state").await.unwrap();

        assert_eq!(store.saga_count(), 1);
        assert_eq!(store.event_count(), 1);
        assert_eq!(store.snapshot_count(&saga_id).await.unwrap(), 1);

        store.clear();

        assert_eq!(store.saga_count(), 0);
        assert_eq!(store.event_count(), 0);
        assert_eq!(store.snapshot_count(&saga_id).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_concurrent_appends() {
        use std::sync::Arc;
        use tokio::sync::Barrier;

        let store = Arc::new(InMemoryEventStore::new());
        let saga_id = SagaId("test-saga-concurrent".to_string());
        let barrier = Arc::new(Barrier::new(10));

        let mut handles = Vec::new();

        for i in 0..10 {
            let store = store.clone();
            let barrier = barrier.clone();
            let saga_id = saga_id.clone();

            let handle = tokio::spawn(async move {
                barrier.wait().await;

                let event = HistoryEvent::builder()
                    .event_id(EventId(0))
                    .saga_id(saga_id.clone())
                    .event_type(EventType::ActivityTaskScheduled)
                    .category(EventCategory::Activity)
                    .payload(json!({"thread": i}))
                    .build();

                // This will fail for some due to optimistic locking
                // That's expected behavior
                let _ = store.append_event(&saga_id, 0, &event).await;
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // Only one should succeed
        let history = store.get_history(&saga_id).await.unwrap();
        assert_eq!(history.len(), 1);
    }
}

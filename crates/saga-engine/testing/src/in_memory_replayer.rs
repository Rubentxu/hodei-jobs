//! In-memory implementation of HistoryReplayer for testing.
//!
//! This module provides a simple, in-memory implementation of [`HistoryReplayer`]
//! for unit testing and development.

use saga_engine_core::event::{HistoryEvent, SagaId};
use saga_engine_core::port::replay::{HistoryReplayer, ReplayConfig, ReplayError, ReplayResult};
use std::fmt::Debug;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Simple state for testing replay operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestState {
    /// Current saga ID.
    pub saga_id: SagaId,

    /// Last event ID processed.
    pub last_event_id: u64,

    /// Number of events applied.
    pub event_count: usize,

    /// Current step number (if applicable).
    pub step: usize,
}

impl TestState {
    /// Create a new empty state.
    pub fn new(saga_id: SagaId) -> Self {
        Self {
            saga_id,
            last_event_id: 0,
            event_count: 0,
            step: 0,
        }
    }

    /// Apply an event to the state.
    pub fn apply_event(&mut self, event: &HistoryEvent) {
        self.last_event_id = event.event_id.0;
        self.event_count += 1;

        match event.event_type {
            saga_engine_core::event::EventType::ActivityTaskScheduled => {
                self.step += 1;
            }
            saga_engine_core::event::EventType::ActivityTaskCompleted => {}
            _ => {}
        }
    }
}

/// In-memory HistoryReplayer implementation for testing.
///
/// This implementation applies events sequentially to reconstruct state.
/// It's designed for unit testing and does not persist state.
#[derive(Debug, Clone)]
pub struct InMemoryReplayer {
    /// Performance tracking: total replay operations.
    replay_count: Arc<AtomicUsize>,
}

impl Default for InMemoryReplayer {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryReplayer {
    /// Create a new in-memory replayer.
    pub fn new() -> Self {
        Self {
            replay_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Reset replay counter.
    pub fn reset(&self) {
        self.replay_count.store(0, Ordering::SeqCst);
    }

    /// Get total number of replay operations.
    pub fn replay_count(&self) -> usize {
        self.replay_count.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl HistoryReplayer<TestState> for InMemoryReplayer {
    type Error = String;

    async fn replay(
        &self,
        mut state: TestState,
        events: &[HistoryEvent],
        config: Option<ReplayConfig>,
    ) -> Result<ReplayResult<TestState>, ReplayError<Self::Error>> {
        let config = config.unwrap_or_default();
        let replay_start = Instant::now();
        let mut events_replayed = 0;
        let mut last_event_id = 0;

        for (i, event) in events.iter().enumerate() {
            // Stop if we've reached the max events limit
            if config.max_events > 0 && events_replayed >= config.max_events {
                break;
            }

            if i > 0 {
                let expected_id = last_event_id + 1;
                if event.event_id.0 != expected_id {
                    return Err(ReplayError::invalid_sequence(expected_id, event.event_id.0));
                }
            }

            state.apply_event(event);
            last_event_id = event.event_id.0;
            events_replayed += 1;

            if events_replayed >= config.max_events && config.max_events > 0 {
                break;
            }
        }

        let replay_duration = replay_start.elapsed();
        self.replay_count.fetch_add(1, Ordering::SeqCst);

        Ok(ReplayResult {
            state,
            events_replayed,
            replay_duration,
            last_event_id,
        })
    }

    async fn replay_from_event_id(
        &self,
        _saga_id: &SagaId,
        _from_event_id: u64,
        _config: Option<ReplayConfig>,
    ) -> Result<ReplayResult<TestState>, ReplayError<Self::Error>> {
        let replay_start = Instant::now();

        Ok(ReplayResult {
            state: TestState::new(_saga_id.clone()),
            events_replayed: 0,
            replay_duration: replay_start.elapsed(),
            last_event_id: _from_event_id,
        })
    }

    async fn get_current_state(
        &self,
        saga_id: &SagaId,
        config: Option<ReplayConfig>,
    ) -> Result<ReplayResult<TestState>, ReplayError<Self::Error>> {
        let replay_start = Instant::now();
        let _config = config.unwrap_or_default();

        Ok(ReplayResult {
            state: TestState::new(saga_id.clone()),
            events_replayed: 0,
            replay_duration: replay_start.elapsed(),
            last_event_id: 0,
        })
    }

    async fn validate_sequence(
        &self,
        events: &[HistoryEvent],
        _allow_gaps: bool,
    ) -> Result<usize, ReplayError<Self::Error>> {
        let mut invalid_count = 0;
        let mut last_event_id: Option<u64> = None;

        for event in events {
            if let Some(last_id) = last_event_id {
                if event.event_id.0 != last_id + 1 {
                    invalid_count += 1;
                }
            }

            last_event_id = Some(event.event_id.0);
        }

        Ok(invalid_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saga_engine_core::event::{EventCategory, EventId, EventType};
    use serde_json::json;

    #[tokio::test]
    async fn test_in_memory_replayer_basic() {
        let replayer = InMemoryReplayer::new();
        let saga_id = SagaId("test-saga".to_string());

        let mut state = TestState::new(saga_id.clone());

        let events = vec![
            HistoryEvent::builder()
                .event_id(EventId(0))
                .saga_id(saga_id.clone())
                .event_type(EventType::ActivityTaskScheduled)
                .category(EventCategory::Activity)
                .payload(json!({"step": 1}))
                .build(),
            HistoryEvent::builder()
                .event_id(EventId(1))
                .saga_id(saga_id.clone())
                .event_type(EventType::ActivityTaskCompleted)
                .category(EventCategory::Activity)
                .payload(json!({"step": 1}))
                .build(),
        ];

        let result = replayer.replay(state, &events, None).await.unwrap();

        assert_eq!(result.events_replayed, 2);
        assert_eq!(result.state.event_count, 2);
        assert_eq!(replayer.replay_count(), 1);
    }

    #[tokio::test]
    async fn test_replay_invalid_sequence() {
        let replayer = InMemoryReplayer::new();
        let saga_id = SagaId("test-saga".to_string());
        let state = TestState::new(saga_id.clone());

        // Create events with IDs that are NOT sequential (0, 2 instead of 0, 1)
        let mut event1 = HistoryEvent::builder()
            .event_id(EventId(0))
            .saga_id(saga_id.clone())
            .event_type(EventType::ActivityTaskScheduled)
            .category(EventCategory::Activity)
            .payload(json!({}))
            .build();

        let mut event2 = HistoryEvent::builder()
            .event_id(EventId(2))
            .saga_id(saga_id.clone())
            .event_type(EventType::ActivityTaskCompleted)
            .category(EventCategory::Activity)
            .payload(json!({}))
            .build();

        // Manually set event IDs to create invalid sequence
        event1.event_id = EventId(0);
        event2.event_id = EventId(2); // Skip 1 - invalid sequence

        let events = vec![event1, event2];

        let result = replayer.replay(state, &events, None).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate_sequence_valid() {
        let replayer = InMemoryReplayer::new();
        let saga_id = SagaId("test-saga".to_string());

        let events = vec![
            HistoryEvent::builder()
                .event_id(EventId(0))
                .saga_id(saga_id.clone())
                .event_type(EventType::ActivityTaskScheduled)
                .category(EventCategory::Activity)
                .payload(json!({}))
                .build(),
            HistoryEvent::builder()
                .event_id(EventId(1))
                .saga_id(saga_id.clone())
                .event_type(EventType::ActivityTaskCompleted)
                .category(EventCategory::Activity)
                .payload(json!({}))
                .build(),
        ];

        let invalid = replayer.validate_sequence(&events, false).await.unwrap();
        assert_eq!(invalid, 0);
    }

    #[tokio::test]
    async fn test_validate_sequence_invalid() {
        let replayer = InMemoryReplayer::new();
        let saga_id = SagaId("test-saga".to_string());

        // Create events with IDs that are NOT sequential (0, 2 instead of 0, 1)
        let mut event1 = HistoryEvent::builder()
            .event_id(EventId(0))
            .saga_id(saga_id.clone())
            .event_type(EventType::ActivityTaskScheduled)
            .category(EventCategory::Activity)
            .payload(json!({}))
            .build();

        let mut event2 = HistoryEvent::builder()
            .event_id(EventId(2))
            .saga_id(saga_id.clone())
            .event_type(EventType::ActivityTaskCompleted)
            .category(EventCategory::Activity)
            .payload(json!({}))
            .build();

        // Manually set event IDs to create invalid sequence
        event1.event_id = EventId(0);
        event2.event_id = EventId(2); // Skip 1 - invalid sequence

        let events = vec![event1, event2];

        let invalid = replayer.validate_sequence(&events, false).await.unwrap();
        assert_eq!(invalid, 1);
    }

    #[tokio::test]
    async fn test_replay_max_events_limit() {
        let replayer = InMemoryReplayer::new();
        let saga_id = SagaId("test-saga".to_string());
        let state = TestState::new(saga_id.clone());

        let events = vec![
            HistoryEvent::builder()
                .event_id(EventId(0))
                .saga_id(saga_id.clone())
                .event_type(EventType::ActivityTaskScheduled)
                .category(EventCategory::Activity)
                .payload(json!({}))
                .build(),
            HistoryEvent::builder()
                .event_id(EventId(1))
                .saga_id(saga_id.clone())
                .event_type(EventType::ActivityTaskCompleted)
                .category(EventCategory::Activity)
                .payload(json!({}))
                .build(),
            HistoryEvent::builder()
                .event_id(EventId(2))
                .saga_id(saga_id.clone())
                .event_type(EventType::ActivityTaskScheduled)
                .category(EventCategory::Activity)
                .payload(json!({}))
                .build(),
        ];

        let config = ReplayConfig {
            max_events: 2,
            ..Default::default()
        };

        let result = replayer.replay(state, &events, Some(config)).await.unwrap();

        assert_eq!(result.events_replayed, 2);
        assert_eq!(replayer.replay_count(), 1);
    }

    #[tokio::test]
    async fn test_get_current_state() {
        let replayer = InMemoryReplayer::new();
        let saga_id = SagaId("test-saga".to_string());

        let result = replayer.get_current_state(&saga_id, None).await.unwrap();

        assert_eq!(result.state.saga_id, saga_id);
        assert_eq!(result.events_replayed, 0);
    }

    #[tokio::test]
    async fn test_replay_config_timeout() {
        let config = ReplayConfig {
            timeout: Duration::from_millis(100),
            ..Default::default()
        };

        assert_eq!(config.timeout, Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_replay_config_snapshots() {
        let config = ReplayConfig {
            use_snapshots: false,
            ..Default::default()
        };

        assert!(!config.use_snapshots);
    }
}

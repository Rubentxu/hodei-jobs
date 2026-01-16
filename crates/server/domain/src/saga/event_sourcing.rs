//! Saga Event Sourcing
//!
//! Provides an event sourcing layer for saga state management.
//! Uses internal saga events (not DomainEvent) for append-only audit log.

use crate::saga::{SagaContext, SagaId, SagaState, SagaType};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use uuid::Uuid;

pub const SAGA_EVENT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SagaEventId(pub Uuid);

impl SagaEventId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for SagaEventId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SagaEventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SagaEventType {
    Created,
    StepStarted,
    StepCompleted,
    StepFailed,
    CompensationStarted,
    CompensationCompleted,
    CompensationFailed,
    Completed,
    Failed,
    Cancelled,
    Persisted,
    Resumed,
}

impl fmt::Display for SagaEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SagaEventType::Created => write!(f, "CREATED"),
            SagaEventType::StepStarted => write!(f, "STEP_STARTED"),
            SagaEventType::StepCompleted => write!(f, "STEP_COMPLETED"),
            SagaEventType::StepFailed => write!(f, "STEP_FAILED"),
            SagaEventType::CompensationStarted => write!(f, "COMPENSATION_STARTED"),
            SagaEventType::CompensationCompleted => write!(f, "COMPENSATION_COMPLETED"),
            SagaEventType::CompensationFailed => write!(f, "COMPENSATION_FAILED"),
            SagaEventType::Completed => write!(f, "COMPLETED"),
            SagaEventType::Failed => write!(f, "FAILED"),
            SagaEventType::Cancelled => write!(f, "CANCELLED"),
            SagaEventType::Persisted => write!(f, "PERSISTED"),
            SagaEventType::Resumed => write!(f, "RESUMED"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaEvent {
    pub event_id: SagaEventId,
    pub saga_id: SagaId,
    pub saga_type: SagaType,
    pub event_type: SagaEventType,
    pub version: u32,
    pub step_name: Option<String>,
    pub step_number: Option<u32>,
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
    pub persisted_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
    pub error_message: Option<String>,
    pub sequence_number: u64,
}

impl SagaEvent {
    pub fn created(
        saga_id: SagaId,
        saga_type: SagaType,
        context: &SagaContext,
        payload: &serde_json::Value,
    ) -> Self {
        Self {
            event_id: SagaEventId::new(),
            saga_id,
            saga_type,
            event_type: SagaEventType::Created,
            version: SAGA_EVENT_VERSION,
            step_name: None,
            step_number: None,
            payload: payload.clone(),
            occurred_at: Utc::now(),
            persisted_at: Utc::now(),
            correlation_id: context.correlation_id.clone(),
            actor: context.actor.clone(),
            error_message: None,
            sequence_number: 0,
        }
    }

    pub fn step_started(
        saga_id: SagaId,
        saga_type: SagaType,
        step_name: &str,
        step_number: u32,
        context: &SagaContext,
    ) -> Self {
        Self {
            event_id: SagaEventId::new(),
            saga_id,
            saga_type,
            event_type: SagaEventType::StepStarted,
            version: SAGA_EVENT_VERSION,
            step_name: Some(step_name.to_string()),
            step_number: Some(step_number),
            payload: serde_json::json!({}),
            occurred_at: Utc::now(),
            persisted_at: Utc::now(),
            correlation_id: context.correlation_id.clone(),
            actor: context.actor.clone(),
            error_message: None,
            sequence_number: step_number as u64,
        }
    }

    pub fn step_completed(
        saga_id: SagaId,
        saga_type: SagaType,
        step_name: &str,
        step_number: u32,
        output: &serde_json::Value,
        context: &SagaContext,
    ) -> Self {
        Self {
            event_id: SagaEventId::new(),
            saga_id,
            saga_type,
            event_type: SagaEventType::StepCompleted,
            version: SAGA_EVENT_VERSION,
            step_name: Some(step_name.to_string()),
            step_number: Some(step_number),
            payload: output.clone(),
            occurred_at: Utc::now(),
            persisted_at: Utc::now(),
            correlation_id: context.correlation_id.clone(),
            actor: context.actor.clone(),
            error_message: None,
            sequence_number: step_number as u64,
        }
    }

    pub fn step_failed(
        saga_id: SagaId,
        saga_type: SagaType,
        step_name: &str,
        step_number: u32,
        error_message: &str,
        context: &SagaContext,
    ) -> Self {
        Self {
            event_id: SagaEventId::new(),
            saga_id,
            saga_type,
            event_type: SagaEventType::StepFailed,
            version: SAGA_EVENT_VERSION,
            step_name: Some(step_name.to_string()),
            step_number: Some(step_number),
            payload: serde_json::json!({}),
            occurred_at: Utc::now(),
            persisted_at: Utc::now(),
            correlation_id: context.correlation_id.clone(),
            actor: context.actor.clone(),
            error_message: Some(error_message.to_string()),
            sequence_number: step_number as u64,
        }
    }

    pub fn completed(
        saga_id: SagaId,
        saga_type: SagaType,
        steps_executed: u32,
        context: &SagaContext,
    ) -> Self {
        Self {
            event_id: SagaEventId::new(),
            saga_id,
            saga_type,
            event_type: SagaEventType::Completed,
            version: SAGA_EVENT_VERSION,
            step_name: None,
            step_number: None,
            payload: serde_json::json!({ "steps_executed": steps_executed }),
            occurred_at: Utc::now(),
            persisted_at: Utc::now(),
            correlation_id: context.correlation_id.clone(),
            actor: context.actor.clone(),
            error_message: None,
            sequence_number: u64::MAX,
        }
    }

    pub fn failed(
        saga_id: SagaId,
        saga_type: SagaType,
        steps_executed: u32,
        error_message: &str,
        context: &SagaContext,
    ) -> Self {
        Self {
            event_id: SagaEventId::new(),
            saga_id,
            saga_type,
            event_type: SagaEventType::Failed,
            version: SAGA_EVENT_VERSION,
            step_name: None,
            step_number: None,
            payload: serde_json::json!({ "steps_executed": steps_executed }),
            occurred_at: Utc::now(),
            persisted_at: Utc::now(),
            correlation_id: context.correlation_id.clone(),
            actor: context.actor.clone(),
            error_message: Some(error_message.to_string()),
            sequence_number: u64::MAX,
        }
    }

    pub fn cancelled(
        saga_id: SagaId,
        saga_type: SagaType,
        steps_executed: u32,
        compensations_executed: u32,
        context: &SagaContext,
    ) -> Self {
        Self {
            event_id: SagaEventId::new(),
            saga_id,
            saga_type,
            event_type: SagaEventType::Cancelled,
            version: SAGA_EVENT_VERSION,
            step_name: None,
            step_number: None,
            payload: serde_json::json!({
                "steps_executed": steps_executed,
                "compensations_executed": compensations_executed
            }),
            occurred_at: Utc::now(),
            persisted_at: Utc::now(),
            correlation_id: context.correlation_id.clone(),
            actor: context.actor.clone(),
            error_message: None,
            sequence_number: u64::MAX,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventSourcedSagaState {
    pub saga_id: SagaId,
    pub saga_type: SagaType,
    pub state: SagaState,
    pub current_step: u32,
    pub is_compensating: bool,
    pub steps_executed: u32,
    pub compensations_executed: u32,
    pub started_at: DateTime<Utc>,
    pub last_event_at: DateTime<Utc>,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
    pub error_message: Option<String>,
    pub step_outputs: HashMap<String, serde_json::Value>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub version: u64,
    pub event_count: u64,
}

impl EventSourcedSagaState {
    pub fn from_context(context: &SagaContext) -> Self {
        Self {
            saga_id: context.saga_id.clone(),
            saga_type: context.saga_type,
            state: SagaState::Pending,
            current_step: 0,
            is_compensating: false,
            steps_executed: 0,
            compensations_executed: 0,
            started_at: context.started_at,
            last_event_at: Utc::now(),
            correlation_id: context.correlation_id.clone(),
            actor: context.actor.clone(),
            error_message: None,
            step_outputs: HashMap::new(),
            metadata: context.metadata.clone(),
            version: context.version,
            event_count: 0,
        }
    }

    pub fn apply(&self, event: &SagaEvent) -> Self {
        let mut new_state = self.clone();

        match event.event_type {
            SagaEventType::Created => {
                new_state.state = SagaState::Pending;
                new_state.current_step = 0;
                new_state.steps_executed = 0;
                new_state.compensations_executed = 0;
            }
            SagaEventType::StepStarted => {
                new_state.state = SagaState::InProgress;
                new_state.current_step = event.step_number.unwrap_or(0);
                new_state.steps_executed += 1;
            }
            SagaEventType::StepCompleted => {
                new_state.current_step += 1;
                if let Some(step_name) = &event.step_name {
                    new_state
                        .step_outputs
                        .insert(step_name.clone(), event.payload.clone());
                }
            }
            SagaEventType::StepFailed => {
                new_state.state = SagaState::Compensating;
                new_state.is_compensating = true;
                new_state.error_message = event.error_message.clone();
            }
            SagaEventType::CompensationStarted => {
                new_state.is_compensating = true;
            }
            SagaEventType::CompensationCompleted => {
                new_state.compensations_executed += 1;
            }
            SagaEventType::CompensationFailed => {
                new_state.state = SagaState::Failed;
            }
            SagaEventType::Completed => {
                new_state.state = SagaState::Completed;
                new_state.is_compensating = false;
            }
            SagaEventType::Failed => {
                new_state.state = SagaState::Failed;
                new_state.is_compensating = false;
            }
            SagaEventType::Cancelled => {
                new_state.state = SagaState::Cancelled;
                new_state.is_compensating = false;
            }
            SagaEventType::Persisted | SagaEventType::Resumed => {}
        }

        new_state.last_event_at = event.occurred_at;
        new_state.event_count += 1;
        new_state.version += 1;

        new_state
    }

    pub fn from_events(events: &[SagaEvent]) -> Option<Self> {
        if events.is_empty() {
            return None;
        }

        let mut sorted_events = events.to_vec();
        sorted_events.sort_by_key(|e| e.sequence_number);

        let initial_event = sorted_events.first().unwrap();
        let mut state = Self {
            saga_id: initial_event.saga_id.clone(),
            saga_type: initial_event.saga_type,
            state: SagaState::Pending,
            current_step: 0,
            is_compensating: false,
            steps_executed: 0,
            compensations_executed: 0,
            started_at: initial_event.occurred_at,
            last_event_at: initial_event.occurred_at,
            correlation_id: initial_event.correlation_id.clone(),
            actor: initial_event.actor.clone(),
            error_message: None,
            step_outputs: HashMap::new(),
            metadata: HashMap::new(),
            version: 0,
            event_count: 0,
        };

        for event in &sorted_events {
            state = state.apply(event);
        }

        Some(state)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            SagaState::Completed | SagaState::Failed | SagaState::Cancelled
        )
    }
}

#[async_trait::async_trait]
pub trait SagaEventStore: Send + Sync {
    async fn append_event(&self, event: &SagaEvent) -> Result<(), SagaEventStoreError>;
    async fn append_events(&self, events: &[SagaEvent]) -> Result<(), SagaEventStoreError>;
    async fn get_events(&self, saga_id: &SagaId) -> Result<Vec<SagaEvent>, SagaEventStoreError>;
    async fn get_events_since(
        &self,
        saga_id: &SagaId,
        sequence_number: u64,
    ) -> Result<Vec<SagaEvent>, SagaEventStoreError>;
    async fn get_latest_event(
        &self,
        saga_id: &SagaId,
    ) -> Result<Option<SagaEvent>, SagaEventStoreError>;
    async fn get_event_count(&self, saga_id: &SagaId) -> Result<u64, SagaEventStoreError>;
    async fn delete_events_older_than(
        &self,
        older_than: DateTime<Utc>,
    ) -> Result<u64, SagaEventStoreError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SagaEventStoreError {
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Persistence error: {0}")]
    PersistenceError(String),
    #[error("Saga not found: {saga_id}")]
    SagaNotFound { saga_id: SagaId },
    #[error("Concurrency conflict: saga {saga_id} was modified")]
    ConcurrencyConflict { saga_id: SagaId },
    #[error("Invalid event sequence: expected {expected}, got {actual}")]
    InvalidSequence { expected: u64, actual: u64 },
}

#[derive(Debug, Default)]
pub struct InMemorySagaEventStore {
    events: Arc<parking_lot::RwLock<HashMap<SagaId, Vec<SagaEvent>>>>,
}

impl InMemorySagaEventStore {
    pub fn new() -> Self {
        Self {
            events: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        }
    }

    pub fn clear(&self) {
        let mut events = self.events.write();
        events.clear();
    }
}

#[async_trait::async_trait]
impl SagaEventStore for InMemorySagaEventStore {
    async fn append_event(&self, event: &SagaEvent) -> Result<(), SagaEventStoreError> {
        let mut events = self.events.write();
        let saga_events = events.entry(event.saga_id.clone()).or_default();

        if let Some(last) = saga_events.last()
            && event.sequence_number <= last.sequence_number {
                return Err(SagaEventStoreError::InvalidSequence {
                    expected: last.sequence_number + 1,
                    actual: event.sequence_number,
                });
            }

        saga_events.push(event.clone());
        Ok(())
    }

    async fn append_events(&self, events: &[SagaEvent]) -> Result<(), SagaEventStoreError> {
        for event in events {
            self.append_event(event).await?;
        }
        Ok(())
    }

    async fn get_events(&self, saga_id: &SagaId) -> Result<Vec<SagaEvent>, SagaEventStoreError> {
        let events = self.events.read();
        Ok(events.get(saga_id).cloned().unwrap_or_default())
    }

    async fn get_events_since(
        &self,
        saga_id: &SagaId,
        sequence_number: u64,
    ) -> Result<Vec<SagaEvent>, SagaEventStoreError> {
        let events = self.events.read();
        if let Some(saga_events) = events.get(saga_id) {
            let filtered: Vec<SagaEvent> = saga_events
                .iter()
                .filter(|e| e.sequence_number > sequence_number)
                .cloned()
                .collect();
            Ok(filtered)
        } else {
            Ok(Vec::new())
        }
    }

    async fn get_latest_event(
        &self,
        saga_id: &SagaId,
    ) -> Result<Option<SagaEvent>, SagaEventStoreError> {
        let events = self.events.read();
        Ok(events.get(saga_id).and_then(|es| es.last().cloned()))
    }

    async fn get_event_count(&self, saga_id: &SagaId) -> Result<u64, SagaEventStoreError> {
        let events = self.events.read();
        Ok(events.get(saga_id).map(|es| es.len() as u64).unwrap_or(0))
    }

    async fn delete_events_older_than(
        &self,
        _older_than: DateTime<Utc>,
    ) -> Result<u64, SagaEventStoreError> {
        Ok(0)
    }
}

#[derive(Default)]
pub struct EventSourcingBuilder {
    event_store: Option<Arc<dyn SagaEventStore>>,
}


impl EventSourcingBuilder {
    pub fn with_event_store(mut self, event_store: Arc<dyn SagaEventStore>) -> Self {
        self.event_store = Some(event_store);
        self
    }

    pub fn build(self) -> EventSourcingLayer {
        EventSourcingLayer {
            event_store: self.event_store,
        }
    }
}

pub struct EventSourcingLayer {
    event_store: Option<Arc<dyn SagaEventStore>>,
}

impl EventSourcingLayer {
    pub fn new(event_store: Arc<dyn SagaEventStore>) -> Self {
        Self {
            event_store: Some(event_store),
        }
    }

    pub fn event_store(&self) -> Option<&Arc<dyn SagaEventStore>> {
        self.event_store.as_ref()
    }

    pub async fn record_created(
        &self,
        saga_id: &SagaId,
        saga_type: SagaType,
        context: &SagaContext,
    ) -> Result<(), SagaEventStoreError> {
        if let Some(store) = &self.event_store {
            let payload = serde_json::json!({
                "saga_type": saga_type.as_str(),
                "metadata_keys": context.metadata.keys().collect::<Vec<_>>()
            });
            let event = SagaEvent::created(saga_id.clone(), saga_type, context, &payload);
            store.append_event(&event).await?;
        }
        Ok(())
    }

    pub async fn record_step_started(
        &self,
        saga_id: &SagaId,
        saga_type: SagaType,
        step_name: &str,
        step_number: u32,
        context: &SagaContext,
    ) -> Result<(), SagaEventStoreError> {
        if let Some(store) = &self.event_store {
            let event = SagaEvent::step_started(
                saga_id.clone(),
                saga_type,
                step_name,
                step_number,
                context,
            );
            store.append_event(&event).await?;
        }
        Ok(())
    }

    pub async fn record_step_completed(
        &self,
        saga_id: &SagaId,
        saga_type: SagaType,
        step_name: &str,
        step_number: u32,
        output: &serde_json::Value,
        context: &SagaContext,
    ) -> Result<(), SagaEventStoreError> {
        if let Some(store) = &self.event_store {
            let event = SagaEvent::step_completed(
                saga_id.clone(),
                saga_type,
                step_name,
                step_number,
                output,
                context,
            );
            store.append_event(&event).await?;
        }
        Ok(())
    }

    pub async fn record_step_failed(
        &self,
        saga_id: &SagaId,
        saga_type: SagaType,
        step_name: &str,
        step_number: u32,
        error_message: &str,
        context: &SagaContext,
    ) -> Result<(), SagaEventStoreError> {
        if let Some(store) = &self.event_store {
            let event = SagaEvent::step_failed(
                saga_id.clone(),
                saga_type,
                step_name,
                step_number,
                error_message,
                context,
            );
            store.append_event(&event).await?;
        }
        Ok(())
    }

    pub async fn record_completed(
        &self,
        saga_id: &SagaId,
        saga_type: SagaType,
        steps_executed: u32,
        context: &SagaContext,
    ) -> Result<(), SagaEventStoreError> {
        if let Some(store) = &self.event_store {
            let event = SagaEvent::completed(saga_id.clone(), saga_type, steps_executed, context);
            store.append_event(&event).await?;
        }
        Ok(())
    }

    pub async fn record_failed(
        &self,
        saga_id: &SagaId,
        saga_type: SagaType,
        steps_executed: u32,
        error_message: &str,
        context: &SagaContext,
    ) -> Result<(), SagaEventStoreError> {
        if let Some(store) = &self.event_store {
            let event = SagaEvent::failed(
                saga_id.clone(),
                saga_type,
                steps_executed,
                error_message,
                context,
            );
            store.append_event(&event).await?;
        }
        Ok(())
    }

    pub async fn reconstruct_state(
        &self,
        saga_id: &SagaId,
    ) -> Result<Option<EventSourcedSagaState>, SagaEventStoreError> {
        if let Some(store) = &self.event_store {
            let events = store.get_events(saga_id).await?;
            Ok(EventSourcedSagaState::from_events(&events))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saga::{SagaContext, SagaId, SagaType};

    #[tokio::test]
    async fn test_event_sourcing_created() {
        let store = Arc::new(InMemorySagaEventStore::new());
        let layer = EventSourcingLayer::new(store.clone());

        let saga_id = SagaId::new();
        let context = SagaContext::new(
            saga_id.clone(),
            SagaType::Execution,
            Some("test-correlation".to_string()),
            Some("test-actor".to_string()),
        );

        layer
            .record_created(&saga_id, SagaType::Execution, &context)
            .await
            .unwrap();

        let events = store.get_events(&saga_id).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, SagaEventType::Created);
    }

    #[tokio::test]
    async fn test_state_reconstruction() {
        let saga_id = SagaId::new();
        let context = SagaContext::new(
            saga_id.clone(),
            SagaType::Execution,
            Some("test-correlation".to_string()),
            Some("test-actor".to_string()),
        );

        let created = SagaEvent::created(
            saga_id.clone(),
            SagaType::Execution,
            &context,
            &serde_json::json!({}),
        );
        let step1_started =
            SagaEvent::step_started(saga_id.clone(), SagaType::Execution, "Step1", 0, &context);
        let step1_completed = SagaEvent::step_completed(
            saga_id.clone(),
            SagaType::Execution,
            "Step1",
            0,
            &serde_json::json!({"result": "ok"}),
            &context,
        );
        let completed = SagaEvent::completed(saga_id.clone(), SagaType::Execution, 1, &context);

        let state = EventSourcedSagaState::from_events(&[
            created,
            step1_started,
            step1_completed,
            completed,
        ]);

        assert!(state.is_some());
        let state = state.unwrap();
        assert_eq!(state.state, SagaState::Completed);
        assert_eq!(state.steps_executed, 1);
    }

    #[tokio::test]
    async fn test_in_memory_event_store() {
        let store = InMemorySagaEventStore::new();

        let saga_id = SagaId::new();
        let context = SagaContext::new(saga_id.clone(), SagaType::Provisioning, None, None);

        let event = SagaEvent::created(
            saga_id.clone(),
            SagaType::Provisioning,
            &context,
            &serde_json::json!({}),
        );
        store.append_event(&event).await.unwrap();

        let events = store.get_events(&saga_id).await.unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_event_sourced_state_from_empty_events() {
        let state = EventSourcedSagaState::from_events(&[]);
        assert!(state.is_none());
    }

    #[test]
    fn test_event_sourced_state_terminal_check() {
        let saga_id = SagaId::new();
        let context = SagaContext::new(saga_id.clone(), SagaType::Execution, None, None);

        let completed_event =
            SagaEvent::completed(saga_id.clone(), SagaType::Execution, 1, &context);
        let state = EventSourcedSagaState::from_events(&[completed_event]).unwrap();

        assert!(state.is_terminal());
        assert_eq!(state.state, SagaState::Completed);
    }
}

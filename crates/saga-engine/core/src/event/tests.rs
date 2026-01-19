//! Tests for HistoryEvent

use crate::codec::{BincodeCodec, EventCodec, JsonCodec};
use crate::event::{EventCategory, EventId, EventType, HistoryEvent, SagaId};
use serde_json::json;

#[test]
fn test_history_event_creation() {
    let event = HistoryEvent::new(
        SagaId("test-saga-id".to_string()),
        EventType::WorkflowExecutionStarted,
        EventCategory::Workflow,
        json!({"key": "value"}),
    );

    assert_eq!(event.saga_id.0, "test-saga-id");
    assert_eq!(event.event_type, EventType::WorkflowExecutionStarted);
    assert_eq!(event.category, EventCategory::Workflow);
    assert_eq!(event.event_version, 1);
    assert!(!event.is_reset_point);
    assert!(!event.is_retry);
}

#[test]
fn test_history_event_with_optional_fields() {
    let event = HistoryEvent::builder()
        .saga_id(SagaId("test-saga".to_string()))
        .event_type(EventType::ActivityTaskCompleted)
        .category(EventCategory::Activity)
        .payload(json!({"result": "ok"}))
        .parent_event_id(EventId(5))
        .task_queue("test-queue".to_string())
        .trace_id("trace-123".to_string())
        .is_reset_point(true)
        .is_retry(true)
        .build();

    assert_eq!(event.parent_event_id, Some(EventId(5)));
    assert_eq!(event.task_queue, Some("test-queue".to_string()));
    assert_eq!(event.trace_id, Some("trace-123".to_string()));
    assert!(event.is_reset_point);
    assert!(event.is_retry);
}

#[test]
fn test_event_id_monotonic_increment() {
    let event1 = HistoryEvent::new(
        SagaId("saga-1".to_string()),
        EventType::WorkflowExecutionStarted,
        EventCategory::Workflow,
        json!({}),
    );

    let event2 = HistoryEvent::new(
        SagaId("saga-1".to_string()),
        EventType::ActivityTaskScheduled,
        EventCategory::Activity,
        json!({}),
    );

    // Events have sequential IDs
    assert!(event2.event_id.0 > event1.event_id.0);
}

#[test]
fn test_different_sagas_have_independent_ids() {
    let _event1 = HistoryEvent::new(
        SagaId("saga-a".to_string()),
        EventType::WorkflowExecutionStarted,
        EventCategory::Workflow,
        json!({}),
    );

    let _event2 = HistoryEvent::new(
        SagaId("saga-b".to_string()),
        EventType::WorkflowExecutionStarted,
        EventCategory::Workflow,
        json!({}),
    );

    // Different sagas can have the same event_id
    // (event_id is monotonic PER saga, not global)
}

#[test]
fn test_history_event_serialization_roundtrip() {
    let codec = JsonCodec::new();

    let original = HistoryEvent::builder()
        .saga_id(SagaId("test-saga".to_string()))
        .event_type(EventType::TimerCreated)
        .category(EventCategory::Timer)
        .payload(json!({"fire_at": "2024-01-01T00:00:00Z"}))
        .build();

    let encoded = codec.encode(&original).unwrap();
    let decoded: HistoryEvent = codec.decode(&encoded).unwrap();

    assert_eq!(original.saga_id, decoded.saga_id);
    assert_eq!(original.event_type, decoded.event_type);
    assert_eq!(original.category, decoded.category);
    assert_eq!(original.event_id.0, decoded.event_id.0);
}

#[test]
fn test_history_event_timestamp_is_set() {
    let before = chrono::Utc::now();
    let event = HistoryEvent::new(
        SagaId("test".to_string()),
        EventType::WorkflowExecutionStarted,
        EventCategory::Workflow,
        json!({}),
    );
    let after = chrono::Utc::now();

    assert!(event.timestamp >= before);
    assert!(event.timestamp <= after);
}

#[test]
fn test_event_builder_default_values() {
    let event = HistoryEvent::builder()
        .saga_id(SagaId("test".to_string()))
        .event_type(EventType::WorkflowExecutionCompleted)
        .category(EventCategory::Workflow)
        .payload(json!({}))
        .build();

    assert_eq!(event.event_version, 1);
    assert!(!event.is_reset_point);
    assert!(!event.is_retry);
    assert_eq!(event.parent_event_id, None);
    assert_eq!(event.task_queue, None);
    assert_eq!(event.trace_id, None);
}

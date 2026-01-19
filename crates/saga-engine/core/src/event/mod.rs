//! Event types for the saga engine.
//!
//! This module contains [`HistoryEvent`], [`EventType`], and [`EventCategory`]
//! which form the foundation of the event sourcing architecture.

pub mod tests;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Unique event ID type.
///
/// Event IDs are monotonically increasing u64 values, local to each saga.
/// This allows for horizontal scaling without coordination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EventId(pub u64);

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Saga identifier type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SagaId(pub String);

impl SagaId {
    /// Create a new saga ID from a UUID string.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid.to_string())
    }

    /// Generate a new random saga ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for SagaId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SagaId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Category of events for filtering and organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventCategory {
    /// Workflow execution lifecycle events.
    Workflow,
    /// Activity task lifecycle events.
    Activity,
    /// Timer events (created, fired, canceled).
    Timer,
    /// External signal events.
    Signal,
    /// User-defined marker events.
    Marker,
    /// Snapshot events for state reconstruction.
    Snapshot,
    /// Command issuance and completion events.
    Command,
}

impl EventCategory {
    /// Returns true if this is a workflow event.
    pub fn is_workflow(&self) -> bool {
        matches!(self, EventCategory::Workflow)
    }

    /// Returns true if this is an activity event.
    pub fn is_activity(&self) -> bool {
        matches!(self, EventCategory::Activity)
    }

    /// Returns true if this is a timer event.
    pub fn is_timer(&self) -> bool {
        matches!(self, EventCategory::Timer)
    }

    /// Returns true if this is a signal event.
    pub fn is_signal(&self) -> bool {
        matches!(self, EventCategory::Signal)
    }
}

/// Type of events in the saga history.
///
/// Each event type represents a specific occurrence in the saga lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    // Workflow Events
    /// Workflow execution has started.
    WorkflowExecutionStarted,
    /// Workflow execution completed successfully.
    WorkflowExecutionCompleted,
    /// Workflow execution failed with an error.
    WorkflowExecutionFailed,
    /// Workflow execution timed out.
    WorkflowExecutionTimedOut,
    /// Workflow execution was canceled.
    WorkflowExecutionCanceled,

    // Activity Events
    /// An activity task has been scheduled.
    ActivityTaskScheduled,
    /// Activity task has started execution.
    ActivityTaskStarted,
    /// Activity task completed successfully.
    ActivityTaskCompleted,
    /// Activity task failed with an error.
    ActivityTaskFailed,
    /// Activity task timed out.
    ActivityTaskTimedOut,
    /// Activity task was canceled.
    ActivityTaskCanceled,

    // Timer Events
    /// A timer has been created.
    TimerCreated,
    /// A timer has fired (deadline reached).
    TimerFired,
    /// A timer has been canceled.
    TimerCanceled,

    // Signal Events
    /// An external signal has been received.
    SignalReceived,

    // Marker Events
    /// A user-defined marker has been recorded.
    MarkerRecorded,

    // Snapshot Events
    /// A snapshot of the saga state has been created.
    SnapshotCreated,

    // Command Events
    /// A command has been issued to an external system.
    CommandIssued,
    /// A command has completed successfully.
    CommandCompleted,
    /// A command has failed.
    CommandFailed,
}

impl EventType {
    /// Returns the category for this event type.
    pub fn category(&self) -> EventCategory {
        match self {
            // Workflow events
            EventType::WorkflowExecutionStarted
            | EventType::WorkflowExecutionCompleted
            | EventType::WorkflowExecutionFailed
            | EventType::WorkflowExecutionTimedOut
            | EventType::WorkflowExecutionCanceled => EventCategory::Workflow,

            // Activity events
            EventType::ActivityTaskScheduled
            | EventType::ActivityTaskStarted
            | EventType::ActivityTaskCompleted
            | EventType::ActivityTaskFailed
            | EventType::ActivityTaskTimedOut
            | EventType::ActivityTaskCanceled => EventCategory::Activity,

            // Timer events
            EventType::TimerCreated | EventType::TimerFired | EventType::TimerCanceled => {
                EventCategory::Timer
            }

            // Signal events
            EventType::SignalReceived => EventCategory::Signal,

            // Marker events
            EventType::MarkerRecorded => EventCategory::Marker,

            // Snapshot events
            EventType::SnapshotCreated => EventCategory::Snapshot,

            // Command events
            EventType::CommandIssued | EventType::CommandCompleted | EventType::CommandFailed => {
                EventCategory::Command
            }
        }
    }
}

impl std::str::FromStr for EventCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "workflow" => Ok(EventCategory::Workflow),
            "activity" => Ok(EventCategory::Activity),
            "timer" => Ok(EventCategory::Timer),
            "signal" => Ok(EventCategory::Signal),
            "marker" => Ok(EventCategory::Marker),
            "snapshot" => Ok(EventCategory::Snapshot),
            "command" => Ok(EventCategory::Command),
            _ => Err(format!("Unknown event category: {}", s)),
        }
    }
}

impl std::str::FromStr for EventType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            // Workflow events
            "workflowexecutionstarted" => Ok(EventType::WorkflowExecutionStarted),
            "workflowexecutioncompleted" => Ok(EventType::WorkflowExecutionCompleted),
            "workflowexecutionfailed" => Ok(EventType::WorkflowExecutionFailed),
            "workflowexecutiontimedout" => Ok(EventType::WorkflowExecutionTimedOut),
            "workflowexecutioncanceled" => Ok(EventType::WorkflowExecutionCanceled),

            // Activity events
            "activitytaskscheduled" => Ok(EventType::ActivityTaskScheduled),
            "activitytaskstarted" => Ok(EventType::ActivityTaskStarted),
            "activitytaskcompleted" => Ok(EventType::ActivityTaskCompleted),
            "activitytaskfailed" => Ok(EventType::ActivityTaskFailed),
            "activitytasktimedout" => Ok(EventType::ActivityTaskTimedOut),
            "activitytaskcanceled" => Ok(EventType::ActivityTaskCanceled),

            // Timer events
            "timercreated" => Ok(EventType::TimerCreated),
            "timerfired" => Ok(EventType::TimerFired),
            "timercanceled" => Ok(EventType::TimerCanceled),

            // Signal events
            "signalreceived" => Ok(EventType::SignalReceived),

            // Marker events
            "markerrecorded" => Ok(EventType::MarkerRecorded),

            // Snapshot events
            "snapshotcreated" => Ok(EventType::SnapshotCreated),

            // Command events
            "commandissued" => Ok(EventType::CommandIssued),
            "commandcompleted" => Ok(EventType::CommandCompleted),
            "commandfailed" => Ok(EventType::CommandFailed),

            _ => Err(format!("Unknown event type: {}", s)),
        }
    }
}

/// A single event in the saga history.
///
/// HistoryEvent is the fundamental unit of the event sourcing architecture.
/// Each event represents something that happened in the saga and is immutable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEvent {
    /// Unique identifier for this event within the saga (monotonic, local).
    pub event_id: EventId,

    /// ID of the saga this event belongs to.
    pub saga_id: SagaId,

    /// Type of the event.
    pub event_type: EventType,

    /// Category of the event for filtering.
    pub category: EventCategory,

    /// Timestamp when the event was created.
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Arbitrary JSON payload for event-specific data.
    pub attributes: Value,

    /// Version of the event schema (for migrations).
    pub event_version: u32,

    /// Whether this event is a reset point for replay.
    pub is_reset_point: bool,

    /// Whether this event is a retry of a previous failed event.
    pub is_retry: bool,

    /// Parent event ID (for event trees/graphs).
    pub parent_event_id: Option<EventId>,

    /// Task queue for activity routing.
    pub task_queue: Option<String>,

    /// Distributed trace ID for observability.
    pub trace_id: Option<String>,
}

impl HistoryEvent {
    /// Create a new HistoryEvent with minimal fields.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The ID of the saga this event belongs to.
    /// * `event_type` - The type of the event.
    /// * `category` - The category of the event.
    /// * `attributes` - The event payload.
    pub fn new(
        saga_id: SagaId,
        event_type: EventType,
        category: EventCategory,
        attributes: Value,
    ) -> Self {
        Self::builder()
            .saga_id(saga_id)
            .event_type(event_type)
            .category(category)
            .payload(attributes)
            .build()
    }

    /// Create a builder for constructing HistoryEvent.
    pub fn builder() -> HistoryEventBuilder {
        HistoryEventBuilder::new()
    }
}

/// Builder for constructing HistoryEvent.
#[derive(Debug, Default)]
pub struct HistoryEventBuilder {
    saga_id: Option<SagaId>,
    event_type: Option<EventType>,
    category: Option<EventCategory>,
    attributes: Option<Value>,
    parent_event_id: Option<EventId>,
    task_queue: Option<String>,
    trace_id: Option<String>,
    is_reset_point: bool,
    is_retry: bool,
}

impl HistoryEventBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the saga ID.
    pub fn saga_id(mut self, saga_id: SagaId) -> Self {
        self.saga_id = Some(saga_id);
        self
    }

    /// Set the event type.
    pub fn event_type(mut self, event_type: EventType) -> Self {
        self.event_type = Some(event_type);
        self
    }

    /// Set the category (can be derived from event_type).
    pub fn category(mut self, category: EventCategory) -> Self {
        self.category = Some(category);
        self
    }

    /// Set the event payload.
    pub fn payload(mut self, attributes: Value) -> Self {
        self.attributes = Some(attributes);
        self
    }

    /// Set the parent event ID.
    pub fn parent_event_id(mut self, parent_event_id: EventId) -> Self {
        self.parent_event_id = Some(parent_event_id);
        self
    }

    /// Set the task queue.
    pub fn task_queue(mut self, task_queue: String) -> Self {
        self.task_queue = Some(task_queue);
        self
    }

    /// Set the trace ID.
    pub fn trace_id(mut self, trace_id: String) -> Self {
        self.trace_id = Some(trace_id);
        self
    }

    /// Set the reset point flag.
    pub fn is_reset_point(mut self, is_reset_point: bool) -> Self {
        self.is_reset_point = is_reset_point;
        self
    }

    /// Set the retry flag.
    pub fn is_retry(mut self, is_retry: bool) -> Self {
        self.is_retry = is_retry;
        self
    }

    /// Build the HistoryEvent.
    ///
    /// # Panics
    ///
    /// Panics if required fields are not set.
    pub fn build(self) -> HistoryEvent {
        let saga_id = self.saga_id.expect("saga_id is required");
        let event_type = self.event_type.expect("event_type is required");
        let attributes = self.attributes.expect("attributes is required");

        let category = self.category.unwrap_or_else(|| event_type.category());

        // Get next event ID (monotonic counter per saga)
        let event_id = EventId(next_event_id());

        HistoryEvent {
            event_id,
            saga_id,
            event_type,
            category,
            timestamp: chrono::Utc::now(),
            attributes,
            event_version: CURRENT_EVENT_VERSION,
            is_reset_point: self.is_reset_point,
            is_retry: self.is_retry,
            parent_event_id: self.parent_event_id,
            task_queue: self.task_queue,
            trace_id: self.trace_id,
        }
    }
}

/// Current event schema version.
pub const CURRENT_EVENT_VERSION: u32 = 1;

static EVENT_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn next_event_id() -> u64 {
    EVENT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

/// Reset the event ID generator.
///
/// This is primarily for testing purposes to ensure consistent event IDs
/// across test runs. Not thread-safe - only use in single-threaded contexts.
pub fn reset_event_id_generator() {
    EVENT_COUNTER.store(0, std::sync::atomic::Ordering::SeqCst);
}

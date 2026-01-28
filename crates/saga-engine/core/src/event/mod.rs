//! Event types for the saga engine.
//!
//! This module contains [`HistoryEvent`], [`EventType`], and [`EventCategory`]
//! which form the foundation of the event sourcing architecture.
//!
//! Event types are organized into modules by category:
//! - [`workflow`](workflow) - Workflow execution events
//! - [`activity`](activity) - Activity task events
//! - [`timer`](timer) - Timer events
//! - [`signal`](signal) - Signal events
//! - [`marker`](marker) - Marker events
//! - [`snapshot`](snapshot) - Snapshot events
//! - [`command`](command) - Command events
//! - [`child_workflow`](child_workflow) - Child workflow events
//! - [`local_activity`](local_activity) - Local activity events
//! - [`side_effect`](side_effect) - Side effect events
//! - [`update`](update) - Workflow update events
//! - [`search_attribute`](search_attribute) - Search attribute events
//! - [`nexus`](nexus) - Nexus events
//! - [`upcasting`](upcasting) - Event schema evolution

pub mod activity;
pub mod child_workflow;
pub mod command;
pub mod local_activity;
pub mod marker;
pub mod nexus;
pub mod search_attribute;
pub mod side_effect;
pub mod signal;
pub mod snapshot;
pub mod timer;
pub mod upcasting;
pub mod update;
pub mod workflow;

// Re-exports for convenience - enables `event::ActivityEventType` syntax
pub use activity::ActivityEventType;
pub use child_workflow::ChildWorkflowEventType;
pub use command::CommandEventType;
pub use local_activity::LocalActivityEventType;
pub use marker::MarkerEventType;
pub use nexus::NexusEventType;
pub use search_attribute::SearchAttributeEventType;
pub use side_effect::SideEffectEventType;
pub use signal::SignalEventType;
pub use snapshot::SnapshotEventType;
pub use timer::TimerEventType;
pub use update::UpdateEventType;
pub use workflow::WorkflowEventType;

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
    /// Child workflow events.
    ChildWorkflow,
    /// Local activity events (short-running, no heartbeat).
    LocalActivity,
    /// Side effect events (deterministic side effects).
    SideEffect,
    /// Workflow update events (proposed/accepted updates).
    Update,
    /// Search attribute events.
    SearchAttribute,
    /// Nexus events (cross-namespace operations).
    Nexus,
}

impl EventCategory {
    /// Returns true if this is a workflow event.
    pub fn is_workflow(&self) -> bool {
        matches!(self, Self::Workflow)
    }

    /// Returns true if this is an activity event.
    pub fn is_activity(&self) -> bool {
        matches!(self, Self::Activity)
    }

    /// Returns true if this is a timer event.
    pub fn is_timer(&self) -> bool {
        matches!(self, Self::Timer)
    }

    /// Returns true if this is a signal event.
    pub fn is_signal(&self) -> bool {
        matches!(self, Self::Signal)
    }

    /// Get the string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Workflow => "workflow",
            Self::Activity => "activity",
            Self::Timer => "timer",
            Self::Signal => "signal",
            Self::Marker => "marker",
            Self::Snapshot => "snapshot",
            Self::Command => "command",
            Self::ChildWorkflow => "child_workflow",
            Self::LocalActivity => "local_activity",
            Self::SideEffect => "side_effect",
            Self::Update => "update",
            Self::SearchAttribute => "search_attribute",
            Self::Nexus => "nexus",
        }
    }
}

impl std::str::FromStr for EventCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "workflow" => Ok(Self::Workflow),
            "activity" => Ok(Self::Activity),
            "timer" => Ok(Self::Timer),
            "signal" => Ok(Self::Signal),
            "marker" => Ok(Self::Marker),
            "snapshot" => Ok(Self::Snapshot),
            "command" => Ok(Self::Command),
            "child_workflow" => Ok(Self::ChildWorkflow),
            "local_activity" => Ok(Self::LocalActivity),
            "side_effect" => Ok(Self::SideEffect),
            "update" => Ok(Self::Update),
            "search_attribute" => Ok(Self::SearchAttribute),
            "nexus" => Ok(Self::Nexus),
            _ => Err(format!("Unknown event category: {}", s)),
        }
    }
}

/// All event types in the saga engine.
///
/// This enum combines all event types from different categories into a flat
/// enum for convenience. Each variant represents a specific event in the
/// saga lifecycle.
///
/// # Examples
///
/// ```rust
/// use saga_engine_core::event::EventType;
///
/// let started = EventType::WorkflowExecutionStarted;
/// println!("Event: {}", started);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Workflow execution continued as a new execution.
    WorkflowExecutionContinuedAsNew,
    /// Workflow execution was terminated.
    WorkflowExecutionTerminated,

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

    // Child Workflow Events
    /// Child workflow execution has started.
    ChildWorkflowExecutionStarted,
    /// Child workflow execution completed successfully.
    ChildWorkflowExecutionCompleted,
    /// Child workflow execution failed.
    ChildWorkflowExecutionFailed,
    /// Child workflow execution was canceled.
    ChildWorkflowExecutionCanceled,
    /// Child workflow execution timed out.
    ChildWorkflowExecutionTimedOut,
    /// Child workflow execution was terminated.
    ChildWorkflowExecutionTerminated,
    /// Child workflow execution continued as new.
    ChildWorkflowExecutionContinuedAsNew,
    /// Start child workflow execution has been initiated.
    StartChildWorkflowExecutionInitiated,
    /// Child workflow execution cancel has been requested.
    ChildWorkflowExecutionCancelRequested,

    // Local Activity Events
    /// Local activity has been scheduled.
    LocalActivityScheduled,
    /// Local activity has started.
    LocalActivityStarted,
    /// Local activity completed successfully.
    LocalActivityCompleted,
    /// Local activity failed.
    LocalActivityFailed,
    /// Local activity timed out.
    LocalActivityTimedOut,
    /// Local activity was canceled.
    LocalActivityCanceled,

    // Side Effect Events
    /// A side effect has been recorded.
    SideEffectRecorded,

    // Update Events
    /// Workflow update has been accepted.
    WorkflowUpdateAccepted,
    /// Workflow update has been rejected.
    WorkflowUpdateRejected,
    /// Workflow update has completed.
    WorkflowUpdateCompleted,
    /// Workflow update has been validated.
    WorkflowUpdateValidated,
    /// Workflow update has been rolled back.
    WorkflowUpdateRolledBack,

    // Search Attribute Events
    /// Search attributes have been upserted.
    UpsertSearchAttributes,

    // Nexus Events
    /// Nexus operation has started.
    NexusOperationStarted,
    /// Nexus operation completed successfully.
    NexusOperationCompleted,
    /// Nexus operation failed.
    NexusOperationFailed,
    /// Nexus operation was canceled.
    NexusOperationCanceled,
    /// Nexus operation timed out.
    NexusOperationTimedOut,
    /// Start Nexus operation has been initiated.
    StartNexusOperationInitiated,
    /// Nexus operation cancel has been requested.
    NexusOperationCancelRequested,
}

impl EventType {
    /// Returns the category for this event type.
    pub fn category(&self) -> EventCategory {
        match self {
            // Workflow events
            Self::WorkflowExecutionStarted
            | Self::WorkflowExecutionCompleted
            | Self::WorkflowExecutionFailed
            | Self::WorkflowExecutionTimedOut
            | Self::WorkflowExecutionCanceled
            | Self::WorkflowExecutionContinuedAsNew
            | Self::WorkflowExecutionTerminated => EventCategory::Workflow,

            // Activity events
            Self::ActivityTaskScheduled
            | Self::ActivityTaskStarted
            | Self::ActivityTaskCompleted
            | Self::ActivityTaskFailed
            | Self::ActivityTaskTimedOut
            | Self::ActivityTaskCanceled => EventCategory::Activity,

            // Timer events
            Self::TimerCreated | Self::TimerFired | Self::TimerCanceled => EventCategory::Timer,

            // Signal events
            Self::SignalReceived => EventCategory::Signal,

            // Marker events
            Self::MarkerRecorded => EventCategory::Marker,

            // Snapshot events
            Self::SnapshotCreated => EventCategory::Snapshot,

            // Command events
            Self::CommandIssued | Self::CommandCompleted | Self::CommandFailed => {
                EventCategory::Command
            }

            // Child Workflow events
            Self::ChildWorkflowExecutionStarted
            | Self::ChildWorkflowExecutionCompleted
            | Self::ChildWorkflowExecutionFailed
            | Self::ChildWorkflowExecutionCanceled
            | Self::ChildWorkflowExecutionTimedOut
            | Self::ChildWorkflowExecutionTerminated
            | Self::ChildWorkflowExecutionContinuedAsNew
            | Self::StartChildWorkflowExecutionInitiated
            | Self::ChildWorkflowExecutionCancelRequested => EventCategory::ChildWorkflow,

            // Local Activity events
            Self::LocalActivityScheduled
            | Self::LocalActivityStarted
            | Self::LocalActivityCompleted
            | Self::LocalActivityFailed
            | Self::LocalActivityTimedOut
            | Self::LocalActivityCanceled => EventCategory::LocalActivity,

            // Side Effect events
            Self::SideEffectRecorded => EventCategory::SideEffect,

            // Update events
            Self::WorkflowUpdateAccepted
            | Self::WorkflowUpdateRejected
            | Self::WorkflowUpdateCompleted
            | Self::WorkflowUpdateValidated
            | Self::WorkflowUpdateRolledBack => EventCategory::Update,

            // Search Attribute events
            Self::UpsertSearchAttributes => EventCategory::SearchAttribute,

            // Nexus events
            Self::NexusOperationStarted
            | Self::NexusOperationCompleted
            | Self::NexusOperationFailed
            | Self::NexusOperationCanceled
            | Self::NexusOperationTimedOut
            | Self::StartNexusOperationInitiated
            | Self::NexusOperationCancelRequested => EventCategory::Nexus,
        }
    }

    /// Get the snake_case string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            // Workflow events
            Self::WorkflowExecutionStarted => "workflow_execution_started",
            Self::WorkflowExecutionCompleted => "workflow_execution_completed",
            Self::WorkflowExecutionFailed => "workflow_execution_failed",
            Self::WorkflowExecutionTimedOut => "workflow_execution_timed_out",
            Self::WorkflowExecutionCanceled => "workflow_execution_canceled",
            Self::WorkflowExecutionContinuedAsNew => "workflow_execution_continued_as_new",
            Self::WorkflowExecutionTerminated => "workflow_execution_terminated",

            // Activity events
            Self::ActivityTaskScheduled => "activity_task_scheduled",
            Self::ActivityTaskStarted => "activity_task_started",
            Self::ActivityTaskCompleted => "activity_task_completed",
            Self::ActivityTaskFailed => "activity_task_failed",
            Self::ActivityTaskTimedOut => "activity_task_timed_out",
            Self::ActivityTaskCanceled => "activity_task_canceled",

            // Timer events
            Self::TimerCreated => "timer_created",
            Self::TimerFired => "timer_fired",
            Self::TimerCanceled => "timer_canceled",

            // Signal events
            Self::SignalReceived => "signal_received",

            // Marker events
            Self::MarkerRecorded => "marker_recorded",

            // Snapshot events
            Self::SnapshotCreated => "snapshot_created",

            // Command events
            Self::CommandIssued => "command_issued",
            Self::CommandCompleted => "command_completed",
            Self::CommandFailed => "command_failed",

            // Child Workflow events
            Self::ChildWorkflowExecutionStarted => "child_workflow_execution_started",
            Self::ChildWorkflowExecutionCompleted => "child_workflow_execution_completed",
            Self::ChildWorkflowExecutionFailed => "child_workflow_execution_failed",
            Self::ChildWorkflowExecutionCanceled => "child_workflow_execution_canceled",
            Self::ChildWorkflowExecutionTimedOut => "child_workflow_execution_timed_out",
            Self::ChildWorkflowExecutionTerminated => "child_workflow_execution_terminated",
            Self::ChildWorkflowExecutionContinuedAsNew => {
                "child_workflow_execution_continued_as_new"
            }
            Self::StartChildWorkflowExecutionInitiated => {
                "start_child_workflow_execution_initiated"
            }
            Self::ChildWorkflowExecutionCancelRequested => {
                "child_workflow_execution_cancel_requested"
            }

            // Local Activity events
            Self::LocalActivityScheduled => "local_activity_scheduled",
            Self::LocalActivityStarted => "local_activity_started",
            Self::LocalActivityCompleted => "local_activity_completed",
            Self::LocalActivityFailed => "local_activity_failed",
            Self::LocalActivityTimedOut => "local_activity_timed_out",
            Self::LocalActivityCanceled => "local_activity_canceled",

            // Side Effect events
            Self::SideEffectRecorded => "side_effect_recorded",

            // Update events
            Self::WorkflowUpdateAccepted => "workflow_update_accepted",
            Self::WorkflowUpdateRejected => "workflow_update_rejected",
            Self::WorkflowUpdateCompleted => "workflow_update_completed",
            Self::WorkflowUpdateValidated => "workflow_update_validated",
            Self::WorkflowUpdateRolledBack => "workflow_update_rolled_back",

            // Search Attribute events
            Self::UpsertSearchAttributes => "upsert_search_attributes",

            // Nexus events
            Self::NexusOperationStarted => "nexus_operation_started",
            Self::NexusOperationCompleted => "nexus_operation_completed",
            Self::NexusOperationFailed => "nexus_operation_failed",
            Self::NexusOperationCanceled => "nexus_operation_canceled",
            Self::NexusOperationTimedOut => "nexus_operation_timed_out",
            Self::StartNexusOperationInitiated => "start_nexus_operation_initiated",
            Self::NexusOperationCancelRequested => "nexus_operation_cancel_requested",
        }
    }
}

impl std::str::FromStr for EventType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            // Workflow events
            "workflow_execution_started" => Ok(Self::WorkflowExecutionStarted),
            "workflow_execution_completed" => Ok(Self::WorkflowExecutionCompleted),
            "workflow_execution_failed" => Ok(Self::WorkflowExecutionFailed),
            "workflow_execution_timed_out" => Ok(Self::WorkflowExecutionTimedOut),
            "workflow_execution_canceled" => Ok(Self::WorkflowExecutionCanceled),
            "workflow_execution_continued_as_new" => Ok(Self::WorkflowExecutionContinuedAsNew),
            "workflow_execution_terminated" => Ok(Self::WorkflowExecutionTerminated),

            // Activity events
            "activity_task_scheduled" => Ok(Self::ActivityTaskScheduled),
            "activity_task_started" => Ok(Self::ActivityTaskStarted),
            "activity_task_completed" => Ok(Self::ActivityTaskCompleted),
            "activity_task_failed" => Ok(Self::ActivityTaskFailed),
            "activity_task_timed_out" => Ok(Self::ActivityTaskTimedOut),
            "activity_task_canceled" => Ok(Self::ActivityTaskCanceled),

            // Timer events
            "timer_created" => Ok(Self::TimerCreated),
            "timer_fired" => Ok(Self::TimerFired),
            "timer_canceled" => Ok(Self::TimerCanceled),

            // Signal events
            "signal_received" => Ok(Self::SignalReceived),

            // Marker events
            "marker_recorded" => Ok(Self::MarkerRecorded),

            // Snapshot events
            "snapshot_created" => Ok(Self::SnapshotCreated),

            // Command events
            "command_issued" => Ok(Self::CommandIssued),
            "command_completed" => Ok(Self::CommandCompleted),
            "command_failed" => Ok(Self::CommandFailed),

            // Child Workflow events
            "child_workflow_execution_started" => Ok(Self::ChildWorkflowExecutionStarted),
            "child_workflow_execution_completed" => Ok(Self::ChildWorkflowExecutionCompleted),
            "child_workflow_execution_failed" => Ok(Self::ChildWorkflowExecutionFailed),
            "child_workflow_execution_canceled" => Ok(Self::ChildWorkflowExecutionCanceled),
            "child_workflow_execution_timed_out" => Ok(Self::ChildWorkflowExecutionTimedOut),
            "child_workflow_execution_terminated" => Ok(Self::ChildWorkflowExecutionTerminated),
            "child_workflow_execution_continued_as_new" => {
                Ok(Self::ChildWorkflowExecutionContinuedAsNew)
            }
            "start_child_workflow_execution_initiated" => {
                Ok(Self::StartChildWorkflowExecutionInitiated)
            }
            "child_workflow_execution_cancel_requested" => {
                Ok(Self::ChildWorkflowExecutionCancelRequested)
            }

            // Local Activity events
            "local_activity_scheduled" => Ok(Self::LocalActivityScheduled),
            "local_activity_started" => Ok(Self::LocalActivityStarted),
            "local_activity_completed" => Ok(Self::LocalActivityCompleted),
            "local_activity_failed" => Ok(Self::LocalActivityFailed),
            "local_activity_timed_out" => Ok(Self::LocalActivityTimedOut),
            "local_activity_canceled" => Ok(Self::LocalActivityCanceled),

            // Side Effect events
            "side_effect_recorded" => Ok(Self::SideEffectRecorded),

            // Update events
            "workflow_update_accepted" => Ok(Self::WorkflowUpdateAccepted),
            "workflow_update_rejected" => Ok(Self::WorkflowUpdateRejected),
            "workflow_update_completed" => Ok(Self::WorkflowUpdateCompleted),
            "workflow_update_validated" => Ok(Self::WorkflowUpdateValidated),
            "workflow_update_rolled_back" => Ok(Self::WorkflowUpdateRolledBack),

            // Search Attribute events
            "upsert_search_attributes" => Ok(Self::UpsertSearchAttributes),

            // Nexus events
            "nexus_operation_started" => Ok(Self::NexusOperationStarted),
            "nexus_operation_completed" => Ok(Self::NexusOperationCompleted),
            "nexus_operation_failed" => Ok(Self::NexusOperationFailed),
            "nexus_operation_canceled" => Ok(Self::NexusOperationCanceled),
            "nexus_operation_timed_out" => Ok(Self::NexusOperationTimedOut),
            "start_nexus_operation_initiated" => Ok(Self::StartNexusOperationInitiated),
            "nexus_operation_cancel_requested" => Ok(Self::NexusOperationCancelRequested),

            _ => Err(format!("Unknown event type: {}", s)),
        }
    }
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// Compact Encoding Methods (for binary codecs)
// ============================================================================

impl EventCategory {
    /// Convert u8 back to EventCategory (for deserialization).
    pub(crate) fn from_u8(n: u8) -> Option<Self> {
        match n {
            0 => Some(Self::Workflow),
            1 => Some(Self::Activity),
            2 => Some(Self::Timer),
            3 => Some(Self::Signal),
            4 => Some(Self::Marker),
            5 => Some(Self::Snapshot),
            6 => Some(Self::Command),
            7 => Some(Self::ChildWorkflow),
            8 => Some(Self::LocalActivity),
            9 => Some(Self::SideEffect),
            10 => Some(Self::Update),
            11 => Some(Self::SearchAttribute),
            12 => Some(Self::Nexus),
            _ => None,
        }
    }
}

impl EventType {
    /// Get the compact u8 representation for this event type.
    pub(crate) fn to_compact_u8(&self) -> u8 {
        match self {
            // Workflow (0-6)
            Self::WorkflowExecutionStarted => 0,
            Self::WorkflowExecutionCompleted => 1,
            Self::WorkflowExecutionFailed => 2,
            Self::WorkflowExecutionTimedOut => 3,
            Self::WorkflowExecutionCanceled => 4,
            Self::WorkflowExecutionContinuedAsNew => 5,
            Self::WorkflowExecutionTerminated => 6,
            // Activity (10-15)
            Self::ActivityTaskScheduled => 10,
            Self::ActivityTaskStarted => 11,
            Self::ActivityTaskCompleted => 12,
            Self::ActivityTaskFailed => 13,
            Self::ActivityTaskTimedOut => 14,
            Self::ActivityTaskCanceled => 15,
            // Timer (20-22)
            Self::TimerCreated => 20,
            Self::TimerFired => 21,
            Self::TimerCanceled => 22,
            // Signal (30)
            Self::SignalReceived => 30,
            // Marker (40)
            Self::MarkerRecorded => 40,
            // Snapshot (50)
            Self::SnapshotCreated => 50,
            // Command (60-62)
            Self::CommandIssued => 60,
            Self::CommandCompleted => 61,
            Self::CommandFailed => 62,
            // Child Workflow (70-78)
            Self::ChildWorkflowExecutionStarted => 70,
            Self::ChildWorkflowExecutionCompleted => 71,
            Self::ChildWorkflowExecutionFailed => 72,
            Self::ChildWorkflowExecutionCanceled => 73,
            Self::ChildWorkflowExecutionTimedOut => 74,
            Self::ChildWorkflowExecutionTerminated => 75,
            Self::ChildWorkflowExecutionContinuedAsNew => 76,
            Self::StartChildWorkflowExecutionInitiated => 77,
            Self::ChildWorkflowExecutionCancelRequested => 78,
            // Local Activity (80-85)
            Self::LocalActivityScheduled => 80,
            Self::LocalActivityStarted => 81,
            Self::LocalActivityCompleted => 82,
            Self::LocalActivityFailed => 83,
            Self::LocalActivityTimedOut => 84,
            Self::LocalActivityCanceled => 85,
            // Side Effect (90)
            Self::SideEffectRecorded => 90,
            // Update (100-104)
            Self::WorkflowUpdateAccepted => 100,
            Self::WorkflowUpdateRejected => 101,
            Self::WorkflowUpdateCompleted => 102,
            Self::WorkflowUpdateValidated => 103,
            Self::WorkflowUpdateRolledBack => 104,
            // Search Attribute (110)
            Self::UpsertSearchAttributes => 110,
            // Nexus (120-126)
            Self::NexusOperationStarted => 120,
            Self::NexusOperationCompleted => 121,
            Self::NexusOperationFailed => 122,
            Self::NexusOperationCanceled => 123,
            Self::NexusOperationTimedOut => 124,
            Self::StartNexusOperationInitiated => 125,
            Self::NexusOperationCancelRequested => 126,
        }
    }

    /// Create EventType from category and compact u8 (for deserialization).
    pub(crate) fn from_category_and_u8(category: EventCategory, n: u8) -> Option<Self> {
        match category {
            EventCategory::Workflow => match n {
                0 => Some(Self::WorkflowExecutionStarted),
                1 => Some(Self::WorkflowExecutionCompleted),
                2 => Some(Self::WorkflowExecutionFailed),
                3 => Some(Self::WorkflowExecutionTimedOut),
                4 => Some(Self::WorkflowExecutionCanceled),
                5 => Some(Self::WorkflowExecutionContinuedAsNew),
                6 => Some(Self::WorkflowExecutionTerminated),
                _ => None,
            },
            EventCategory::Activity => match n {
                10 => Some(Self::ActivityTaskScheduled),
                11 => Some(Self::ActivityTaskStarted),
                12 => Some(Self::ActivityTaskCompleted),
                13 => Some(Self::ActivityTaskFailed),
                14 => Some(Self::ActivityTaskTimedOut),
                15 => Some(Self::ActivityTaskCanceled),
                _ => None,
            },
            EventCategory::Timer => match n {
                20 => Some(Self::TimerCreated),
                21 => Some(Self::TimerFired),
                22 => Some(Self::TimerCanceled),
                _ => None,
            },
            EventCategory::Signal => match n {
                30 => Some(Self::SignalReceived),
                _ => None,
            },
            EventCategory::Marker => match n {
                40 => Some(Self::MarkerRecorded),
                _ => None,
            },
            EventCategory::Snapshot => match n {
                50 => Some(Self::SnapshotCreated),
                _ => None,
            },
            EventCategory::Command => match n {
                60 => Some(Self::CommandIssued),
                61 => Some(Self::CommandCompleted),
                62 => Some(Self::CommandFailed),
                _ => None,
            },
            EventCategory::ChildWorkflow => match n {
                70 => Some(Self::ChildWorkflowExecutionStarted),
                71 => Some(Self::ChildWorkflowExecutionCompleted),
                72 => Some(Self::ChildWorkflowExecutionFailed),
                73 => Some(Self::ChildWorkflowExecutionCanceled),
                74 => Some(Self::ChildWorkflowExecutionTimedOut),
                75 => Some(Self::ChildWorkflowExecutionTerminated),
                76 => Some(Self::ChildWorkflowExecutionContinuedAsNew),
                77 => Some(Self::StartChildWorkflowExecutionInitiated),
                78 => Some(Self::ChildWorkflowExecutionCancelRequested),
                _ => None,
            },
            EventCategory::LocalActivity => match n {
                80 => Some(Self::LocalActivityScheduled),
                81 => Some(Self::LocalActivityStarted),
                82 => Some(Self::LocalActivityCompleted),
                83 => Some(Self::LocalActivityFailed),
                84 => Some(Self::LocalActivityTimedOut),
                85 => Some(Self::LocalActivityCanceled),
                _ => None,
            },
            EventCategory::SideEffect => match n {
                90 => Some(Self::SideEffectRecorded),
                _ => None,
            },
            EventCategory::Update => match n {
                100 => Some(Self::WorkflowUpdateAccepted),
                101 => Some(Self::WorkflowUpdateRejected),
                102 => Some(Self::WorkflowUpdateCompleted),
                103 => Some(Self::WorkflowUpdateValidated),
                104 => Some(Self::WorkflowUpdateRolledBack),
                _ => None,
            },
            EventCategory::SearchAttribute => match n {
                110 => Some(Self::UpsertSearchAttributes),
                _ => None,
            },
            EventCategory::Nexus => match n {
                120 => Some(Self::NexusOperationStarted),
                121 => Some(Self::NexusOperationCompleted),
                122 => Some(Self::NexusOperationFailed),
                123 => Some(Self::NexusOperationCanceled),
                124 => Some(Self::NexusOperationTimedOut),
                125 => Some(Self::StartNexusOperationInitiated),
                126 => Some(Self::NexusOperationCancelRequested),
                _ => None,
            },
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
        event_id: EventId,
        saga_id: SagaId,
        event_type: EventType,
        category: EventCategory,
        attributes: Value,
    ) -> Self {
        Self::builder()
            .event_id(event_id)
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
    event_id: Option<EventId>,
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

    /// Set the event ID.
    pub fn event_id(mut self, event_id: EventId) -> Self {
        self.event_id = Some(event_id);
        self
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
        let event_id = self.event_id.expect("event_id is required");
        let saga_id = self.saga_id.expect("saga_id is required");
        let event_type = self.event_type.expect("event_type is required");
        let attributes = self.attributes.expect("attributes is required");

        let category = self.category.unwrap_or_else(|| event_type.category());

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

//! # saga-engine-core
//!
//! Core saga engine library with zero infrastructure dependencies.
//! Provides Event Sourcing, Workflows, and Activities abstractions.
//!
//! ## Architecture
//!
//! This crate defines the core interfaces (traits) and types for the saga engine.
//! It has ZERO dependencies on infrastructure (database, messaging, etc.).
//!
//! ## Modules
//!
//! - [`event`]: [`HistoryEvent`], [`EventType`], [`EventCategory`]
//! - [`codec`]: [`EventCodec`] trait for serialization
//! - [`port`]: Ports for infrastructure adapters (EventStore, SignalDispatcher, TaskQueue, TimerStore)
//! - [`snapshot`]: [`SnapshotManager`] for efficient event replay
//! - [`workflow`]: [`WorkflowDefinition`], [`WorkflowStep`], [`Activity`] for saga execution
//! - [`error`]: Domain errors
//!
//! ## Usage
//!
//! ```rust
//! use saga_engine_core::event::{HistoryEvent, EventType, EventCategory, SagaId, EventId};
//! use saga_engine_core::codec::{JsonCodec, EventCodec};
//! use saga_engine_core::workflow::{WorkflowDefinition, Activity, WorkflowStep};
//!
//! let codec = JsonCodec::new();
//! let event = HistoryEvent::new(
//!     EventId(1),
//!     SagaId("test-saga-id".to_string()),
//!     EventType::WorkflowExecutionStarted,
//!     EventCategory::Workflow,
//!     serde_json::json!({"key": "value"}),
//! );
//!
//! let encoded = codec.encode(&event).unwrap();
//! let decoded = codec.decode(&encoded).unwrap();
//! ```

pub mod codec;
pub mod error;
pub mod event;
pub mod port;
pub mod snapshot;
pub mod workflow;

pub use codec::{BincodeCodec, CodecError, EventCodec, JsonCodec};
pub use error::{Error, Result};
pub use event::{CURRENT_EVENT_VERSION, EventCategory, EventId, EventType, HistoryEvent, SagaId};
pub use port::{
    ConsumerConfig, DurableTimer, HistoryReplayer, ReplayConfig, ReplayError, ReplayResult,
    SignalDispatcher, SignalDispatcherError, SignalNotification, SignalSubscription, SignalType,
    Task, TaskId, TaskMessage, TaskQueue, TaskQueueError, TimerStatus, TimerStore, TimerStoreError,
    TimerType,
};
pub use snapshot::{
    Snapshot, SnapshotChecksum, SnapshotConfig, SnapshotError, SnapshotManager, SnapshotResult,
};
pub use workflow::{
    Activity, DynWorkflowStep, InputValidationError, RetryPolicy, StepCompensationError, StepError,
    StepErrorKind, StepResult, WorkflowConfig, WorkflowContext, WorkflowDefinition,
    WorkflowExecutionError, WorkflowExecutionErrorKind, WorkflowExecutor, WorkflowInput,
    WorkflowResult, WorkflowStep, WorkflowTypeId,
};

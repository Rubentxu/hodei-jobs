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

#![allow(deprecated)]

pub mod activity_registry;
pub mod codec;
pub mod compensation;
pub mod determinism_enforcer;
pub mod error;
pub mod event;
pub mod port;
pub mod relay;
pub mod saga_engine;
pub mod snapshot;
pub mod telemetry;
pub mod worker;
pub mod workflow;

pub use activity_registry::{
    ActivityContext, ActivityError, ActivityRegistry, ActivityResult, ActivityTypeId,
};
pub use codec::{BincodeCodec, CodecError, EventCodec, JsonCodec};
pub use compensation::{
    CompensationAction, CompensationError, CompensationHandler, CompensationTracker,
    CompensationWorkflowResult, CompletedStep, InMemoryCompensationHandler, WorkflowError,
};
pub use determinism_enforcer::{
    DeterminismConfig, DeterminismEnforcer, DeterminismError, DeterminismReport,
    DeterminismViolation, ViolationKind,
};
pub use error::{Error, Result};
pub use event::{CURRENT_EVENT_VERSION, EventCategory, EventId, EventType, HistoryEvent, SagaId};
pub use port::{
    Command, CommandBus, CommandBusConfig, CommandBusError, CommandHandler, CommandResult,
    ConsumerConfig, DomainEvent, DurableTimer, EventBus, EventBusConfig, EventBusError,
    EventHandler, EventHandlerAny, HistoryReplayer, OutboxConfig, OutboxMessage, OutboxRepository,
    OutboxStatus, ReplayConfig, ReplayError, ReplayResult, SignalDispatcher, SignalDispatcherError,
    SignalNotification, SignalSubscription, SignalType, Subscription, SubscriptionId, Task, TaskId,
    TaskMessage, TaskQueue, TaskQueueError, TimerStatus, TimerStore, TimerStoreError, TimerType,
};
pub use relay::{
    OutboxPublisher, OutboxRelay, OutboxRelayConfig, OutboxRelayMetrics, ProcessResult, RelayError,
};
pub use saga_engine::{
    SagaEngine, SagaEngineConfig, SagaEngineError, SagaExecutionResult, WorkflowTask,
};
pub use snapshot::{
    Snapshot, SnapshotChecksum, SnapshotConfig, SnapshotError, SnapshotManager, SnapshotResult,
};
pub use worker::{TaskHandler, TaskProcessingResult, Worker, WorkerConfig, WorkerError};
pub use workflow::{
    Activity, ActivityOptions, DurableWorkflow, DurableWorkflowState, DynWorkflowStep,
    ExecuteActivityError, InputValidationError, StepCompensationError, StepError, StepErrorKind,
    StepResult, WorkflowConfig, WorkflowContext, WorkflowDefinition, WorkflowExecutionError,
    WorkflowExecutionErrorKind, WorkflowExecutor, WorkflowInput, WorkflowPaused, WorkflowResult,
    WorkflowStep, WorkflowTypeId,
};

pub use telemetry::{DefaultSagaTelemetry, SagaTelemetry, TelemetryConfig, TelemetryGuard};

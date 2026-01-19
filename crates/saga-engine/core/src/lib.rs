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
//! - [`error`]: Domain errors
//!
//! ## Usage
//!
//! ```rust
//! use saga_engine_core::event::{HistoryEvent, EventType, EventCategory, SagaId};
//! use saga_engine_core::codec::{JsonCodec, EventCodec};
//!
//! let codec = JsonCodec::new();
//! let event = HistoryEvent::new(
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
pub mod event;
pub mod port;
pub mod snapshot;

pub use codec::{BincodeCodec, CodecError, EventCodec, JsonCodec};
pub use event::{CURRENT_EVENT_VERSION, EventCategory, EventId, EventType, HistoryEvent, SagaId};
pub use port::{
    ConsumerConfig, DurableTimer, EventStore, EventStoreError, SignalDispatcher,
    SignalDispatcherError, SignalNotification, SignalSubscription, SignalType, Task, TaskId,
    TaskMessage, TaskQueue, TaskQueueError, TimerStatus, TimerStore, TimerStoreError, TimerType,
};
pub use snapshot::{
    Snapshot, SnapshotChecksum, SnapshotConfig, SnapshotError, SnapshotManager, SnapshotResult,
};

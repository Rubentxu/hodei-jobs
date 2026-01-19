//! Ports for saga engine infrastructure adapters.
//!
//! This module defines the trait abstractions (ports) that the saga engine
//! uses to interact with infrastructure. Each port can have multiple implementations
//! (PostgreSQL, in-memory, mock, etc.).

pub mod event_store;
pub mod signal_dispatcher;
pub mod task_queue;
pub mod timer_store;

pub use event_store::{EventStore, EventStoreError};
pub use signal_dispatcher::{
    SignalDispatcher, SignalDispatcherError, SignalNotification, SignalSubscription, SignalType,
};
pub use task_queue::{ConsumerConfig, Task, TaskId, TaskMessage, TaskQueue, TaskQueueError};
pub use timer_store::{DurableTimer, TimerStatus, TimerStore, TimerStoreError, TimerType};

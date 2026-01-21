//! Ports for saga engine infrastructure adapters.
//!
//! This module defines the trait abstractions (ports) that are saga engine
//! uses to interact with infrastructure. Each port can have multiple implementations
//! (PostgreSQL, in-memory, mock, etc.).

pub mod command_bus;
pub mod event_bus;
pub mod event_store;
pub mod outbox;
pub mod replay;
pub mod signal_dispatcher;
pub mod task_queue;
pub mod timer_store;

pub use command_bus::{
    Command, CommandBus, CommandBusConfig, CommandBusError, CommandHandler, CommandResult,
};
pub use event_bus::{
    DomainEvent, EventBus, EventBusConfig, EventBusError, EventHandler, EventHandlerAny,
    Subscription, SubscriptionId,
};
pub use event_store::{EventStore, EventStoreError};
pub use outbox::{OutboxConfig, OutboxMessage, OutboxRepository, OutboxStatus};
pub use replay::{HistoryReplayer, ReplayConfig, ReplayError, ReplayResult};
pub use signal_dispatcher::{
    SignalDispatcher, SignalDispatcherError, SignalNotification, SignalSubscription, SignalType,
};
pub use task_queue::{ConsumerConfig, Task, TaskId, TaskMessage, TaskQueue, TaskQueueError};
pub use timer_store::{DurableTimer, TimerStatus, TimerStore, TimerStoreError, TimerType};

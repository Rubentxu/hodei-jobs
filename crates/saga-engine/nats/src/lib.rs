//! NATS backend for saga-engine
//!
//! This module provides NATS-based implementations for:
//! - [`SignalDispatcher`]: Lightweight Pub/Sub for saga event notifications
//! - [`TaskQueue`]: Durable task processing with JetStream pull consumers

pub mod signal_dispatcher;
pub mod task_queue;

pub use signal_dispatcher::{NatsSignalDispatcher, NatsSignalDispatcherConfig};
pub use task_queue::{NatsTaskQueue, NatsTaskQueueConfig};

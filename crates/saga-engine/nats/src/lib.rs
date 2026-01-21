//! NATS backend for saga-engine
//!
//! This module provides NATS-based implementations for:
//! - [`NatsEventBus`]: Domain event publishing and subscribing
//! - [`SignalDispatcher`]: Lightweight Pub/Sub for saga event notifications
//! - [`TaskQueue`]: Durable task processing with JetStream pull consumers
//!
//! # Features
//!
//! - **JetStream Pull Consumers**: On-demand message fetching for horizontal scaling
//! - **Exactly-Once Semantics**: Explicit ACK/NAK for reliable processing
//! - **Dead Letter Queue**: Automatic handling of permanently failed tasks
//! - **Automatic Retry**: Configurable exponential backoff for transient failures

pub mod event_bus;
pub mod signal_dispatcher;
pub mod task_queue;

pub use event_bus::{NatsEventBus, NatsEventBusConfig};
pub use signal_dispatcher::{NatsSignalDispatcher, NatsSignalDispatcherConfig};
pub use task_queue::{NatsTaskQueue, NatsTaskQueueConfig};

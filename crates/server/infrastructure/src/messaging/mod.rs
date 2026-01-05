//! Messaging and Event Bus Infrastructure
//!
//! Provides infrastructure implementations for:
//! - Event bus (NATS JetStream, PostgreSQL)
//! - Outbox pattern for reliable event publishing
//! - Reactive outbox relay using PostgreSQL LISTEN/NOTIFY
//! - Saga consumers for event-driven saga execution

pub mod cleanup_saga_consumer;
pub mod execution_saga_consumer;
pub mod nats;
pub mod nats_outbox_relay;
pub mod outbox_adapter;
pub mod outbox_relay;
pub mod postgres;
pub mod reactive_outbox_relay;
pub mod resilient_subscriber;
pub mod saga_consumer;

pub use cleanup_saga_consumer::{
    CleanupSagaConsumer, CleanupSagaConsumerBuilder, CleanupSagaConsumerConfig,
    CleanupSagaTriggerResult,
};
pub use execution_saga_consumer::{
    ExecutionSagaConsumer, ExecutionSagaConsumerBuilder, ExecutionSagaConsumerConfig,
    ExecutionSagaTriggerResult,
};
pub use nats::{NatsConfig, NatsEventBus, NatsEventBusMetrics};
pub use nats_outbox_relay::{
    NatsOutboxRelay, NatsOutboxRelayConfig, NatsOutboxRelayConfigBuilder, NatsOutboxRelayError,
    NatsOutboxRelayMetrics, NatsOutboxRelayMetricsSnapshot,
};
pub use outbox_adapter::OutboxEventBus;
pub use reactive_outbox_relay::{
    OutboxRelayExt, ReactiveOutboxConfig, ReactiveOutboxError, ReactiveOutboxRelay,
};
pub use resilient_subscriber::{ResilientSubscriber, ResilientSubscriberConfig};
pub use saga_consumer::{NatsSagaConsumer, NatsSagaConsumerBuilder, SagaConsumerConfig};

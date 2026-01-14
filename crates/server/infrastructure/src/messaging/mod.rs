//! Messaging and Event Bus Infrastructure
//!
//! Provides infrastructure implementations for:
//! - Event bus (NATS JetStream, PostgreSQL)
//! - Outbox pattern for reliable event publishing
//! - Reactive outbox relay using PostgreSQL LISTEN/NOTIFY
//! - Saga consumers for event-driven saga execution
//! - Hybrid outbox components (LISTEN/NOTIFY + polling) for EPIC-64

pub mod cancellation_saga_consumer;
pub mod cleanup_saga_consumer;
pub mod event_archiver;
pub mod event_deduplication;
pub mod event_dlq;
pub mod execution_saga_consumer;
pub mod hybrid;
pub mod nats;
pub mod nats_outbox_relay;
pub mod orphan_worker_detector_consumer;
pub mod outbox_adapter;
pub mod outbox_relay;
pub mod postgres;
pub mod reactive_outbox_relay;
pub mod resilient_subscriber;
pub mod saga_consumer;
pub mod worker_disconnection_handler_consumer;
pub mod worker_ephemeral_terminating_consumer;

pub use cancellation_saga_consumer::{
    CancellationSagaConsumer, CancellationSagaConsumerBuilder, CancellationSagaConsumerConfig,
    CancellationSagaTriggerResult,
};
pub use cleanup_saga_consumer::{
    CleanupSagaConsumer, CleanupSagaConsumerBuilder, CleanupSagaConsumerConfig,
    CleanupSagaTriggerResult,
};
pub use event_archiver::{EventArchiver, EventArchiverConfig};
pub use event_deduplication::{
    DeduplicationResult, DeduplicationStats, DeduplicationStatsSnapshot, EventDeduplicationConfig,
    EventDeduplicator, EventDeduplicatorBuilder,
};
pub use event_dlq::{
    DlqEntry, DlqError, DlqOperationResult, EventDlq, EventDlqConfig, EventDlqMetrics,
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

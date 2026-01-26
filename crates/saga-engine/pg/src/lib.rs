//! # saga-engine-pg
//!
//! PostgreSQL backend implementation for saga-engine.
//!
//! This crate provides production-ready PostgreSQL implementations of:
//! - [`PostgresEventStore`] - Event persistence with ACID transactions
//! - [`PostgresTimerStore`] - Durable timer storage
//! - [`PostgresReplayer`] - Saga state reconstruction from events
//! - [`PostgresOutboxRepository`] - Outbox pattern for eventual consistency
//!
//! ## Architecture (DDD + Ports & Adapters)
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │           Application Layer                          │
//! │        PostgresSagaEngine (Facade)                   │
//! └──────────────────────┬──────────────────────────────┘
//!                        │ uses adapters
//! ┌──────────────────────▼──────────────────────────────┐
//! │           Infrastructure Layer                       │
//! │  PostgresEventStore  │  PostgresOutboxRepository  │  │
//! │  PostgresTimerStore  │        (Adapter)           │  │
//! └──────────────────────┴──────────────────────────────┘
//! ```

pub mod event_store;
pub mod notify_listener;
pub mod outbox;
pub mod reactive_timer_scheduler;
pub mod reactive_worker;
pub mod replayer;
pub mod timer_processor;
pub mod timer_store;

// Engine module (Application Layer - Facade)
pub mod engine;

pub use engine::{PostgresSagaEngine, PostgresSagaEngineConfig};
pub use event_store::{PostgresEventStore, PostgresEventStoreConfig};
pub use notify_listener::{
    CHANNEL_SAGA_EVENTS, CHANNEL_SAGA_SIGNALS, CHANNEL_SAGA_SNAPSHOTS, CHANNEL_SAGA_TIMERS,
    NotificationReceiver, NotifyListener, PgNotifyListener,
};
pub use outbox::{OutboxRepositoryConfig, PostgresOutboxRepository};
pub use reactive_timer_scheduler::{ReactiveTimerScheduler, ReactiveTimerSchedulerConfig};
pub use reactive_worker::{
    ReactiveWorker, ReactiveWorkerConfig, WorkerError as ReactiveWorkerError,
};
pub use replayer::PostgresReplayer;
pub use timer_processor::{
    ProcessingMode, TimerProcessor, TimerProcessorError, TimerProcessorMetrics,
    TimerProcessorMetricsSnapshot,
};
pub use timer_store::{PostgresTimerStore, PostgresTimerStoreConfig};

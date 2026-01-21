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
pub mod outbox;
pub mod replayer;
pub mod timer_store;

// Engine module (Application Layer - Facade)
pub mod engine;

pub use engine::{PostgresSagaEngine, PostgresSagaEngineConfig};
pub use event_store::{PostgresEventStore, PostgresEventStoreConfig};
pub use outbox::{OutboxRepositoryConfig, PostgresOutboxRepository};
pub use replayer::PostgresReplayer;
pub use timer_store::{PostgresTimerStore, PostgresTimerStoreConfig};

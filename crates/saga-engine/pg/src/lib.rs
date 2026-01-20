//! # saga-engine-pg
//!
//! PostgreSQL backend implementation for saga-engine.
//!
//! This crate provides production-ready PostgreSQL implementations of:
//! - [`PostgresEventStore`] - Event persistence with ACID transactions
//! - [`PostgresTimerStore`] - Durable timer storage
//! - [`PostgresReplayer`] - Saga state reconstruction from events
//!
//! ## Features
//!
//! - ACID transactions for data integrity
//! - Connection pooling for high concurrency
//! - JSONB for flexible event payloads
//! - Composite indexes for efficient queries
//! - Durable execution with event replay
//!
//! ## Usage
//!
//! ```rust,ignore
//! use saga_engine_pg::{PostgresEventStore, PostgresTimerStore, PostgresReplayer};
//!
//! let store = PostgresEventStore::with_config(
//!     "postgres://user:pass@localhost/saga",
//!     PostgresEventStoreConfig::default(),
//! ).await?;
//!
//! let timer_store = PostgresTimerStore::with_config(
//!     "postgres://user:pass@localhost/saga",
//!     PostgresTimerStoreConfig::default(),
//! ).await?;
//!
//! let replayer = PostgresReplayer::new(Arc::new(store));
//!
//! // Run migrations
//! store.migrate().await?;
//! timer_store.migrate().await?;
//!
//! // Use the stores...
//! ```

pub mod event_store;
pub mod replayer;
pub mod timer_store;

pub use event_store::{PostgresEventStore, PostgresEventStoreConfig};
pub use replayer::PostgresReplayer;
pub use timer_store::{PostgresTimerStore, PostgresTimerStoreConfig};

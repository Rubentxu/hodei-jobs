//! # saga-engine-pg
//!
//! PostgreSQL backend implementation for saga-engine.
//!
//! This crate provides production-ready PostgreSQL implementations of:
//! - [`PostgresEventStore`] - Event persistence with ACID transactions
//! - [`PostgresTimerStore`] - Durable timer storage
//!
//! ## Features
//!
//! - ACID transactions for data integrity
//! - Connection pooling for high concurrency
//! - JSONB for flexible event payloads
//! - Composite indexes for efficient queries
//!
//! ## Usage
//!
//! ```rust,ignore
//! use saga_engine_pg::{PostgresEventStore, PostgresTimerStore};
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
//! // Run migrations
//! store.migrate().await?;
//! timer_store.migrate().await?;
//!
//! // Use the stores...
//! ```

pub mod event_store;
pub mod timer_store;

pub use event_store::{PostgresEventStore, PostgresEventStoreConfig};
pub use timer_store::{PostgresTimerStore, PostgresTimerStoreConfig};

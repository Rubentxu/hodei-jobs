//! # saga-engine-testing
//!
//! Testing utilities for saga-engine with in-memory implementations.
//! Provides [`InMemoryEventStore`], [`InMemoryTimerStore`], [`InMemoryReplayer`], and test helpers.

pub mod in_memory_replayer;
pub mod memory_event_store;
pub mod memory_timer_store;
pub mod test_builder;

#[cfg(feature = "test-postgres")]
pub mod test_database;

#[cfg(feature = "test-nats")]
pub mod test_nats;

#[cfg(feature = "test-postgres")]
pub mod integration;

pub use in_memory_replayer::{InMemoryReplayer, TestState};
pub use memory_event_store::InMemoryEventStore;
pub use memory_timer_store::InMemoryTimerStore;
pub use test_builder::{
    ActivityCall, ActivityMock, CallTracker, TestConfig, WorkflowTest, WorkflowTestBuilder,
};

#[cfg(feature = "test-postgres")]
pub use test_database::{IntegrationError, TestDatabase, TestPostgresConfig};

#[cfg(feature = "test-nats")]
pub use test_nats::{IntegrationError as NatsIntegrationError, TestNats, TestNatsConfig};

#[cfg(feature = "test-postgres")]
pub use integration::{IntegrationTestBuilder, IntegrationTestContext};

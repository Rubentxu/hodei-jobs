//! Outbox Relay Implementation
//!
//! Background process that publishes outbox events to the event bus.
//! Implements the Transactional Outbox Pattern for reliable event publishing.

pub mod relay;

pub use relay::{OutboxRelay, OutboxRelayConfig, OutboxRelayError, OutboxRelayMetrics};

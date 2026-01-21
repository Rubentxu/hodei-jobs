//! # Relay
//!
//! This module provides background processing components for saga engine.
//! Currently includes:
//! - [`OutboxRelay`]: Background worker for processing outbox messages

pub mod outbox_relay;

pub use outbox_relay::{
    DefaultOutboxRelay, OutboxPublisher, OutboxRelay, OutboxRelayConfig, OutboxRelayMetrics,
    ProcessResult, RelayError,
};

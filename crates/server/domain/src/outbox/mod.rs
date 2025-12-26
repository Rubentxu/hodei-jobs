//! Transactional Outbox Pattern Implementation
//!
//! This module provides the domain-level abstractions for the Transactional Outbox Pattern,
//! which solves the dual-write problem between the database and event bus.

pub mod model;
pub mod repository;
pub mod transactional_outbox;

pub use model::{AggregateType, OutboxError, OutboxEventInsert, OutboxEventView, OutboxStatus};
pub use repository::{OutboxRepository, OutboxStats};
pub use transactional_outbox::{
    EventBusPublisher, EventPublisher, OutboxPoller, TransactionalOutbox, TransactionalOutboxError,
};

//! Transactional Outbox Pattern Implementation
//!
//! This module provides the domain-level abstractions for the Transactional Outbox Pattern,
//! which solves the dual-write problem between the database and event bus.

pub mod model;
pub mod repository;
pub mod transactional_outbox;

pub use dlq_handler::DlqHandler;
pub use dlq_model::{DlqEntry, DlqError, DlqStats, DlqStatus};
pub use dlq_repository::DlqRepository;
pub use model::{AggregateType, OutboxError, OutboxEventInsert, OutboxEventView, OutboxStatus};
pub use repository::{OutboxRepository, OutboxRepositoryTx, OutboxStats};
pub use transactional_outbox::{
    EventBusPublisher, EventPublisher, OutboxPoller, TransactionalOutbox, TransactionalOutboxError,
};

pub mod dlq_handler;
pub mod dlq_model;
pub mod dlq_repository;

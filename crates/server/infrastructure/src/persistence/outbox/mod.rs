//! PostgreSQL Outbox Repository Implementation
//!
//! Provides PostgreSQL-specific implementation of the OutboxRepository trait
//! using SQLx for database operations.

pub mod postgres;

pub use postgres::{PostgresOutboxRepository, PostgresOutboxRepositoryError};

/// Type alias for the PostgreSQL outbox repository
pub type OutboxRepository = PostgresOutboxRepository;

/// Type alias for errors from the PostgreSQL outbox repository
pub type OutboxRepositoryError = PostgresOutboxRepositoryError;

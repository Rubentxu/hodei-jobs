//! Transaction Manager - ACID Transaction Support

use async_trait::async_trait;
use thiserror::Error;

/// Error type for transaction operations
#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Transaction already completed")]
    AlreadyCompleted,

    #[error("Transaction panicked: {message}")]
    Panic { message: String },
}

impl From<TransactionError> for crate::shared_kernel::DomainError {
    fn from(err: TransactionError) -> Self {
        crate::shared_kernel::DomainError::InfrastructureError {
            message: err.to_string(),
        }
    }
}

/// Type alias for PostgreSQL transaction
pub type PgTransaction<'a> = sqlx::Transaction<'a, sqlx::Postgres>;

/// Trait for objects that can provide a transaction.
#[async_trait]
pub trait TransactionProvider: Send + Sync {
    /// Begin a new transaction.
    async fn begin_transaction(&self) -> Result<PgTransaction<'_>, TransactionError>;
}

#[async_trait]
impl TransactionProvider for sqlx::PgPool {
    async fn begin_transaction(&self) -> Result<PgTransaction<'_>, TransactionError> {
        self.begin()
            .await
            .map_err(|e| TransactionError::Database(e.to_string()))
    }
}

/// A wrapper to easily manage transaction lifecycle if needed.
pub struct TransactionManager<P: TransactionProvider> {
    provider: P,
}

impl<P: TransactionProvider> TransactionManager<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    pub async fn begin(&self) -> Result<PgTransaction<'_>, TransactionError> {
        self.provider.begin_transaction().await
    }
}

#[cfg(test)]
mod tests;

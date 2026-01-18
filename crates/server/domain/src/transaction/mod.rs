//! Transaction Manager - ACID Transaction Support
//!
//! Provides atomic transaction support for saga operations,
//! ensuring saga state updates and command/event outbox writes
//! happen in a single ACID transaction.
//!
//! # Problem Solved
//!
//! Without this, the following operations happen in separate transactions:
//! 1. Update saga step state to IN_PROGRESS
//! 2. Write command to hodei_commands (via CommandBus)
//! 3. Update saga step state to COMPLETED
//!
//! If the system fails between step 2 and 3, we get:
//! - Command exists in DB (will be processed)
//! - Saga thinks step didn't complete
//! - On recovery, saga retries → "Ghost Commands" (duplicates)
//!
//! # Solution
//!
//! Wrap all operations in a single PostgreSQL transaction:
//!
//! ```sql
//! BEGIN;
//! UPDATE saga_steps SET state = 'IN_PROGRESS' WHERE id = ?;
//! INSERT INTO hodei_commands (...) VALUES (...);
//! UPDATE saga_steps SET state = 'COMPLETED' WHERE id = ?;
//! COMMIT;
//! ```

use thiserror::Error;
use uuid::Uuid;

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

/// Result type for transaction operations
pub type TransactionResult<T> = Result<T, TransactionError>;

/// Type alias for PostgreSQL transaction
type PgTransaction<'a> = sqlx::Transaction<'a, sqlx::Postgres>;

/// Transaction context that can be passed to repositories and command buses.
///
/// This struct wraps a PostgreSQL transaction and provides methods
/// for saga state updates, command insertion, and event persistence,
/// all within the same transaction.
pub struct TransactionContext<'a> {
    /// Pool reference for operations that need new connections
    pool: &'a sqlx::PgPool,
    /// Saga ID being processed
    saga_id: Uuid,
    /// Step order for current operation
    step_order: i32,
    /// Optional transaction - None if already committed/rolled back
    tx: Option<PgTransaction<'a>>,
}

impl<'a> TransactionContext<'a> {
    /// Create a new transaction context (internal use)
    pub fn new(pool: &'a sqlx::PgPool, saga_id: Uuid, step_order: i32) -> Self {
        Self {
            pool,
            saga_id,
            step_order,
            tx: None,
        }
    }

    /// Get the saga ID
    #[inline]
    pub fn saga_id(&self) -> Uuid {
        self.saga_id
    }

    /// Get the step order
    #[inline]
    pub fn step_order(&self) -> i32 {
        self.step_order
    }

    /// Get mutable access to transaction (internal use)
    fn tx_mut(&mut self) -> Result<&mut PgTransaction<'a>, TransactionError> {
        self.tx.as_mut().ok_or(TransactionError::AlreadyCompleted)
    }

    /// Update saga step state to IN_PROGRESS
    pub async fn mark_step_in_progress(&mut self) -> TransactionResult<()> {
        // Clone values BEFORE mutable borrow
        let saga_id = self.saga_id;
        let step_order = self.step_order;
        let tx = self.tx_mut()?;
        sqlx::query(
            "UPDATE saga_steps SET state = 'IN_PROGRESS', started_at = COALESCE(started_at, NOW()), updated_at = NOW() WHERE saga_id = $1 AND step_order = $2"
        )
        .bind(saga_id)
        .bind(step_order)
        .execute(&mut **tx)
        .await
        .map_err(|e| TransactionError::Database(e.to_string()))?;
        Ok(())
    }

    /// Update saga step state to COMPLETED
    pub async fn mark_step_completed(
        &mut self,
        output: Option<serde_json::Value>,
    ) -> TransactionResult<()> {
        // Clone values BEFORE mutable borrow
        let saga_id = self.saga_id;
        let step_order = self.step_order;
        let tx = self.tx_mut()?;
        sqlx::query(
            "UPDATE saga_steps SET state = 'COMPLETED', output_data = COALESCE($3, output_data), completed_at = NOW(), updated_at = NOW() WHERE saga_id = $1 AND step_order = $2"
        )
        .bind(saga_id)
        .bind(step_order)
        .bind(output)
        .execute(&mut **tx)
        .await
        .map_err(|e| TransactionError::Database(e.to_string()))?;
        Ok(())
    }

    /// Update saga step state to FAILED
    pub async fn mark_step_failed(&mut self, error_message: &str) -> TransactionResult<()> {
        // Clone values BEFORE mutable borrow
        let saga_id = self.saga_id;
        let step_order = self.step_order;
        let tx = self.tx_mut()?;
        sqlx::query(
            "UPDATE saga_steps SET state = 'FAILED', error_message = $3, completed_at = NOW(), updated_at = NOW() WHERE saga_id = $1 AND step_order = $2"
        )
        .bind(saga_id)
        .bind(step_order)
        .bind(error_message)
        .execute(&mut **tx)
        .await
        .map_err(|e| TransactionError::Database(e.to_string()))?;
        Ok(())
    }

    /// Update saga global state
    pub async fn update_saga_state(
        &mut self,
        state: &str,
        error_message: Option<&str>,
    ) -> TransactionResult<()> {
        // Clone values BEFORE mutable borrow
        let saga_id = self.saga_id;
        let tx = self.tx_mut()?;
        sqlx::query(
            "UPDATE sagas SET state = $2, error_message = COALESCE($3, error_message), updated_at = NOW() WHERE id = $1"
        )
        .bind(saga_id)
        .bind(state)
        .bind(error_message)
        .execute(&mut **tx)
        .await
        .map_err(|e| TransactionError::Database(e.to_string()))?;
        Ok(())
    }

    /// Insert a command into the hodei_commands table (same transaction)
    pub async fn insert_command(
        &mut self,
        command_type: &str,
        target_id: Uuid,
        target_type: &str,
        payload: &serde_json::Value,
        metadata: Option<&serde_json::Value>,
        idempotency_key: Option<&str>,
    ) -> TransactionResult<Uuid> {
        let tx = self.tx_mut()?;
        let command_id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO hodei_commands (id, command_type, target_id, target_type, payload, metadata, idempotency_key, status) VALUES ($1, $2, $3, $4, $5, $6, $7, 'PENDING')"
        )
        .bind(command_id)
        .bind(command_type)
        .bind(target_id)
        .bind(target_type)
        .bind(payload)
        .bind(metadata)
        .bind(idempotency_key)
        .execute(&mut **tx)
        .await
        .map_err(|e| TransactionError::Database(e.to_string()))?;

        Ok(command_id)
    }

    /// Insert an event into the outbox_events table (same transaction)
    pub async fn insert_event(
        &mut self,
        aggregate_id: Uuid,
        aggregate_type: &str,
        event_type: &str,
        payload: &serde_json::Value,
        metadata: Option<&serde_json::Value>,
    ) -> TransactionResult<Uuid> {
        let tx = self.tx_mut()?;
        let event_id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO outbox_events (id, aggregate_id, aggregate_type, event_type, payload, metadata, status) VALUES ($1, $2, $3, $4, $5, $6, 'PENDING')"
        )
        .bind(event_id)
        .bind(aggregate_id)
        .bind(aggregate_type)
        .bind(event_type)
        .bind(payload)
        .bind(metadata)
        .execute(&mut **tx)
        .await
        .map_err(|e| TransactionError::Database(e.to_string()))?;

        Ok(event_id)
    }
}

/// Transaction Manager for ACID-compliant saga operations.
///
/// This manager provides a clean API for executing operations within
/// a single PostgreSQL transaction, ensuring atomicity for saga workflows.
#[derive(Debug, Clone)]
pub struct TransactionManager {
    pool: sqlx::PgPool,
}

impl TransactionManager {
    /// Create a new transaction manager
    #[inline]
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Execute an operation within a transaction.
    ///
    /// This method automatically handles:
    /// - Beginning the transaction
    /// - Passing a `TransactionContext` to the operation
    /// - Committing on success
    /// - Rolling back on panic or error
    pub async fn execute<F, T>(&self, operation: F) -> TransactionResult<T>
    where
        F: FnOnce(
            &mut TransactionContext<'_>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = TransactionResult<T>> + Send>,
        >,
    {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| TransactionError::Database(e.to_string()))?;

        let saga_id = Uuid::nil();
        let step_order = 0i32;
        let mut ctx = TransactionContext::new(&self.pool, saga_id, step_order);
        ctx.tx = Some(tx);

        match operation(&mut ctx).await {
            Ok(result) => {
                // Take ownership of the transaction and commit
                if let Some(tx_inner) = ctx.tx.take() {
                    tx_inner
                        .commit()
                        .await
                        .map_err(|e| TransactionError::Database(e.to_string()))?;
                }
                Ok(result)
            }
            Err(e) => {
                // ctx.tx will be dropped here, causing automatic rollback
                Err(e)
            }
        }
    }

    /// Execute an operation within a transaction for a specific step.
    pub async fn execute_for_step<F, T>(
        &self,
        saga_id: Uuid,
        step_order: i32,
        operation: F,
    ) -> TransactionResult<T>
    where
        F: FnOnce(
            &mut TransactionContext<'_>,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = TransactionResult<T>> + Send>,
        >,
    {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| TransactionError::Database(e.to_string()))?;

        let mut ctx = TransactionContext::new(&self.pool, saga_id, step_order);
        ctx.tx = Some(tx);

        match operation(&mut ctx).await {
            Ok(result) => {
                // Take ownership of the transaction and commit
                if let Some(tx_inner) = ctx.tx.take() {
                    tx_inner
                        .commit()
                        .await
                        .map_err(|e| TransactionError::Database(e.to_string()))?;
                }
                Ok(result)
            }
            Err(e) => {
                // ctx.tx will be dropped here, causing automatic rollback
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: TransactionManager tests require a real PostgreSQL database.
    // Integration tests with real DB are in the infrastructure crate.

    #[test]
    fn test_transaction_error_display() {
        let error = TransactionError::Database("connection refused".to_string());
        assert!(error.to_string().contains("connection refused"));
    }

    #[test]
    fn test_transaction_error_already_completed() {
        let error = TransactionError::AlreadyCompleted;
        assert_eq!(error.to_string(), "Transaction already completed");
    }
}

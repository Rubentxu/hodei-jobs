//! PostgreSQL-backed Idempotency Checker
//!
//! This module provides a persistent idempotency checker implementation
//! for production environments.

use async_trait::async_trait;
use hodei_server_domain::command::IdempotencyChecker;
use sqlx::PgPool;
use tracing::debug;

/// PostgreSQL-backed idempotency checker.
///
/// This implementation uses the `hodei_commands` table to track processed commands.
/// It checks for commands with any status except 'CANCELLED' to determine if a
/// command has been processed.
#[derive(Debug)]
pub struct PostgresIdempotencyChecker {
    pool: PgPool,
}

impl PostgresIdempotencyChecker {
    /// Create a new PostgresIdempotencyChecker.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl IdempotencyChecker for PostgresIdempotencyChecker {
    async fn is_duplicate(&self, key: &str) -> bool {
        let row = sqlx::query(
            "SELECT 1 FROM hodei_commands WHERE idempotency_key = $1 AND status != 'CANCELLED' LIMIT 1"
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await;

        match row {
            Ok(Some(_)) => {
                debug!(key = %key, "Idempotency check: duplicate found");
                true
            }
            Ok(None) => {
                debug!(key = %key, "Idempotency check: no duplicate");
                false
            }
            Err(e) => {
                tracing::error!(key = %key, error = %e, "Idempotency check failed");
                false
            }
        }
    }

    async fn mark_processed(&self, key: &str) {
        let result = sqlx::query(
            "UPDATE hodei_commands SET status = 'COMPLETED', processed_at = NOW() WHERE idempotency_key = $1 AND status != 'COMPLETED'"
        )
        .bind(key)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => {
                debug!(key = %key, "Marked command as processed");
            }
            Err(e) => {
                tracing::error!(key = %key, error = %e, "Failed to mark command as processed");
            }
        }
    }

    async fn clear(&self) {
        sqlx::query("DELETE FROM hodei_commands")
            .execute(&self.pool)
            .await
            .expect("Failed to clear hodei_commands table");
    }
}

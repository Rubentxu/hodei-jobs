//! # PostgresOutboxRepository
//!
//! PostgreSQL implementation of the Outbox Pattern for eventual consistency.
//!
//! This is an **Infrastructure Layer** adapter that implements [`OutboxRepository`].
//!
//! ## Outbox Pattern
//!
//! The outbox pattern ensures reliable event delivery without distributed transactions:
//! 1. Save domain data AND outbox messages in the same database transaction
//! 2. A separate relay process reads pending messages and publishes them
//! 3. This ensures "exactly-once" semantics within a single database
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │           Application Layer                          │
//! │   (SagaEngine, Worker - use OutboxRepository port)  │
//! └──────────────────────┬──────────────────────────────┘
//!                        │ implements port
//! ┌──────────────────────▼──────────────────────────────┐
//! │           Infrastructure Layer                       │
//! │        PostgresOutboxRepository (Adapter)            │
//! │                    ↓                                 │
//! │              PostgreSQL (Persistence)                │
//! └─────────────────────────────────────────────────────┘
//! ```

use async_trait::async_trait;
use saga_engine_core::port::{OutboxMessage, OutboxRepository, OutboxStatus};
use sqlx::{Pool, Postgres};
use std::str::FromStr;
use std::sync::Arc;

/// PostgreSQL implementation of OutboxRepository.
///
/// This adapter persists outbox messages to PostgreSQL and provides
/// operations for the outbox relay to process them.
pub struct PostgresOutboxRepository {
    pool: Pool<Postgres>,
    config: OutboxRepositoryConfig,
}

impl PostgresOutboxRepository {
    /// Create a new repository with default config.
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self::with_config(pool, OutboxRepositoryConfig::default())
    }

    /// Create a new repository with custom config.
    pub fn with_config(pool: Pool<Postgres>, config: OutboxRepositoryConfig) -> Self {
        Self { pool, config }
    }

    /// Run database migrations.
    pub async fn migrate(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS saga_outbox (
                id UUID PRIMARY KEY,
                topic VARCHAR(512) NOT NULL,
                payload JSONB NOT NULL,
                headers JSONB DEFAULT '{}',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                retry_count INTEGER DEFAULT 0,
                status VARCHAR(32) NOT NULL DEFAULT 'pending',
                last_error TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_saga_outbox_status ON saga_outbox(status, created_at)",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[async_trait]
impl OutboxRepository for PostgresOutboxRepository {
    type Error = sqlx::Error;

    async fn write(&self, message: OutboxMessage) -> Result<(), Self::Error> {
        sqlx::query(
            r#"
            INSERT INTO saga_outbox (id, topic, payload, headers, created_at, retry_count, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(message.id)
        .bind(message.topic)
        .bind(message.payload)
        .bind(serde_json::to_value(&message.headers).unwrap_or_else(|_| serde_json::json!({})))
        .bind(message.created_at)
        .bind(message.retry_count as i32)
        .bind(message.status.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_pending(&self, batch_size: usize) -> Result<Vec<OutboxMessage>, Self::Error> {
        let rows = sqlx::query_as::<_, OutboxRow>(
            r#"
            SELECT id, topic, payload, headers, created_at, retry_count,
                   status, last_error
            FROM saga_outbox
            WHERE status = 'pending' OR (status = 'failed' AND retry_count < $1)
            ORDER BY created_at ASC
            LIMIT $2
            "#,
        )
        .bind(self.config.max_retries as i32)
        .bind(batch_size as i32)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn mark_published(&self, message_id: uuid::Uuid) -> Result<(), Self::Error> {
        sqlx::query("UPDATE saga_outbox SET status = 'published' WHERE id = $1")
            .bind(message_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn mark_failed(&self, message_id: uuid::Uuid, error: &str) -> Result<(), Self::Error> {
        sqlx::query(
            "UPDATE saga_outbox SET status = 'failed', last_error = $1,
             retry_count = retry_count + 1 WHERE id = $2",
        )
        .bind(error)
        .bind(message_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn retry_failed(&self, max_retries: u32) -> Result<usize, Self::Error> {
        let result = sqlx::query(
            "UPDATE saga_outbox SET status = 'pending'
             WHERE status = 'failed' AND retry_count < $1",
        )
        .bind(max_retries as i64)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() as usize)
    }

    async fn pending_count(&self) -> Result<u64, Self::Error> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM saga_outbox WHERE status = 'pending'")
                .fetch_one(&self.pool)
                .await?;

        Ok(count.0 as u64)
    }
}

/// Configuration for the outbox repository.
#[derive(Debug, Clone)]
pub struct OutboxRepositoryConfig {
    /// Maximum retry attempts for failed messages.
    pub max_retries: u32,
    /// Batch size for processing.
    pub batch_size: usize,
}

impl Default for OutboxRepositoryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            batch_size: 100,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct OutboxRow {
    id: uuid::Uuid,
    topic: String,
    payload: serde_json::Value,
    headers: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
    retry_count: i32,
    status: String,
    last_error: Option<String>,
}

impl From<OutboxRow> for OutboxMessage {
    fn from(row: OutboxRow) -> Self {
        let status = match row.status.to_lowercase().as_str() {
            "pending" => OutboxStatus::Pending,
            "processing" => OutboxStatus::Processing,
            "published" => OutboxStatus::Published,
            "failed" => OutboxStatus::Failed,
            _ => OutboxStatus::Pending,
        };

        Self {
            id: row.id,
            topic: row.topic,
            payload: row.payload,
            headers: serde_json::from_value(row.headers).unwrap_or_default(),
            created_at: row.created_at,
            retry_count: row.retry_count as u32,
            status,
            last_error: row.last_error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_outbox_config_defaults() {
        let config = OutboxRepositoryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.batch_size, 100);
    }
}

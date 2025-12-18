//! PostgreSQL Worker Bootstrap Token Store Implementation
//!
//! Implements the WorkerBootstrapTokenStore trait using PostgreSQL as the backend.

use crate::persistence::postgres::DatabaseConfig;
use hodei_server_domain::shared_kernel::DomainError;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};

use async_trait::async_trait;
use hodei_server_domain::iam::{OtpToken, WorkerBootstrapTokenStore};
use hodei_server_domain::shared_kernel::{Result, WorkerId};
use std::time::Duration;

/// PostgreSQL-backed Worker Bootstrap Token Store
#[derive(Debug, Clone)]
pub struct PostgresWorkerBootstrapTokenStore {
    pool: Pool<Postgres>,
}

impl PostgresWorkerBootstrapTokenStore {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    pub async fn connect(config: &DatabaseConfig) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .connect(&config.url)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;

        Ok(Self::new(pool))
    }

    /// Run database migrations for the token store
    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS worker_bootstrap_tokens (
                token UUID PRIMARY KEY,
                worker_id UUID NOT NULL,
                expires_at TIMESTAMPTZ NOT NULL,
                consumed_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Failed to create worker_bootstrap_tokens table: {}", e),
            }
        })?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_worker_bootstrap_tokens_worker_id ON worker_bootstrap_tokens(worker_id)
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(|e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: format!("Failed to create worker_bootstrap_tokens worker_id index: {}", e),
        })?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_worker_bootstrap_tokens_expires_at ON worker_bootstrap_tokens(expires_at)
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(|e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: format!("Failed to create worker_bootstrap_tokens expires_at index: {}", e),
        })?;

        Ok(())
    }
}

#[async_trait]
impl WorkerBootstrapTokenStore for PostgresWorkerBootstrapTokenStore {
    async fn issue(&self, worker_id: &WorkerId, ttl: Duration) -> Result<OtpToken> {
        let token = OtpToken::new();
        let worker_uuid = worker_id.0;
        let expires_at = chrono::Utc::now()
            + chrono::Duration::from_std(ttl).map_err(|e| {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Invalid TTL duration: {}", e),
                }
            })?;

        sqlx::query(
            r#"
            INSERT INTO worker_bootstrap_tokens (token, worker_id, expires_at)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(token.0)
        .bind(worker_uuid)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Failed to issue bootstrap token: {}", e),
            }
        })?;

        Ok(token)
    }

    async fn consume(&self, token: &OtpToken, worker_id: &WorkerId) -> Result<()> {
        let token_uuid = token.0;
        let worker_uuid = worker_id.0;

        let rows_affected = sqlx::query(
            r#"
            UPDATE worker_bootstrap_tokens
            SET consumed_at = NOW()
            WHERE token = $1 AND worker_id = $2 AND expires_at > NOW() AND consumed_at IS NULL
            "#,
        )
        .bind(token_uuid)
        .bind(worker_uuid)
        .execute(&self.pool)
        .await
        .map_err(
            |e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Failed to consume bootstrap token: {}", e),
            },
        )?
        .rows_affected();

        if rows_affected == 0 {
            return Err(
                hodei_server_domain::shared_kernel::DomainError::InvalidOtpToken {
                    message: "Token not found, expired, or already consumed".to_string(),
                },
            );
        }

        Ok(())
    }

    async fn cleanup_expired(&self) -> Result<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM worker_bootstrap_tokens
            WHERE expires_at < NOW()
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Failed to cleanup expired tokens: {}", e),
            }
        })?;

        Ok(result.rows_affected())
    }
}

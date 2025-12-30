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
use tracing::info;

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
            .acquire_timeout(config.connection_timeout)
            .connect(&config.url)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to connect to database: {}", e),
            })?;
        Ok(Self { pool })
    }

    /// Run database migrations for the token store
    ///
    /// DEPRECATED: Migrations are now handled by the central MigrationService.
    /// This method is kept for backwards compatibility but does nothing.
    pub async fn run_migrations(&self) -> Result<()> {
        // Migrations are now handled by the central MigrationService
        // See: hodei_server_infrastructure::persistence::postgres::migrations::run_migrations
        Ok(())
    }
}

#[async_trait]
impl WorkerBootstrapTokenStore for PostgresWorkerBootstrapTokenStore {
    async fn issue(&self, worker_id: &WorkerId, ttl: Duration) -> Result<OtpToken> {
        let token = OtpToken::new();
        let worker_uuid = worker_id.0;
        let expires_at = chrono::Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default();

        sqlx::query("INSERT INTO worker_bootstrap_tokens (token, worker_id, expires_at) VALUES ($1, $2, $3)")
            .bind(token.0)  // Bind the UUID directly
            .bind(worker_uuid)
            .bind(expires_at)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to issue bootstrap token: {}", e),
            })?;

        info!(
            "✅ PostgresWorkerBootstrapTokenStore::issue: Token {} issued for worker {}",
            token, worker_uuid
        );

        Ok(token)
    }

    async fn consume(&self, token: &OtpToken, worker_id: &WorkerId) -> Result<()> {
        let token_uuid = token.0; // Use UUID directly
        let worker_uuid = worker_id.0;
        let token_uuid_log = token.to_string();

        info!(
            "🔐 PostgresWorkerBootstrapTokenStore::consume: Attempting to consume token {} for worker {}",
            token_uuid_log, worker_uuid
        );

        let rows_affected = sqlx::query("UPDATE worker_bootstrap_tokens SET consumed_at = NOW() WHERE token = $1 AND worker_id = $2 AND expires_at > NOW() AND consumed_at IS NULL")
            .bind(token_uuid)  // Bind UUID directly
            .bind(worker_uuid)
            .execute(&self.pool)
            .await
            .map_err(
                |e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Failed to consume bootstrap token: {}", e),
                },
            )?
            .rows_affected();

        info!(
            "🔐 PostgresWorkerBootstrapTokenStore::consume: Token {} - rows_affected={}",
            token_uuid_log, rows_affected
        );

        if rows_affected == 0 {
            info!(
                "❌ PostgresWorkerBootstrapTokenStore::consume: Token {} NOT consumed (not found, expired, or already consumed)",
                token_uuid_log
            );

            return Err(
                hodei_server_domain::shared_kernel::DomainError::InvalidOtpToken {
                    message: "Token not found, expired, or already consumed".to_string(),
                },
            );
        }

        info!(
            "✅ PostgresWorkerBootstrapTokenStore::consume: Token {} consumed successfully for worker {}",
            token_uuid_log, worker_uuid
        );
        Ok(())
    }

    async fn cleanup_expired(&self) -> Result<u64> {
        let result = sqlx::query("DELETE FROM worker_bootstrap_tokens WHERE expires_at < NOW()")
            .execute(&self.pool)
            .await
            .map_err(
                |e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Failed to cleanup expired tokens: {}", e),
                },
            )?;

        Ok(result.rows_affected())
    }
}

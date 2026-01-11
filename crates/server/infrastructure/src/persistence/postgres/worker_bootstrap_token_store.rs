//! PostgreSQL Worker Bootstrap Token Store Implementation
//!
//! Implements the WorkerBootstrapTokenStore trait using PostgreSQL as the backend.
//!
//! # Pool Management
//!
//! This store expects a `PgPool` to be passed in via `new()`.
//! The pool should be created using `DatabasePool` for consistent configuration.

use std::time::Duration;

use hodei_server_domain::shared_kernel::DomainError;
use sqlx::{Pool, Postgres};

use async_trait::async_trait;
use hodei_server_domain::iam::{OtpToken, WorkerBootstrapTokenStore};
use hodei_server_domain::shared_kernel::{Result, WorkerId};
use tracing::info;

/// PostgreSQL-backed Worker Bootstrap Token Store
#[derive(Debug, Clone)]
pub struct PostgresWorkerBootstrapTokenStore {
    pool: Pool<Postgres>,
}

impl PostgresWorkerBootstrapTokenStore {
    /// Create new store with existing pool
    ///
    /// The pool should be created centrally (e.g., using `DatabasePool`)
    /// to ensure consistent configuration across all repositories.
    #[inline]
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
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
    async fn issue(
        &self,
        worker_id: &WorkerId,
        ttl: Duration,
        provider_resource_id: Option<String>,
    ) -> Result<OtpToken> {
        let worker_uuid = worker_id.0;
        let expires_at = chrono::Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default();

        // Check if there's already an unconsumed token for this worker
        let existing_token = sqlx::query!(
            "SELECT token FROM worker_bootstrap_tokens WHERE worker_id = $1 AND consumed_at IS NULL AND expires_at > NOW()",
            worker_uuid
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to check existing token: {}", e),
        })?;

        let token = if let Some(existing) = existing_token {
            // Update existing token with provider_resource_id
            let existing_token = OtpToken(existing.token);
            sqlx::query(
                "UPDATE worker_bootstrap_tokens SET provider_resource_id = $1, expires_at = $2 WHERE token = $3"
            )
                .bind(provider_resource_id.as_deref())
                .bind(expires_at)
                .bind(existing_token.0)
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to update bootstrap token: {}", e),
                })?;

            info!(
                "✅ PostgresWorkerBootstrapTokenStore::issue: Updated existing token {} for worker {} (resource_id: {:?})",
                existing_token, worker_uuid, provider_resource_id
            );

            existing_token
        } else {
            // Create new token
            let new_token = OtpToken::new();
            sqlx::query(
                "INSERT INTO worker_bootstrap_tokens (token, worker_id, expires_at, provider_resource_id) VALUES ($1, $2, $3, $4)"
            )
                .bind(new_token.0)
                .bind(worker_uuid)
                .bind(expires_at)
                .bind(provider_resource_id.as_deref())
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to issue bootstrap token: {}", e),
                })?;

            info!(
                "✅ PostgresWorkerBootstrapTokenStore::issue: New token {} issued for worker {} (resource_id: {:?})",
                new_token, worker_uuid, provider_resource_id
            );

            new_token
        };

        Ok(token)
    }

    async fn consume(
        &self,
        token: &OtpToken,
        worker_id: &WorkerId,
    ) -> Result<Option<String>> {
        let token_uuid = token.0;
        let worker_uuid = worker_id.0;
        let token_uuid_log = token.to_string();

        info!(
            "🔐 PostgresWorkerBootstrapTokenStore::consume: Attempting to consume token {} for worker {}",
            token_uuid_log, worker_uuid
        );

        // First, fetch the provider_resource_id before consuming
        let token_data = sqlx::query!(
            "SELECT provider_resource_id FROM worker_bootstrap_tokens WHERE token = $1 AND worker_id = $2 AND expires_at > NOW() AND consumed_at IS NULL",
            token_uuid,
            worker_uuid
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to fetch token data: {}", e),
        })?;

        let provider_resource_id = token_data.as_ref().and_then(|d| d.provider_resource_id.clone());

        // Now consume the token
        let rows_affected = sqlx::query(
            "UPDATE worker_bootstrap_tokens SET consumed_at = NOW() WHERE token = $1 AND worker_id = $2 AND expires_at > NOW() AND consumed_at IS NULL"
        )
            .bind(token_uuid)
            .bind(worker_uuid)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to consume bootstrap token: {}", e),
            })?
            .rows_affected();

        info!(
            "🔐 PostgresWorkerBootstrapTokenStore::consume: Token {} - rows_affected={}",
            token_uuid_log, rows_affected
        );

        if rows_affected == 0 {
            // Check for idempotency: if token was already consumed by the same worker, allow it
            let existing = sqlx::query!(
                "SELECT worker_id, consumed_at, provider_resource_id FROM worker_bootstrap_tokens WHERE token = $1",
                token_uuid
            )
            .fetch_one(&self.pool)
            .await
            .ok();

            if let Some(existing) = existing {
                if existing.consumed_at.is_some() && existing.worker_id == worker_uuid {
                    info!(
                        "✅ PostgresWorkerBootstrapTokenStore::consume: Token {} already consumed by same worker {} (idempotent success)",
                        token_uuid_log, worker_uuid
                    );
                    return Ok(existing.provider_resource_id);
                }
            }

            info!(
                "❌ PostgresWorkerBootstrapTokenStore::consume: Token {} NOT consumed (not found, expired, or already consumed)",
                token_uuid_log
            );

            return Err(DomainError::InvalidOtpToken {
                message: "Token not found, expired, or already consumed".to_string(),
            });
        }

        info!(
            "✅ PostgresWorkerBootstrapTokenStore::consume: Token {} consumed successfully for worker {} (resource_id: {:?})",
            token_uuid_log, worker_uuid, provider_resource_id
        );
        Ok(provider_resource_id)
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

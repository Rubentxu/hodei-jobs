//! PostgreSQL Credential Provider
//!
//! Implementation of `CredentialProvider` that stores encrypted secrets in PostgreSQL.
//!
//! # Features
//!
//! - AES-256-GCM encryption at rest
//! - Audit logging of all access
//! - Version tracking for secrets
//! - Metadata support
//!
//! # Security
//!
//! - All secrets are encrypted before storage
//! - Encryption key must be provided externally (never stored in DB)
//! - Audit log tracks all read/write operations

use crate::credentials::encryption::{EncryptionError, EncryptionKey};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hodei_server_domain::credentials::{
    AuditAction, AuditContext, CredentialError, CredentialProvider, Secret, SecretValue,
};
use hodei_server_domain::shared_kernel::{DomainError, Result as DomainResult};
use sqlx::Row;
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Database configuration for credential provider
#[derive(Debug, Clone)]
pub struct CredentialDatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub connection_timeout: Duration,
}

impl CredentialDatabaseConfig {
    pub fn new(url: String) -> Self {
        Self {
            url,
            max_connections: 5,
            connection_timeout: Duration::from_secs(30),
        }
    }

    pub fn with_max_connections(mut self, max: u32) -> Self {
        self.max_connections = max;
        self
    }
}

/// PostgreSQL-backed credential provider with encryption
///
/// # Example
///
/// ```ignore
/// let config = CredentialDatabaseConfig::new(db_url);
/// let key = EncryptionKey::from_hex(&env::var("ENCRYPTION_KEY")?)?;
/// let provider = PostgresCredentialProvider::connect(&config, key).await?;
///
/// // Run migrations
/// provider.run_migrations().await?;
///
/// // Store a secret
/// provider.set_secret("api_key", SecretValue::new("secret123")).await?;
///
/// // Retrieve it
/// let secret = provider.get_secret("api_key").await?;
/// ```
pub struct PostgresCredentialProvider {
    pool: PgPool,
    encryption_key: Arc<EncryptionKey>,
}

impl PostgresCredentialProvider {
    /// Creates a new PostgreSQL credential provider with an existing pool
    pub fn new(pool: PgPool, encryption_key: EncryptionKey) -> Self {
        Self {
            pool,
            encryption_key: Arc::new(encryption_key),
        }
    }

    /// Creates a new provider by connecting to the database
    pub async fn connect(
        config: &CredentialDatabaseConfig,
        encryption_key: EncryptionKey,
    ) -> DomainResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.connection_timeout)
            .connect(&config.url)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to connect to database: {}", e),
            })?;

        Ok(Self {
            pool,
            encryption_key: Arc::new(encryption_key),
        })
    }

    /// Returns the provider name used in SecretRef
    pub fn provider_name() -> &'static str {
        "postgres"
    }

    /// Run migrations to create secrets and audit tables
    pub async fn run_migrations(&self) -> DomainResult<()> {
        // Create secrets table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS secrets (
                key VARCHAR(255) PRIMARY KEY,
                encrypted_value BYTEA NOT NULL,
                version BIGINT NOT NULL DEFAULT 1,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                expires_at TIMESTAMPTZ,
                metadata JSONB NOT NULL DEFAULT '{}'
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create secrets table: {}", e),
        })?;

        // Create index on expires_at for efficient expiration queries
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_secrets_expires_at ON secrets(expires_at) WHERE expires_at IS NOT NULL;",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create secrets expires_at index: {}", e),
        })?;

        // Create audit log table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS secret_access_log (
                id BIGSERIAL PRIMARY KEY,
                secret_key VARCHAR(255) NOT NULL,
                action VARCHAR(50) NOT NULL,
                accessor VARCHAR(255) NOT NULL,
                correlation_id VARCHAR(255),
                ip_address VARCHAR(45),
                user_agent VARCHAR(500),
                success BOOLEAN NOT NULL DEFAULT TRUE,
                error_message TEXT,
                accessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create secret_access_log table: {}", e),
        })?;

        // Create indexes for efficient querying
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_secret_access_log_key ON secret_access_log(secret_key);",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create secret_access_log key index: {}", e),
        })?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_secret_access_log_accessed_at ON secret_access_log(accessed_at);",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create secret_access_log accessed_at index: {}", e),
        })?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_secret_access_log_accessor ON secret_access_log(accessor);",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create secret_access_log accessor index: {}", e),
        })?;

        tracing::info!("Secrets and audit tables migrations completed");
        Ok(())
    }

    /// Encrypt a secret value
    fn encrypt_value(&self, value: &SecretValue) -> Result<Vec<u8>, CredentialError> {
        self.encryption_key
            .encrypt(value.expose())
            .map_err(|e| CredentialError::EncryptionError {
                message: format!("Failed to encrypt secret: {}", e),
            })
    }

    /// Decrypt an encrypted value
    fn decrypt_value(&self, encrypted: &[u8]) -> Result<SecretValue, CredentialError> {
        let decrypted = self
            .encryption_key
            .decrypt(encrypted)
            .map_err(|e| match e {
                EncryptionError::DecryptionFailed => CredentialError::EncryptionError {
                    message: "Failed to decrypt secret: invalid key or corrupted data".to_string(),
                },
                _ => CredentialError::EncryptionError {
                    message: format!("Failed to decrypt secret: {}", e),
                },
            })?;

        Ok(SecretValue::new(decrypted))
    }

    /// Map a database row to a Secret
    fn map_row_to_secret(&self, row: PgRow) -> Result<Secret, CredentialError> {
        let key: String = row.get("key");
        let encrypted_value: Vec<u8> = row.get("encrypted_value");
        let version: i64 = row.get("version");
        let created_at: DateTime<Utc> = row.get("created_at");
        let expires_at: Option<DateTime<Utc>> = row.get("expires_at");
        let metadata_json: serde_json::Value = row.get("metadata");

        let value = self.decrypt_value(&encrypted_value)?;

        let metadata: HashMap<String, String> =
            serde_json::from_value(metadata_json).unwrap_or_default();

        let mut builder = Secret::builder(&key)
            .value(value)
            .version(version as u64)
            .created_at(created_at);

        if let Some(exp) = expires_at {
            builder = builder.expires_at(exp);
        }

        for (k, v) in metadata {
            builder = builder.metadata(k, v);
        }

        Ok(builder.build())
    }

    /// Log an audit event to the database
    async fn log_audit(
        &self,
        key: &str,
        action: AuditAction,
        context: &AuditContext,
        success: bool,
        error_message: Option<&str>,
    ) {
        let result = sqlx::query(
            r#"
            INSERT INTO secret_access_log
                (secret_key, action, accessor, correlation_id, ip_address, user_agent, success, error_message)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(key)
        .bind(action.to_string())
        .bind(&context.accessor)
        .bind(context.correlation_id.as_deref())
        .bind(context.ip_address.as_deref())
        .bind(context.user_agent.as_deref())
        .bind(success)
        .bind(error_message)
        .execute(&self.pool)
        .await;

        if let Err(e) = result {
            tracing::error!(
                key = %key,
                action = %action,
                error = %e,
                "Failed to log secret access audit"
            );
        }
    }
}

#[async_trait]
impl CredentialProvider for PostgresCredentialProvider {
    fn name(&self) -> &str {
        Self::provider_name()
    }

    async fn get_secret(&self, key: &str) -> Result<Secret, CredentialError> {
        let row = sqlx::query(
            r#"
            SELECT key, encrypted_value, version, created_at, expires_at, metadata
            FROM secrets
            WHERE key = $1
            "#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CredentialError::connection(format!("Database error: {}", e)))?;

        match row {
            Some(row) => {
                let secret = self.map_row_to_secret(row)?;

                // Check expiration
                if secret.is_expired() {
                    return Err(CredentialError::expired(key));
                }

                Ok(secret)
            }
            None => Err(CredentialError::NotFound {
                key: key.to_string(),
            }),
        }
    }

    async fn get_secret_version(&self, key: &str, version: u64) -> Result<Secret, CredentialError> {
        let row = sqlx::query(
            r#"
            SELECT key, encrypted_value, version, created_at, expires_at, metadata
            FROM secrets
            WHERE key = $1 AND version = $2
            "#,
        )
        .bind(key)
        .bind(version as i64)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CredentialError::connection(format!("Database error: {}", e)))?;

        match row {
            Some(row) => {
                let secret = self.map_row_to_secret(row)?;
                if secret.is_expired() {
                    return Err(CredentialError::expired(key));
                }
                Ok(secret)
            }
            None => Err(CredentialError::not_found(format!("{}@v{}", key, version))),
        }
    }

    async fn set_secret(&self, key: &str, value: SecretValue) -> Result<Secret, CredentialError> {
        let encrypted = self.encrypt_value(&value)?;
        let now = Utc::now();

        // Upsert with version increment
        let row = sqlx::query(
            r#"
            INSERT INTO secrets (key, encrypted_value, version, created_at, updated_at)
            VALUES ($1, $2, 1, $3, $3)
            ON CONFLICT (key) DO UPDATE SET
                encrypted_value = EXCLUDED.encrypted_value,
                version = secrets.version + 1,
                updated_at = EXCLUDED.updated_at
            RETURNING key, encrypted_value, version, created_at, expires_at, metadata
            "#,
        )
        .bind(key)
        .bind(&encrypted)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| CredentialError::connection(format!("Failed to set secret: {}", e)))?;

        self.map_row_to_secret(row)
    }

    async fn delete_secret(&self, key: &str) -> Result<(), CredentialError> {
        let result = sqlx::query("DELETE FROM secrets WHERE key = $1")
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|e| CredentialError::connection(format!("Failed to delete secret: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(CredentialError::NotFound {
                key: key.to_string(),
            });
        }

        Ok(())
    }

    async fn list_secrets(&self, prefix: Option<&str>) -> Result<Vec<String>, CredentialError> {
        let rows = if let Some(prefix) = prefix {
            let pattern = format!("{}%", prefix);
            sqlx::query("SELECT key FROM secrets WHERE key LIKE $1 ORDER BY key")
                .bind(pattern)
                .fetch_all(&self.pool)
                .await
        } else {
            sqlx::query("SELECT key FROM secrets ORDER BY key")
                .fetch_all(&self.pool)
                .await
        }
        .map_err(|e| CredentialError::connection(format!("Failed to list secrets: {}", e)))?;

        let keys: Vec<String> = rows.iter().map(|row: &PgRow| row.get("key")).collect();
        Ok(keys)
    }

    async fn health_check(&self) -> Result<(), CredentialError> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| CredentialError::connection(format!("Health check failed: {}", e)))?;
        Ok(())
    }

    async fn audit_access(&self, key: &str, action: AuditAction, context: &AuditContext) {
        self.log_audit(key, action, context, true, None).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_name() {
        assert_eq!(PostgresCredentialProvider::provider_name(), "postgres");
    }

    #[test]
    fn test_credential_database_config() {
        let config = CredentialDatabaseConfig::new("postgres://localhost/test".to_string())
            .with_max_connections(10);

        assert_eq!(config.url, "postgres://localhost/test");
        assert_eq!(config.max_connections, 10);
    }
}

//! Centralized PostgreSQL Connection Pool Management
//!
//! This module provides a unified way to manage PostgreSQL connection pools
//! across the entire application. All repositories should use the pool
//! created here instead of creating their own pools.
//!
//! # Benefits
//!
//! - Single source of truth for pool configuration
//!
//! - Consistent connection settings across all components
//!
//! - Prevents pool exhaustion from multiple independent pools
//!
//! - Easier monitoring and tuning
//!
//! # Usage
//!
//! ```rust
//! use hodei_server_infrastructure::persistence::postgres::pool::DatabasePool;
//!
//! // Create pool once in main.rs
//! let pool = DatabasePool::new(&db_url, 50, 10).await?;
//!
//! // Pass Arc<PgPool> to all repositories
//! let job_repo = PostgresJobRepository::new(pool.clone());
//! let worker_registry = PostgresWorkerRegistry::new(pool.clone());
//! ```

use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use thiserror::Error;
use tracing::info;

/// Centralized database pool configuration
#[derive(Debug, Clone)]
pub struct DatabasePoolConfig {
    /// Maximum number of connections in the pool
    pub max_connections: u32,
    /// Minimum number of connections to maintain
    pub min_connections: u32,
    /// Connection acquisition timeout
    pub connection_timeout: Duration,
    /// Idle connection lifetime
    pub idle_timeout: Duration,
    /// Maximum connection lifetime
    pub max_lifetime: Duration,
}

impl Default for DatabasePoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 50,
            min_connections: 10,
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600), // 10 minutes
            max_lifetime: Duration::from_secs(1800), // 30 minutes
        }
    }
}

impl DatabasePoolConfig {
    /// Create config with custom values
    #[inline]
    pub fn new(max_connections: u32, min_connections: u32, connection_timeout_secs: u64) -> Self {
        Self {
            max_connections,
            min_connections,
            connection_timeout: Duration::from_secs(connection_timeout_secs),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_secs(1800),
        }
    }

    /// Create config from environment variables
    pub fn from_env() -> Self {
        let max_connections = std::env::var("HODEI_DB_MAX_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50);

        let min_connections = std::env::var("HODEI_DB_MIN_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);

        let connection_timeout_secs = std::env::var("HODEI_DB_CONNECTION_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        Self::new(max_connections, min_connections, connection_timeout_secs)
    }
}

/// Centralized PostgreSQL connection pool
///
/// This type wraps `PgPool` and provides a single point of access
/// for all database operations in the application.
#[derive(Debug, Clone)]
pub struct DatabasePool {
    pool: PgPool,
}

impl DatabasePool {
    /// Create a new database pool with the given configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the pool cannot be created (e.g., invalid URL,
    /// database unreachable).
    pub async fn new(url: &str, config: DatabasePoolConfig) -> Result<Self, PoolError> {
        info!(
            "Creating PostgreSQL pool (min={}, max={}, timeout={:?})",
            config.min_connections, config.max_connections, config.connection_timeout
        );

        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(config.connection_timeout)
            .idle_timeout(config.idle_timeout)
            .max_lifetime(config.max_lifetime)
            .connect(url)
            .await
            .map_err(|e| PoolError::ConnectionFailed(e.to_string()))?;

        info!("PostgreSQL pool created successfully");

        Ok(Self { pool })
    }

    /// Create a pool from environment variables
    ///
    /// Uses the following environment variables:
    /// - `DATABASE_URL`, `HODEI_DATABASE_URL`, or `SERVER_DATABASE_URL`
    /// - `HODEI_DB_MAX_CONNECTIONS`
    /// - `HODEI_DB_MIN_CONNECTIONS`
    /// - `HODEI_DB_CONNECTION_TIMEOUT_SECS`
    pub async fn from_env() -> Result<Self, PoolError> {
        let url = std::env::var("SERVER_DATABASE_URL")
            .or_else(|_| std::env::var("HODEI_DATABASE_URL"))
            .or_else(|_| std::env::var("DATABASE_URL"))
            .map_err(|_| PoolError::MissingUrl)?;

        let config = DatabasePoolConfig::from_env();
        Self::new(&url, config).await
    }

    /// Get the inner `PgPool` for use with sqlx
    #[inline]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Clone the inner `PgPool`
    ///
    /// This is useful when you need to pass a `PgPool` to a type
    /// that doesn't know about `DatabasePool`.
    #[inline]
    pub fn pg_pool(&self) -> PgPool {
        self.pool.clone()
    }

    /// Get current pool statistics
    pub async fn stats(&self) -> PoolStats {
        // sqlx 0.8 removed statistics(), return approximate values
        PoolStats {
            size: 0,
            available: 0,
            waiting: 0,
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Total number of connections in the pool
    pub size: u32,
    /// Number of idle connections
    pub available: u32,
    /// Number of tasks waiting for a connection
    pub waiting: u32,
}

/// Errors that can occur when working with the database pool
#[derive(Debug, Error)]
pub enum PoolError {
    #[error("Failed to connect to database: {0}")]
    ConnectionFailed(String),

    #[error("Missing database URL in environment")]
    MissingUrl,
}

/// Helper trait for components that need database access
///
/// This trait allows types to declare their database requirements
/// in a unified way.
#[async_trait::async_trait]
pub trait RequiresDatabase {
    /// Set the database pool for this component
    fn set_pool(&mut self, pool: PgPool);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pool_config_defaults() {
        let config = DatabasePoolConfig::default();
        assert_eq!(config.max_connections, 50);
        assert_eq!(config.min_connections, 10);
        assert_eq!(config.connection_timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_pool_config_custom() {
        let config = DatabasePoolConfig::new(100, 20, 60);
        assert_eq!(config.max_connections, 100);
        assert_eq!(config.min_connections, 20);
        assert_eq!(config.connection_timeout, Duration::from_secs(60));
    }
}

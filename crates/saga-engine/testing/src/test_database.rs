//! # Test Database Helper
//!
//! Provides a test PostgreSQL database using testcontainers.
//! Automatically starts a container and runs migrations on startup.
//! Cleans up on drop.

use sqlx::postgres::{PgConnectOptions, PgPool};
use testcontainers::Container;
use testcontainers::clients::DockerCli;
use testcontainers::core::WaitFor;
use testcontainers::images::postgres::Postgres;

use crate::IntegrationError;

/// Configuration for test PostgreSQL database
#[derive(Debug, Clone)]
pub struct TestPostgresConfig {
    pub image: String,
    pub tag: String,
    pub database: String,
    pub username: String,
    pub password: String,
}

impl Default for TestPostgresConfig {
    fn default() -> Self {
        Self {
            image: "postgres".to_string(),
            tag: "16-alpine".to_string(),
            database: "test_saga_engine".to_string(),
            username: "test_user".to_string(),
            password: "test_password".to_string(),
        }
    }
}

/// Test database wrapper that manages a PostgreSQL container
///
/// # Example
///
/// ```rust,ignore
/// #[tokio::test]
/// async fn test_with_db() {
///     let db = TestDatabase::new().await;
///     let pool = db.pool();
///     // Use pool in tests...
/// }
/// ```
pub struct TestDatabase {
    _container: Container<'static, Postgres>,
    pool: PgPool,
    connection_string: String,
}

impl TestDatabase {
    /// Create a new test database with default configuration
    pub async fn new() -> Result<Self, IntegrationError> {
        Self::with_config(TestPostgresConfig::default()).await
    }

    /// Create a new test database with custom configuration
    pub async fn with_config(config: TestPostgresConfig) -> Result<Self, IntegrationError> {
        let docker = DockerCli::from_env()?;

        let postgres = Postgres::default()
            .with_image(format!("{}:{}", config.image, config.tag))
            .with_database(&config.database)
            .with_username(&config.username)
            .with_password(&config.password)
            .with_wait_for(WaitFor::text_on_stderr(
                "listening on IPv4 address \"0.0.0.0\"",
            ));

        let container = docker.run(postgres);

        let host = container.get_host();
        let port = container.get_host_port(5432)?;

        let connection_string = format!(
            "postgres://{}:{}@{}:{}/{}",
            config.username, config.password, host, port, config.database
        );

        // Create connection options
        let options = PgConnectOptions::new()
            .host(host)
            .port(port)
            .database(&config.database)
            .username(&config.username)
            .password(&config.password);

        // Create pool with retries
        let pool = retry_pool(options.clone()).await?;

        // Run migrations
        run_migrations(&pool).await?;

        Ok(Self {
            _container: container,
            pool,
            connection_string,
        })
    }

    /// Get the database pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Get the connection string
    pub fn connection_string(&self) -> &str {
        &self.connection_string
    }

    /// Get a clone of the pool for sharing
    pub fn pool_clone(&self) -> PgPool {
        self.pool.clone()
    }

    /// Execute a query for testing
    pub async fn execute(&self, query: &str) -> Result<(), sqlx::Error> {
        sqlx::query(query).execute(&self.pool).await?;
        Ok(())
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        // Container is dropped automatically, which stops and removes the container
    }
}

/// Retry creating the pool with exponential backoff
async fn retry_pool(options: PgConnectOptions) -> Result<PgPool, sqlx::Error> {
    let mut attempts = 0;
    let max_attempts = 10;

    loop {
        match PgPool::connect_with(options.clone()).await {
            Ok(pool) => return Ok(pool),
            Err(e) if attempts < max_attempts => {
                attempts += 1;
                let delay = std::time::Duration::from_millis(500 * 2_u64.pow(attempts));
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Run database migrations
async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    // Run the migrations from the saga-engine-pg crate
    sqlx::migrate!("../pg/migrations")
        .run(pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to run migrations: {:?}", e);
            e
        })
}

/// Custom error type for integration tests
#[derive(Debug, thiserror::Error)]
pub enum IntegrationError {
    #[error("Failed to start container: {0}")]
    ContainerError(#[from] testcontainers::Error),

    #[error("Database connection error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Migration error: {0}")]
    MigrationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_creation() {
        let db = TestDatabase::new().await.unwrap();

        // Verify pool works
        let result: i32 = sqlx::query_scalar("SELECT 1")
            .fetch_one(db.pool())
            .await
            .unwrap();

        assert_eq!(result, 1);
    }

    #[tokio::test]
    async fn test_database_execute() {
        let db = TestDatabase::new().await.unwrap();

        // Create test table
        db.execute("CREATE TABLE IF NOT EXISTS test_table (id SERIAL PRIMARY KEY, value TEXT)")
            .await
            .unwrap();

        // Insert data
        db.execute("INSERT INTO test_table (value) VALUES ('test')")
            .await
            .unwrap();

        // Query data
        let result: String = sqlx::query_scalar("SELECT value FROM test_table LIMIT 1")
            .fetch_one(db.pool())
            .await
            .unwrap();

        assert_eq!(result, "test");
    }
}

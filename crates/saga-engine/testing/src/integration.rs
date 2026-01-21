//! # Integration Test Helpers for saga-engine
//!
//! Provides utilities for integration testing with real PostgreSQL and NATS.
//! These helpers are designed for use in test binaries, not in production code.
//!
//! # Usage
//!
//! ```rust,ignore
//! #[tokio::test]
//! async fn test_with_real_db() {
//!     // Setup: Start containers or connect to test services
//!     let db = TestDatabase::new().await;
//!     let nats = TestNats::new().await;
//!
//!     // Create engine with real connections
//!     let engine = SagaEngine::builder()
//!         .with_event_store(db.event_store())
//!         .with_task_queue(nats.task_queue())
//!         .build()
//!         .await;
//!
//!     // Run tests...
//!
//!     // Cleanup: Automatic when dropped
//! }
//! ```

use std::sync::Arc;
use tokio::sync::Mutex;

#[cfg(feature = "test-postgres")]
pub mod test_database;

#[cfg(feature = "test-nats")]
pub mod test_nats;

#[cfg(feature = "test-postgres")]
pub use test_database::{IntegrationError, TestDatabase, TestPostgresConfig};

#[cfg(feature = "test-nats")]
pub use test_nats::{IntegrationError as NatsIntegrationError, TestNats, TestNatsConfig};

/// Test fixtures for common test data
pub mod fixtures {
    use saga_engine_core::event::SagaId;

    /// Create a unique saga ID for each test
    pub fn unique_saga_id() -> SagaId {
        SagaId(format!(
            "test-{}",
            uuid::Uuid::new_v4()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>()
        ))
    }

    /// Default test timeout
    pub fn default_test_timeout() -> std::time::Duration {
        std::time::Duration::from_secs(30)
    }

    /// Create test input for ProvisioningWorkflow
    #[cfg(feature = "test-postgres")]
    pub fn provisioning_test_input() -> crate::test_database::serde_json::Value {
        serde_json::json!({
            "provider_id": "test-provider",
            "spec": {
                "command": "echo",
                "args": ["hello"],
                "env": {},
                "resources": {
                    "cpu": 1.0,
                    "memory_mb": 128
                }
            },
            "job_id": null
        })
    }
}

/// Builder for integration test scenarios
pub struct IntegrationTestBuilder {
    #[cfg(feature = "test-postgres")]
    database: Option<Arc<Mutex<Option<TestDatabase>>>>,
    #[cfg(feature = "test-nats")]
    nats: Option<Arc<Mutex<Option<TestNats>>>>,
}

impl Default for IntegrationTestBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl IntegrationTestBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "test-postgres")]
            database: None,
            #[cfg(feature = "test-nats")]
            nats: None,
        }
    }

    /// Configure test database
    #[cfg(feature = "test-postgres")]
    pub async fn with_database(mut self) -> Self {
        let db = TestDatabase::new()
            .await
            .expect("Failed to create test database");
        self.database = Some(Arc::new(Mutex::new(Some(db))));
        self
    }

    /// Configure test NATS
    #[cfg(feature = "test-nats")]
    pub async fn with_nats(mut self) -> Self {
        let nats = TestNats::new().await.expect("Failed to create test NATS");
        self.nats = Some(Arc::new(Mutex::new(Some(nats))));
        self
    }

    /// Build the test context
    pub async fn build(self) -> IntegrationTestContext {
        #[cfg(feature = "test-postgres")]
        let db = self.database.map(|m| m.lock().await.take()).flatten();
        #[cfg(feature = "test-nats")]
        let nats = self.nats.map(|m| m.lock().await.take()).flatten();

        IntegrationTestContext {
            #[cfg(feature = "test-postgres")]
            database: db,
            #[cfg(feature = "test-nats")]
            nats,
        }
    }
}

/// Context for integration tests
pub struct IntegrationTestContext {
    #[cfg(feature = "test-postgres")]
    database: Option<TestDatabase>,
    #[cfg(feature = "test-nats")]
    nats: Option<TestNats>,
}

impl IntegrationTestContext {
    /// Get database if configured
    #[cfg(feature = "test-postgres")]
    pub fn database(&self) -> Option<&TestDatabase> {
        self.database.as_ref()
    }

    /// Get NATS if configured
    #[cfg(feature = "test-nats")]
    pub fn nats(&self) -> Option<&TestNats> {
        self.nats.as_ref()
    }

    /// Create saga engine with available services
    #[cfg(all(feature = "test-postgres", feature = "test-nats"))]
    pub async fn create_engine(&self) -> Option<saga_engine_pg::SagaEngine> {
        use saga_engine_nats::NatsTaskQueue;
        use saga_engine_pg::{PostgresEventStore, PostgresTimerStore};

        let db = self.database.as_ref()?;
        let nats = self.nats.as_ref()?;

        let event_store = PostgresEventStore::new(db.pool_clone()).await.ok()?;
        let timer_store = PostgresTimerStore::new(db.pool_clone()).await.ok()?;
        let task_queue = NatsTaskQueue::new(nats.connection_string().to_string())
            .await
            .ok()?;

        Some(saga_engine_pg::SagaEngine::new(
            event_store,
            timer_store,
            task_queue,
        ))
    }

    /// Wait for condition with timeout
    pub async fn wait_for<F, T>(
        &self,
        condition: F,
        timeout: std::time::Duration,
        interval: std::time::Duration,
    ) -> Result<T, String>
    where
        F: Fn() -> Option<T>,
    {
        let start = std::time::Instant::now();
        loop {
            if let Some(result) = condition() {
                return Ok(result);
            }
            if start.elapsed() > timeout {
                return Err("Timeout waiting for condition".to_string());
            }
            tokio::time::sleep(interval).await;
        }
    }
}

#[cfg(all(feature = "test-postgres", feature = "test-nats"))]
impl Drop for IntegrationTestContext {
    fn drop(&mut self) {
        // Cleanup is handled by Drop implementations of TestDatabase and TestNats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixtures() {
        let id1 = fixtures::unique_saga_id();
        let id2 = fixtures::unique_saga_id();

        // Different IDs should be different
        assert_ne!(id1.0, id2.0);

        // Timeout should be reasonable
        assert!(fixtures::default_test_timeout().as_secs() >= 30);
    }

    #[test]
    fn test_integration_test_builder_default() {
        let builder = IntegrationTestBuilder::new();
        assert!(builder.database.is_none());
        assert!(builder.nats.is_none());
    }
}

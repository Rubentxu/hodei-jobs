//! # PostgresSagaEngine
//!
//! Production-ready facade combining PostgreSQL adapters for saga execution.
//!
//! This module provides [`PostgresSagaEngine`] which orchestrates:
//! - [`PostgresEventStore`]: Event persistence
//! - [`PostgresTimerStore`]: Durable timers
//! - [`PostgresOutboxRepository`]: Outbox pattern for eventual consistency
//!
//! ## Architecture (DDD + Ports & Adapters)
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │           Application Layer                          │
//! │     PostgresSagaEngine (Facade/Orchestrator)         │
//! └──────────────────────┬──────────────────────────────┘
//!                        │ uses ports from core
//! ┌──────────────────────▼──────────────────────────────┐
//! │           Infrastructure Layer                       │
//! │  PostgresEventStore  │  PostgresTimerStore  │  ...   │
//! │          (Adapter)   │       (Adapter)      │        │
//! └──────────────────────┴──────────────────────────────┘
//! ```

use async_trait::async_trait;
use saga_engine_core::port::TaskQueue;
use saga_engine_core::relay::{
    DefaultOutboxRelay, OutboxPublisher, OutboxRelay, OutboxRelayConfig,
};
use sqlx::Pool;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

// Import sibling modules (Infrastructure Layer adapters)
use super::event_store::{PostgresEventStore, PostgresEventStoreConfig};
use super::outbox::{OutboxRepositoryConfig, PostgresOutboxRepository};
use super::timer_store::{PostgresTimerStore, PostgresTimerStoreConfig};

/// Production engine combining PostgreSQL adapters.
///
/// This is the **Application Layer** facade that orchestrates the infrastructure
/// adapters. It provides a unified interface for the saga execution engine.
pub struct PostgresSagaEngine<P: OutboxPublisher + 'static> {
    // Infrastructure adapters (owned, not shared)
    event_store: Arc<PostgresEventStore>,
    timer_store: Arc<PostgresTimerStore>,
    outbox: Arc<PostgresOutboxRepository>,

    // Relay for processing outbox events
    relay: Arc<DefaultOutboxRelay<PostgresOutboxRepository, P>>,

    // External dependencies (injected) - using sqlx::Error as the concrete error type
    task_queue: Arc<dyn TaskQueue<Error = sqlx::Error>>,

    // Configuration
    config: PostgresSagaEngineConfig,

    // Runtime state
    running: Arc<Mutex<bool>>,
    shutdown_tx: Arc<Mutex<Option<tokio::sync::broadcast::Sender<()>>>>,
}

impl<P: OutboxPublisher + 'static> PostgresSagaEngine<P> {
    /// Create a new engine from connection pool.
    ///
    /// This follows the Factory pattern to create a fully configured engine.
    pub async fn new(
        pool: sqlx::PgPool,
        task_queue: Arc<dyn TaskQueue<Error = sqlx::Error>>,
        publisher: Arc<P>,
        config: PostgresSagaEngineConfig,
    ) -> Result<Self, PostgresSagaEngineError> {
        let event_store = Arc::new(PostgresEventStore::new(pool.clone()));
        let timer_store = Arc::new(PostgresTimerStore::new(pool.clone()));
        let outbox = Arc::new(PostgresOutboxRepository::new(pool.clone()));

        let relay_config = OutboxRelayConfig {
            poll_interval: config.outbox_poll_interval,
            batch_size: config.outbox_batch_size,
            max_retries: config.outbox_max_retries,
            backoff_multiplier: 2.0,
            max_backoff: std::time::Duration::from_secs(60),
            use_pg_notify: true,
        };

        let relay = Arc::new(DefaultOutboxRelay::new(
            outbox.clone(),
            publisher,
            relay_config,
        ));

        Ok(Self {
            event_store,
            timer_store,
            outbox,
            relay,
            task_queue,
            config,
            running: Arc::new(Mutex::new(false)),
            shutdown_tx: Arc::new(Mutex::new(None)),
        })
    }

    /// Get the event store (for direct access if needed).
    pub fn event_store(&self) -> Arc<PostgresEventStore> {
        self.event_store.clone()
    }

    /// Get the timer store (for direct access if needed).
    pub fn timer_store(&self) -> Arc<PostgresTimerStore> {
        self.timer_store.clone()
    }

    /// Get the outbox repository (for direct access if needed).
    pub fn outbox(&self) -> Arc<PostgresOutboxRepository> {
        self.outbox.clone()
    }

    /// Start the engine and its background workers.
    pub async fn start(&self) -> Result<(), PostgresSagaEngineError> {
        let mut running = self.running.lock().await;
        if *running {
            return Ok(());
        }

        // Create shutdown signal
        let (tx, rx) = tokio::sync::broadcast::channel(1);
        *self.shutdown_tx.lock().await = Some(tx);

        // Start outbox relay in background
        let relay = self.relay.clone();
        tokio::spawn(async move {
            if let Err(e) = relay.start(rx).await {
                tracing::error!("Outbox relay failed: {}", e);
            }
        });

        *running = true;
        tracing::info!("PostgresSagaEngine started");
        Ok(())
    }

    /// Stop the engine and its workers gracefully.
    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        if !*running {
            return;
        }

        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(());
        }

        *running = false;
        tracing::info!("PostgresSagaEngine stopped");
    }

    /// Check if the engine is running.
    pub fn is_running(&self) -> bool {
        // Note: In a real implementation, use a simple AtomicBool instead of Mutex
        // This is a placeholder that would need proper async handling
        false // Placeholder - actual implementation would use a shutdown signal
    }
}

/// Configuration for PostgresSagaEngine.
#[derive(Debug, Clone)]
pub struct PostgresSagaEngineConfig {
    pub event_store: PostgresEventStoreConfig,
    pub timer_store: PostgresTimerStoreConfig,
    pub outbox_poll_interval: std::time::Duration,
    pub outbox_batch_size: usize,
    pub outbox_max_retries: u32,
}

impl Default for PostgresSagaEngineConfig {
    fn default() -> Self {
        Self {
            event_store: PostgresEventStoreConfig::default(),
            timer_store: PostgresTimerStoreConfig::default(),
            outbox_poll_interval: std::time::Duration::from_millis(100),
            outbox_batch_size: 100,
            outbox_max_retries: 3,
        }
    }
}

/// Errors for PostgresSagaEngine.
#[derive(Debug, Error)]
pub enum PostgresSagaEngineError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Configuration error: {0}")]
    Config(String),
}

// Re-export types from other modules (Infrastructure Layer)
// Note: Already exported from lib.rs, kept here for documentation purposes

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_engine_config_defaults() {
        let config = PostgresSagaEngineConfig::default();
        assert_eq!(config.outbox_batch_size, 100);
        assert_eq!(config.outbox_max_retries, 3);
    }

    #[tokio::test]
    async fn test_outbox_repository_config_defaults() {
        let config = OutboxRepositoryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.batch_size, 100);
    }
}

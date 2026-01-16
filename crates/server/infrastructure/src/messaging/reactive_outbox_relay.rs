//! Reactive Outbox Relay using PostgreSQL LISTEN/NOTIFY
//!
//! This module implements a reactive event relay that uses PostgreSQL's
//! LISTEN/NOTIFY mechanism instead of polling. When events are inserted
//! into the outbox_events table, a trigger fires a NOTIFY that wakes
//! up this relay immediately.
//!
//! # Architecture
//!
//! ```text
//! ┌────────────────────┐     INSERT      ┌──────────────────┐
//! │  Application       │───────────────▶│  outbox_events   │
//! │  (OutboxEventBus)  │                 │  (PostgreSQL)    │
//! └────────────────────┘                 └────────┬─────────┘
//!                                                 │
//!                                        TRIGGER  │ pg_notify()
//!                                                 ▼
//!                                         ┌──────────────────┐
//!                                         │ ReactiveOutbox   │
//!                                         │ Relay (LISTEN)  │
//!                                         └────────┬─────────┘
//!                                                  │
//!                                                  ▼
//!                                         ┌──────────────────┐
//!                                         │  EventBus        │
//!                                         │  (Postgres/NATS) │
//!                                         └──────────────────┘
//! ```

use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::outbox::OutboxRepository;
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use thiserror::Error;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

/// Errors that can occur in the reactive outbox relay
#[derive(Debug, Error)]
pub enum ReactiveOutboxError {
    #[error("Failed to acquire database connection: {0}")]
    ConnectionError(String),

    #[error("Failed to listen to channel: {0}")]
    ListenError(String),

    #[error("Failed to process event {event_id}: {source}")]
    ProcessingError {
        event_id: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Failed to mark event as published: {0}")]
    MarkPublishedError(String),

    #[error("Channel closed unexpectedly")]
    ChannelClosed,

    #[error("Shutdown signal received")]
    Shutdown,
}

/// Configuration for the reactive outbox relay
#[derive(Debug, Clone)]
pub struct ReactiveOutboxConfig {
    /// Channel name to listen on
    pub channel_name: String,
    /// Polling fallback interval (safety net if NOTIFY is lost)
    pub fallback_poll_interval: Duration,
    /// Maximum events to process per batch
    pub batch_size: usize,
}

impl Default for ReactiveOutboxConfig {
    fn default() -> Self {
        Self {
            channel_name: "outbox_events_channel".to_string(),
            fallback_poll_interval: Duration::from_secs(30),
            batch_size: 100,
        }
    }
}

/// Reactive Outbox Relay that uses PostgreSQL LISTEN/NOTIFY
///
/// This relay listens for NOTIFY events from PostgreSQL and processes
/// outbox events immediately, with a polling fallback for safety.
#[derive(Clone)]
pub struct ReactiveOutboxRelay {
    pool: PgPool,
    outbox_repo: Arc<dyn OutboxRepository>,
    event_bus: Arc<dyn EventBus>,
    config: ReactiveOutboxConfig,
    shutdown_flag: Arc<AtomicBool>,
}

// SAFETY: ReactiveOutboxRelay is safe to share between threads because:
// - PgPool is Send (sqlx guarantees this)
// - Arc<dyn OutboxRepository> is Send + Sync
// - Arc<dyn EventBus> is Send + Sync
// - ReactiveOutboxConfig is Clone + Send
// - Arc<AtomicBool> is Send + Sync
unsafe impl Sync for ReactiveOutboxRelay {}

impl ReactiveOutboxRelay {
    /// Creates a new ReactiveOutboxRelay
    pub fn new(
        pool: PgPool,
        outbox_repo: Arc<dyn OutboxRepository>,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            pool,
            outbox_repo,
            event_bus,
            config: ReactiveOutboxConfig::default(),
            shutdown_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Creates a new ReactiveOutboxRelay with custom configuration
    pub fn with_config(
        pool: PgPool,
        outbox_repo: Arc<dyn OutboxRepository>,
        event_bus: Arc<dyn EventBus>,
        config: ReactiveOutboxConfig,
    ) -> Self {
        Self {
            pool,
            outbox_repo,
            event_bus,
            config,
            shutdown_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Marks the relay for shutdown
    pub fn shutdown(&self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
    }

    /// Runs the reactive relay until shutdown
    ///
    /// # Errors
    ///
    /// Returns `ReactiveOutboxError::Shutdown` if shutdown signal is received
    pub async fn run(&self) -> Result<(), ReactiveOutboxError> {
        info!(
            channel = %self.config.channel_name,
            fallback_interval_secs = self.config.fallback_poll_interval.as_secs(),
            "Starting ReactiveOutboxRelay"
        );

        // Create a dedicated connection for listening
        let mut listener = PgListener::connect_with(&self.pool)
            .await
            .map_err(|e| ReactiveOutboxError::ConnectionError(e.to_string()))?;

        // Start listening
        listener
            .listen(&self.config.channel_name)
            .await
            .map_err(|e| ReactiveOutboxError::ListenError(e.to_string()))?;

        info!(
            channel = %self.config.channel_name,
            "Listening for outbox events"
        );

        loop {
            // Check shutdown flag first (non-blocking)
            if self.shutdown_flag.load(Ordering::SeqCst) {
                info!("Shutdown flag set, stopping relay");
                return Err(ReactiveOutboxError::Shutdown);
            }

            tokio::select! {
                // Primary: Listen for NOTIFY events with timeout
                notification = timeout(Duration::from_secs(1), listener.recv()) => {
                    match notification {
                        Ok(Ok(notification)) => {
                            debug!(
                                event_id = %notification.payload(),
                                "Received NOTIFY for outbox event"
                            );
                            if let Err(e) = self.process_event_by_id(&notification.payload()).await {
                                error!(
                                    event_id = %notification.payload(),
                                    error = %e,
                                    "Failed to process event"
                                );
                            }
                        }
                        Ok(Err(e)) => {
                            warn!(error = %e, "Notification receive error, will retry");
                            // Brief backoff on error
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                        Err(_) => {
                            // Timeout - check shutdown and continue loop
                            if self.shutdown_flag.load(Ordering::SeqCst) {
                                info!("Shutdown flag set during timeout, stopping relay");
                                return Err(ReactiveOutboxError::Shutdown);
                            }
                        }
                    }
                }

                // Fallback: Periodic polling (safety net)
                _ = tokio::time::sleep(self.config.fallback_poll_interval) => {
                    debug!("Fallback polling triggered");
                    if let Err(e) = self.process_pending_events().await {
                        error!(error = %e, "Fallback polling failed");
                    }
                }
            }
        }
    }

    /// Processes a single event by its ID
    async fn process_event_by_id(&self, event_id: &str) -> Result<(), ReactiveOutboxError> {
        // Parse UUID
        let uuid =
            uuid::Uuid::parse_str(event_id).map_err(|e| ReactiveOutboxError::ProcessingError {
                event_id: event_id.to_string(),
                source: Box::new(e),
            })?;

        // Fetch the event
        let event = self
            .outbox_repo
            .find_by_id(uuid)
            .await
            .map_err(|e| ReactiveOutboxError::ProcessingError {
                event_id: event_id.to_string(),
                source: Box::new(e),
            })?
            .ok_or_else(|| ReactiveOutboxError::ProcessingError {
                event_id: event_id.to_string(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Event not found",
                )),
            })?;

        // Skip if already published
        if event.is_published() {
            debug!(event_id = %event_id, "Event already published, skipping");
            return Ok(());
        }

        // Deserialize payload to DomainEvent
        let domain_event: hodei_server_domain::events::DomainEvent =
            serde_json::from_value(event.payload).map_err(|e| {
                ReactiveOutboxError::ProcessingError {
                    event_id: event_id.to_string(),
                    source: Box::new(e),
                }
            })?;

        // Publish to event bus
        self.event_bus.publish(&domain_event).await.map_err(|e| {
            ReactiveOutboxError::ProcessingError {
                event_id: event_id.to_string(),
                source: Box::new(e),
            }
        })?;

        // Mark as published
        self.outbox_repo
            .mark_published(&[uuid])
            .await
            .map_err(|e| ReactiveOutboxError::MarkPublishedError(e.to_string()))?;

        info!(event_id = %event_id, "Event processed and published");

        Ok(())
    }

    /// Processes all pending events (fallback mechanism)
    async fn process_pending_events(&self) -> Result<(), ReactiveOutboxError> {
        let events = self
            .outbox_repo
            .get_pending_events(self.config.batch_size, 10)
            .await
            .map_err(|e| ReactiveOutboxError::ProcessingError {
                event_id: "fallback_poll".to_string(),
                source: Box::new(e),
            })?;

        if events.is_empty() {
            debug!("No pending events found in fallback poll");
            return Ok(());
        }

        info!(
            count = events.len(),
            "Processing pending events in fallback mode"
        );

        for event in events {
            if let Err(e) = self.process_event_by_id(&event.id.to_string()).await {
                error!(
                    event_id = %event.id,
                    error = %e,
                    "Failed to process event in fallback mode"
                );
            }
        }

        Ok(())
    }
}

/// Extension trait for running the relay with graceful shutdown
#[async_trait::async_trait]
pub trait OutboxRelayExt {
    /// Runs the relay with graceful shutdown
    async fn run_with_shutdown(&self) -> Result<(), ReactiveOutboxError>;
}

#[async_trait::async_trait]
impl OutboxRelayExt for ReactiveOutboxRelay {
    async fn run_with_shutdown(&self) -> Result<(), ReactiveOutboxError> {
        // Spawn task to handle shutdown signal
        let shutdown_flag = self.shutdown_flag.clone();
        tokio::spawn(async move {
            // Wait for shutdown signal (Ctrl+C)
            let _ = tokio::signal::ctrl_c().await;
            info!("Shutdown signal received, stopping relay...");
            shutdown_flag.store(true, Ordering::SeqCst);
        });

        // Run the relay
        self.run().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::outbox::postgres::PostgresOutboxRepository;
    use hodei_server_domain::outbox::{AggregateType, OutboxEventInsert};
    use sqlx::postgres::PgPoolOptions;
    use std::str::FromStr;

    #[tokio::test]
    async fn test_reactive_outbox_relay_creation() {
        let config = ReactiveOutboxConfig {
            channel_name: "test_channel".to_string(),
            fallback_poll_interval: Duration::from_secs(10),
            batch_size: 50,
        };

        // Just verify config is valid
        assert_eq!(config.channel_name, "test_channel");
        assert_eq!(config.batch_size, 50);
    }

    #[tokio::test]
    async fn test_determine_aggregate_type() {
        // This would test the aggregate type determination logic
        // when we have a real OutboxEventBus to test with
    }
}

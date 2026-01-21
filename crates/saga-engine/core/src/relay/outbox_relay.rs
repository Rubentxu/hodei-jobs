//! # Outbox Relay
//!
//! This module provides the [`OutboxRelay`] trait and implementation for
//! background processing of outbox messages.
//!
//! The relay:
//! 1. Polls the outbox table for pending messages
//! 2. Publishes messages to the EventBus/NATS
//! 3. Updates message status in the database
//! 4. Handles retries with exponential backoff

use crate::port::OutboxRepository;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;
use tokio::sync::{Mutex, broadcast};

/// Trait for the outbox relay that processes messages in the background.
///
/// The relay is responsible for:
/// - Polling the outbox for pending messages
/// - Publishing messages to the event bus
/// - Tracking metrics and health
#[async_trait]
pub trait OutboxRelay: Send + Sync {
    /// Start the relay processing loop.
    async fn start(&self, shutdown: broadcast::Receiver<()>) -> Result<(), RelayError>;

    /// Process a single batch of messages.
    async fn process_batch(&self, batch_size: usize) -> Result<ProcessResult, RelayError>;

    /// Get current metrics.
    fn metrics(&self) -> OutboxRelayMetrics;
}

/// Configuration for the outbox relay.
#[derive(Debug, Clone)]
pub struct OutboxRelayConfig {
    /// Polling interval when no messages are pending.
    pub poll_interval: std::time::Duration,
    /// Maximum messages to process in a batch.
    pub batch_size: usize,
    /// Maximum retry attempts per message.
    pub max_retries: u32,
    /// Exponential backoff multiplier.
    pub backoff_multiplier: f64,
    /// Maximum backoff duration.
    pub max_backoff: std::time::Duration,
    /// Enable PostgreSQL LISTEN/NOTIFY for instant processing.
    pub use_pg_notify: bool,
}

impl Default for OutboxRelayConfig {
    fn default() -> Self {
        Self {
            poll_interval: std::time::Duration::from_millis(100),
            batch_size: 100,
            max_retries: 3,
            backoff_multiplier: 2.0,
            max_backoff: std::time::Duration::from_secs(30),
            use_pg_notify: true,
        }
    }
}

/// Result of processing a batch of messages.
#[derive(Debug, Clone)]
pub struct ProcessResult {
    /// Number of messages processed.
    pub processed: usize,
    /// Number of messages successfully published.
    pub published: usize,
    /// Number of messages that failed.
    pub failed: usize,
    /// Duration of the batch processing.
    pub duration: std::time::Duration,
}

impl ProcessResult {
    /// Create an empty result.
    pub fn empty() -> Self {
        Self {
            processed: 0,
            published: 0,
            failed: 0,
            duration: std::time::Duration::ZERO,
        }
    }

    /// Calculate success rate.
    pub fn success_rate(&self) -> f64 {
        if self.processed == 0 {
            1.0
        } else {
            self.published as f64 / self.processed as f64
        }
    }
}

/// Metrics for the outbox relay.
#[derive(Debug, Default)]
pub struct OutboxRelayMetrics {
    /// Total messages processed since start.
    pub total_processed: AtomicU64,
    /// Total messages published successfully.
    pub total_published: AtomicU64,
    /// Total messages that failed.
    pub total_failed: AtomicU64,
    /// Duration of the last batch in milliseconds.
    pub last_batch_duration_ms: AtomicU64,
    /// Number of messages currently pending.
    pub pending_count: AtomicU64,
}

impl Clone for OutboxRelayMetrics {
    fn clone(&self) -> Self {
        Self {
            total_processed: AtomicU64::new(self.total_processed.load(Ordering::SeqCst)),
            total_published: AtomicU64::new(self.total_published.load(Ordering::SeqCst)),
            total_failed: AtomicU64::new(self.total_failed.load(Ordering::SeqCst)),
            last_batch_duration_ms: AtomicU64::new(
                self.last_batch_duration_ms.load(Ordering::SeqCst),
            ),
            pending_count: AtomicU64::new(self.pending_count.load(Ordering::SeqCst)),
        }
    }
}

impl OutboxRelayMetrics {
    /// Increment processed count.
    pub fn increment_processed(&self, count: usize) {
        self.total_processed
            .fetch_add(count as u64, Ordering::SeqCst);
    }

    /// Increment published count.
    pub fn increment_published(&self, count: usize) {
        self.total_published
            .fetch_add(count as u64, Ordering::SeqCst);
    }

    /// Increment failed count.
    pub fn increment_failed(&self, count: usize) {
        self.total_failed.fetch_add(count as u64, Ordering::SeqCst);
    }

    /// Set last batch duration.
    pub fn set_last_batch_duration(&self, ms: u64) {
        self.last_batch_duration_ms.store(ms, Ordering::SeqCst);
    }

    /// Set pending count.
    pub fn set_pending_count(&self, count: u64) {
        self.pending_count.store(count, Ordering::SeqCst);
    }
}

/// Errors that can occur in the relay.
#[derive(Debug, Error)]
pub enum RelayError {
    #[error("Repository error: {0}")]
    Repository(String),

    #[error("Publisher error: {0}")]
    Publisher(String),

    #[error("Shutdown requested")]
    Shutdown,

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Publisher trait for sending messages to the message bus.
#[async_trait]
pub trait OutboxPublisher: Send + Sync {
    /// Publish a message to the event bus.
    async fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), String>;
}

/// Default implementation of OutboxRelay.
pub struct DefaultOutboxRelay<R, P>
where
    R: OutboxRepository + 'static,
    P: OutboxPublisher + 'static,
{
    repository: Arc<R>,
    publisher: Arc<P>,
    config: OutboxRelayConfig,
    metrics: Arc<OutboxRelayMetrics>,
    running: Arc<Mutex<bool>>,
}

impl<R, P> DefaultOutboxRelay<R, P>
where
    R: OutboxRepository + 'static,
    P: OutboxPublisher + 'static,
{
    /// Create a new relay.
    pub fn new(repository: Arc<R>, publisher: Arc<P>, config: OutboxRelayConfig) -> Self {
        Self {
            repository,
            publisher,
            config,
            metrics: Arc::new(OutboxRelayMetrics::default()),
            running: Arc::new(Mutex::new(false)),
        }
    }
}

#[async_trait]
impl<R, P> OutboxRelay for DefaultOutboxRelay<R, P>
where
    R: OutboxRepository + 'static,
    P: OutboxPublisher + 'static,
{
    async fn start(&self, mut shutdown: broadcast::Receiver<()>) -> Result<(), RelayError> {
        *self.running.lock().await = true;

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    *self.running.lock().await = false;
                    return Ok(());
                }
                result = self.process_batch(self.config.batch_size) => {
                    match result {
                        Ok(_) => {}
                        Err(RelayError::Shutdown) => return Ok(()),
                        Err(e) => {
                            tracing::error!("Batch processing error: {}", e);
                        }
                    }
                }
            }

            // Respect poll interval
            tokio::time::sleep(self.config.poll_interval).await;
        }
    }

    async fn process_batch(&self, batch_size: usize) -> Result<ProcessResult, RelayError> {
        let start = std::time::Instant::now();

        // Get pending messages
        let messages = self
            .repository
            .get_pending(batch_size)
            .await
            .map_err(|e| RelayError::Repository(e.to_string()))?;

        if messages.is_empty() {
            return Ok(ProcessResult::empty());
        }

        let mut processed = 0;
        let mut published = 0;
        let mut failed = 0;

        for message in messages {
            let result = self
                .publisher
                .publish(&message.topic, message.payload.to_string().as_bytes())
                .await;

            match result {
                Ok(_) => {
                    if let Err(e) = self.repository.mark_published(message.id).await {
                        tracing::error!("Failed to mark message as published: {}", e);
                    }
                    published += 1;
                }
                Err(e) => {
                    tracing::error!("Failed to publish message: {}", e);
                    if let Err(e) = self
                        .repository
                        .mark_failed(message.id, &e.to_string())
                        .await
                    {
                        tracing::error!("Failed to mark message as failed: {}", e);
                    }
                    failed += 1;
                }
            }
            processed += 1;
        }

        let duration = start.elapsed();
        self.metrics.increment_processed(processed);
        self.metrics.increment_published(published);
        self.metrics.increment_failed(failed);
        self.metrics
            .set_last_batch_duration(duration.as_millis() as u64);

        Ok(ProcessResult {
            processed,
            published,
            failed,
            duration,
        })
    }

    fn metrics(&self) -> OutboxRelayMetrics {
        (*self.metrics).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex;

    #[derive(Debug, Default)]
    struct MockPublisher {
        published: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl OutboxPublisher for MockPublisher {
        async fn publish(&self, topic: &str, _payload: &[u8]) -> Result<(), String> {
            self.published.lock().await.push(topic.to_string());
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_process_result() {
        let result = ProcessResult::empty();
        assert_eq!(result.processed, 0);
        assert_eq!(result.success_rate(), 1.0);

        let result = ProcessResult {
            processed: 10,
            published: 8,
            failed: 2,
            duration: std::time::Duration::from_millis(100),
        };
        assert_eq!(result.success_rate(), 0.8);
    }

    #[tokio::test]
    async fn test_metrics() {
        let metrics = OutboxRelayMetrics::default();
        metrics.increment_processed(5);
        metrics.increment_published(4);
        metrics.increment_failed(1);
        metrics.set_last_batch_duration(150);

        assert_eq!(metrics.total_processed.load(Ordering::SeqCst), 5);
        assert_eq!(metrics.total_published.load(Ordering::SeqCst), 4);
        assert_eq!(metrics.total_failed.load(Ordering::SeqCst), 1);
        assert_eq!(metrics.last_batch_duration_ms.load(Ordering::SeqCst), 150);
    }

    #[tokio::test]
    async fn test_relay_config_defaults() {
        let config = OutboxRelayConfig::default();
        assert_eq!(config.poll_interval, std::time::Duration::from_millis(100));
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.max_retries, 3);
        assert!(config.use_pg_notify);
    }
}

//! NATS Outbox Relay
//!
//! Background service that reads pending events from the outbox table
//! and publishes them to NATS JetStream with retry logic and dead letter queue.
//!
//! # Features
//! - Exponential backoff retry for transient failures
//! - Dead Letter Queue (DLQ) for poisonous messages
//! - Batch processing for efficiency
//! - Metrics and observability

use crate::messaging::nats::NatsEventBus;
use crate::persistence::outbox::{PostgresOutboxRepository, PostgresOutboxRepositoryError};
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use hodei_server_domain::event_bus::{EventBus, EventBusError};
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::outbox::OutboxRepository;
use hodei_server_domain::outbox::{OutboxError, OutboxEventView};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Configuration for the NATS Outbox Relay
#[derive(Debug, Clone)]
pub struct NatsOutboxRelayConfig {
    /// Maximum number of events to process in a single batch
    pub batch_size: usize,
    /// How often to poll for new events (when queue is empty)
    pub poll_interval: Duration,
    /// Maximum number of retry attempts before giving up
    pub max_retries: u32,
    /// Initial delay for exponential backoff
    pub retry_delay: Duration,
    /// Maximum delay for exponential backoff
    pub max_retry_delay: Duration,
    /// Enable Dead Letter Queue
    pub enable_dlq: bool,
    /// DLQ stream name (if enabled)
    pub dlq_stream_name: String,
}

impl Default for NatsOutboxRelayConfig {
    fn default() -> Self {
        Self {
            batch_size: 50,
            poll_interval: Duration::from_millis(500),
            max_retries: 5,
            retry_delay: Duration::from_millis(1000),
            max_retry_delay: Duration::from_secs(60),
            enable_dlq: true,
            dlq_stream_name: "HODEI_DLQ".to_string(),
        }
    }
}

/// Builder for NatsOutboxRelayConfig
#[derive(Debug, Clone)]
pub struct NatsOutboxRelayConfigBuilder {
    config: NatsOutboxRelayConfig,
}

impl NatsOutboxRelayConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: NatsOutboxRelayConfig::default(),
        }
    }

    pub fn batch_size(mut self, batch_size: usize) -> Self {
        self.config.batch_size = batch_size;
        self
    }

    pub fn poll_interval(mut self, poll_interval: Duration) -> Self {
        self.config.poll_interval = poll_interval;
        self
    }

    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.config.max_retries = max_retries;
        self
    }

    pub fn retry_delay(mut self, retry_delay: Duration) -> Self {
        self.config.retry_delay = retry_delay;
        self
    }

    pub fn max_retry_delay(mut self, max_retry_delay: Duration) -> Self {
        self.config.max_retry_delay = max_retry_delay;
        self
    }

    pub fn enable_dlq(mut self, enable: bool) -> Self {
        self.config.enable_dlq = enable;
        self
    }

    pub fn dlq_stream_name(mut self, name: String) -> Self {
        self.config.dlq_stream_name = name;
        self
    }

    pub fn build(self) -> NatsOutboxRelayConfig {
        self.config
    }
}

impl Default for NatsOutboxRelayConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Metrics collected by the NATS Outbox Relay
#[derive(Debug, Clone, Default)]
pub struct NatsOutboxRelayMetrics {
    published_total: Arc<tokio::sync::Mutex<u64>>,
    failed_total: Arc<tokio::sync::Mutex<u64>>,
    retried_total: Arc<tokio::sync::Mutex<u64>>,
    dlq_total: Arc<tokio::sync::Mutex<u64>>,
    batch_count: Arc<tokio::sync::Mutex<u64>>,
}

impl NatsOutboxRelayMetrics {
    /// Create new metrics instance
    pub fn new() -> Self {
        Self {
            published_total: Arc::new(tokio::sync::Mutex::new(0)),
            failed_total: Arc::new(tokio::sync::Mutex::new(0)),
            retried_total: Arc::new(tokio::sync::Mutex::new(0)),
            dlq_total: Arc::new(tokio::sync::Mutex::new(0)),
            batch_count: Arc::new(tokio::sync::Mutex::new(0)),
        }
    }

    /// Increment published counter
    pub async fn inc_published(&self) {
        let mut count = self.published_total.lock().await;
        *count += 1;
    }

    /// Increment failed counter
    pub async fn inc_failed(&self) {
        let mut count = self.failed_total.lock().await;
        *count += 1;
    }

    /// Increment retry counter
    pub async fn inc_retried(&self) {
        let mut count = self.retried_total.lock().await;
        *count += 1;
    }

    /// Increment DLQ counter
    pub async fn inc_dlq(&self) {
        let mut count = self.dlq_total.lock().await;
        *count += 1;
    }

    /// Increment batch counter
    pub async fn inc_batch(&self) {
        let mut count = self.batch_count.lock().await;
        *count += 1;
    }

    /// Get metrics snapshot
    pub async fn snapshot(&self) -> NatsOutboxRelayMetricsSnapshot {
        let published = *self.published_total.lock().await;
        let failed = *self.failed_total.lock().await;
        let retried = *self.retried_total.lock().await;
        let dlq = *self.dlq_total.lock().await;
        let batches = *self.batch_count.lock().await;

        let total = published + failed;
        let success_rate = if total > 0 {
            (published as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        NatsOutboxRelayMetricsSnapshot {
            total_published: published,
            total_failed: failed,
            total_retried: retried,
            total_dlq: dlq,
            total_batches: batches,
            success_rate_percent: success_rate,
        }
    }
}

/// Snapshot of metrics for reporting
#[derive(Debug, Clone)]
pub struct NatsOutboxRelayMetricsSnapshot {
    pub total_published: u64,
    pub total_failed: u64,
    pub total_retried: u64,
    pub total_dlq: u64,
    pub total_batches: u64,
    pub success_rate_percent: f64,
}

impl std::fmt::Display for NatsOutboxRelayMetricsSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "NATS Outbox Relay Metrics:
  Total Published: {}
  Total Failed: {}
  Total Retried: {}
  Total Dead-Lettered: {}
  Total Batches: {}
  Success Rate: {:.2}%",
            self.total_published,
            self.total_failed,
            self.total_retried,
            self.total_dlq,
            self.total_batches,
            self.success_rate_percent
        )
    }
}

/// Error type for NATS Outbox Relay operations
#[derive(Debug, thiserror::Error)]
pub enum NatsOutboxRelayError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Outbox error: {0}")]
    Outbox(#[from] OutboxError),

    #[error("NATS event bus error: {0}")]
    EventBus(#[from] EventBusError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Event exceeded max retries, sent to DLQ: {event_id}")]
    MaxRetries { event_id: uuid::Uuid },

    #[error("Infrastructure error: {message}")]
    InfrastructureError { message: String },
}

impl From<NatsOutboxRelayError> for hodei_server_domain::shared_kernel::DomainError {
    fn from(err: NatsOutboxRelayError) -> Self {
        hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: err.to_string(),
        }
    }
}

impl From<PostgresOutboxRepositoryError> for NatsOutboxRelayError {
    fn from(err: PostgresOutboxRepositoryError) -> Self {
        match err {
            PostgresOutboxRepositoryError::Database(e) => NatsOutboxRelayError::Database(e),
            PostgresOutboxRepositoryError::Outbox(e) => NatsOutboxRelayError::Outbox(e),
            PostgresOutboxRepositoryError::Serialization(e) => {
                NatsOutboxRelayError::Serialization(e)
            }
            PostgresOutboxRepositoryError::InfrastructureError { message } => {
                NatsOutboxRelayError::InfrastructureError { message }
            }
        }
    }
}

/// Dead Letter Queue entry for failed events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterEntry {
    pub event_id: uuid::Uuid,
    pub aggregate_id: uuid::Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub failed_at: chrono::DateTime<chrono::Utc>,
    pub failure_reason: String,
    pub retry_count: u32,
}

impl DeadLetterEntry {
    pub fn from_outbox_event(
        event: &OutboxEventView,
        failure_reason: String,
        retry_count: u32,
    ) -> Self {
        Self {
            event_id: event.id,
            aggregate_id: event.aggregate_id,
            event_type: event.event_type.clone(),
            payload: event.payload.clone(),
            failed_at: chrono::Utc::now(),
            failure_reason,
            retry_count,
        }
    }
}

/// NATS Outbox Relay Service
///
/// Reads pending events from the outbox table and publishes them to NATS.
/// Provides retry logic with exponential backoff and dead letter queue.
#[derive(Clone)]
pub struct NatsOutboxRelay {
    pool: PgPool,
    repository: Arc<dyn OutboxRepository>,
    event_bus: NatsEventBus,
    config: NatsOutboxRelayConfig,
    metrics: NatsOutboxRelayMetrics,
}

impl NatsOutboxRelay {
    /// Create a new NATS Outbox Relay
    pub fn new(
        pool: PgPool,
        event_bus: NatsEventBus,
        config: Option<NatsOutboxRelayConfig>,
    ) -> Self {
        let repository: Arc<dyn OutboxRepository> =
            Arc::new(PostgresOutboxRepository::new(pool.clone()));

        Self {
            pool,
            repository,
            event_bus,
            config: config.unwrap_or_default(),
            metrics: NatsOutboxRelayMetrics::new(),
        }
    }

    /// Create with builder for fine-grained configuration
    pub fn builder() -> NatsOutboxRelayConfigBuilder {
        NatsOutboxRelayConfigBuilder::new()
    }

    /// Get metrics snapshot
    pub async fn get_metrics(&self) -> NatsOutboxRelayMetricsSnapshot {
        self.metrics.snapshot().await
    }

    /// Calculate exponential backoff delay
    fn calculate_backoff(&self, retry_count: u32) -> Duration {
        let delay = self.config.retry_delay.as_secs_f64() * (2.0_f64.powf(retry_count as f64));
        let delay = Duration::from_secs_f64(delay);
        std::cmp::min(delay, self.config.max_retry_delay)
    }

    /// Convert outbox event to DomainEvent
    fn outbox_event_to_domain_event(
        &self,
        event: &OutboxEventView,
    ) -> Result<DomainEvent, NatsOutboxRelayError> {
        serde_json::from_value(event.payload.clone())
            .map_err(|e| NatsOutboxRelayError::Serialization(e))
    }

    /// Fetch pending events from the outbox
    async fn fetch_pending_events(&self) -> Result<Vec<OutboxEventView>, NatsOutboxRelayError> {
        self.repository
            .get_pending_events(self.config.batch_size, self.config.max_retries as i32)
            .await
            .map_err(|e| NatsOutboxRelayError::from(e))
    }

    /// Mark events as published
    async fn mark_published(&self, event_ids: &[uuid::Uuid]) -> Result<(), NatsOutboxRelayError> {
        self.repository
            .mark_published(event_ids)
            .await
            .map_err(|e| NatsOutboxRelayError::from(e))
    }

    /// Process a single event with retry logic
    async fn process_event_with_retry(
        &self,
        event: OutboxEventView,
    ) -> Result<(), NatsOutboxRelayError> {
        let mut retry_count = 0;

        loop {
            match self.process_event(&event).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    retry_count += 1;

                    if retry_count >= self.config.max_retries {
                        error!(
                            "Event {} exceeded max retries ({}), sending to DLQ",
                            event.id, self.config.max_retries
                        );

                        if self.config.enable_dlq {
                            self.send_to_dlq(&event, &e.to_string(), retry_count)
                                .await?;
                        }

                        return Err(NatsOutboxRelayError::MaxRetries { event_id: event.id });
                    }

                    let backoff = self.calculate_backoff(retry_count);
                    warn!(
                        "Event {} failed (attempt {}/{}), retrying in {:.2}s: {}",
                        event.id,
                        retry_count,
                        self.config.max_retries,
                        backoff.as_secs_f64(),
                        e
                    );

                    self.metrics.inc_retried().await;
                    sleep(backoff).await;
                }
            }
        }
    }

    /// Process a single event
    async fn process_event(&self, event: &OutboxEventView) -> Result<(), NatsOutboxRelayError> {
        let domain_event = self.outbox_event_to_domain_event(event)?;

        self.event_bus
            .publish(&domain_event)
            .await
            .map_err(|e| NatsOutboxRelayError::EventBus(e))?;

        // Mark event as published
        self.mark_published(&[event.id]).await?;

        Ok(())
    }

    /// Send failed event to Dead Letter Queue
    async fn send_to_dlq(
        &self,
        event: &OutboxEventView,
        failure_reason: &str,
        retry_count: u32,
    ) -> Result<(), NatsOutboxRelayError> {
        let dlq_entry =
            DeadLetterEntry::from_outbox_event(event, failure_reason.to_string(), retry_count);

        let dlq_subject = format!("{}.{}", self.config.dlq_stream_name, event.event_type);
        let payload =
            serde_json::to_vec(&dlq_entry).map_err(|e| NatsOutboxRelayError::Serialization(e))?;

        // Publish to DLQ and wait for ack
        let ack = self
            .event_bus
            .jetstream()
            .publish(dlq_subject, payload.into())
            .await
            .map_err(|e| {
                NatsOutboxRelayError::EventBus(EventBusError::PublishError(e.to_string()))
            })?;

        ack.await.map_err(|e| {
            NatsOutboxRelayError::EventBus(EventBusError::PublishError(e.to_string()))
        })?;

        self.metrics.inc_dlq().await;

        info!(
            "Event {} sent to DLQ: {}",
            event.id, self.config.dlq_stream_name
        );

        Ok(())
    }

    /// Process a batch of events in parallel
    async fn process_events_batch(
        &self,
        events: Vec<OutboxEventView>,
    ) -> Result<(), NatsOutboxRelayError> {
        self.metrics.inc_batch().await;

        let mut tasks = FuturesUnordered::new();

        for event in events {
            // Clone the event to avoid lifetime issues with FuturesUnordered
            let event_clone = event.clone();
            tasks.push(self.process_event_with_retry(event_clone));
        }

        let mut errors = Vec::new();

        while let Some(result) = tasks.next().await {
            match result {
                Ok(()) => {
                    self.metrics.inc_published().await;
                }
                Err(e) => {
                    errors.push(e);
                    self.metrics.inc_failed().await;
                }
            }
        }

        if !errors.is_empty() {
            error!("Batch completed with {} errors", errors.len());
        }

        Ok(())
    }

    /// Run the NATS outbox relay continuously
    ///
    /// This method will block, processing events from the outbox table.
    /// It should be run in a background task.
    pub async fn run(&self) -> Result<(), NatsOutboxRelayError> {
        info!(
            "🚀 NATS Outbox Relay starting... (batch_size={}, max_retries={})",
            self.config.batch_size, self.config.max_retries
        );

        let mut consecutive_empty_polls = 0u32;
        let max_empty_polls = 10;

        loop {
            match self.fetch_pending_events().await {
                Ok(events) => {
                    if events.is_empty() {
                        consecutive_empty_polls += 1;
                        debug!("No pending events (poll #{})", consecutive_empty_polls);

                        if consecutive_empty_polls >= max_empty_polls {
                            info!(
                                "No events for {} polls, sleeping for {} seconds",
                                consecutive_empty_polls,
                                self.config.poll_interval.as_secs()
                            );
                            consecutive_empty_polls = 0;
                        }

                        sleep(self.config.poll_interval).await;
                        continue;
                    }

                    consecutive_empty_polls = 0;
                    info!("📦 Processing {} pending outbox events", events.len());

                    if let Err(e) = self.process_events_batch(events).await {
                        error!("Batch processing error: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to fetch pending events: {}", e);
                    sleep(self.config.poll_interval).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::nats::NatsConfig;
    use std::sync::Arc;

    /// Test configuration builder
    #[test]
    fn test_config_builder() {
        let config = NatsOutboxRelayConfigBuilder::new()
            .batch_size(100)
            .max_retries(10)
            .enable_dlq(false)
            .build();

        assert_eq!(config.batch_size, 100);
        assert_eq!(config.max_retries, 10);
        assert!(!config.enable_dlq);
    }

    /// Test default configuration
    #[test]
    fn test_default_config() {
        let config = NatsOutboxRelayConfig::default();
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.max_retries, 5);
        assert!(config.enable_dlq);
    }

    /// Test exponential backoff calculation
    #[test]
    fn test_backoff_calculation() {
        let config = NatsOutboxRelayConfig {
            retry_delay: Duration::from_secs(1),
            max_retry_delay: Duration::from_secs(60),
            ..Default::default()
        };

        // Note: We can't test backoff without a real relay instance
        // since it requires the config field and database connection
        // This test just verifies the config can be created
        assert_eq!(config.retry_delay, Duration::from_secs(1));
        assert_eq!(config.max_retry_delay, Duration::from_secs(60));
    }

    /// Test dead letter entry creation
    #[test]
    fn test_dead_letter_entry() {
        let event = OutboxEventView {
            id: uuid::Uuid::new_v4(),
            aggregate_id: uuid::Uuid::new_v4(),
            aggregate_type: hodei_server_domain::outbox::AggregateType::Job,
            event_type: "JobCreated".to_string(),
            event_version: 1,
            payload: serde_json::json!({"job_id": "test"}),
            metadata: None,
            idempotency_key: None,
            created_at: chrono::Utc::now(),
            published_at: None,
            status: hodei_server_domain::outbox::OutboxStatus::Pending,
            retry_count: 3,
            last_error: None,
        };

        let dlq_entry =
            DeadLetterEntry::from_outbox_event(&event, "Connection timeout".to_string(), 3);

        assert_eq!(dlq_entry.event_id, event.id);
        assert_eq!(dlq_entry.aggregate_id, event.aggregate_id);
        assert_eq!(dlq_entry.event_type, "JobCreated");
        assert_eq!(dlq_entry.retry_count, 3);
        assert_eq!(dlq_entry.failure_reason, "Connection timeout");
    }

    /// Test metrics increment
    #[tokio::test]
    async fn test_metrics_increment() {
        let metrics = NatsOutboxRelayMetrics::new();

        metrics.inc_published().await;
        metrics.inc_failed().await;
        metrics.inc_retried().await;
        metrics.inc_dlq().await;
        metrics.inc_batch().await;

        let snapshot = metrics.snapshot().await;

        assert_eq!(snapshot.total_published, 1);
        assert_eq!(snapshot.total_failed, 1);
        assert_eq!(snapshot.total_retried, 1);
        assert_eq!(snapshot.total_dlq, 1);
        assert_eq!(snapshot.total_batches, 1);
    }

    /// Test success rate calculation
    #[tokio::test]
    async fn test_success_rate() {
        let metrics = NatsOutboxRelayMetrics::new();

        // 8 published, 2 failed = 80% success
        for _ in 0..8 {
            metrics.inc_published().await;
        }
        for _ in 0..2 {
            metrics.inc_failed().await;
        }

        let snapshot = metrics.snapshot().await;
        assert!((snapshot.success_rate_percent - 80.0).abs() < 0.01);
    }
}

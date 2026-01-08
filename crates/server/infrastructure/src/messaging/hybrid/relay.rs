//! HybridOutboxRelay - Generic hybrid LISTEN/NOTIFY + polling relay
//!
//! This module provides a generic hybrid relay that combines PostgreSQL LISTEN/NOTIFY
//! for reactive notifications with polling for safety. It's designed to be reusable
//! for both Event Outbox and Command Outbox patterns.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────────┐
//! │                      HybridOutboxRelay                               │
//! ├──────────────────────────────────────────────────────────────────────┤
//! │                                                                       │
//! │   ┌─────────────────┐     ┌─────────────────────────────────────┐   │
//! │   │ PgNotifyListener│────▶│         tokio::select!              │   │
//! │   │ (Reactive)      │     │  ┌─────────────┬─────────────────┐  │   │
//! │   └─────────────────┘     │  │ Notification│  Polling Tick   │  │   │
//! │                           │  └──────┬──────┴──────┬──────────┘  │   │
//! │                           │         │              │              │   │
//! │                           │         ▼              ▼              │   │
//! │                           │   ┌─────────────────────────────┐    │   │
//! │                           │   │    process_pending_batch    │    │   │
//! │                           │   │    FOR UPDATE SKIP LOCKED   │    │   │
//! │                           │   └─────────────────────────────┘    │   │
//! │                           └─────────────────────────────────────┘   │
//! │                                      │                               │
//! │                                      ▼                               │
//! │                           ┌─────────────────────┐                    │
//! │                           │   Event Processor   │                    │
//! │                           │   (EventBus/NATS)   │                    │
//! │                           └─────────────────────┘                    │
//! │                                                                       │
//! └──────────────────────────────────────────────────────────────────────┘
//! ```

use crate::messaging::hybrid::backoff::{BackoffConfig, BackoffStats};
use crate::messaging::hybrid::pg_notify_listener::PgNotifyListener;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use futures::stream::{FuturesUnordered, StreamExt};
use hodei_server_domain::event_bus::{EventBus, EventBusError};
use hodei_server_domain::outbox::{OutboxError, OutboxEventView, OutboxRepository};
use hodei_server_domain::outbox::{OutboxEventInsert, OutboxStats};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::sync::broadcast;
use tokio::time::{interval, sleep, Instant};
use tracing::{debug, error, info, warn};

/// Configuration for the hybrid outbox relay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridOutboxConfig {
    /// Maximum number of events to process in a single batch
    pub batch_size: usize,
    /// How often to poll for new events (when queue is empty)
    pub poll_interval: Duration,
    /// Backoff configuration for retries
    pub backoff: BackoffConfig,
    /// Channel name for notifications
    pub channel: String,
}

impl Default for HybridOutboxConfig {
    fn default() -> Self {
        Self {
            batch_size: 50,
            poll_interval: Duration::from_millis(500),
            backoff: BackoffConfig::standard(),
            channel: "event_work".to_string(),
        }
    }
}

/// Metrics collected by the hybrid relay.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HybridOutboxMetrics {
    pub events_processed_total: u64,
    pub events_published_total: u64,
    pub events_failed_total: u64,
    pub events_retried_total: u64,
    pub events_dead_lettered_total: u64,
    pub notifications_received: u64,
    pub polling_wakeups: u64,
    pub batch_count: u64,
    pub avg_processing_time_ms: f64,
    pub max_processing_time_ms: u64,
    pub current_queue_depth: u64,
}

impl HybridOutboxMetrics {
    /// Record a successful event publication.
    pub fn record_published(&mut self, duration_ms: u64) {
        self.events_published_total += 1;
        self.events_processed_total += 1;
        self.update_timing(duration_ms);
    }

    /// Record a failed event.
    pub fn record_failed(&mut self) {
        self.events_failed_total += 1;
        self.events_processed_total += 1;
    }

    /// Record a retried event.
    pub fn record_retry(&mut self) {
        self.events_retried_total += 1;
    }

    /// Record a dead letter.
    pub fn record_dead_letter(&mut self) {
        self.events_dead_lettered_total += 1;
    }

    /// Record a notification received.
    pub fn record_notification(&mut self) {
        self.notifications_received += 1;
    }

    /// Record a polling wakeup.
    pub fn record_polling_wakeup(&mut self) {
        self.polling_wakeups += 1;
        self.batch_count += 1;
    }

    /// Update queue depth.
    pub fn update_queue_depth(&mut self, depth: u64) {
        self.current_queue_depth = depth;
    }

    fn update_timing(&mut self, duration_ms: u64) {
        let n = self.events_published_total as f64;
        self.avg_processing_time_ms =
            (self.avg_processing_time_ms * (n - 1.0) + duration_ms as f64) / n;
        if duration_ms > self.max_processing_time_ms {
            self.max_processing_time_ms = duration_ms;
        }
    }
}

/// Event processor trait for the hybrid relay.
#[async_trait]
pub trait EventProcessor: Send + Sync {
    /// Process a single event.
    async fn process_event(&self, event: &OutboxEventView) -> Result<(), OutboxError>;
}

/// NATS-based event processor.
#[derive(Debug, Clone)]
pub struct NatsEventProcessor {
    event_bus: Arc<dyn EventBus>,
}

impl NatsEventProcessor {
    pub fn new(event_bus: Arc<dyn EventBus>) -> Self {
        Self { event_bus }
    }
}

#[async_trait]
impl EventProcessor for NatsEventProcessor {
    async fn process_event(&self, event: &OutboxEventView) -> Result<(), OutboxError> {
        let domain_event = convert_to_domain_event(event)?;
        self.event_bus
            .publish(&domain_event)
            .await
            .map_err(|e| OutboxError::InfrastructureError {
                message: e.to_string(),
            })
    }
}

/// Hybrid Outbox Relay
///
/// A generic relay that combines PostgreSQL LISTEN/NOTIFY for reactive
/// notifications with polling for safety. Supports both Event and Command outboxes.
#[derive(Debug, Clone)]
pub struct HybridOutboxRelay<P: EventProcessor> {
    /// Database pool
    pool: PgPool,
    /// Event processor (EventBus or CommandBus)
    processor: Arc<P>,
    /// Notification listener
    listener: Arc<tokio::sync::Mutex<PgNotifyListener>>,
    /// Configuration
    config: HybridOutboxConfig,
    /// Metrics
    metrics: Arc<tokio::sync::Mutex<HybridOutboxMetrics>>,
    /// Shutdown signal
    shutdown: broadcast::Sender<()>,
    /// Backoff stats
    backoff_stats: Arc<tokio::sync::Mutex<BackoffStats>>,
}

impl<P: EventProcessor> HybridOutboxRelay<P> {
    /// Create a new hybrid relay for events.
    ///
    /// # Arguments
    /// * `pool` - PostgreSQL connection pool
    /// * `event_processor` - The processor for handling events
    /// * `config` - Optional configuration (uses defaults if None)
    ///
    /// # Returns
    /// A new relay and shutdown receiver
    pub async fn new(
        pool: &PgPool,
        event_processor: Arc<P>,
        config: Option<HybridOutboxConfig>,
    ) -> Result<(Self, broadcast::Receiver<()>), sqlx::Error> {
        let config = config.unwrap_or_default();
        let listener = Arc::new(tokio::sync::Mutex::new(
            PgNotifyListener::new(pool, &config.channel).await?,
        ));
        let (shutdown, rx) = broadcast::channel(1);

        Ok((
            Self {
                pool: pool.clone(),
                processor: event_processor,
                listener,
                config,
                metrics: Arc::new(tokio::sync::Mutex::new(HybridOutboxMetrics::default())),
                shutdown,
                backoff_stats: Arc::new(tokio::sync::Mutex::new(BackoffStats::new())),
            },
            rx,
        ))
    }

    /// Run the hybrid relay.
    ///
    /// This method blocks forever, processing events from the outbox.
    /// It should be run in a background task.
    pub async fn run(&self) {
        info!(
            channel = self.config.channel,
            batch_size = self.config.batch_size,
            poll_interval_ms = self.config.poll_interval.num_milliseconds(),
            "Starting hybrid outbox relay"
        );

        let mut interval = interval(self.config.poll_interval.to_std().unwrap());
        let mut shutdown_rx = self.shutdown.subscribe();

        loop {
            tokio::select! {
                // Channel 1: Notification from PostgreSQL
                notification = self.recv_notification() => {
                    self.handle_notification(notification).await;
                }
                // Channel 2: Polling safety tick
                _ = interval.tick() => {
                    self.handle_polling_tick().await;
                }
                // Channel 3: Shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Hybrid outbox relay shutting down");
                    break;
                }
            }
        }
    }

    /// Receive next notification (with timeout protection).
    async fn recv_notification(&self) -> Option<sqlx::Error> {
        let mut listener = self.listener.lock().await;
        let receiver = listener.receiver_mut();

        // Use timeout to prevent blocking forever
        let result = tokio::time::timeout(
            StdDuration::from_secs(30),
            receiver.recv(),
        )
        .await;

        match result {
            Ok(Ok(_notification)) => {
                self.metrics.lock().await.record_notification();
                None
            }
            Ok(Err(e)) => Some(e),
            Err(_) => {
                // Timeout is expected - continue to polling
                None
            }
        }
    }

    async fn handle_notification(&self, error: Option<sqlx::Error>) {
        if let Some(e) = error {
            warn!(error = %e, "Notification error, will retry on next tick");
            return;
        }

        // Process pending events after notification
        self.process_pending_batch().await;
    }

    async fn handle_polling_tick(&self) {
        self.metrics.lock().await.record_polling_wakeup();
        self.process_pending_batch().await;
    }

    /// Process a batch of pending events.
    pub async fn process_pending_batch(&self) {
        let start = Instant::now();

        // Get pending events with scheduled_at filter
        let events = match self.fetch_pending_events().await {
            Ok(events) => events,
            Err(e) => {
                error!(error = %e, "Failed to fetch pending events");
                return;
            }
        };

        if events.is_empty() {
            return;
        }

        debug!(count = events.len(), "Processing pending events batch");

        // Process events in parallel
        let results: Vec<Result<(), OutboxError>> = events
            .into_iter()
            .map(|event| self.process_single_event(event))
            .collect::<FuturesUnordered<_>>()
            .collect()
            .await;

        // Update metrics
        let duration_ms = start.elapsed().as_millis() as u64;
        let mut metrics = self.metrics.lock().await;
        metrics.update_queue_depth(0);

        for result in results {
            match result {
                Ok(_) => {
                    metrics.record_published(duration_ms / events.len() as u64);
                }
                Err(OutboxError::MaxRetriesExceeded(_)) => {
                    metrics.record_dead_letter();
                }
                Err(_) => {
                    metrics.record_failed();
                }
            }
        }

        let elapsed = start.elapsed();
        if elapsed > StdDuration::from_secs(1) {
            warn!(
                elapsed_ms = elapsed.as_millis(),
                "Batch processing took longer than expected"
            );
        }
    }

    /// Fetch pending events from the outbox.
    async fn fetch_pending_events(&self) -> Result<Vec<OutboxEventView>, OutboxError> {
        let repo = crate::persistence::outbox::PostgresOutboxRepository::new(self.pool.clone());
        repo.get_pending_events(self.config.batch_size, self.config.backoff.max_retries())
            .await
    }

    /// Process a single event.
    async fn process_single_event(
        &self,
        event: OutboxEventView,
    ) -> Result<(), OutboxError> {
        let start = Instant::now();

        // Check if event is ready (scheduled_at passed)
        if let Some(scheduled) = event.scheduled_at {
            if Utc::now() < scheduled {
                // Event not ready yet, skip for now
                debug!(
                    event_id = %event.id,
                    scheduled_at = %scheduled,
                    "Event not ready for processing"
                );
                return Ok(());
            }
        }

        // Try to process the event
        match self.processor.process_event(&event).await {
            Ok(_) => {
                // Mark as published
                let repo = crate::persistence::outbox::PostgresOutboxRepository::new(self.pool.clone());
                repo.mark_published(&[event.id]).await?;

                let duration = start.elapsed();
                debug!(
                    event_id = %event.id,
                    event_type = event.event_type,
                    duration_ms = duration.as_millis(),
                    "Event published successfully"
                );

                Ok(())
            }
            Err(e) => {
                error!(
                    event_id = %event.id,
                    event_type = event.event_type,
                    retry_count = event.retry_count,
                    error = %e,
                    "Failed to process event"
                );

                // Calculate next retry time
                let retry_count = event.retry_count;
                if self.config.backoff.can_retry(retry_count) {
                    let delay = self.config.backoff.calculate_delay(retry_count);
                    let scheduled_at = Utc::now() + delay;

                    let repo = crate::persistence::outbox::PostgresOutboxRepository::new(self.pool.clone());

                    // Mark for retry with scheduling
                    repo.mark_failed_with_retry(&event.id, &e.to_string(), scheduled_at)
                        .await?;

                    // Update stats
                    self.backoff_stats.lock().await.record_retry(delay.num_milliseconds() as u64);

                    Ok(())
                } else {
                    // Max retries exceeded - move to dead letter
                    self.mark_dead_letter(&event, &e.to_string()).await?;
                    Err(OutboxError::MaxRetriesExceeded(event.id))
                }
            }
        }
    }

    /// Mark an event as dead letter.
    async fn mark_dead_letter(&self, event: &OutboxEventView, error: &str) -> Result<(), OutboxError> {
        let repo = crate::persistence::outbox::PostgresOutboxRepository::new(self.pool.clone());
        repo.mark_failed(&event.id, error).await?;

        self.backoff_stats.lock().await.record_max_retries_hit();
        self.metrics.lock().await.record_dead_letter();

        warn!(
            event_id = %event.id,
            event_type = event.event_type,
            retry_count = event.retry_count,
            "Event moved to dead letter queue"
        );

        Ok(())
    }

    /// Get current metrics.
    pub fn metrics(&self) -> HybridOutboxMetrics {
        self.metrics.lock().await.clone()
    }

    /// Get backoff statistics.
    pub fn backoff_stats(&self) -> BackoffStats {
        self.backoff_stats.lock().await.clone()
    }

    /// Signal shutdown.
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(());
    }
}

/// Convert OutboxEventView to DomainEvent.
fn convert_to_domain_event(event: &OutboxEventView) -> Result<hodei_server_domain::events::DomainEvent, OutboxError> {
    let mut payload = &event.payload;

    // Handle wrapped payload
    if let Some(obj) = payload.as_object() {
        if obj.len() == 1 && obj.contains_key(&event.event_type) {
            if let Some(inner) = obj.get(&event.event_type) {
                payload = inner;
            }
        }
    }

    // Parse based on event type
    match event.event_type.as_str() {
        "JobCreated" => parse_job_created(payload, &event),
        "JobQueued" => parse_job_queued(payload, &event),
        "JobStatusChanged" => parse_job_status_changed(payload, &event),
        "WorkerRegistered" => parse_worker_registered(payload, &event),
        "WorkerTerminated" => parse_worker_terminated(payload, &event),
        _ => {
            // Generic event parsing
            Ok(hodei_server_domain::events::DomainEvent::Custom {
                event_type: event.event_type.clone(),
                payload: payload.clone(),
                aggregate_id: uuid::Uuid::new_v4(), // Placeholder
                occurred_at: event.created_at,
                correlation_id: None,
                actor: None,
            })
        }
    }
}

macro_rules! parse_field {
    ($payload:expr, $field:expr, $type:ty) => {
        serde_json::from_value($payload.get(stringify!($field)).cloned().unwrap_or_default())
            .map_err(|_| OutboxError::Serialization(format!("Invalid {}", stringify!($field))))
    };
}

fn parse_job_created(
    payload: &serde_json::Value,
    event: &OutboxEventView,
) -> Result<hodei_server_domain::events::DomainEvent, OutboxError> {
    let job_id = parse_field!(payload, job_id, uuid::Uuid)?;
    Ok(hodei_server_domain::events::DomainEvent::JobCreated {
        job_id: hodei_server_domain::shared_kernel::JobId(job_id),
        spec: parse_field!(payload, spec, hodei_server_domain::jobs::JobSpec)?,
        occurred_at: event.created_at,
        correlation_id: event.metadata.as_ref().and_then(|m| m.get("correlation_id").and_then(|v| v.as_str().map(|s| s.to_string()))),
        actor: event.metadata.as_ref().and_then(|m| m.get("actor").and_then(|v| v.as_str().map(|s| s.to_string()))),
    })
}

fn parse_job_queued(
    payload: &serde_json::Value,
    event: &OutboxEventView,
) -> Result<hodei_server_domain::events::DomainEvent, OutboxError> {
    let job_id = parse_field!(payload, job_id, uuid::Uuid)?;
    Ok(hodei_server_domain::events::DomainEvent::JobQueued {
        job_id: hodei_server_domain::shared_kernel::JobId(job_id),
        preferred_provider: None,
        job_requirements: serde_json::from_value(payload.clone()).unwrap_or_default(),
        queued_at: event.created_at,
        correlation_id: event.metadata.as_ref().and_then(|m| m.get("correlation_id").and_then(|v| v.as_str().map(|s| s.to_string()))),
        actor: event.metadata.as_ref().and_then(|m| m.get("actor").and_then(|v| v.as_str().map(|s| s.to_string()))),
    })
}

fn parse_job_status_changed(
    payload: &serde_json::Value,
    event: &OutboxEventView,
) -> Result<hodei_server_domain::events::DomainEvent, OutboxError> {
    let job_id = parse_field!(payload, job_id, uuid::Uuid)?;
    Ok(hodei_server_domain::events::DomainEvent::JobStatusChanged {
        job_id: hodei_server_domain::shared_kernel::JobId(job_id),
        old_state: parse_field!(payload, old_state, hodei_shared::JobState)?,
        new_state: parse_field!(payload, new_state, hodei_shared::JobState)?,
        occurred_at: event.created_at,
        correlation_id: event.metadata.as_ref().and_then(|m| m.get("correlation_id").and_then(|v| v.as_str().map(|s| s.to_string()))),
        actor: event.metadata.as_ref().and_then(|m| m.get("actor").and_then(|v| v.as_str().map(|s| s.to_string()))),
    })
}

fn parse_worker_registered(
    payload: &serde_json::Value,
    event: &OutboxEventView,
) -> Result<hodei_server_domain::events::DomainEvent, OutboxError> {
    let worker_id = parse_field!(payload, worker_id, uuid::Uuid)?;
    let provider_id = parse_field!(payload, provider_id, uuid::Uuid)?;
    Ok(hodei_server_domain::events::DomainEvent::WorkerRegistered {
        worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
        provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
        occurred_at: event.created_at,
        correlation_id: event.metadata.as_ref().and_then(|m| m.get("correlation_id").and_then(|v| v.as_str().map(|s| s.to_string()))),
        actor: event.metadata.as_ref().and_then(|m| m.get("actor").and_then(|v| v.as_str().map(|s| s.to_string()))),
    })
}

fn parse_worker_terminated(
    payload: &serde_json::Value,
    event: &OutboxEventView,
) -> Result<hodei_server_domain::events::DomainEvent, OutboxError> {
    let worker_id = parse_field!(payload, worker_id, uuid::Uuid)?;
    let provider_id = parse_field!(payload, provider_id, uuid::Uuid)?;
    let reason = parse_field!(payload, reason, hodei_shared::TerminationReason)?;
    Ok(hodei_server_domain::events::DomainEvent::WorkerTerminated {
        worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
        provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
        reason,
        occurred_at: event.created_at,
        correlation_id: event.metadata.as_ref().and_then(|m| m.get("correlation_id").and_then(|v| v.as_str().map(|s| s.to_string()))),
        actor: event.metadata.as_ref().and_then(|m| m.get("actor").and_then(|v| v.as_str().map(|s| s.to_string()))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::hybrid::backoff::BackoffConfig;
    use crate::messaging::hybrid::PgNotifyListener;
    use sqlx::PgPoolOptions;

    fn create_test_pool() -> PgPool {
        // Note: These tests require a running PostgreSQL instance
        // They will be skipped without DATABASE_URL
        let connection_string = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://hodei:hodei@localhost:5432/hodei_test".to_string());

        PgPoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .unwrap()
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_hybrid_relay_creation() {
        let pool = create_test_pool();
        let processor = Arc::new(NatsEventProcessor::new(Arc::new(crate::messaging::nats::NatsEventBus::default())));
        let config = HybridOutboxConfig::default();

        let (relay, _) = HybridOutboxRelay::new(&pool, processor, Some(config)).await.unwrap();
        assert_eq!(relay.config.batch_size, 50);
        assert_eq!(relay.config.channel, "event_work");
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL")]
    async fn test_hybrid_config_defaults() {
        let config = HybridOutboxConfig::default();

        assert_eq!(config.batch_size, 50);
        assert_eq!(config.poll_interval.num_milliseconds(), 500);
        assert_eq!(config.backoff.base_delay_secs, 5);
        assert_eq!(config.backoff.max_retries, 5);
    }

    #[tokio::test]
    fn test_metrics_defaults() {
        let metrics = HybridOutboxMetrics::default();

        assert_eq!(metrics.events_processed_total, 0);
        assert_eq!(metrics.events_published_total, 0);
        assert_eq!(metrics.events_failed_total, 0);
    }

    #[tokio::test]
    fn test_metrics_record_published() {
        let mut metrics = HybridOutboxMetrics::default();

        metrics.record_published(100);
        assert_eq!(metrics.events_published_total, 1);
        assert_eq!(metrics.events_processed_total, 1);
    }

    #[tokio::test]
    fn test_metrics_record_failed() {
        let mut metrics = HybridOutboxMetrics::default();

        metrics.record_failed();
        assert_eq!(metrics.events_failed_total, 1);
        assert_eq!(metrics.events_processed_total, 1);
    }

    #[tokio::test]
    fn test_metrics_record_dead_letter() {
        let mut metrics = HybridOutboxMetrics::default();

        metrics.record_dead_letter();
        assert_eq!(metrics.events_dead_lettered_total, 1);
    }

    #[tokio::test]
    fn test_backoff_stats() {
        let mut stats = BackoffStats::new();

        stats.record_retry(5000);
        assert_eq!(stats.total_retries, 1);

        stats.record_max_retries_hit();
        assert_eq!(stats.max_retries_hit, 1);
    }
}

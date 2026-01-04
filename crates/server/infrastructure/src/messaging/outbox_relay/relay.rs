//! Outbox Relay
//!
//! Background service that reads pending events from the outbox table
//! and publishes them to the event bus.

use crate::persistence::outbox::PostgresOutboxRepository;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use hodei_server_domain::event_bus::{EventBus, EventBusError};
use hodei_server_domain::events::{CleanupReason, DomainEvent, TerminationReason};
use hodei_server_domain::jobs::JobSpec;
use hodei_server_domain::outbox::OutboxRepository;
use hodei_server_domain::outbox::{OutboxError, OutboxEventView};
use hodei_server_domain::shared_kernel::DomainError;
use sqlx::postgres::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{Instant, sleep};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Configuration for the Outbox Relay
#[derive(Debug, Clone)]
pub struct OutboxRelayConfig {
    /// Maximum number of events to process in a single batch
    pub batch_size: usize,
    /// How often to poll for new events (when queue is empty)
    pub poll_interval: Duration,
    /// Maximum number of retry attempts before giving up
    pub max_retries: i32,
    /// Initial delay for exponential backoff
    pub retry_delay: Duration,
    /// Maximum delay for exponential backoff
    pub max_retry_delay: Duration,
}

impl Default for OutboxRelayConfig {
    fn default() -> Self {
        Self {
            batch_size: 50,
            poll_interval: Duration::from_millis(500),
            max_retries: 3,
            retry_delay: Duration::from_millis(1000),
            max_retry_delay: Duration::from_secs(30),
        }
    }
}

/// Metrics collected by the Outbox Relay
#[derive(Debug, Clone)]
pub struct OutboxRelayMetrics {
    // Counters
    pub events_published_total: u64,
    pub events_failed_total: u64,
    pub events_skipped_total: u64,
    pub events_retried_total: u64,
    pub events_dead_lettered_total: u64,
    pub batch_count: u64,

    // Timing metrics (in milliseconds)
    pub publish_duration_sum_ms: u64,
    pub fetch_duration_sum_ms: u64,
    pub max_publish_duration_ms: u64,
    pub min_publish_duration_ms: u64,

    // Current state
    pub current_queue_depth: u64,
    pub oldest_pending_event_age_seconds: Option<i64>,

    // Rate calculations (updated periodically)
    pub events_published_per_second: f64,
    pub events_failed_per_second: f64,
    pub error_rate_percent: f64,

    // Retry statistics
    pub total_retries: u64,
    pub max_retry_attempts_seen: i32,

    // Historical tracking for sliding windows
    pub events_in_last_minute: u64,
    pub events_in_last_hour: u64,
    pub last_minute_timestamp: std::time::Instant,
    pub last_hour_timestamp: std::time::Instant,
}

impl Default for OutboxRelayMetrics {
    fn default() -> Self {
        Self {
            events_published_total: 0,
            events_failed_total: 0,
            events_skipped_total: 0,
            events_retried_total: 0,
            events_dead_lettered_total: 0,
            batch_count: 0,
            publish_duration_sum_ms: 0,
            fetch_duration_sum_ms: 0,
            max_publish_duration_ms: 0,
            min_publish_duration_ms: 0,
            current_queue_depth: 0,
            oldest_pending_event_age_seconds: None,
            events_published_per_second: 0.0,
            events_failed_per_second: 0.0,
            error_rate_percent: 0.0,
            total_retries: 0,
            max_retry_attempts_seen: 0,
            events_in_last_minute: 0,
            events_in_last_hour: 0,
            last_minute_timestamp: std::time::Instant::now(),
            last_hour_timestamp: std::time::Instant::now(),
        }
    }
}

impl OutboxRelayMetrics {
    /// Create new metrics instance
    pub fn new() -> Self {
        let now = std::time::Instant::now();
        Self {
            events_published_total: 0,
            events_failed_total: 0,
            events_skipped_total: 0,
            events_retried_total: 0,
            events_dead_lettered_total: 0,
            batch_count: 0,
            publish_duration_sum_ms: 0,
            fetch_duration_sum_ms: 0,
            max_publish_duration_ms: 0,
            min_publish_duration_ms: u64::MAX,
            current_queue_depth: 0,
            oldest_pending_event_age_seconds: None,
            events_published_per_second: 0.0,
            events_failed_per_second: 0.0,
            error_rate_percent: 0.0,
            total_retries: 0,
            max_retry_attempts_seen: 0,
            events_in_last_minute: 0,
            events_in_last_hour: 0,
            last_minute_timestamp: now,
            last_hour_timestamp: now,
        }
    }

    /// Increment the published counter
    pub fn increment_published(&mut self, duration_ms: u64) {
        self.events_published_total += 1;
        self.publish_duration_sum_ms += duration_ms;

        // Update min/max
        if duration_ms < self.min_publish_duration_ms {
            self.min_publish_duration_ms = duration_ms;
        }
        if duration_ms > self.max_publish_duration_ms {
            self.max_publish_duration_ms = duration_ms;
        }

        // Update sliding windows
        self.update_sliding_windows();
    }

    /// Increment the failed counter
    pub fn increment_failed(&mut self) {
        self.events_failed_total += 1;
        self.update_sliding_windows();
    }

    /// Increment the skipped counter
    pub fn increment_skipped(&mut self) {
        self.events_skipped_total += 1;
    }

    /// Increment the batch counter
    pub fn increment_batch(&mut self) {
        self.batch_count += 1;
    }

    /// Update queue depth
    pub fn update_queue_depth(&mut self, depth: u64) {
        self.current_queue_depth = depth;
    }

    /// Update oldest pending event age
    pub fn update_oldest_event_age(&mut self, age_seconds: Option<i64>) {
        self.oldest_pending_event_age_seconds = age_seconds;
    }

    /// Record retry
    pub fn record_retry(&mut self, retry_count: i32) {
        self.events_retried_total += 1;
        self.total_retries += 1;
        if retry_count > self.max_retry_attempts_seen {
            self.max_retry_attempts_seen = retry_count;
        }
    }

    /// Record dead letter
    pub fn record_dead_letter(&mut self) {
        self.events_dead_lettered_total += 1;
    }

    /// Update sliding window counters
    fn update_sliding_windows(&mut self) {
        let now = std::time::Instant::now();

        // Update minute window
        if now.duration_since(self.last_minute_timestamp).as_secs() >= 60 {
            self.events_in_last_minute = 0;
            self.last_minute_timestamp = now;
        }
        self.events_in_last_minute += 1;

        // Update hour window
        if now.duration_since(self.last_hour_timestamp).as_secs() >= 3600 {
            self.events_in_last_hour = 0;
            self.last_hour_timestamp = now;
        }
        self.events_in_last_hour += 1;
    }

    /// Get average publish duration in milliseconds
    pub fn avg_publish_duration_ms(&self) -> f64 {
        if self.events_published_total == 0 {
            0.0
        } else {
            self.publish_duration_sum_ms as f64 / self.events_published_total as f64
        }
    }

    /// Get processing rate (events per second) over last minute
    pub fn processing_rate_per_second(&self) -> f64 {
        let elapsed_secs = self.last_minute_timestamp.elapsed().as_secs_f64();
        if elapsed_secs == 0.0 {
            0.0
        } else {
            self.events_in_last_minute as f64 / elapsed_secs
        }
    }

    /// Get success rate percentage
    pub fn success_rate_percent(&self) -> f64 {
        let total = self.events_published_total + self.events_failed_total;
        if total == 0 {
            100.0
        } else {
            (self.events_published_total as f64 / total as f64) * 100.0
        }
    }

    /// Get comprehensive metrics snapshot
    pub fn snapshot(&self) -> OutboxRelayMetricsSnapshot {
        OutboxRelayMetricsSnapshot {
            total_events_published: self.events_published_total,
            total_events_failed: self.events_failed_total,
            total_events_skipped: self.events_skipped_total,
            total_events_retried: self.events_retried_total,
            total_events_dead_lettered: self.events_dead_lettered_total,
            total_batches: self.batch_count,
            avg_publish_duration_ms: self.avg_publish_duration_ms(),
            min_publish_duration_ms: if self.min_publish_duration_ms == u64::MAX {
                0
            } else {
                self.min_publish_duration_ms
            },
            max_publish_duration_ms: self.max_publish_duration_ms,
            current_queue_depth: self.current_queue_depth,
            oldest_pending_event_age_seconds: self.oldest_pending_event_age_seconds,
            processing_rate_per_second: self.processing_rate_per_second(),
            success_rate_percent: self.success_rate_percent(),
            error_rate_percent: self.error_rate_percent,
            max_retry_attempts_seen: self.max_retry_attempts_seen,
        }
    }
}

/// Snapshot of metrics for reporting/export
#[derive(Debug, Clone)]
pub struct OutboxRelayMetricsSnapshot {
    pub total_events_published: u64,
    pub total_events_failed: u64,
    pub total_events_skipped: u64,
    pub total_events_retried: u64,
    pub total_events_dead_lettered: u64,
    pub total_batches: u64,
    pub avg_publish_duration_ms: f64,
    pub min_publish_duration_ms: u64,
    pub max_publish_duration_ms: u64,
    pub current_queue_depth: u64,
    pub oldest_pending_event_age_seconds: Option<i64>,
    pub processing_rate_per_second: f64,
    pub success_rate_percent: f64,
    pub error_rate_percent: f64,
    pub max_retry_attempts_seen: i32,
}

impl std::fmt::Display for OutboxRelayMetricsSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Outbox Relay Metrics:
  Total Events Published: {}
  Total Events Failed: {}
  Total Events Skipped: {}
  Total Events Retried: {}
  Total Events Dead-lettered: {}
  Total Batches Processed: {}
  Current Queue Depth: {}
  Oldest Pending Event Age: {} seconds
  Processing Rate: {:.2} events/sec
  Avg Publish Duration: {:.2} ms
  Min Publish Duration: {} ms
  Max Publish Duration: {} ms
  Success Rate: {:.2}%
  Error Rate: {:.2}%
  Max Retry Attempts Seen: {}",
            self.total_events_published,
            self.total_events_failed,
            self.total_events_skipped,
            self.total_events_retried,
            self.total_events_dead_lettered,
            self.total_batches,
            self.current_queue_depth,
            self.oldest_pending_event_age_seconds.unwrap_or(0),
            self.processing_rate_per_second,
            self.avg_publish_duration_ms,
            self.min_publish_duration_ms,
            self.max_publish_duration_ms,
            self.success_rate_percent,
            self.error_rate_percent,
            self.max_retry_attempts_seen
        )
    }
}

/// Error type for Outbox Relay operations
#[derive(Debug, thiserror::Error)]
pub enum OutboxRelayError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Outbox error: {0}")]
    Outbox(#[from] OutboxError),

    #[error("Event bus error: {0}")]
    EventBus(#[from] EventBusError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Postgres outbox error: {0}")]
    PostgresRepository(#[from] crate::persistence::outbox::postgres::PostgresOutboxRepositoryError),

    #[error("Infrastructure error: {message}")]
    InfrastructureError { message: String },
}

impl From<OutboxRelayError> for hodei_server_domain::shared_kernel::DomainError {
    fn from(err: OutboxRelayError) -> Self {
        match err {
            OutboxRelayError::Database(e) => {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: e.to_string(),
                }
            }
            OutboxRelayError::Outbox(e) => {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: e.to_string(),
                }
            }
            OutboxRelayError::EventBus(e) => {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: e.to_string(),
                }
            }
            OutboxRelayError::Serialization(e) => {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: e.to_string(),
                }
            }
            OutboxRelayError::PostgresRepository(e) => {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: e.to_string(),
                }
            }
            OutboxRelayError::InfrastructureError { message } => {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError { message }
            }
        }
    }
}

impl From<OutboxRelayError> for OutboxError {
    fn from(err: OutboxRelayError) -> Self {
        match err {
            OutboxRelayError::Database(e) => OutboxError::Database(e),
            OutboxRelayError::Outbox(e) => e,
            OutboxRelayError::EventBus(_) => OutboxError::InfrastructureError {
                message: "Event bus error in relay".to_string(),
            },
            OutboxRelayError::Serialization(e) => OutboxError::Serialization(e),
            OutboxRelayError::PostgresRepository(e) => OutboxError::InfrastructureError {
                message: e.to_string(),
            },
            OutboxRelayError::InfrastructureError { message } => {
                OutboxError::InfrastructureError { message }
            }
        }
    }
}

/// Outbox Relay Service
///
/// Reads pending events from the outbox table and publishes them to the event bus.
/// Runs continuously in a background task.
pub struct OutboxRelay {
    pool: PgPool,
    event_bus: Arc<dyn EventBus>,
    config: OutboxRelayConfig,
    metrics: std::sync::Arc<std::sync::Mutex<OutboxRelayMetrics>>,
}

impl OutboxRelay {
    /// Create a new Outbox Relay
    ///
    /// # Arguments
    /// * `pool` - PostgreSQL connection pool
    /// * `event_bus` - Event bus to publish events to
    /// * `config` - Relay configuration (uses default if None)
    ///
    /// # Returns
    /// * `Self` - New relay instance
    pub fn new(
        pool: PgPool,
        event_bus: Arc<dyn EventBus>,
        config: Option<OutboxRelayConfig>,
    ) -> Self {
        Self {
            pool,
            event_bus,
            config: config.unwrap_or_default(),
            metrics: Arc::new(std::sync::Mutex::new(OutboxRelayMetrics::new())),
        }
    }

    /// Create a new Outbox Relay with default configuration
    pub fn new_with_defaults(pool: PgPool, event_bus: Arc<dyn EventBus>) -> Self {
        Self::new(pool, event_bus, None)
    }

    /// Get a snapshot of current metrics (thread-safe)
    pub fn get_metrics(&self) -> OutboxRelayMetricsSnapshot {
        self.metrics.lock().unwrap().snapshot()
    }

    /// Get a reference to the metrics (for internal updates)
    fn metrics(&self) -> std::sync::MutexGuard<'_, OutboxRelayMetrics> {
        self.metrics.lock().unwrap()
    }

    /// Run the outbox relay continuously
    ///
    /// This method will block forever, processing events from the outbox table.
    /// It should be run in a background task using `tokio::spawn`.
    ///
    /// # Returns
    /// * `Result<(), OutboxRelayError>` - Error if the relay crashes
    pub async fn run(&self) -> Result<(), OutboxRelayError> {
        info!("🚀 Outbox Relay starting...");

        let mut consecutive_empty_polls = 0u32;
        let max_empty_polls = 10;

        loop {
            let start = Instant::now();

            // Get pending events from the outbox
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

                    // Process events in parallel for better throughput
                    self.process_events_batch(events).await?;
                }
                Err(e) => {
                    error!("Failed to fetch pending events: {}", e);
                    sleep(Duration::from_secs(5)).await;
                }
            }

            // Rate limiting - ensure we don't poll too aggressively
            let elapsed = start.elapsed();
            if elapsed < self.config.poll_interval {
                sleep(self.config.poll_interval - elapsed).await;
            }
        }
    }

    /// Fetch pending events from the outbox table
    async fn fetch_pending_events(&self) -> Result<Vec<OutboxEventView>, OutboxRelayError> {
        let repo = PostgresOutboxRepository::new(self.pool.clone());
        let events = repo
            .get_pending_events(self.config.batch_size, self.config.max_retries)
            .await?;
        Ok(events)
    }

    /// Process a batch of events
    async fn process_events_batch(
        &self,
        events: Vec<OutboxEventView>,
    ) -> Result<(), OutboxRelayError> {
        if events.is_empty() {
            return Ok(());
        }

        let mut stream = events
            .into_iter()
            .map(|event| self.process_single_event(event))
            .collect::<FuturesUnordered<_>>();

        let mut processed_count = 0u64;
        let mut failed_count = 0u64;

        while let Some(result) = stream.next().await {
            match result {
                Ok(_) => {
                    processed_count += 1;
                    self.metrics.lock().unwrap().increment_batch();
                }
                Err(e) => {
                    error!("Failed to process event: {}", e);
                    failed_count += 1;
                }
            }
        }

        if processed_count > 0 {
            info!(
                "✅ Successfully processed {} events ({} failed)",
                processed_count, failed_count
            );
        }

        Ok(())
    }

    /// Process a single event
    async fn process_single_event(
        &self,
        event: OutboxEventView,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let start = Instant::now();

        // Convert outbox event to domain event
        let domain_event = self.convert_to_domain_event(&event)?;
        info!(
            event_id = %event.id,
            event_type = %event.event_type,
            "[OutboxRelay] Converting event, about to publish to event bus"
        );

        // Publish to event bus
        if let Err(e) = self.event_bus.publish(&domain_event).await {
            error!(
                event_id = %event.id,
                event_type = %event.event_type,
                retry_count = event.retry_count,
                error = %e,
                "Failed to publish event to event bus"
            );

            // Mark as failed in the outbox
            self.mark_event_failed(event.id, &e.to_string()).await?;

            // Record retry statistics
            self.metrics().record_retry(event.retry_count);
            self.metrics().increment_failed();
            return Err(e.into());
        }

        // Mark as published in the outbox
        self.mark_event_published(event.id).await?;

        let duration = start.elapsed();
        let duration_ms = duration.as_millis() as u64;

        debug!(
            event_id = %event.id,
            event_type = %event.event_type,
            duration_ms = duration_ms,
            "Event published successfully"
        );

        self.metrics().increment_published(duration_ms);

        Ok(())
    }

    /// Convert an outbox event to a domain event
    fn convert_to_domain_event(
        &self,
        event: &OutboxEventView,
    ) -> Result<DomainEvent, hodei_server_domain::shared_kernel::DomainError> {
        let mut payload = &event.payload;

        // Check if payload is wrapped in the event type name (legacy/default serde serialization)
        // This handles cases where events were persisted with the enum variant wrapper
        if let Some(obj) = payload.as_object() {
            if obj.len() == 1 && obj.contains_key(&event.event_type) {
                if let Some(inner) = obj.get(&event.event_type) {
                    payload = inner;
                }
            }
        }

        // Parse the payload based on event type
        match event.event_type.as_str() {
            "JobCreated" => {
                let job_id =
                    serde_json::from_value::<Uuid>(payload["job_id"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid job_id in JobCreated event".to_string(),
                        }
                    })?;
                let job_id = hodei_server_domain::shared_kernel::JobId(job_id);

                Ok(DomainEvent::JobCreated {
                    job_id,
                    spec: serde_json::from_value(payload["spec"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid spec in JobCreated event".to_string(),
                        }
                    })?,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // EPIC-29: JobQueued event triggers reactive job processing
            "JobQueued" => {
                let job_id =
                    serde_json::from_value::<Uuid>(payload["job_id"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid job_id in JobQueued event".to_string(),
                        }
                    })?;
                let job_id = hodei_server_domain::shared_kernel::JobId(job_id);

                // Parse preferred_provider if present
                let preferred_provider = payload
                    .get("preferred_provider")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .map(hodei_server_domain::shared_kernel::ProviderId);

                // Get job_requirements or fallback to spec
                let job_requirements_value = payload
                    .get("job_requirements")
                    .or(payload.get("spec"))
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                Ok(DomainEvent::JobQueued {
                    job_id,
                    preferred_provider,
                    job_requirements: serde_json::from_value(job_requirements_value).map_err(
                        |_| DomainError::InfrastructureError {
                            message: "Invalid job_requirements in JobQueued event".to_string(),
                        },
                    )?,
                    queued_at: payload
                        .get("queued_at")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
                        .unwrap_or(event.created_at),
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            "JobAssigned" => {
                let job_id =
                    serde_json::from_value::<Uuid>(payload["job_id"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid job_id in JobAssigned event".to_string(),
                        }
                    })?;
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in JobAssigned event".to_string(),
                    })?;

                Ok(DomainEvent::JobAssigned {
                    job_id: hodei_server_domain::shared_kernel::JobId(job_id),
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            "JobAccepted" => {
                let job_id =
                    serde_json::from_value::<Uuid>(payload["job_id"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid job_id in JobAccepted event".to_string(),
                        }
                    })?;
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in JobAccepted event".to_string(),
                    })?;

                Ok(DomainEvent::JobAccepted {
                    job_id: hodei_server_domain::shared_kernel::JobId(job_id),
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            "JobStatusChanged" => {
                let job_id =
                    serde_json::from_value::<Uuid>(payload["job_id"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid job_id in JobStatusChanged event".to_string(),
                        }
                    })?;

                Ok(DomainEvent::JobStatusChanged {
                    job_id: hodei_server_domain::shared_kernel::JobId(job_id),
                    old_state: serde_json::from_value(payload["old_state"].clone()).map_err(
                        |_| DomainError::InfrastructureError {
                            message: "Invalid old_state in JobStatusChanged event".to_string(),
                        },
                    )?,
                    new_state: serde_json::from_value(payload["new_state"].clone()).map_err(
                        |_| DomainError::InfrastructureError {
                            message: "Invalid new_state in JobStatusChanged event".to_string(),
                        },
                    )?,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            "WorkerReady" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerReady event".to_string(),
                    })?;
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in WorkerReady event".to_string(),
                    })?;

                Ok(DomainEvent::WorkerReady {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    ready_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            "WorkerStatusChanged" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerStatusChanged event".to_string(),
                    })?;

                Ok(DomainEvent::WorkerStatusChanged {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    old_status: serde_json::from_value(payload["old_status"].clone()).map_err(
                        |_| DomainError::InfrastructureError {
                            message: "Invalid old_status in WorkerStatusChanged event".to_string(),
                        },
                    )?,
                    new_status: serde_json::from_value(payload["new_status"].clone()).map_err(
                        |_| DomainError::InfrastructureError {
                            message: "Invalid new_status in WorkerStatusChanged event".to_string(),
                        },
                    )?,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            "WorkerRegistered" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerRegistered event".to_string(),
                    })?;
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in WorkerRegistered event".to_string(),
                    })?;

                Ok(DomainEvent::WorkerRegistered {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            "WorkerTerminated" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerTerminated event".to_string(),
                    })?;
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in WorkerTerminated event".to_string(),
                    })?;
                let reason = serde_json::from_value::<TerminationReason>(payload["reason"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid reason in WorkerTerminated event".to_string(),
                    })?;

                Ok(DomainEvent::WorkerTerminated {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    reason,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            "WorkerDisconnected" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerDisconnected event".to_string(),
                    })?;

                let last_heartbeat = if let Some(val) = payload.get("last_heartbeat") {
                    serde_json::from_value::<Option<chrono::DateTime<chrono::Utc>>>(val.clone())
                        .map_err(|_| DomainError::InfrastructureError {
                            message: "Invalid last_heartbeat in WorkerDisconnected event"
                                .to_string(),
                        })?
                } else {
                    None
                };

                Ok(DomainEvent::WorkerDisconnected {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    last_heartbeat,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.1: JobDispatchAcknowledged
            // =====================================================
            "JobDispatchAcknowledged" => {
                let job_id =
                    serde_json::from_value::<Uuid>(payload["job_id"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid job_id in JobDispatchAcknowledged event".to_string(),
                        }
                    })?;
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in JobDispatchAcknowledged event".to_string(),
                    })?;

                let acknowledged_at = serde_json::from_value::<chrono::DateTime<chrono::Utc>>(
                    payload["acknowledged_at"].clone(),
                )
                .map_err(|_| DomainError::InfrastructureError {
                    message: "Invalid acknowledged_at in JobDispatchAcknowledged event".to_string(),
                })?;

                Ok(DomainEvent::JobDispatchAcknowledged {
                    job_id: hodei_server_domain::shared_kernel::JobId(job_id),
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    acknowledged_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.2: RunJobReceived
            // =====================================================
            "RunJobReceived" => {
                let job_id =
                    serde_json::from_value::<Uuid>(payload["job_id"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid job_id in RunJobReceived event".to_string(),
                        }
                    })?;
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in RunJobReceived event".to_string(),
                    })?;

                let received_at = serde_json::from_value::<chrono::DateTime<chrono::Utc>>(
                    payload["received_at"].clone(),
                )
                .map_err(|_| DomainError::InfrastructureError {
                    message: "Invalid received_at in RunJobReceived event".to_string(),
                })?;

                Ok(DomainEvent::RunJobReceived {
                    job_id: hodei_server_domain::shared_kernel::JobId(job_id),
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    received_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.3: JobRetried
            // =====================================================
            "JobRetried" => {
                let job_id =
                    serde_json::from_value::<Uuid>(payload["job_id"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid job_id in JobRetried event".to_string(),
                        }
                    })?;

                Ok(DomainEvent::JobRetried {
                    job_id: hodei_server_domain::shared_kernel::JobId(job_id),
                    attempt: serde_json::from_value(payload["attempt"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid attempt in JobRetried event".to_string(),
                        }
                    })?,
                    max_attempts: serde_json::from_value(payload["max_attempts"].clone()).map_err(
                        |_| DomainError::InfrastructureError {
                            message: "Invalid max_attempts in JobRetried event".to_string(),
                        },
                    )?,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.3: JobCancelled
            // =====================================================
            "JobCancelled" => {
                let job_id =
                    serde_json::from_value::<Uuid>(payload["job_id"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid job_id in JobCancelled event".to_string(),
                        }
                    })?;

                Ok(DomainEvent::JobCancelled {
                    job_id: hodei_server_domain::shared_kernel::JobId(job_id),
                    reason: payload
                        .get("reason")
                        .and_then(|r| r.as_str().map(|s| s.to_string())),
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.3: ProviderHealthChanged
            // =====================================================
            "ProviderHealthChanged" => {
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in ProviderHealthChanged event".to_string(),
                    })?;

                Ok(DomainEvent::ProviderHealthChanged {
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    old_status: serde_json::from_value(payload["old_status"].clone()).map_err(
                        |_| DomainError::InfrastructureError {
                            message: "Invalid old_status in ProviderHealthChanged event"
                                .to_string(),
                        },
                    )?,
                    new_status: serde_json::from_value(payload["new_status"].clone()).map_err(
                        |_| DomainError::InfrastructureError {
                            message: "Invalid new_status in ProviderHealthChanged event"
                                .to_string(),
                        },
                    )?,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.3: WorkerReconnected
            // =====================================================
            "WorkerReconnected" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerReconnected event".to_string(),
                    })?;

                Ok(DomainEvent::WorkerReconnected {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    session_id: serde_json::from_value(payload["session_id"].clone()).map_err(
                        |_| DomainError::InfrastructureError {
                            message: "Invalid session_id in WorkerReconnected event".to_string(),
                        },
                    )?,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.3: WorkerRecoveryFailed
            // =====================================================
            "WorkerRecoveryFailed" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerRecoveryFailed event".to_string(),
                    })?;

                Ok(DomainEvent::WorkerRecoveryFailed {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    invalid_session_id: serde_json::from_value(
                        payload["invalid_session_id"].clone(),
                    )
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid invalid_session_id in WorkerRecoveryFailed event"
                            .to_string(),
                    })?,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // WorkerHeartbeat - Worker sends periodic heartbeat
            // =====================================================
            "WorkerHeartbeat" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerHeartbeat event".to_string(),
                    })?;

                let current_job_id = payload
                    .get("current_job_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .map(hodei_server_domain::shared_kernel::JobId);

                Ok(DomainEvent::WorkerHeartbeat {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    state: serde_json::from_value(payload["state"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid state in WorkerHeartbeat event".to_string(),
                        }
                    })?,
                    load_average: payload.get("load_average").and_then(|v| v.as_f64()),
                    memory_usage_mb: payload.get("memory_usage_mb").and_then(|v| v.as_u64()),
                    current_job_id,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.3: ProviderRecovered
            // =====================================================
            "ProviderRecovered" => {
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in ProviderRecovered event".to_string(),
                    })?;

                Ok(DomainEvent::ProviderRecovered {
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    previous_status: serde_json::from_value(payload["previous_status"].clone())
                        .map_err(|_| DomainError::InfrastructureError {
                            message: "Invalid previous_status in ProviderRecovered event"
                                .to_string(),
                        })?,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.3: WorkerEphemeralCreated
            // =====================================================
            "WorkerEphemeralCreated" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerEphemeralCreated event".to_string(),
                    })?;
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in WorkerEphemeralCreated event".to_string(),
                    })?;

                Ok(DomainEvent::WorkerEphemeralCreated {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    max_lifetime_secs: serde_json::from_value(payload["max_lifetime_secs"].clone())
                        .map_err(|_| DomainError::InfrastructureError {
                            message: "Invalid max_lifetime_secs in WorkerEphemeralCreated event"
                                .to_string(),
                        })?,
                    ttl_after_completion_secs: payload
                        .get("ttl_after_completion_secs")
                        .and_then(|v| v.as_u64())
                        .map(Some)
                        .unwrap_or(None),
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.3: WorkerEphemeralReady
            // =====================================================
            "WorkerEphemeralReady" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerEphemeralReady event".to_string(),
                    })?;
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in WorkerEphemeralReady event".to_string(),
                    })?;

                Ok(DomainEvent::WorkerEphemeralReady {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.8: WorkerStateUpdated
            // =====================================================
            "WorkerStateUpdated" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerStateUpdated event".to_string(),
                    })?;
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in WorkerStateUpdated event".to_string(),
                    })?;

                let current_job_id = payload
                    .get("current_job_id")
                    .map(|v| {
                        serde_json::from_value::<Uuid>(v.clone())
                            .map(|id| hodei_server_domain::shared_kernel::JobId(id))
                    })
                    .transpose()
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid current_job_id in WorkerStateUpdated event".to_string(),
                    })?;

                let last_heartbeat = payload
                    .get("last_heartbeat")
                    .map(|v| {
                        serde_json::from_value::<Option<chrono::DateTime<chrono::Utc>>>(v.clone())
                    })
                    .transpose()
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid last_heartbeat in WorkerStateUpdated event".to_string(),
                    })?
                    .flatten();

                let capabilities = payload
                    .get("capabilities")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                let metadata = payload
                    .get("metadata")
                    .and_then(|v| v.as_object())
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default();

                Ok(DomainEvent::WorkerStateUpdated {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    old_state: serde_json::from_value(payload["old_state"].clone()).map_err(
                        |_| DomainError::InfrastructureError {
                            message: "Invalid old_state in WorkerStateUpdated event".to_string(),
                        },
                    )?,
                    new_state: serde_json::from_value(payload["new_state"].clone()).map_err(
                        |_| DomainError::InfrastructureError {
                            message: "Invalid new_state in WorkerStateUpdated event".to_string(),
                        },
                    )?,
                    current_job_id,
                    last_heartbeat,
                    capabilities,
                    metadata,
                    transition_reason: serde_json::from_value(payload["transition_reason"].clone())
                        .map_err(|_| DomainError::InfrastructureError {
                            message: "Invalid transition_reason in WorkerStateUpdated event"
                                .to_string(),
                        })?,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.3: WorkerEphemeralTerminating
            // =====================================================
            "WorkerEphemeralTerminating" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerEphemeralTerminating event"
                            .to_string(),
                    })?;
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in WorkerEphemeralTerminating event"
                            .to_string(),
                    })?;
                let reason = serde_json::from_value::<TerminationReason>(payload["reason"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid reason in WorkerEphemeralTerminating event".to_string(),
                    })?;

                Ok(DomainEvent::WorkerEphemeralTerminating {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    reason,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.3: WorkerEphemeralTerminated
            // =====================================================
            "WorkerEphemeralTerminated" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerEphemeralTerminated event".to_string(),
                    })?;
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in WorkerEphemeralTerminated event"
                            .to_string(),
                    })?;

                let ttl_expires_at = payload
                    .get("ttl_expires_at")
                    .map(|v| serde_json::from_value::<chrono::DateTime<chrono::Utc>>(v.clone()))
                    .transpose()
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid ttl_expires_at in WorkerEphemeralTerminated event"
                            .to_string(),
                    })?;

                Ok(DomainEvent::WorkerEphemeralTerminated {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    cleanup_scheduled: payload["cleanup_scheduled"].as_bool().unwrap_or(false),
                    ttl_expires_at,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.3: WorkerEphemeralCleanedUp
            // =====================================================
            "WorkerEphemeralCleanedUp" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerEphemeralCleanedUp event".to_string(),
                    })?;
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in WorkerEphemeralCleanedUp event"
                            .to_string(),
                    })?;

                let cleanup_reason =
                    serde_json::from_value::<CleanupReason>(payload["cleanup_reason"].clone())
                        .map_err(|_| DomainError::InfrastructureError {
                            message: "Invalid cleanup_reason in WorkerEphemeralCleanedUp event"
                                .to_string(),
                        })?;

                Ok(DomainEvent::WorkerEphemeralCleanedUp {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    cleanup_reason,
                    cleanup_duration_ms: serde_json::from_value(
                        payload["cleanup_duration_ms"].clone(),
                    )
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid cleanup_duration_ms in WorkerEphemeralCleanedUp event"
                            .to_string(),
                    })?,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.3: OrphanWorkerDetected
            // =====================================================
            "OrphanWorkerDetected" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in OrphanWorkerDetected event".to_string(),
                    })?;
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in OrphanWorkerDetected event".to_string(),
                    })?;

                let last_seen = serde_json::from_value::<chrono::DateTime<chrono::Utc>>(
                    payload["last_seen"].clone(),
                )
                .map_err(|_| DomainError::InfrastructureError {
                    message: "Invalid last_seen in OrphanWorkerDetected event".to_string(),
                })?;

                Ok(DomainEvent::OrphanWorkerDetected {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    last_seen,
                    orphaned_duration_secs: serde_json::from_value(
                        payload["orphaned_duration_secs"].clone(),
                    )
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid orphaned_duration_secs in OrphanWorkerDetected event"
                            .to_string(),
                    })?,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.3: GarbageCollectionCompleted
            // =====================================================
            "GarbageCollectionCompleted" => {
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in GarbageCollectionCompleted event"
                            .to_string(),
                    })?;

                Ok(DomainEvent::GarbageCollectionCompleted {
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    workers_cleaned: serde_json::from_value(payload["workers_cleaned"].clone())
                        .map_err(|_| DomainError::InfrastructureError {
                            message: "Invalid workers_cleaned in GarbageCollectionCompleted event"
                                .to_string(),
                        })?,
                    orphans_detected: serde_json::from_value(payload["orphans_detected"].clone())
                        .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid orphans_detected in GarbageCollectionCompleted event"
                            .to_string(),
                    })?,
                    errors: serde_json::from_value(payload["errors"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid errors in GarbageCollectionCompleted event"
                                .to_string(),
                        }
                    })?,
                    duration_ms: serde_json::from_value(payload["duration_ms"].clone()).map_err(
                        |_| DomainError::InfrastructureError {
                            message: "Invalid duration_ms in GarbageCollectionCompleted event"
                                .to_string(),
                        },
                    )?,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.3: WorkerEphemeralIdle
            // =====================================================
            "WorkerEphemeralIdle" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerEphemeralIdle event".to_string(),
                    })?;
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in WorkerEphemeralIdle event".to_string(),
                    })?;

                let idle_since = serde_json::from_value::<chrono::DateTime<chrono::Utc>>(
                    payload["idle_since"].clone(),
                )
                .map_err(|_| DomainError::InfrastructureError {
                    message: "Invalid idle_since in WorkerEphemeralIdle event".to_string(),
                })?;

                Ok(DomainEvent::WorkerEphemeralIdle {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    idle_since,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // US-26.4: ProviderError (TerminationReason::ProviderError)
            // =====================================================
            "ProviderError" => {
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in ProviderError event".to_string(),
                    })?;

                let message = payload
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown provider error")
                    .to_string();

                Ok(DomainEvent::WorkerTerminated {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(
                        payload
                            .get("worker_id")
                            .and_then(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                            .unwrap_or_else(Uuid::new_v4),
                    ),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    reason: TerminationReason::ProviderError { message },
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // Additional events: ProviderRegistered, ProviderUpdated
            // =====================================================
            "ProviderRegistered" => {
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in ProviderRegistered event".to_string(),
                    })?;

                Ok(DomainEvent::ProviderRegistered {
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    provider_type: serde_json::from_value(payload["provider_type"].clone())
                        .map_err(|_| DomainError::InfrastructureError {
                            message: "Invalid provider_type in ProviderRegistered event"
                                .to_string(),
                        })?,
                    config_summary: serde_json::from_value(payload["config_summary"].clone())
                        .map_err(|_| DomainError::InfrastructureError {
                            message: "Invalid config_summary in ProviderRegistered event"
                                .to_string(),
                        })?,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            "ProviderUpdated" => {
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in ProviderUpdated event".to_string(),
                    })?;

                Ok(DomainEvent::ProviderUpdated {
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    changes: payload
                        .get("changes")
                        .and_then(|v| v.as_str().map(|s| s.to_string())),
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            "AutoScalingTriggered" => {
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in AutoScalingTriggered event".to_string(),
                    })?;

                Ok(DomainEvent::AutoScalingTriggered {
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    reason: serde_json::from_value(payload["reason"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid reason in AutoScalingTriggered event".to_string(),
                        }
                    })?,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            "WorkerProvisioned" => {
                let worker_id = serde_json::from_value::<Uuid>(payload["worker_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid worker_id in WorkerProvisioned event".to_string(),
                    })?;
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in WorkerProvisioned event".to_string(),
                    })?;

                Ok(DomainEvent::WorkerProvisioned {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(worker_id),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    spec_summary: serde_json::from_value(payload["spec_summary"].clone()).map_err(
                        |_| DomainError::InfrastructureError {
                            message: "Invalid spec_summary in WorkerProvisioned event".to_string(),
                        },
                    )?,
                    occurred_at: event.created_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // EPIC-29: WorkerProvisioningRequested event
            // =====================================================
            "WorkerProvisioningRequested" => {
                let job_id =
                    serde_json::from_value::<Uuid>(payload["job_id"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid job_id in WorkerProvisioningRequested event"
                                .to_string(),
                        }
                    })?;
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in WorkerProvisioningRequested event"
                            .to_string(),
                    })?;

                let requested_at = payload
                    .get("requested_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now);

                Ok(DomainEvent::WorkerProvisioningRequested {
                    job_id: hodei_server_domain::shared_kernel::JobId(job_id),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    job_requirements: serde_json::from_value(payload["job_requirements"].clone())
                        .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid job_requirements in WorkerProvisioningRequested event"
                            .to_string(),
                    })?,
                    requested_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // EPIC-31: JobQueued event
            // =====================================================
            "JobQueued" => {
                let job_id =
                    serde_json::from_value::<Uuid>(payload["job_id"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid job_id in JobQueued event".to_string(),
                        }
                    })?;

                // Extract preferred_provider as Option<ProviderId>
                let preferred_provider: Option<hodei_server_domain::shared_kernel::ProviderId> =
                    payload
                        .get("preferred_provider")
                        .and_then(|v| v.as_str())
                        .and_then(|s| {
                            Uuid::parse_str(s)
                                .ok()
                                .map(|u| hodei_server_domain::shared_kernel::ProviderId(u))
                        });

                let job_requirements: hodei_server_domain::jobs::JobSpec =
                    serde_json::from_value(payload["job_requirements"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid job_requirements in JobQueued event".to_string(),
                        }
                    })?;

                let queued_at = payload
                    .get("queued_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now);

                Ok(DomainEvent::JobQueued {
                    job_id: hodei_server_domain::shared_kernel::JobId(job_id),
                    preferred_provider,
                    job_requirements,
                    queued_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // EPIC-32: ProviderSelected event (trazabilidad de scheduling)
            // =====================================================
            "ProviderSelected" => {
                let job_id =
                    serde_json::from_value::<Uuid>(payload["job_id"].clone()).map_err(|_| {
                        DomainError::InfrastructureError {
                            message: "Invalid job_id in ProviderSelected event".to_string(),
                        }
                    })?;
                let provider_id = serde_json::from_value::<Uuid>(payload["provider_id"].clone())
                    .map_err(|_| DomainError::InfrastructureError {
                        message: "Invalid provider_id in ProviderSelected event".to_string(),
                    })?;

                let provider_type = payload
                    .get("provider_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Parse provider_type string to ProviderType enum
                let provider_type_enum = match provider_type.as_str() {
                    "docker" => hodei_server_domain::workers::ProviderType::Docker,
                    "kubernetes" => hodei_server_domain::workers::ProviderType::Kubernetes,
                    "k8s" => hodei_server_domain::workers::ProviderType::Kubernetes,
                    "fargate" => hodei_server_domain::workers::ProviderType::Fargate,
                    "cloudrun" => hodei_server_domain::workers::ProviderType::CloudRun,
                    "containerapps" => hodei_server_domain::workers::ProviderType::ContainerApps,
                    "lambda" => hodei_server_domain::workers::ProviderType::Lambda,
                    "cloudfunctions" => hodei_server_domain::workers::ProviderType::CloudFunctions,
                    "azurefunctions" => hodei_server_domain::workers::ProviderType::AzureFunctions,
                    "ec2" => hodei_server_domain::workers::ProviderType::EC2,
                    "computeengine" => hodei_server_domain::workers::ProviderType::ComputeEngine,
                    "azurevms" => hodei_server_domain::workers::ProviderType::AzureVMs,
                    "test" => hodei_server_domain::workers::ProviderType::Test,
                    "baremetal" => hodei_server_domain::workers::ProviderType::BareMetal,
                    _ => hodei_server_domain::workers::ProviderType::Test,
                };

                let selection_strategy = payload
                    .get("selection_strategy")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default")
                    .to_string();

                let effective_cost = payload
                    .get("effective_cost")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                let effective_startup_ms = payload
                    .get("effective_startup_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                let elapsed_ms = payload
                    .get("elapsed_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                let occurred_at = payload
                    .get("occurred_at")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now);

                Ok(DomainEvent::ProviderSelected {
                    job_id: hodei_server_domain::shared_kernel::JobId(job_id),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId(provider_id),
                    provider_type: provider_type_enum,
                    selection_strategy,
                    effective_cost,
                    effective_startup_ms,
                    elapsed_ms,
                    occurred_at,
                    correlation_id: event.metadata.as_ref().and_then(|m| {
                        m.get("correlation_id")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                    actor: event.metadata.as_ref().and_then(|m| {
                        m.get("actor")
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    }),
                })
            }

            // =====================================================
            // Unknown event type
            // =====================================================
            _ => {
                warn!(
                    event_type = %event.event_type,
                    "Unknown event type in conversion"
                );
                return Err(DomainError::InfrastructureError {
                    message: format!("Unknown event type: {}", event.event_type),
                });
            }
        }
    }

    /// Mark an event as published in the outbox
    async fn mark_event_published(
        &self,
        event_id: Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let repo = PostgresOutboxRepository::new(self.pool.clone());
        repo.mark_published(&[event_id]).await?;
        Ok(())
    }

    /// Mark an event as failed in the outbox
    async fn mark_event_failed(
        &self,
        event_id: Uuid,
        error: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let repo = PostgresOutboxRepository::new(self.pool.clone());
        repo.mark_failed(&event_id, error).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::event_bus::EventBus;
    use sqlx::postgres::PgPoolOptions;
    use std::sync::atomic::{AtomicU64, Ordering};

    // Mock event bus for testing
    struct MockEventBus {
        published_events: AtomicU64,
    }

    impl MockEventBus {
        fn new() -> Self {
            Self {
                published_events: AtomicU64::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl EventBus for MockEventBus {
        async fn publish(&self, _event: &DomainEvent) -> Result<(), EventBusError> {
            self.published_events.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn subscribe(
            &self,
            _topic: &str,
        ) -> Result<
            futures::stream::BoxStream<'static, Result<DomainEvent, EventBusError>>,
            EventBusError,
        > {
            Ok(futures::stream::empty().boxed())
        }
    }

    async fn setup_test_db() -> PgPool {
        let connection_string = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://hodei:hodei@localhost:5432/hodei_test".to_string());

        // Create a unique database for this test
        let db_name = format!("hodei_relay_test_{}", uuid::Uuid::new_v4());
        let base_url = connection_string.trim_end_matches(&format!(
            "/{}",
            connection_string.split('/').last().unwrap()
        ));
        let admin_conn_string = format!("{}/postgres", base_url);

        let mut admin_conn = sqlx::postgres::PgPool::connect(&admin_conn_string)
            .await
            .expect("Failed to connect to postgres");

        sqlx::query(&format!("CREATE DATABASE {}", db_name))
            .execute(&admin_conn)
            .await
            .expect("Failed to create test database");

        let test_conn_string = format!("{}/{}", base_url, db_name);

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&test_conn_string)
            .await
            .expect("Failed to connect to test database");

        // Create outbox table
        sqlx::query(
            r#"
            CREATE TABLE outbox_events (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                aggregate_id UUID NOT NULL,
                aggregate_type VARCHAR(20) NOT NULL CHECK (aggregate_type IN ('JOB', 'WORKER', 'PROVIDER')),
                event_type VARCHAR(50) NOT NULL,
                event_version INTEGER DEFAULT 1,
                payload JSONB NOT NULL,
                metadata JSONB,
                idempotency_key VARCHAR(100),
                created_at TIMESTAMPTZ DEFAULT NOW(),
                published_at TIMESTAMPTZ,
                status VARCHAR(20) DEFAULT 'PENDING' CHECK (status IN ('PENDING', 'PUBLISHED', 'FAILED')),
                retry_count INTEGER DEFAULT 0,
                last_error TEXT,
                UNIQUE(idempotency_key)
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create outbox_events table");

        pool
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_outbox_relay_publishes_events() {
        let pool = setup_test_db().await;
        let event_bus = Arc::new(MockEventBus::new());
        let relay = OutboxRelay::new_with_defaults(pool, event_bus.clone());

        // Insert a test event
        let job_id = Uuid::new_v4();
        let event = hodei_server_domain::outbox::OutboxEventInsert::for_job(
            job_id,
            "JobAssigned".to_string(),
            serde_json::json!({
                "job_id": job_id,
                "worker_id": Uuid::new_v4()
            }),
            Some(serde_json::json!({
                "correlation_id": "test-corr",
                "actor": "test-actor"
            })),
            Some("test-key-123".to_string()),
        );

        let repo = PostgresOutboxRepository::new(relay.pool.clone());
        repo.insert_events(&[event]).await.unwrap();

        // Verify event is pending
        let pending = repo.get_pending_events(10, 3).await.unwrap();
        assert_eq!(pending.len(), 1);

        // Process a small batch (just one iteration)
        let events = relay.fetch_pending_events().await.unwrap();
        relay.process_events_batch(events).await.unwrap();

        // Verify event was published
        assert_eq!(event_bus.published_events.load(Ordering::SeqCst), 1);

        // Verify event is no longer pending
        let pending = repo.get_pending_events(10, 3).await.unwrap();
        assert!(pending.is_empty());
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_outbox_relay_retry_failed_events() {
        // This test would need a mock event bus that simulates failures
        // For now, we just verify the basic structure
        let pool = setup_test_db().await;
        let event_bus = Arc::new(MockEventBus::new());
        let relay = OutboxRelay::new_with_defaults(pool, event_bus.clone());

        // Insert a test event
        let job_id = Uuid::new_v4();
        let event = hodei_server_domain::outbox::OutboxEventInsert::for_job(
            job_id,
            "JobAssigned".to_string(),
            serde_json::json!({
                "job_id": job_id,
                "worker_id": Uuid::new_v4()
            }),
            None,
            Some("test-key-failed".to_string()),
        );

        let repo = PostgresOutboxRepository::new(relay.pool.clone());
        repo.insert_events(&[event]).await.unwrap();

        // Mark as failed
        let pending = repo.get_pending_events(10, 3).await.unwrap();
        let event_id = pending[0].id;
        repo.mark_failed(&event_id, "Test error").await.unwrap();

        // Verify it's marked as failed
        let pending = repo.get_pending_events(10, 3).await.unwrap();
        assert!(pending.is_empty());
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_metrics() {
        let pool = setup_test_db().await;
        let event_bus = Arc::new(MockEventBus::new());
        let relay = OutboxRelay::new_with_defaults(pool, event_bus.clone());

        let initial_metrics = relay.metrics();
        assert_eq!(initial_metrics.events_published_total, 0);
        assert_eq!(initial_metrics.events_failed_total, 0);

        // Insert and process an event
        let job_id = Uuid::new_v4();
        let event = hodei_server_domain::outbox::OutboxEventInsert::for_job(
            job_id,
            "JobAssigned".to_string(),
            serde_json::json!({
                "job_id": job_id,
                "worker_id": Uuid::new_v4()
            }),
            None,
            Some("test-metrics".to_string()),
        );

        let repo = PostgresOutboxRepository::new(relay.pool.clone());
        repo.insert_events(&[event]).await.unwrap();

        let events = relay.fetch_pending_events().await.unwrap();
        relay.process_events_batch(events).await.unwrap();

        let final_metrics = relay.metrics();
        assert_eq!(final_metrics.events_published_total, 1);
        assert!(final_metrics.avg_publish_duration_ms() >= 0.0);
    }
}

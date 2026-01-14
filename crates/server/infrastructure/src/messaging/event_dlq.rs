//! Event Dead Letter Queue (DLQ) Implementation
//!
//! This module provides a DLQ mechanism for handling events that cannot be
//! processed successfully after multiple retry attempts. Events are moved
//! to a DLQ stream for later inspection and manual intervention.
//!
//! # Architecture
//!
//! The DLQ works by:
//! 1. Intercepting failed message processing in saga consumers
//! 2. Tracking retry counts per message
//! 3. Moving messages exceeding max retries to the DLQ stream
//! 4. Logging DLQ entries for observability

use async_nats::jetstream::Context as JetStreamContext;
use async_nats::jetstream::consumer::pull::Config as PullConsumerConfig;
use async_nats::jetstream::consumer::{AckPolicy, DeliverPolicy, PullConsumer};
use async_nats::jetstream::stream::Config as StreamConfig;
use async_nats::jetstream::stream::Stream as StreamHandle;
use chrono::{DateTime, Utc};
use hodei_server_domain::events::DomainEvent;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Maximum number of retry attempts before moving to DLQ
const DEFAULT_MAX_RETRIES: u32 = 3;

/// Configuration for DLQ behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDlqConfig {
    /// Stream prefix for DLQ streams
    pub stream_prefix: String,
    /// Subject for DLQ events
    pub dlq_subject: String,
    /// Maximum retry attempts before DLQ
    pub max_retries: u32,
    /// DLQ stream retention period in days
    pub retention_days: i64,
    /// Whether DLQ is enabled
    pub enabled: bool,
}

impl Default for EventDlqConfig {
    fn default() -> Self {
        Self {
            stream_prefix: "HODEI".to_string(),
            dlq_subject: "dlq.events".to_string(),
            max_retries: DEFAULT_MAX_RETRIES,
            retention_days: 7,
            enabled: true,
        }
    }
}

/// A DLQ entry containing the failed event and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqEntry {
    /// Original event that failed processing
    pub event: DomainEvent,
    /// Number of retry attempts made
    pub retry_count: u32,
    /// Error message from last failure
    pub last_error: String,
    /// When the event was first received
    pub received_at: DateTime<Utc>,
    /// When the event was moved to DLQ
    pub dlq_at: DateTime<Utc>,
    /// Original subject the event was published to
    pub original_subject: String,
    /// Consumer name that failed to process
    pub consumer_name: String,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
    /// Actor that triggered the event
    pub actor: Option<String>,
}

impl DlqEntry {
    /// Create a new DLQ entry from a failed event
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        event: DomainEvent,
        retry_count: u32,
        last_error: String,
        received_at: DateTime<Utc>,
        original_subject: String,
        consumer_name: String,
        correlation_id: Option<String>,
        actor: Option<String>,
    ) -> Self {
        Self {
            event,
            retry_count,
            last_error,
            received_at,
            dlq_at: Utc::now(),
            original_subject,
            consumer_name,
            correlation_id,
            actor,
        }
    }

    /// Serialize the DLQ entry to JSON bytes
    pub fn to_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Deserialize DLQ entry from JSON bytes
    pub fn from_json(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// Result of DLQ operations
#[derive(Debug)]
pub enum DlqOperationResult {
    /// Event was successfully moved to DLQ
    MovedToDlq,
    /// Event was processed successfully (not moved to DLQ)
    Processed,
    /// Event exceeded retries and was dropped (DLQ disabled or full)
    Dropped,
    /// DLQ is disabled
    Disabled,
}

/// Event Dead Letter Queue
///
/// Handles moving failed events to a separate NATS stream for later inspection.
#[derive(Clone)]
pub struct EventDlq {
    jetstream: JetStreamContext,
    config: EventDlqConfig,
}

impl EventDlq {
    /// Create a new EventDlq
    pub fn new(jetstream: JetStreamContext, config: Option<EventDlqConfig>) -> Self {
        let config = config.unwrap_or_default();
        Self { jetstream, config }
    }

    /// Create a new EventDlq with default configuration
    pub fn with_default_config(jetstream: JetStreamContext) -> Self {
        Self::new(jetstream, None)
    }

    /// Initialize the DLQ stream
    pub async fn initialize(&self) -> Result<(), DlqError> {
        if !self.config.enabled {
            debug!("DLQ is disabled, skipping initialization");
            return Ok(());
        }

        let stream_name = format!("{}_DLQ", self.config.stream_prefix);

        match self.jetstream.get_stream(&stream_name).await {
            Ok(_) => {
                debug!("DLQ stream '{}' already exists", stream_name);
                Ok(())
            }
            Err(_) => {
                info!("Creating DLQ stream '{}'", stream_name);
                let stream_config = StreamConfig {
                    name: stream_name.clone(),
                    subjects: vec![format!(
                        "{}.{}",
                        self.config.stream_prefix, self.config.dlq_subject
                    )],
                    max_messages: -1,
                    max_bytes: -1,
                    max_age: Duration::from_secs(60 * 60 * 24 * self.config.retention_days as u64),
                    storage: async_nats::jetstream::stream::StorageType::File,
                    num_replicas: 1,
                    ..Default::default()
                };

                self.jetstream
                    .create_stream(stream_config)
                    .await
                    .map_err(|e| DlqError::StreamCreationFailed(e.to_string()))?;

                info!("DLQ stream '{}' created successfully", stream_name);
                Ok(())
            }
        }
    }

    /// Move a failed event to the DLQ
    ///
    /// # Arguments
    ///
    /// * `event` - The event that failed processing
    /// * `retry_count` - Number of retry attempts made
    /// * `last_error` - Error message from the last failure
    /// * `received_at` - When the event was first received
    /// * `original_subject` - Subject the event was published to
    /// * `consumer_name` - Consumer that failed to process
    /// * `correlation_id` - Optional correlation ID
    /// * `actor` - Optional actor that triggered the event
    ///
    /// # Returns
    ///
    /// Whether the event was moved to DLQ
    pub async fn move_to_dlq(
        &self,
        event: &DomainEvent,
        retry_count: u32,
        last_error: &str,
        received_at: DateTime<Utc>,
        original_subject: &str,
        consumer_name: &str,
        correlation_id: Option<&str>,
        actor: Option<&str>,
    ) -> DlqOperationResult {
        if !self.config.enabled {
            return DlqOperationResult::Disabled;
        }

        if retry_count >= self.config.max_retries {
            let dlq_subject = format!("{}.{}", self.config.stream_prefix, self.config.dlq_subject);

            let entry = DlqEntry::new(
                event.clone(),
                retry_count,
                last_error.to_string(),
                received_at,
                original_subject.to_string(),
                consumer_name.to_string(),
                correlation_id.map(|s| s.to_string()),
                actor.map(|s| s.to_string()),
            );

            match entry.to_json() {
                Ok(json_bytes) => {
                    match self.jetstream.publish(dlq_subject, json_bytes.into()).await {
                        Ok(_) => {
                            info!(
                                "Event moved to DLQ after {} retries (consumer: {}, subject: {})",
                                retry_count, consumer_name, original_subject
                            );
                            DlqOperationResult::MovedToDlq
                        }
                        Err(e) => {
                            error!("Failed to publish to DLQ: {}", e);
                            DlqOperationResult::Dropped
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to serialize DLQ entry: {}", e);
                    DlqOperationResult::Dropped
                }
            }
        } else {
            DlqOperationResult::Processed
        }
    }

    /// Get the number of messages in the DLQ
    pub async fn dlq_count(&self) -> Result<u64, DlqError> {
        if !self.config.enabled {
            return Ok(0);
        }

        let stream_name = format!("{}_DLQ", self.config.stream_prefix);

        let mut stream = self
            .jetstream
            .get_stream(&stream_name)
            .await
            .map_err(|e| DlqError::StreamNotFound(e.to_string()))?;

        let info = stream
            .info()
            .await
            .map_err(|e| DlqError::StreamInfoFailed(e.to_string()))?;

        Ok(info.state.messages)
    }

    /// Create a consumer for DLQ events (for manual inspection/reprocessing)
    pub async fn create_dlq_consumer(&self, consumer_name: &str) -> Result<PullConsumer, DlqError> {
        let stream_name = format!("{}_DLQ", self.config.stream_prefix);
        let consumer_id = format!("{}-{}", stream_name, consumer_name);

        let mut stream = self
            .jetstream
            .get_stream(&stream_name)
            .await
            .map_err(|e| DlqError::StreamNotFound(e.to_string()))?;

        // Check if consumer already exists
        match stream.get_consumer(&consumer_id).await {
            Ok(consumer) => {
                debug!("DLQ consumer '{}' already exists", consumer_id);
                return Ok(consumer);
            }
            Err(_) => {
                info!("Creating DLQ consumer '{}'", consumer_id);
            }
        }

        let consumer_config = PullConsumerConfig {
            durable_name: Some(consumer_id.clone()),
            deliver_policy: DeliverPolicy::All,
            ack_policy: AckPolicy::Explicit,
            ack_wait: Duration::from_secs(30),
            max_deliver: 1,
            ..Default::default()
        };

        stream
            .create_consumer(consumer_config)
            .await
            .map_err(|e| DlqError::ConsumerCreationFailed(e.to_string()))
    }
}

/// Errors that can occur during DLQ operations
#[derive(Debug, thiserror::Error)]
pub enum DlqError {
    #[error("Stream creation failed: {0}")]
    StreamCreationFailed(String),

    #[error("Stream not found: {0}")]
    StreamNotFound(String),

    #[error("Failed to get stream info: {0}")]
    StreamInfoFailed(String),

    #[error("Consumer creation failed: {0}")]
    ConsumerCreationFailed(String),

    #[error("DLQ is disabled")]
    Disabled,
}

/// Metrics for DLQ operations
#[derive(Debug, Default)]
pub struct EventDlqMetrics {
    moved_to_dlq: Arc<std::sync::atomic::AtomicU64>,
    processed: Arc<std::sync::atomic::AtomicU64>,
    dropped: Arc<std::sync::atomic::AtomicU64>,
    disabled: Arc<std::sync::atomic::AtomicU64>,
}

impl EventDlqMetrics {
    /// Create new metrics
    pub fn new() -> Self {
        Self {
            moved_to_dlq: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            processed: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            dropped: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            disabled: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Record a DLQ operation result
    pub fn record(&self, result: &DlqOperationResult) {
        match result {
            DlqOperationResult::MovedToDlq => {
                self.moved_to_dlq
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            DlqOperationResult::Processed => {
                self.processed
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            DlqOperationResult::Dropped => {
                self.dropped
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            DlqOperationResult::Disabled => {
                self.disabled
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }

    /// Get current counts
    pub fn counts(&self) -> (u64, u64, u64, u64) {
        (
            self.moved_to_dlq.load(std::sync::atomic::Ordering::Relaxed),
            self.processed.load(std::sync::atomic::Ordering::Relaxed),
            self.dropped.load(std::sync::atomic::Ordering::Relaxed),
            self.disabled.load(std::sync::atomic::Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::events::JobCreated;
    use hodei_server_domain::jobs::JobSpec;
    use hodei_server_domain::shared_kernel::JobId;
    use serde_json;

    #[test]
    fn test_dlq_entry_serialization() {
        let job_id = JobId::new();
        let spec = JobSpec::new(vec!["echo".to_string(), "hello".to_string()]);

        let event = DomainEvent::JobCreated(JobCreated {
            job_id: job_id.clone(),
            spec,
            occurred_at: Utc::now(),
            correlation_id: Some("test-correlation".to_string()),
            actor: Some("test-actor".to_string()),
        });

        let entry = DlqEntry::new(
            event.clone(),
            3,
            "Processing failed".to_string(),
            Utc::now(),
            "test.subject".to_string(),
            "test-consumer".to_string(),
            Some("test-correlation".to_string()),
            Some("test-actor".to_string()),
        );

        let json = entry.to_json().expect("Should serialize");
        let deserialized = DlqEntry::from_json(&json).expect("Should deserialize");

        assert_eq!(entry.retry_count, deserialized.retry_count);
        assert_eq!(entry.last_error, deserialized.last_error);
        assert_eq!(entry.original_subject, deserialized.original_subject);
        assert_eq!(entry.consumer_name, deserialized.consumer_name);
    }

    #[test]
    fn test_dlq_config_defaults() {
        let config = EventDlqConfig::default();
        assert_eq!(config.max_retries, DEFAULT_MAX_RETRIES);
        assert!(config.enabled);
        assert_eq!(config.retention_days, 7);
    }

    #[test]
    fn test_dlq_entry_json_format() {
        let entry = DlqEntry::new(
            DomainEvent::JobQueued {
                job_id: JobId::new(),
                preferred_provider: None,
                job_requirements: JobSpec::new(vec!["test".to_string()]),
                queued_at: Utc::now(),
                correlation_id: None,
                actor: None,
            },
            1,
            "Test error".to_string(),
            Utc::now(),
            "hodei.jobs.queued".to_string(),
            "test-consumer".to_string(),
            None,
            None,
        );

        let json = entry.to_json().unwrap();
        let value: serde_json::Value = serde_json::from_slice(&json).unwrap();

        assert_eq!(value["retry_count"], 1);
        assert_eq!(value["last_error"], "Test error");
        assert_eq!(value["original_subject"], "hodei.jobs.queued");
        assert_eq!(value["consumer_name"], "test-consumer");
    }

    #[test]
    fn test_dlq_metrics_counts() {
        let metrics = EventDlqMetrics::new();

        metrics.record(&DlqOperationResult::MovedToDlq);
        metrics.record(&DlqOperationResult::Processed);
        metrics.record(&DlqOperationResult::Processed);
        metrics.record(&DlqOperationResult::Dropped);

        let (moved, processed, dropped, disabled) = metrics.counts();

        assert_eq!(moved, 1);
        assert_eq!(processed, 2);
        assert_eq!(dropped, 1);
        assert_eq!(disabled, 0);
    }
}

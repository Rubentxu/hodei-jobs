//! Dead Letter Queue (DLQ) Processor
//!
//! Handles failed messages with replay capability and alerting.
//! EPIC-70: Addresses NATS JetStream reliability with proper DLQ handling.
//!
//! # Features
//! - Automatic routing of failed messages to DLQ
//! - Replay capability for manual recovery
//! - Failure counting and alerting
//! - Metrics collection

use async_nats::jetstream::Context as JetStreamContext;
use async_nats::jetstream::consumer::pull::Config as PullConsumerConfig;
use async_nats::jetstream::consumer::{AckPolicy, DeliverPolicy};
use async_nats::jetstream::stream::{Config as StreamConfig, RetentionPolicy};
use async_nats::Client;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// DLQ Configuration
#[derive(Debug, Clone)]
pub struct DLQConfig {
    /// Stream name prefix
    pub stream_prefix: String,
    /// DLQ stream name
    pub dlq_stream_name: String,
    /// Consumer name for DLQ processing
    pub consumer_name: String,
    /// Max messages to process per run
    pub batch_size: usize,
    /// Ack wait timeout
    pub ack_wait: Duration,
    /// Maximum delivery attempts before DLQ
    pub max_deliver: i64,
    /// Alert threshold for repeated failures
    pub alert_threshold: u64,
}

impl Default for DLQConfig {
    fn default() -> Self {
        Self {
            stream_prefix: "HODEI".to_string(),
            dlq_stream_name: "HODEI_DLQ".to_string(),
            consumer_name: "dlq-processor".to_string(),
            batch_size: 100,
            ack_wait: Duration::from_secs(60),
            max_deliver: 5,
            alert_threshold: 5,
        }
    }
}

impl DLQConfig {
    /// Get the DLQ stream subject pattern
    pub fn dlq_subject_pattern(&self) -> String {
        format!("{}.dlq.>", self.stream_prefix)
    }
}

/// DLQ Processing Result
#[derive(Debug, Default)]
pub struct DLQProcessingResult {
    pub processed: u32,
    pub failed: u32,
    pub duration: Duration,
}

impl DLQProcessingResult {
    pub fn new(duration: Duration) -> Self {
        Self {
            processed: 0,
            failed: 0,
            duration,
        }
    }
}

/// DLQ Message Envelope for transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DLQMessageEnvelope {
    /// Original subject where message was published
    pub original_subject: String,
    /// The message payload
    pub payload: Vec<u8>,
    /// Message metadata
    pub metadata: DLQMessageMetadata,
    /// Number of delivery attempts
    pub deliver_count: u64,
    /// When the message was first published
    pub first_published_at: DateTime<Utc>,
    /// When the message was moved to DLQ
    pub moved_to_dlq_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DLQMessageMetadata {
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
    pub trace_id: Option<String>,
    pub source: String,
}

/// DLQ Metrics
#[derive(Debug, Default)]
pub struct DLQMetrics {
    processed: AtomicU64,
    failed: AtomicU64,
    failure_counts: Arc<std::collections::HashMap<String, AtomicU64>>,
}

impl DLQMetrics {
    pub fn new() -> Self {
        Self {
            processed: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            failure_counts: Arc::new(std::collections::HashMap::new()),
        }
    }

    pub fn record_processing(&self, result: &DLQProcessingResult) {
        self.processed.fetch_add(result.processed, Ordering::SeqCst);
        self.failed.fetch_add(result.failed, Ordering::SeqCst);
    }

    pub fn record_failure(&self, subject: String) {
        *self
            .failure_counts
            .entry(subject)
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::SeqCst);
    }

    pub fn get_failure_count(&self, subject: &str) -> u64 {
        self.failure_counts
            .get(subject)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    pub fn snapshot(&self) -> (u64, u64) {
        (
            self.processed.load(Ordering::SeqCst),
            self.failed.load(Ordering::SeqCst),
        )
    }
}

/// DLQ Processor for handling failed messages
#[derive(Clone)]
pub struct DLQProcessor {
    nats_client: Client,
    jetstream: JetStreamContext,
    config: DLQConfig,
    metrics: Arc<DLQMetrics>,
    alert_sender: Option<mpsc::Sender<DLQAlert>>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl DLQProcessor {
    /// Creates a new DLQ Processor
    pub fn new(
        nats_client: Client,
        jetstream: JetStreamContext,
        config: Option<DLQConfig>,
        metrics: Option<Arc<DLQMetrics>>,
        alert_sender: Option<mpsc::Sender<DLQAlert>>,
    ) -> Self {
        Self {
            nats_client,
            jetstream,
            config: config.unwrap_or_default(),
            metrics: metrics.unwrap_or_else(|| Arc::new(DLQMetrics::new())),
            alert_sender,
            shutdown_tx: None,
        }
    }

    /// Initialize the DLQ stream and consumer
    pub async fn initialize(&mut self) -> Result<(), DLQError> {
        info!("Initializing DLQ processor...");

        // Create DLQ stream if it doesn't exist
        self.ensure_dlq_stream().await?;

        // Create consumer if it doesn't exist
        self.ensure_consumer().await?;

        let (shutdown_tx, _) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        Ok(())
    }

    async fn ensure_dlq_stream(&self) -> Result<(), DLQError> {
        let stream_name = &self.config.dlq_stream_name;

        match self.jetstream.get_stream(stream_name).await {
            Ok(_) => {
                info!("DLQ stream {} already exists", stream_name);
            }
            Err(_) => {
                info!("Creating DLQ stream: {}", stream_name);

                self.jetstream
                    .create_stream(StreamConfig {
                        name: stream_name.to_string(),
                        subjects: vec![self.config.dlq_subject_pattern()],
                        retention: RetentionPolicy::Limits,
                        max_messages: 100_000,
                        max_bytes: 1024 * 1024 * 100, // 100MB
                        max_age: Duration::from_secs(30 * 24 * 60 * 60), // 30 days
                        ..Default::default()
                    })
                    .await
                    .map_err(|e| DLQError::StreamCreationFailed(e.to_string()))?;

                info!("DLQ stream {} created successfully", stream_name);
            }
        }

        Ok(())
    }

    async fn ensure_consumer(&self) -> Result<(), DLQError> {
        let stream_name = &self.config.dlq_stream_name;
        let consumer_name = &self.config.consumer_name;

        let stream = self
            .jetstream
            .get_stream(stream_name)
            .await
            .map_err(|e| DLQError::StreamNotFound(e.to_string()))?;

        match stream.get_consumer::<PullConsumerConfig>(consumer_name).await {
            Ok(_) => {
                info!("DLQ consumer {} already exists", consumer_name);
            }
            Err(_) => {
                info!("Creating DLQ consumer: {}", consumer_name);

                stream
                    .add_consumer(PullConsumerConfig {
                        name: Some(consumer_name.to_string()),
                        durable_name: Some(consumer_name.to_string()),
                        description: Some("Dead Letter Queue processor consumer".to_string()),
                        ack_policy: AckPolicy::Explicit,
                        deliver_policy: DeliverPolicy::All,
                        ack_wait: self.config.ack_wait,
                        max_deliver: self.config.max_deliver,
                        ..Default::default()
                    })
                    .await
                    .map_err(|e| DLQError::ConsumerCreationFailed(e.to_string()))?;

                info!("DLQ consumer {} created successfully", consumer_name);
            }
        }

        Ok(())
    }

    /// Process messages from the DLQ
    pub async fn process_dlq(&self) -> Result<DLQProcessingResult, DLQError> {
        let start_time = std::time::Instant::now();
        let mut result = DLQProcessingResult::new(Duration::ZERO);

        let stream = self
            .jetstream
            .get_stream(&self.config.dlq_stream_name)
            .await
            .map_err(|e| DLQError::StreamNotFound(e.to_string()))?;

        let consumer = stream
            .get_consumer::<PullConsumerConfig>(&self.config.consumer_name)
            .await
            .map_err(|e| DLQError::ConsumerNotFound(e.to_string()))?;

        let mut messages = consumer
            .messages()
            .await
            .map_err(|e| DLQError::ConsumerError(e.to_string()))?;

        let mut batch_count = 0;
        while batch_count < self.config.batch_size {
            let message_result = tokio::time::timeout(Duration::from_secs(1), messages.next()).await;

            match message_result {
                Ok(Some(Ok(message))) => {
                    match self.process_message(&message.payload).await {
                        Ok(_) => {
                            result.processed += 1;
                            message.ack().await.map_err(|e| DLQError::AckFailed(e.to_string()))?;
                        }
                        Err(e) => {
                            result.failed += 1;
                            self.metrics.record_failure(message.subject.clone());
                            error!("Failed to process DLQ message: {}", e);
                        }
                    }
                    batch_count += 1;
                }
                Ok(None) => {
                    // No more messages
                    break;
                }
                Err(_) => {
                    // Timeout, no more messages in batch
                    break;
                }
            }
        }

        result.duration = start_time.elapsed();
        self.metrics.record_processing(&result);

        info!(
            "DLQ processing complete: processed={}, failed={}, duration={:?}",
            result.processed, result.failed, result.duration
        );

        Ok(result)
    }

    async fn process_message(&self, payload: &[u8]) -> Result<(), DLQError> {
        let envelope: DLQMessageEnvelope = serde_json::from_slice(payload)
            .map_err(|e| DLQError::DeserializationFailed(e.to_string()))?;

        // Check if we should alert on repeated failures
        let failure_count = self.metrics.get_failure_count(&envelope.original_subject);
        if failure_count >= self.config.alert_threshold {
            self.send_alert(DLQAlert {
                severity: DLQAlertSeverity::High,
                subject: envelope.original_subject.clone(),
                message: format!(
                    "Subject {} has failed {} times, requires manual intervention",
                    envelope.original_subject, failure_count
                ),
                deliver_count: envelope.deliver_count,
            }).await;
        }

        // Here you would implement the actual replay logic
        // For now, we just log the message
        info!(
            "Processing DLQ message from subject: {}, correlation_id: {:?}",
            envelope.original_subject, envelope.metadata.correlation_id
        );

        Ok(())
    }

    async fn send_alert(&self, alert: DLQAlert) {
        if let Some(sender) = &self.alert_sender {
            if let Err(e) = sender.send(alert).await {
                error!("Failed to send DLQ alert: {}", e);
            }
        }
    }

    /// Replay all messages from DLQ to their original subjects
    pub async fn replay_all(&self) -> Result<DLQProcessingResult, DLQError> {
        let start_time = std::time::Instant::now();
        let mut result = DLQProcessingResult::new(Duration::ZERO);

        let stream = self
            .jetstream
            .get_stream(&self.config.dlq_stream_name)
            .await
            .map_err(|e| DLQError::StreamNotFound(e.to_string()))?;

        let consumer = stream
            .get_consumer::<PullConsumerConfig>(&self.config.consumer_name)
            .await
            .map_err(|e| DLQError::ConsumerNotFound(e.to_string()))?;

        let mut messages = consumer
            .messages()
            .await
            .map_err(|e| DLQError::ConsumerError(e.to_string()))?;

        while let Some(message_result) = messages.next().await {
            match message_result {
                Ok(message) => {
                    match self.replay_message(&message.payload).await {
                        Ok(_) => {
                            result.processed += 1;
                            message.ack().await.map_err(|e| DLQError::AckFailed(e.to_string()))?;
                        }
                        Err(e) => {
                            result.failed += 1;
                            error!("Failed to replay DLQ message: {}", e);
                        }
                    }
                }
                Err(e) => {
                    result.failed += 1;
                    error!("Failed to receive DLQ message: {}", e);
                }
            }
        }

        result.duration = start_time.elapsed();
        self.metrics.record_processing(&result);

        Ok(result)
    }

    async fn replay_message(&self, payload: &[u8]) -> Result<(), DLQError> {
        let envelope: DLQMessageEnvelope = serde_json::from_slice(payload)
            .map_err(|e| DLQError::DeserializationFailed(e.to_string()))?;

        // Replay to original subject
        self.jetstream
            .publish(&envelope.original_subject, envelope.payload.into())
            .await
            .map_err(|e| DLQError::PublishFailed(e.to_string()))?
            .await
            .map_err(|e| DLQError::AckFailed(e.to_string()))?;

        info!(
            "Replayed message to subject: {}",
            envelope.original_subject
        );

        Ok(())
    }

    /// Get current metrics snapshot
    pub fn metrics(&self) -> (u64, u64) {
        self.metrics.snapshot()
    }
}

/// DLQ Alert for notifying about repeated failures
#[derive(Debug, Clone)]
pub struct DLQAlert {
    pub severity: DLQAlertSeverity,
    pub subject: String,
    pub message: String,
    pub deliver_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DLQAlertSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// DLQ Error types
#[derive(Debug, thiserror::Error)]
pub enum DLQError {
    #[error("Stream not found: {0}")]
    StreamNotFound(String),

    #[error("Failed to create stream: {0}")]
    StreamCreationFailed(String),

    #[error("Consumer not found: {0}")]
    ConsumerNotFound(String),

    #[error("Failed to create consumer: {0}")]
    ConsumerCreationFailed(String),

    #[error("Consumer error: {0}")]
    ConsumerError(String),

    #[error("Failed to deserialize message: {0}")]
    DeserializationFailed(String),

    #[error("Failed to publish message: {0}")]
    PublishFailed(String),

    #[error("Failed to ack message: {0}")]
    AckFailed(String),

    #[error("Replay failed: {0}")]
    ReplayFailed(String),
}

/// Commands for DLQ administration
#[derive(Debug, Clone, clap::Subcommand)]
pub enum DLQCommands {
    /// List messages in DLQ
    List {
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// Replay messages from DLQ
    Replay {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        subject: Option<String>,
    },

    /// Clear DLQ (dangerous!)
    Clear {
        #[arg(long)]
        force: bool,
    },

    /// Show DLQ statistics
    Stats,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dlq_config_defaults() {
        let config = DLQConfig::default();
        assert_eq!(config.stream_prefix, "HODEI");
        assert_eq!(config.dlq_stream_name, "HODEI_DLQ");
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.max_deliver, 5);
    }

    #[test]
    fn test_dlq_message_envelope_serialization() {
        let envelope = DLQMessageEnvelope {
            original_subject: "hodei.events.jobs.created".to_string(),
            payload: vec![1, 2, 3],
            metadata: DLQMessageMetadata {
                correlation_id: Some("corr-123".to_string()),
                causation_id: None,
                trace_id: None,
                source: "test".to_string(),
            },
            deliver_count: 3,
            first_published_at: Utc::now(),
            moved_to_dlq_at: Utc::now(),
        };

        let json = serde_json::to_string(&envelope).unwrap();
        let deserialized: DLQMessageEnvelope = serde_json::from_str(&json).unwrap();

        assert_eq!(envelope.original_subject, deserialized.original_subject);
        assert_eq!(envelope.deliver_count, deserialized.deliver_count);
    }

    #[test]
    fn test_dlq_processing_result() {
        let result = DLQProcessingResult::new(Duration::from_secs(5));
        assert_eq!(result.processed, 0);
        assert_eq!(result.failed, 0);
        assert_eq!(result.duration, Duration::from_secs(5));
    }
}

//! Cancellation Saga NATS Consumer
//!
//! This module provides reactive cancellation saga triggering by subscribing to
//! domain events via NATS JetStream when users request job cancellation.
//!
//! # Features
//! - Event-driven cancellation saga triggering
//! - Automatic worker notification for job cancellation
//! - Support for JobCancelled events from the cancel API
//! - Durable consumers with checkpointing

use async_nats::Client;
use async_nats::jetstream::Context as JetStreamContext;
use async_nats::jetstream::consumer::pull::Config as PullConsumerConfig;
use async_nats::jetstream::consumer::{AckPolicy, DeliverPolicy};
use async_nats::jetstream::stream::{Config as StreamConfig, RetentionPolicy};
use futures::StreamExt;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::saga::{SagaOrchestrator, SagaType};
use hodei_server_domain::shared_kernel::{DomainError, JobId};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use hodei_shared::event_topics::job_topics;

/// Message envelope for NATS transport (matches nats.rs)
#[derive(Debug, Clone, Deserialize)]
pub struct NatsMessageEnvelope {
    pub payload: String,
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub correlation_id: Option<String>,
}

/// Configuration for cancellation saga consumer
#[derive(Debug, Clone)]
pub struct CancellationSagaConsumerConfig {
    /// Consumer name identifier
    pub consumer_name: String,

    /// Stream name prefix
    pub stream_prefix: String,

    /// Topic for JobCancelled events
    pub job_cancelled_topic: String,

    /// Consumer group for load balancing
    pub consumer_group: String,

    /// Maximum concurrent cancellations
    pub concurrency: usize,

    /// Ack wait timeout
    pub ack_wait: Duration,

    /// Maximum delivery attempts
    pub max_deliver: i64,

    /// Default saga timeout
    pub saga_timeout: Duration,
}

impl Default for CancellationSagaConsumerConfig {
    fn default() -> Self {
        Self {
            consumer_name: "cancellation-saga-consumer".to_string(),
            stream_prefix: "HODEI".to_string(),
            job_cancelled_topic: job_topics::CANCELLED.to_string(),
            consumer_group: "cancellation-processors".to_string(),
            concurrency: 5,
            ack_wait: Duration::from_secs(30),
            max_deliver: 3,
            saga_timeout: Duration::from_secs(30),
        }
    }
}

/// Result of cancellation saga processing
#[derive(Debug)]
pub enum CancellationSagaTriggerResult {
    /// Cancellation saga triggered successfully
    Triggered,
    /// Job not in cancellable state
    NotCancellable,
    /// No worker assigned to job
    NoWorkerAssigned,
    /// Cancellation trigger failed
    Failed(String),
}

/// Cancellation Saga Consumer
///
/// Consumes JobCancelled events from NATS JetStream and triggers CancellationSaga
/// to handle the graceful termination of running jobs.
///
/// The CancellationSaga will:
/// 1. Validate the job can be cancelled
/// 2. Notify the worker to stop execution
/// 3. Update job state to CANCELLED
/// 4. Release the worker for new jobs
#[derive(Clone)]
pub struct CancellationSagaConsumer<SO, JR>
where
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
    JR: JobRepository + Send + Sync + 'static,
{
    /// NATS client
    _client: Client,

    /// NATS JetStream context
    jetstream: JetStreamContext,

    /// Saga orchestrator
    orchestrator: Arc<SO>,

    /// Job repository for validation
    job_repository: Arc<JR>,

    /// Consumer configuration
    config: CancellationSagaConsumerConfig,

    /// Shutdown signal
    shutdown_tx: mpsc::Sender<()>,
}

impl<SO, JR> CancellationSagaConsumer<SO, JR>
where
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
    JR: JobRepository + Send + Sync + 'static,
{
    /// Create a new CancellationSagaConsumer
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: Client,
        jetstream: JetStreamContext,
        orchestrator: Arc<SO>,
        job_repository: Arc<JR>,
        config: Option<CancellationSagaConsumerConfig>,
    ) -> Self {
        let config = config.unwrap_or_default();
        let (shutdown_tx, _) = mpsc::channel(1);

        Self {
            _client: client,
            jetstream,
            orchestrator,
            job_repository,
            config,
            shutdown_tx,
        }
    }

    /// Start the consumer and begin processing events
    ///
    /// # Arguments
    /// * `shutdown_rx` - Optional broadcast receiver for shutdown signals. If provided, the consumer
    ///   will drain in-flight messages for the configured drain timeout before stopping.
    pub async fn start(
        &self,
        shutdown_rx: Option<tokio::sync::broadcast::Receiver<()>>,
    ) -> Result<(), DomainError> {
        info!(
            "🚫 CancellationSagaConsumer: Starting consumer '{}'",
            self.config.consumer_name
        );

        // Create stream for cancellation events and get stream info
        let stream_name = {
            let mut stream = self.ensure_stream().await?;
            let stream_info =
                stream
                    .info()
                    .await
                    .map_err(|e| DomainError::InfrastructureError {
                        message: format!("Failed to get stream info: {}", e),
                    })?;
            stream_info.config.name.clone()
        };

        // Create or get consumer
        let consumer = self.create_or_get_consumer_by_name(&stream_name).await?;

        // Start consuming messages
        let mut messages =
            consumer
                .messages()
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to create consumer stream: {}", e),
                })?;

        info!(
            "🚫 CancellationSagaConsumer: Started consuming from stream '{}'",
            stream_name
        );

        // Drain timeout configuration (US-18: Graceful Shutdown)
        let drain_timeout = Duration::from_secs(10);
        let mut drain_deadline = None;
        let mut in_shutdown = false;
        let mut shutdown_rx = shutdown_rx;

        while let Some(message_result) = messages.next().await {
            // Check if shutdown signal received
            if !in_shutdown {
                if let Some(ref mut rx) = shutdown_rx {
                    match rx.try_recv() {
                        Ok(()) | Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                            info!(
                                "🚫 CancellationSagaConsumer: Shutdown signal received, starting drain phase"
                            );
                            in_shutdown = true;
                            drain_deadline = Some(std::time::Instant::now() + drain_timeout);
                        }
                        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                            info!(
                                "🚫 CancellationSagaConsumer: Receiver lagged, entering drain phase"
                            );
                            in_shutdown = true;
                            drain_deadline = Some(std::time::Instant::now() + drain_timeout);
                        }
                        Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {}
                    }
                }
            } else if let Some(deadline) = drain_deadline {
                if std::time::Instant::now() >= deadline {
                    warn!("🚫 CancellationSagaConsumer: Drain timeout exceeded, forcing shutdown");
                    break;
                }
            }

            match message_result {
                Ok(message) => {
                    if in_shutdown {
                        if let Err(e) = message.ack().await {
                            error!(
                                "🚫 CancellationSagaConsumer: Failed to ack message during drain: {}",
                                e
                            );
                        }
                        continue;
                    }

                    if let Err(e) = self.process_message(&message.payload).await {
                        error!(
                            "🚫 CancellationSagaConsumer: Error processing message: {}",
                            e
                        );
                    }
                    if let Err(e) = message.ack().await {
                        error!("🚫 CancellationSagaConsumer: Failed to ack message: {}", e);
                    }
                }
                Err(e) => {
                    if in_shutdown {
                        error!(
                            "🚫 CancellationSagaConsumer: Message receive error during drain: {}, exiting",
                            e
                        );
                        break;
                    }
                    error!("🚫 CancellationSagaConsumer: Message receive error: {}", e);
                }
            }
        }

        info!("🚫 CancellationSagaConsumer: Consumer stopped");
        Ok(())
    }

    /// Process a single NATS message payload
    async fn process_message(&self, payload: &[u8]) -> Result<(), DomainError> {
        // Parse the envelope from the message payload
        let envelope: NatsMessageEnvelope =
            serde_json::from_slice(payload).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to deserialize envelope: {}", e),
            })?;

        // Deserialize the domain event from the envelope's payload
        let event: DomainEvent = serde_json::from_str(&envelope.payload).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to deserialize event from envelope: {}", e),
            }
        })?;

        match &event {
            DomainEvent::JobCancelled { job_id, reason, .. } => {
                info!(
                    "🚫 CancellationSagaConsumer: Received JobCancelled event for job {}",
                    job_id
                );
                self.handle_job_cancelled(job_id, reason.as_deref()).await?;
            }
            _ => {
                debug!(
                    "🚫 CancellationSagaConsumer: Ignoring event type '{}'",
                    event.event_type()
                );
            }
        }

        Ok(())
    }

    /// Handle JobCancelled event - trigger cancellation saga
    async fn handle_job_cancelled(
        &self,
        job_id: &JobId,
        reason: Option<&str>,
    ) -> Result<CancellationSagaTriggerResult, DomainError> {
        // Fetch job details for validation
        let job = self.job_repository.find_by_id(job_id).await.map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to fetch job {}: {}", job_id, e),
            }
        })?;

        if job.is_none() {
            debug!(
                "🚫 CancellationSagaConsumer: Job {} not found, skipping cancellation",
                job_id
            );
            return Ok(CancellationSagaTriggerResult::Failed(format!(
                "Job {} not found",
                job_id
            )));
        }

        let job = job.unwrap();

        // Check if job is in cancellable state
        let cancellable_states = [
            hodei_server_domain::shared_kernel::JobState::Pending,
            hodei_server_domain::shared_kernel::JobState::Assigned,
            hodei_server_domain::shared_kernel::JobState::Running,
            hodei_server_domain::shared_kernel::JobState::Scheduled,
        ];

        if !cancellable_states.contains(&job.state()) {
            info!(
                "🚫 CancellationSagaConsumer: Job {} is in state {:?}, not cancellable",
                job_id,
                job.state()
            );
            return Ok(CancellationSagaTriggerResult::NotCancellable);
        }

        // Trigger cancellation saga
        self.trigger_cancellation_saga(job_id, reason.unwrap_or("user_requested"))
            .await?;

        Ok(CancellationSagaTriggerResult::Triggered)
    }

    /// Trigger cancellation saga for a job
    async fn trigger_cancellation_saga(
        &self,
        job_id: &JobId,
        reason: &str,
    ) -> Result<(), DomainError> {
        // Create saga context with Cancellation saga type
        let saga_id = hodei_server_domain::saga::SagaId::new();
        let mut context = hodei_server_domain::saga::SagaContext::new(
            saga_id,
            SagaType::Cancellation,
            Some(format!("cancellation-{}", job_id.0)),
            Some("cancellation_saga_consumer".to_string()),
        );

        context.set_metadata("job_id", &job_id.to_string()).ok();
        context
            .set_metadata("cancellation_reason", &reason.to_string())
            .ok();

        // Create cancellation saga
        let saga = hodei_server_domain::saga::CancellationSaga::new(job_id.clone(), reason);

        // Execute saga
        match self.orchestrator.execute_saga(&saga, context).await {
            Ok(result) => {
                if result.state == hodei_server_domain::saga::SagaState::Completed {
                    info!(
                        "✅ CancellationSagaConsumer: Cancellation completed for job {}",
                        job_id
                    );
                    Ok(())
                } else {
                    let error_msg = result
                        .error_message
                        .clone()
                        .unwrap_or_else(|| "Unknown cancellation error".to_string());
                    error!(
                        "❌ CancellationSagaConsumer: Cancellation failed for job {}: {}",
                        job_id, error_msg
                    );
                    Err(DomainError::SagaError { message: error_msg })
                }
            }
            Err(e) => {
                error!(
                    "❌ CancellationSagaConsumer: Orchestrator error for job {}: {}",
                    job_id, e
                );
                Err(e)
            }
        }
    }

    /// Ensure the cancellation events stream exists
    async fn ensure_stream(&self) -> Result<async_nats::jetstream::stream::Stream, DomainError> {
        let stream_name = format!("{}_CANCELLATION_EVENTS", self.config.stream_prefix);

        match self.jetstream.get_stream(&stream_name).await {
            Ok(stream) => {
                debug!(
                    "🚫 CancellationSagaConsumer: Stream '{}' already exists",
                    stream_name
                );
                Ok(stream)
            }
            Err(_) => {
                let stream = self
                    .jetstream
                    .create_stream(StreamConfig {
                        name: stream_name.clone(),
                        subjects: vec![self.config.job_cancelled_topic.clone()],
                        retention: RetentionPolicy::WorkQueue,
                        max_messages: 50000,
                        max_bytes: 50 * 1024 * 1024, // 50MB
                        ..Default::default()
                    })
                    .await
                    .map_err(|e| DomainError::InfrastructureError {
                        message: format!("Failed to create stream {}: {}", stream_name, e),
                    })?;

                info!(
                    "🚫 CancellationSagaConsumer: Created stream '{}'",
                    stream_name
                );
                Ok(stream)
            }
        }
    }

    /// Create or get consumer for the stream by stream name
    async fn create_or_get_consumer_by_name(
        &self,
        stream_name: &str,
    ) -> Result<async_nats::jetstream::consumer::PullConsumer, DomainError> {
        let consumer_id = format!(
            "{}-{}",
            self.config.consumer_name, self.config.consumer_group
        );

        // Get stream to interact with consumer
        let stream = self.jetstream.get_stream(stream_name).await.map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to get stream {}: {}", stream_name, e),
            }
        })?;

        // Try to get existing consumer
        match stream.get_consumer(&consumer_id).await {
            Ok(consumer) => {
                debug!(
                    "🚫 CancellationSagaConsumer: Consumer '{}' already exists",
                    consumer_id
                );
                Ok(consumer)
            }
            Err(_) => {
                let consumer_config = PullConsumerConfig {
                    durable_name: Some(consumer_id.clone()),
                    deliver_policy: DeliverPolicy::All,
                    ack_policy: AckPolicy::Explicit,
                    ack_wait: self.config.ack_wait,
                    max_deliver: self.config.max_deliver,
                    max_ack_pending: self.config.concurrency as i64,
                    filter_subject: "".to_string(), // Subscribe to all subjects
                    ..Default::default()
                };

                let consumer = stream.create_consumer(consumer_config).await.map_err(|e| {
                    DomainError::InfrastructureError {
                        message: format!("Failed to create consumer {}: {}", consumer_id, e),
                    }
                })?;

                info!(
                    "🚫 CancellationSagaConsumer: Created consumer '{}'",
                    consumer_id
                );
                Ok(consumer)
            }
        }
    }

    /// Stop the consumer gracefully
    pub async fn stop(&self) {
        let _ = self.shutdown_tx.send(()).await;
        info!("🚫 CancellationSagaConsumer: Stop signal sent");
    }
}

/// Builder for CancellationSagaConsumer
#[derive(Debug)]
pub struct CancellationSagaConsumerBuilder<SO, JR>
where
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
    JR: JobRepository + Send + Sync + 'static,
{
    client: Option<Client>,
    jetstream: Option<JetStreamContext>,
    orchestrator: Option<Arc<SO>>,
    job_repository: Option<Arc<JR>>,
    config: Option<CancellationSagaConsumerConfig>,
}

impl<SO, JR> Default for CancellationSagaConsumerBuilder<SO, JR>
where
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
    JR: JobRepository + Send + Sync + 'static,
{
    fn default() -> Self {
        Self {
            client: None,
            jetstream: None,
            orchestrator: None,
            job_repository: None,
            config: None,
        }
    }
}

impl<SO, JR> CancellationSagaConsumerBuilder<SO, JR>
where
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
    JR: JobRepository + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_client(mut self, client: Client) -> Self {
        self.client = Some(client);
        self
    }

    pub fn with_jetstream(mut self, jetstream: JetStreamContext) -> Self {
        self.jetstream = Some(jetstream);
        self
    }

    pub fn with_orchestrator(mut self, orchestrator: Arc<SO>) -> Self {
        self.orchestrator = Some(orchestrator);
        self
    }

    pub fn with_job_repository(mut self, job_repository: Arc<JR>) -> Self {
        self.job_repository = Some(job_repository);
        self
    }

    pub fn with_config(mut self, config: CancellationSagaConsumerConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(self) -> anyhow::Result<CancellationSagaConsumer<SO, JR>> {
        let client = self
            .client
            .ok_or_else(|| anyhow::anyhow!("client is required"))?;
        let jetstream = self
            .jetstream
            .ok_or_else(|| anyhow::anyhow!("jetstream is required"))?;
        let orchestrator = self
            .orchestrator
            .ok_or_else(|| anyhow::anyhow!("orchestrator is required"))?;
        let job_repository = self
            .job_repository
            .ok_or_else(|| anyhow::anyhow!("job_repository is required"))?;

        Ok(CancellationSagaConsumer::new(
            client,
            jetstream,
            orchestrator,
            job_repository,
            self.config,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::saga::SagaType;

    #[test]
    fn test_cancellation_saga_trigger_result_variants() {
        let triggered = CancellationSagaTriggerResult::Triggered;
        let not_cancellable = CancellationSagaTriggerResult::NotCancellable;
        let no_worker = CancellationSagaTriggerResult::NoWorkerAssigned;
        let failed = CancellationSagaTriggerResult::Failed("error".to_string());

        assert!(matches!(
            triggered,
            CancellationSagaTriggerResult::Triggered
        ));
        assert!(matches!(
            not_cancellable,
            CancellationSagaTriggerResult::NotCancellable
        ));
        assert!(matches!(
            no_worker,
            CancellationSagaTriggerResult::NoWorkerAssigned
        ));
        assert!(matches!(failed, CancellationSagaTriggerResult::Failed(_)));
    }

    #[test]
    fn test_config_default_values() {
        let config = CancellationSagaConsumerConfig::default();
        assert_eq!(
            config.consumer_name,
            "cancellation-saga-consumer".to_string()
        );
        assert_eq!(config.concurrency, 5);
        assert_eq!(config.max_deliver, 3);
        assert_eq!(config.saga_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_saga_type_is_cancellation() {
        assert!(SagaType::Cancellation.is_cancellation());
        assert!(!SagaType::Execution.is_cancellation());
        assert!(!SagaType::Recovery.is_cancellation());
        assert!(!SagaType::Cleanup.is_cancellation());
    }
}

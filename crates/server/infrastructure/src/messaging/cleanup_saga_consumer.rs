//! Cleanup Saga NATS Consumer
//!
//! This module provides reactive cleanup saga triggering by subscribing to
//! domain events via NATS JetStream when workers or jobs need cleanup.
//!
//! # Features
//! - Event-driven cleanup saga triggering
//! - Automatic resource cleanup when jobs complete or workers terminate
//! - Support for JobCompleted, JobFailed, and WorkerTerminated events
//! - Durable consumers with checkpointing

use async_nats::Client;
use async_nats::jetstream::Context as JetStreamContext;
use async_nats::jetstream::consumer::pull::Config as PullConsumerConfig;
use async_nats::jetstream::consumer::{AckPolicy, DeliverPolicy};
use futures::StreamExt;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::saga::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitState,
};
use hodei_server_domain::saga::{SagaOrchestrator, SagaType};
use hodei_server_domain::shared_kernel::{DomainError, JobId, WorkerId};
use hodei_server_domain::workers::WorkerRegistry;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, instrument, warn};

use hodei_shared::event_topics::{job_topics, worker_topics};

/// Message envelope for NATS transport (matches nats.rs)
#[derive(Debug, Clone, Deserialize)]
pub struct NatsMessageEnvelope {
    pub payload: String,
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub correlation_id: Option<String>,
}

/// Configuration for cleanup saga consumer
#[derive(Debug, Clone)]
pub struct CleanupSagaConsumerConfig {
    /// Consumer name identifier
    pub consumer_name: String,

    /// Stream name prefix
    pub stream_prefix: String,

    /// Topic for JobCompleted events
    pub job_completed_topic: String,

    /// Topic for JobFailed events
    pub job_failed_topic: String,

    /// Topic for WorkerTerminated events
    pub worker_terminated_topic: String,

    /// Consumer group for load balancing
    pub consumer_group: String,

    /// Maximum concurrent cleanups
    pub concurrency: usize,

    /// Ack wait timeout
    pub ack_wait: Duration,

    /// Maximum delivery attempts
    pub max_deliver: i64,

    /// Enable automatic cleanup dispatch
    pub enable_auto_dispatch: bool,

    /// Default saga timeout
    pub saga_timeout: Duration,

    /// EPIC-85 US-05: Circuit breaker configuration
    pub circuit_breaker_failure_threshold: u64,
    pub circuit_breaker_open_duration: Duration,
    pub circuit_breaker_success_threshold: u64,
    pub circuit_breaker_call_timeout: Duration,
}

impl Default for CleanupSagaConsumerConfig {
    fn default() -> Self {
        Self {
            consumer_name: "cleanup-saga-consumer".to_string(),
            stream_prefix: "HODEI".to_string(),
            job_completed_topic: job_topics::COMPLETED.to_string(),
            job_failed_topic: job_topics::FAILED.to_string(),
            worker_terminated_topic: worker_topics::TERMINATED.to_string(),
            consumer_group: "cleanup-dispatchers".to_string(),
            concurrency: 5,
            ack_wait: Duration::from_secs(30),
            max_deliver: 3,
            enable_auto_dispatch: true,
            saga_timeout: Duration::from_secs(60),
            // EPIC-85 US-05: Circuit breaker defaults
            circuit_breaker_failure_threshold: 5,
            circuit_breaker_open_duration: Duration::from_secs(30),
            circuit_breaker_success_threshold: 2,
            circuit_breaker_call_timeout: Duration::from_secs(10),
        }
    }
}

/// Result of cleanup saga processing
#[derive(Debug)]
pub enum CleanupSagaTriggerResult {
    /// Cleanup dispatched successfully
    Dispatched,
    /// No cleanup needed
    NoCleanupNeeded,
    /// Resource already cleaned up
    AlreadyCleanedUp,
    /// Cleanup trigger failed
    Failed(String),
}

/// Cleanup Saga Consumer
///
/// Consumes JobCompleted, JobFailed, and WorkerTerminated events from NATS JetStream
/// and triggers cleanup sagas for resource cleanup.
#[derive(Clone)]
pub struct CleanupSagaConsumer<SO, JR, WR>
where
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
    JR: JobRepository + Send + Sync + 'static,
    WR: WorkerRegistry + Send + Sync + 'static,
{
    /// NATS client
    _client: Client,

    /// NATS JetStream context
    jetstream: JetStreamContext,

    /// Saga orchestrator
    orchestrator: Arc<SO>,

    /// Job repository for cleanup operations
    job_repository: Arc<JR>,

    /// Worker registry for cleanup operations
    worker_registry: Arc<WR>,

    /// Consumer configuration
    config: CleanupSagaConsumerConfig,

    /// EPIC-85 US-05: Circuit breaker for resilience
    circuit_breaker: Option<Arc<CircuitBreaker>>,

    /// Shutdown signal
    shutdown_tx: mpsc::Sender<()>,
}

impl<SO, JR, WR> CleanupSagaConsumer<SO, JR, WR>
where
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
    JR: JobRepository + Send + Sync + 'static,
    WR: WorkerRegistry + Send + Sync + 'static,
{
    /// Create a new CleanupSagaConsumer
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: Client,
        jetstream: JetStreamContext,
        orchestrator: Arc<SO>,
        job_repository: Arc<JR>,
        worker_registry: Arc<WR>,
        config: Option<CleanupSagaConsumerConfig>,
    ) -> Self {
        let config = config.unwrap_or_default();
        let (shutdown_tx, _) = mpsc::channel(1);

        // EPIC-85 US-05: Initialize circuit breaker
        let circuit_breaker = Arc::new(CircuitBreaker::new(
            format!("cleanup-saga-{}", config.consumer_name),
            CircuitBreakerConfig {
                failure_threshold: config.circuit_breaker_failure_threshold,
                open_duration: config.circuit_breaker_open_duration,
                success_threshold: config.circuit_breaker_success_threshold,
                call_timeout: config.circuit_breaker_call_timeout,
                failure_rate_threshold: 50,
                failure_rate_window: 100,
            },
        ));

        Self {
            _client: client,
            jetstream,
            orchestrator,
            job_repository,
            worker_registry,
            config,
            circuit_breaker: Some(circuit_breaker),
            shutdown_tx,
        }
    }

    /// EPIC-85 US-05: Get current circuit breaker state
    pub fn circuit_breaker_state(&self) -> CircuitState {
        self.circuit_breaker
            .as_ref()
            .map(|cb| cb.state())
            .unwrap_or(CircuitState::Closed)
    }

    /// EPIC-85 US-05: Check if circuit allows requests
    pub fn is_circuit_closed(&self) -> bool {
        self.circuit_breaker
            .as_ref()
            .map(|cb| cb.allow_request())
            .unwrap_or(true)
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
            "🧹 CleanupSagaConsumer: Starting consumer '{}'",
            self.config.consumer_name
        );

        // Create stream for cleanup events and get stream info
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
            "🧹 CleanupSagaConsumer: Started consuming from stream '{}'",
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
                                "🧹 CleanupSagaConsumer: Shutdown signal received, starting drain phase"
                            );
                            in_shutdown = true;
                            drain_deadline = Some(std::time::Instant::now() + drain_timeout);
                        }
                        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                            info!("🧹 CleanupSagaConsumer: Receiver lagged, entering drain phase");
                            in_shutdown = true;
                            drain_deadline = Some(std::time::Instant::now() + drain_timeout);
                        }
                        Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {}
                    }
                }
            } else if let Some(deadline) = drain_deadline {
                if std::time::Instant::now() >= deadline {
                    warn!("🧹 CleanupSagaConsumer: Drain timeout exceeded, forcing shutdown");
                    break;
                }
            }

            match message_result {
                Ok(message) => {
                    if in_shutdown {
                        if let Err(e) = message.ack().await {
                            error!(
                                "🧹 CleanupSagaConsumer: Failed to ack message during drain: {}",
                                e
                            );
                        }
                        continue;
                    }

                    // EPIC-85 US-05: Check if circuit allows processing
                    let can_process = self
                        .circuit_breaker
                        .as_ref()
                        .map(|cb| cb.allow_request())
                        .unwrap_or(true);

                    if can_process {
                        let payload = message.payload.clone();
                        let circuit_breaker = self.circuit_breaker.clone();

                        // EPIC-85 US-05: Process with circuit breaker protection
                        let result = match &circuit_breaker {
                            Some(cb) => {
                                let process_future = self.process_message(&payload);
                                cb.execute(async {
                                    process_future.await.map_err(|e| {
                                        DomainError::InfrastructureError {
                                            message: e.to_string(),
                                        }
                                    })
                                })
                                .await
                                .map_err(|e| match e {
                                    CircuitBreakerError::Open => DomainError::InfrastructureError {
                                        message: "Circuit breaker open".to_string(),
                                    },
                                    CircuitBreakerError::Timeout => {
                                        DomainError::InfrastructureError {
                                            message: "Circuit breaker timeout".to_string(),
                                        }
                                    }
                                    CircuitBreakerError::Failed(e) => e,
                                })
                            }
                            None => self.process_message(&message.payload).await,
                        };

                        match result {
                            Ok(()) => {
                                // EPIC-85 US-05: Record success
                                if let Some(ref cb) = circuit_breaker {
                                    cb.record_success();
                                }
                            }
                            Err(e) => {
                                error!("🧹 CleanupSagaConsumer: Error processing message: {}", e);
                                // EPIC-85 US-05: Record failure
                                if let Some(ref cb) = circuit_breaker {
                                    cb.record_failure();
                                }
                            }
                        }
                    } else {
                        // Circuit is open - log warning
                        warn!("🧹 CleanupSagaConsumer: Circuit breaker open, skipping message");
                        // EPIC-85 US-05: Record failure for circuit being open
                        if let Some(ref cb) = self.circuit_breaker {
                            cb.record_failure();
                        }
                    }

                    if let Err(e) = message.ack().await {
                        error!("🧹 CleanupSagaConsumer: Failed to ack message: {}", e);
                    }
                }
                Err(e) => {
                    if in_shutdown {
                        error!(
                            "🧹 CleanupSagaConsumer: Message receive error during drain: {}, exiting",
                            e
                        );
                        break;
                    }
                    error!("🧹 CleanupSagaConsumer: Message receive error: {}", e);
                    // EPIC-85 US-05: Record failure in circuit breaker
                    if let Some(ref cb) = self.circuit_breaker {
                        cb.record_failure();
                    }
                }
            }
        }

        info!("🧹 CleanupSagaConsumer: Consumer stopped");
        Ok(())
    }

    /// Process a single NATS message payload
    #[instrument(skip_all)]
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
            DomainEvent::JobStatusChanged {
                job_id, new_state, ..
            } => {
                // Check for terminal states that need cleanup
                let needs_cleanup = matches!(
                    new_state,
                    hodei_server_domain::shared_kernel::JobState::Succeeded
                        | hodei_server_domain::shared_kernel::JobState::Failed
                        | hodei_server_domain::shared_kernel::JobState::Cancelled
                        | hodei_server_domain::shared_kernel::JobState::Timeout
                );

                if needs_cleanup {
                    info!(
                        "🧹 CleanupSagaConsumer: Job {} transitioned to terminal state {:?}",
                        job_id, new_state
                    );
                    self.handle_job_completed(job_id).await?;
                } else {
                    debug!(
                        "🧹 CleanupSagaConsumer: Job {} transitioned to state {:?}, skipping cleanup",
                        job_id, new_state
                    );
                }
            }
            DomainEvent::WorkerTerminated { worker_id, .. } => {
                info!(
                    "🧹 CleanupSagaConsumer: Received WorkerTerminated event for worker {}",
                    worker_id
                );
                self.handle_worker_terminated(worker_id).await?;
            }
            _ => {
                debug!(
                    "🧹 CleanupSagaConsumer: Ignoring event type '{}'",
                    event.event_type()
                );
            }
        }

        Ok(())
    }

    /// Handle JobCompleted event - trigger cleanup saga
    async fn handle_job_completed(&self, job_id: &JobId) -> Result<(), DomainError> {
        // Fetch job details
        let job = self.job_repository.find_by_id(job_id).await.map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to fetch job {}: {}", job_id, e),
            }
        })?;

        if job.is_none() {
            debug!(
                "🧹 CleanupSagaConsumer: Job {} not found, skipping cleanup",
                job_id
            );
            return Ok(());
        }

        let job = job.unwrap();

        // Check if cleanup is needed
        if job.is_terminal_state() {
            info!(
                "🧹 CleanupSagaConsumer: Job {} is in terminal state, triggering cleanup",
                job_id
            );
            self.trigger_cleanup_saga(job_id, "job_completion").await?;
        } else {
            debug!(
                "🧹 CleanupSagaConsumer: Job {} is not in terminal state, skipping",
                job_id
            );
        }

        Ok(())
    }

    /// Handle WorkerTerminated event - trigger cleanup saga
    async fn handle_worker_terminated(&self, worker_id: &WorkerId) -> Result<(), DomainError> {
        // Check if worker exists
        let worker = self.worker_registry.get(worker_id).await.map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to fetch worker {}: {}", worker_id, e),
            }
        })?;

        if worker.is_none() {
            debug!(
                "🧹 CleanupSagaConsumer: Worker {} not found, skipping cleanup",
                worker_id
            );
            return Ok(());
        }

        info!(
            "🧹 CleanupSagaConsumer: Worker {} terminated, triggering cleanup",
            worker_id
        );

        self.trigger_cleanup_saga_by_worker(worker_id).await?;
        Ok(())
    }

    /// Trigger cleanup saga for a job
    #[instrument(skip(self), fields(job_id = %job_id, trigger_reason = %trigger_reason))]
    async fn trigger_cleanup_saga(
        &self,
        job_id: &JobId,
        trigger_reason: &str,
    ) -> Result<(), DomainError> {
        // Create saga context with correct saga type
        let saga_id = hodei_server_domain::saga::SagaId::new();
        let mut context = hodei_server_domain::saga::SagaContext::new(
            saga_id,
            SagaType::Cleanup, // Use correct saga type
            Some(format!("cleanup-{}", job_id.0)),
            Some("cleanup_saga_consumer".to_string()),
        );

        context.set_metadata("job_id", &job_id.to_string()).ok();
        context
            .set_metadata("trigger_reason", &trigger_reason.to_string())
            .ok();

        // Create cleanup saga
        let saga = hodei_server_domain::saga::CleanupSaga::new();

        // Execute saga
        match self.orchestrator.execute_saga(&saga, context).await {
            Ok(result) => {
                if result.state == hodei_server_domain::saga::SagaState::Completed {
                    info!(
                        "✅ CleanupSagaConsumer: Cleanup completed for job {}",
                        job_id
                    );
                    Ok(())
                } else {
                    let error_msg = result
                        .error_message
                        .clone()
                        .unwrap_or_else(|| "Unknown cleanup error".to_string());
                    error!(
                        "❌ CleanupSagaConsumer: Cleanup failed for job {}: {}",
                        job_id, error_msg
                    );
                    Err(DomainError::SagaError { message: error_msg })
                }
            }
            Err(e) => {
                error!(
                    "❌ CleanupSagaConsumer: Orchestrator error for job {}: {}",
                    job_id, e
                );
                Err(e)
            }
        }
    }

    /// Trigger cleanup saga for a worker
    #[instrument(skip(self), fields(worker_id = %worker_id))]
    async fn trigger_cleanup_saga_by_worker(
        &self,
        worker_id: &WorkerId,
    ) -> Result<(), DomainError> {
        // Create saga context with correct saga type
        let saga_id = hodei_server_domain::saga::SagaId::new();
        let mut context = hodei_server_domain::saga::SagaContext::new(
            saga_id,
            SagaType::Cleanup, // Use correct saga type
            Some(format!("cleanup-worker-{}", worker_id.0)),
            Some("cleanup_saga_consumer".to_string()),
        );

        context
            .set_metadata("worker_id", &worker_id.to_string())
            .ok();
        context
            .set_metadata("trigger_reason", &"worker_termination".to_string())
            .ok();

        // Create cleanup saga for worker cleanup
        let saga = hodei_server_domain::saga::CleanupSaga::new();

        // Execute saga
        match self.orchestrator.execute_saga(&saga, context).await {
            Ok(result) => {
                if result.is_success() {
                    info!(
                        "✅ CleanupSagaConsumer: Cleanup completed for worker {}",
                        worker_id
                    );
                    Ok(())
                } else {
                    let error_msg = result
                        .error_message
                        .clone()
                        .unwrap_or_else(|| "Unknown cleanup error".to_string());
                    error!(
                        "❌ CleanupSagaConsumer: Cleanup failed for worker {}: {}",
                        worker_id, error_msg
                    );
                    Err(DomainError::SagaError { message: error_msg })
                }
            }
            Err(e) => {
                error!(
                    "❌ CleanupSagaConsumer: Orchestrator error for worker {}: {}",
                    worker_id, e
                );
                Err(e)
            }
        }
    }

    /// Ensure the cleanup events stream exists
    /// Uses the shared HODEI_EVENTS stream (EPIC-32 design) instead of creating separate streams
    /// to avoid NATS stream subject overlap errors
    async fn ensure_stream(&self) -> Result<async_nats::jetstream::stream::Stream, DomainError> {
        // Use the shared events stream - all domain events use HODEI_EVENTS
        let stream_name = format!("{}_EVENTS", self.config.stream_prefix);

        match self.jetstream.get_stream(&stream_name).await {
            Ok(stream) => {
                debug!(
                    "🧹 CleanupSagaConsumer: Using shared stream '{}'",
                    stream_name
                );
                Ok(stream)
            }
            Err(_) => {
                // This should not happen - the main server creates HODEI_EVENTS
                Err(DomainError::InfrastructureError {
                    message: format!(
                        "Stream '{}' does not exist. Ensure the server has created the events stream.",
                        stream_name
                    ),
                })
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
                    "🧹 CleanupSagaConsumer: Consumer '{}' already exists",
                    consumer_id
                );
                Ok(consumer)
            }
            Err(_) => {
                // Use filter_subject with wildcard to capture all events from shared stream
                // Specific event filtering is done in process_message based on event type
                let consumer_config = PullConsumerConfig {
                    durable_name: Some(consumer_id.clone()),
                    deliver_policy: DeliverPolicy::All,
                    ack_policy: AckPolicy::Explicit,
                    ack_wait: self.config.ack_wait,
                    max_deliver: self.config.max_deliver,
                    max_ack_pending: self.config.concurrency as i64,
                    // Subscribe to all events from shared stream, filter in process_message
                    filter_subject: "hodei.events.>".to_string(),
                    ..Default::default()
                };

                let consumer = stream.create_consumer(consumer_config).await.map_err(|e| {
                    DomainError::InfrastructureError {
                        message: format!("Failed to create consumer {}: {}", consumer_id, e),
                    }
                })?;

                info!("🧹 CleanupSagaConsumer: Created consumer '{}'", consumer_id);
                Ok(consumer)
            }
        }
    }

    /// Stop the consumer gracefully
    pub async fn stop(&self) {
        let _ = self.shutdown_tx.send(()).await;
        info!("🧹 CleanupSagaConsumer: Stop signal sent");
    }
}

/// Builder for CleanupSagaConsumer
#[derive(Debug)]
pub struct CleanupSagaConsumerBuilder<SO, JR, WR>
where
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
    JR: JobRepository + Send + Sync + 'static,
    WR: WorkerRegistry + Send + Sync + 'static,
{
    client: Option<Client>,
    jetstream: Option<JetStreamContext>,
    orchestrator: Option<Arc<SO>>,
    job_repository: Option<Arc<JR>>,
    worker_registry: Option<Arc<WR>>,
    config: Option<CleanupSagaConsumerConfig>,
}

impl<SO, JR, WR> Default for CleanupSagaConsumerBuilder<SO, JR, WR>
where
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
    JR: JobRepository + Send + Sync + 'static,
    WR: WorkerRegistry + Send + Sync + 'static,
{
    fn default() -> Self {
        Self {
            client: None,
            jetstream: None,
            orchestrator: None,
            job_repository: None,
            worker_registry: None,
            config: None,
        }
    }
}

impl<SO, JR, WR> CleanupSagaConsumerBuilder<SO, JR, WR>
where
    SO: SagaOrchestrator<Error = DomainError> + Send + Sync + 'static,
    JR: JobRepository + Send + Sync + 'static,
    WR: WorkerRegistry + Send + Sync + 'static,
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

    pub fn with_worker_registry(mut self, worker_registry: Arc<WR>) -> Self {
        self.worker_registry = Some(worker_registry);
        self
    }

    pub fn with_config(mut self, config: CleanupSagaConsumerConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(self) -> anyhow::Result<CleanupSagaConsumer<SO, JR, WR>> {
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
        let worker_registry = self
            .worker_registry
            .ok_or_else(|| anyhow::anyhow!("worker_registry is required"))?;

        Ok(CleanupSagaConsumer::new(
            client,
            jetstream,
            orchestrator,
            job_repository,
            worker_registry,
            self.config,
        ))
    }
}

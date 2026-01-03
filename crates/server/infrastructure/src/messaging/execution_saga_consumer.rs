//! Execution Saga NATS Consumer
//!
//! This module provides reactive execution saga triggering by subscribing to
//! domain events via NATS JetStream when jobs are ready for execution.
//!
//! # Features
//! - Event-driven execution saga triggering
//! - Automatic job dispatch when workers become ready
//! - Support for JobQueued and WorkerReady events
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
use hodei_server_domain::shared_kernel::{DomainError, JobId, JobState, WorkerId};
use hodei_server_domain::workers::WorkerRegistry;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Configuration for execution saga consumer
#[derive(Debug, Clone)]
pub struct ExecutionSagaConsumerConfig {
    /// Consumer name identifier
    pub consumer_name: String,

    /// Stream name prefix
    pub stream_prefix: String,

    /// Topic for JobQueued events
    pub job_queued_topic: String,

    /// Topic for WorkerReady events
    pub worker_ready_topic: String,

    /// Consumer group for load balancing
    pub consumer_group: String,

    /// Maximum concurrent executions
    pub concurrency: usize,

    /// Ack wait timeout
    pub ack_wait: Duration,

    /// Maximum delivery attempts
    pub max_deliver: i64,

    /// Enable automatic job dispatch
    pub enable_auto_dispatch: bool,

    /// Default saga timeout
    pub saga_timeout: Duration,
}

impl Default for ExecutionSagaConsumerConfig {
    fn default() -> Self {
        Self {
            consumer_name: "execution-saga-consumer".to_string(),
            stream_prefix: "HODEI".to_string(),
            job_queued_topic: "hodei_events.jobs.queued".to_string(),
            worker_ready_topic: "hodei_events.workers.ready".to_string(),
            consumer_group: "execution-dispatchers".to_string(),
            concurrency: 10,
            ack_wait: Duration::from_secs(30),
            max_deliver: 3,
            enable_auto_dispatch: true,
            saga_timeout: Duration::from_secs(60),
        }
    }
}

/// Result of execution saga processing
#[derive(Debug)]
pub enum ExecutionSagaTriggerResult {
    /// Job dispatched successfully
    Dispatched,
    /// No workers available
    NoWorkersAvailable,
    /// Job already being processed
    AlreadyProcessing,
    /// Saga trigger failed
    Failed(String),
}

/// Execution Saga Consumer
///
/// Consumes JobQueued and WorkerReady events from NATS JetStream
/// and triggers execution sagas for job dispatch.
/// Uses trait objects for dynamic dispatch with the SagaOrchestrator.
#[derive(Clone)]
pub struct ExecutionSagaConsumer {
    /// NATS client
    _client: Client,

    /// NATS JetStream context
    jetstream: JetStreamContext,

    /// Saga orchestrator (trait object for dynamic dispatch)
    orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,

    /// Job repository for fetching job details
    job_repository: Arc<dyn JobRepository + Send + Sync>,

    /// Worker registry for checking worker availability
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,

    /// Consumer configuration
    config: ExecutionSagaConsumerConfig,

    /// Shutdown signal
    shutdown_tx: mpsc::Sender<()>,
}

impl ExecutionSagaConsumer {
    /// Create a new ExecutionSagaConsumer
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: Client,
        jetstream: JetStreamContext,
        orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,
        job_repository: Arc<dyn JobRepository + Send + Sync>,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
        config: Option<ExecutionSagaConsumerConfig>,
    ) -> Self {
        let config = config.unwrap_or_default();
        let (shutdown_tx, _) = mpsc::channel(1);

        Self {
            _client: client,
            jetstream,
            orchestrator,
            job_repository,
            worker_registry,
            config,
            shutdown_tx,
        }
    }

    /// Start the consumer and begin processing events
    pub async fn start(&self) -> Result<(), DomainError> {
        info!(
            "📦 ExecutionSagaConsumer: Starting consumer '{}'",
            self.config.consumer_name
        );

        // Create stream for execution events and get stream info
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
            "📦 ExecutionSagaConsumer: Started consuming from stream '{}'",
            stream_name
        );

        while let Some(message_result) = messages.next().await {
            match message_result {
                Ok(message) => {
                    if let Err(e) = self.process_message(&message.payload).await {
                        error!("📦 ExecutionSagaConsumer: Error processing message: {}", e);
                    }
                    // Ack the message
                    if let Err(e) = message.ack().await {
                        error!("📦 ExecutionSagaConsumer: Failed to ack message: {}", e);
                    }
                }
                Err(e) => {
                    error!("📦 ExecutionSagaConsumer: Message receive error: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Process a single NATS message payload
    async fn process_message(&self, payload: &[u8]) -> Result<(), DomainError> {
        // Parse the event from the message payload
        let event: DomainEvent =
            serde_json::from_slice(payload).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to deserialize event: {}", e),
            })?;

        match &event {
            DomainEvent::JobQueued { job_id, .. } => {
                info!(
                    "📦 ExecutionSagaConsumer: Received JobQueued event for job {}",
                    job_id
                );
                self.handle_job_queued(job_id).await?;
            }
            DomainEvent::WorkerReady { worker_id, .. } => {
                info!(
                    "📦 ExecutionSagaConsumer: Received WorkerReady event for worker {}",
                    worker_id
                );
                self.handle_worker_ready(worker_id).await?;
            }
            _ => {
                debug!(
                    "📦 ExecutionSagaConsumer: Ignoring event type '{}'",
                    event.event_type()
                );
            }
        }

        Ok(())
    }

    /// Handle JobQueued event - trigger execution saga
    async fn handle_job_queued(&self, job_id: &JobId) -> Result<(), DomainError> {
        // Fetch job details
        let job = self
            .job_repository
            .find_by_id(job_id)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to fetch job {}: {}", job_id, e),
            })?
            .ok_or_else(|| DomainError::InfrastructureError {
                message: format!("Job {} not found", job_id),
            })?;

        // Check if job is in correct state
        if *job.state() != JobState::Pending {
            debug!(
                "📦 ExecutionSagaConsumer: Job {} is not pending (state: {:?}), skipping",
                job_id,
                job.state()
            );
            return Ok(());
        }

        // Find available workers
        let available_workers = self.worker_registry.find_available().await.map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to find available workers: {}", e),
            }
        })?;

        if available_workers.is_empty() {
            info!(
                "📦 ExecutionSagaConsumer: No workers available for job {}, will retry",
                job_id
            );
            return Err(DomainError::InfrastructureError {
                message: "No workers available".to_string(),
            });
        }

        info!(
            "📦 ExecutionSagaConsumer: Found {} available workers for job {}",
            available_workers.len(),
            job_id
        );

        // Select best worker and trigger saga
        let worker = &available_workers[0];
        let worker_id = worker.id();

        self.trigger_execution_saga(job_id, worker_id).await
    }

    /// Handle WorkerReady event - dispatch pending job
    async fn handle_worker_ready(&self, worker_id: &WorkerId) -> Result<(), DomainError> {
        // Check if worker exists and is in correct state
        let _worker = self
            .worker_registry
            .get(worker_id)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to fetch worker {}: {}", worker_id, e),
            })?
            .ok_or_else(|| DomainError::InfrastructureError {
                message: format!("Worker {} not found", worker_id),
            })?;

        // Find pending jobs that can be dispatched to this worker
        let pending_jobs = self.job_repository.find_pending().await.map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to find pending jobs: {}", e),
            }
        })?;

        if pending_jobs.is_empty() {
            debug!(
                "📦 ExecutionSagaConsumer: No pending jobs for worker {}",
                worker_id
            );
            return Ok(());
        }

        // Dispatch first pending job
        let job = &pending_jobs[0];
        let job_id = &job.id;

        info!(
            "📦 ExecutionSagaConsumer: Dispatching job {} to worker {}",
            job_id, worker_id
        );

        self.trigger_execution_saga(job_id, worker_id).await
    }

    /// Trigger execution saga for a job-worker pair
    async fn trigger_execution_saga(
        &self,
        job_id: &JobId,
        worker_id: &WorkerId,
    ) -> Result<(), DomainError> {
        // Create saga context
        let saga_id = hodei_server_domain::saga::SagaId::new();
        let mut context = hodei_server_domain::saga::SagaContext::new(
            saga_id,
            SagaType::Execution,
            Some(format!("job-{}", job_id.0)),
            Some("execution_saga_consumer".to_string()),
        );

        context.set_metadata("job_id", &job_id.to_string()).ok();
        context
            .set_metadata("worker_id", &worker_id.to_string())
            .ok();

        // Create execution saga
        let saga = hodei_server_domain::saga::ExecutionSaga::new(job_id.clone());

        // Execute saga
        match self.orchestrator.execute_saga(&saga, context).await {
            Ok(result) => {
                if result.state == hodei_server_domain::saga::SagaState::Completed {
                    info!(
                        "✅ ExecutionSagaConsumer: Saga completed for job {} on worker {}",
                        job_id, worker_id
                    );
                    Ok(())
                } else {
                    let error_msg = result
                        .error_message
                        .clone()
                        .unwrap_or_else(|| "Unknown error".to_string());
                    error!(
                        "❌ ExecutionSagaConsumer: Saga failed for job {}: {}",
                        job_id, error_msg
                    );
                    Err(DomainError::SagaError { message: error_msg })
                }
            }
            Err(e) => {
                error!(
                    "❌ ExecutionSagaConsumer: Orchestrator error for job {}: {}",
                    job_id, e
                );
                Err(e)
            }
        }
    }

    /// Ensure the execution events stream exists
    async fn ensure_stream(&self) -> Result<async_nats::jetstream::stream::Stream, DomainError> {
        let stream_name = format!("{}_EXECUTION_EVENTS", self.config.stream_prefix);

        match self.jetstream.get_stream(&stream_name).await {
            Ok(stream) => {
                debug!(
                    "📦 ExecutionSagaConsumer: Stream '{}' already exists",
                    stream_name
                );
                Ok(stream)
            }
            Err(_) => {
                let stream = self
                    .jetstream
                    .create_stream(StreamConfig {
                        name: stream_name.clone(),
                        subjects: vec![
                            self.config.job_queued_topic.clone(),
                            self.config.worker_ready_topic.clone(),
                        ],
                        retention: RetentionPolicy::WorkQueue,
                        max_messages: 50000,
                        max_bytes: 50 * 1024 * 1024, // 50MB
                        ..Default::default()
                    })
                    .await
                    .map_err(|e| DomainError::InfrastructureError {
                        message: format!("Failed to create stream {}: {}", stream_name, e),
                    })?;

                info!("📦 ExecutionSagaConsumer: Created stream '{}'", stream_name);
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
                    "📦 ExecutionSagaConsumer: Consumer '{}' already exists",
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
                    "📦 ExecutionSagaConsumer: Created consumer '{}'",
                    consumer_id
                );
                Ok(consumer)
            }
        }
    }

    /// Create or get consumer for the stream (legacy method)
    #[allow(dead_code)]
    async fn create_or_get_consumer(
        &self,
        stream: &async_nats::jetstream::stream::Stream,
    ) -> Result<async_nats::jetstream::consumer::PullConsumer, DomainError> {
        let consumer_id = format!(
            "{}-{}",
            self.config.consumer_name, self.config.consumer_group
        );

        // Try to get existing consumer
        match stream.get_consumer(&consumer_id).await {
            Ok(consumer) => {
                debug!(
                    "📦 ExecutionSagaConsumer: Consumer '{}' already exists",
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
                    "📦 ExecutionSagaConsumer: Created consumer '{}'",
                    consumer_id
                );
                Ok(consumer)
            }
        }
    }

    /// Stop the consumer gracefully
    pub async fn stop(&self) {
        let _ = self.shutdown_tx.send(()).await;
        info!("📦 ExecutionSagaConsumer: Stop signal sent");
    }
}

/// Builder for ExecutionSagaConsumer
pub struct ExecutionSagaConsumerBuilder {
    client: Option<Client>,
    jetstream: Option<JetStreamContext>,
    orchestrator: Option<Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>>,
    job_repository: Option<Arc<dyn JobRepository + Send + Sync>>,
    worker_registry: Option<Arc<dyn WorkerRegistry + Send + Sync>>,
    config: Option<ExecutionSagaConsumerConfig>,
}

impl Default for ExecutionSagaConsumerBuilder {
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

impl fmt::Debug for ExecutionSagaConsumerBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecutionSagaConsumerBuilder")
            .field("client", &self.client.is_some())
            .field("jetstream", &self.jetstream.is_some())
            .field("orchestrator", &self.orchestrator.is_some())
            .field("job_repository", &self.job_repository.is_some())
            .field("worker_registry", &self.worker_registry.is_some())
            .field("config", &self.config)
            .finish()
    }
}

impl ExecutionSagaConsumerBuilder {
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

    pub fn with_orchestrator(
        mut self,
        orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,
    ) -> Self {
        self.orchestrator = Some(orchestrator);
        self
    }

    pub fn with_job_repository(
        mut self,
        job_repository: Arc<dyn JobRepository + Send + Sync>,
    ) -> Self {
        self.job_repository = Some(job_repository);
        self
    }

    pub fn with_worker_registry(
        mut self,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    ) -> Self {
        self.worker_registry = Some(worker_registry);
        self
    }

    pub fn with_config(mut self, config: ExecutionSagaConsumerConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(self) -> anyhow::Result<ExecutionSagaConsumer> {
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

        Ok(ExecutionSagaConsumer::new(
            client,
            jetstream,
            orchestrator,
            job_repository,
            worker_registry,
            self.config,
        ))
    }
}

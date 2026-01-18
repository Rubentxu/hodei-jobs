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
//! - Circuit breaker integration for resilience (EPIC-85 US-05)

use async_nats::Client;
use async_nats::jetstream::Context as JetStreamContext;
use async_nats::jetstream::consumer::pull::Config as PullConsumerConfig;
use async_nats::jetstream::consumer::{AckPolicy, DeliverPolicy};
use futures::StreamExt;
use hodei_server_domain::command::DynCommandBus;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::saga::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitState,
};
use hodei_server_domain::saga::{SagaOrchestrator, SagaServices, SagaType};
use hodei_server_domain::shared_kernel::{DomainError, JobId, JobState, WorkerId};
use hodei_server_domain::workers::WorkerRegistry;
use serde::Deserialize;
use std::fmt;
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

    /// EPIC-85 US-05: Circuit breaker configuration
    pub circuit_breaker_failure_threshold: u64,
    pub circuit_breaker_open_duration: Duration,
    pub circuit_breaker_success_threshold: u64,
    pub circuit_breaker_call_timeout: Duration,
}

impl Default for ExecutionSagaConsumerConfig {
    fn default() -> Self {
        Self {
            consumer_name: "execution-saga-consumer".to_string(),
            stream_prefix: "HODEI".to_string(),
            job_queued_topic: job_topics::QUEUED.to_string(),
            worker_ready_topic: worker_topics::READY.to_string(),
            consumer_group: "execution-dispatchers".to_string(),
            concurrency: 10,
            ack_wait: Duration::from_secs(30),
            max_deliver: 3,
            enable_auto_dispatch: true,
            saga_timeout: Duration::from_secs(60),
            // EPIC-85 US-05: Circuit breaker defaults
            circuit_breaker_failure_threshold: 5,
            circuit_breaker_open_duration: Duration::from_secs(30),
            circuit_breaker_success_threshold: 2,
            circuit_breaker_call_timeout: Duration::from_secs(60),
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
/// Includes circuit breaker integration for resilience (EPIC-85 US-05).
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

    /// Event bus for saga services (GAP-006 FIX)
    event_bus: Arc<dyn EventBus + Send + Sync>,

    /// Command bus for saga command dispatch (GAP-006 FIX)
    command_bus: DynCommandBus,

    /// Consumer configuration
    config: ExecutionSagaConsumerConfig,

    /// EPIC-85 US-05: Circuit breaker for resilience
    circuit_breaker: Option<Arc<CircuitBreaker>>,

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
        event_bus: Arc<dyn EventBus + Send + Sync>,
        command_bus: DynCommandBus,
        config: Option<ExecutionSagaConsumerConfig>,
    ) -> Self {
        let config = config.unwrap_or_default();
        let (shutdown_tx, _) = mpsc::channel(1);

        // EPIC-85 US-05: Initialize circuit breaker
        let circuit_breaker = Arc::new(CircuitBreaker::new(
            format!("execution-saga-{}", config.consumer_name),
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
            event_bus,
            command_bus,
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

        // EPIC-85 US-05: Get circuit breaker reference for the loop
        let circuit_breaker = self.circuit_breaker.clone();

        while let Some(message_result) = messages.next().await {
            match message_result {
                Ok(message) => {
                    // EPIC-85 US-05: Check circuit breaker before processing
                    let can_process = circuit_breaker
                        .as_ref()
                        .map(|cb| cb.allow_request())
                        .unwrap_or(true);

                    if can_process {
                        let payload = message.payload.clone();
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
                            None => self.process_message(&payload).await,
                        };

                        match result {
                            Ok(()) => {
                                // EPIC-85 US-05: Record success
                                if let Some(ref cb) = circuit_breaker {
                                    cb.record_success();
                                }
                            }
                            Err(e) => {
                                error!("📦 ExecutionSagaConsumer: Error processing message: {}", e);
                                // EPIC-85 US-05: Record failure
                                if let Some(ref cb) = circuit_breaker {
                                    cb.record_failure();
                                }
                            }
                        }
                    } else {
                        // Circuit is open - log warning
                        warn!("📦 ExecutionSagaConsumer: Circuit breaker open, skipping message");
                        // EPIC-85 US-05: Record failure for circuit being open
                        if let Some(ref cb) = circuit_breaker {
                            cb.record_failure();
                        }
                    }

                    // Ack the message (we always ack to avoid redelivery)
                    if let Err(e) = message.ack().await {
                        error!("📦 ExecutionSagaConsumer: Failed to ack message: {}", e);
                    }
                }
                Err(e) => {
                    // EPIC-85 US-05: Record failure in circuit breaker
                    if let Some(ref cb) = circuit_breaker {
                        cb.record_failure();
                    }
                    error!("📦 ExecutionSagaConsumer: Message receive error: {}", e);
                }
            }
        }

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
            DomainEvent::JobAssigned {
                job_id, worker_id, ..
            } => {
                info!(
                    "📦 ExecutionSagaConsumer: Received JobAssigned event for job {} on worker {}",
                    job_id, worker_id
                );
                self.handle_job_assigned(job_id, worker_id).await?;
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
    #[instrument(skip(self), fields(job_id = %job_id))]
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
    #[instrument(skip(self), fields(worker_id = %worker_id))]
    async fn handle_worker_ready(&self, worker_id: &WorkerId) -> Result<(), DomainError> {
        // Step 1: Check if worker exists and if it has a pre-assigned job
        let worker = self
            .worker_registry
            .get(worker_id)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to fetch worker {}: {}", worker_id, e),
            })?
            .ok_or_else(|| DomainError::InfrastructureError {
                message: format!("Worker {} not found", worker_id),
            })?;

        // Step 2: Determine which job to dispatch
        let job_id = if let Some(intended_job_id) = worker.current_job_id() {
            info!(
                worker_id = %worker_id,
                job_id = %intended_job_id,
                "📦 ExecutionSagaConsumer: Worker has pre-assigned job, prioritizing it"
            );
            intended_job_id.clone()
        } else {
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
            pending_jobs[0].id.clone()
        };

        info!(
            "📦 ExecutionSagaConsumer: Dispatching job {} to worker {}",
            job_id, worker_id
        );

        self.trigger_execution_saga(&job_id, worker_id).await
    }

    /// Handle JobAssigned event - trigger execution saga with pre-assigned worker
    ///
    /// This is the main entry point for the saga when using the path:
    /// JobQueued → Provisioning → WorkerReady → JobAssignmentService.assign_job() → JobAssigned
    ///
    /// The saga will:
    /// 1. Validate the job is in ASSIGNED state
    /// 2. Execute the job (send RUN_JOB via CommandBus → ExecuteJobHandler → JobExecutorImpl)
    /// 3. Complete the job when worker reports result
    #[instrument(skip(self), fields(job_id = %job_id, worker_id = %worker_id))]
    async fn handle_job_assigned(
        &self,
        job_id: &JobId,
        worker_id: &WorkerId,
    ) -> Result<(), DomainError> {
        // Fetch job to verify it's in ASSIGNED state
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

        // Check if job is in correct state for execution
        if *job.state() != JobState::Assigned {
            debug!(
                "📦 ExecutionSagaConsumer: Job {} is not in ASSIGNED state (state: {:?}), skipping",
                job_id,
                job.state()
            );
            return Ok(());
        }

        // Verify worker exists
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

        info!(
            "📦 ExecutionSagaConsumer: Triggering execution saga for job {} on worker {}",
            job_id, worker_id
        );

        // Trigger execution saga with pre-assigned worker
        self.trigger_execution_saga(job_id, worker_id).await
    }

    /// Trigger execution saga for a job-worker pair
    ///
    /// Uses ExecutionSaga::with_worker when worker_id is provided,
    /// which is the case for JobAssigned events where the worker was pre-assigned.
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

        // GAP-006 FIX: Create SagaServices with command_bus and inject into context
        // This enables saga steps to dispatch commands via CommandBus
        let saga_services = Arc::new(SagaServices::with_command_bus(
            self.worker_registry.clone(),
            self.event_bus.clone(),
            Some(self.job_repository.clone()),
            None, // provisioning_service - not needed for execution sagas
            None, // orchestrator - not needed for execution sagas
            self.command_bus.clone(),
        ));

        // Inject services into context (critical for saga steps to access CommandBus)
        let context = context.with_services(saga_services);

        info!(
            saga_id = %context.saga_id,
            job_id = %job_id,
            worker_id = %worker_id,
            "🚀 ExecutionSagaConsumer: Triggering saga with SagaServices (CommandBus injected)"
        );

        // Create execution saga with pre-assigned worker
        // This ensures the saga uses the correct worker for execution
        let saga = hodei_server_domain::saga::ExecutionSaga::with_worker(
            job_id.clone(),
            worker_id.clone(),
        );

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
    /// Uses the shared HODEI_EVENTS stream instead of creating a separate one
    async fn ensure_stream(&self) -> Result<async_nats::jetstream::stream::Stream, DomainError> {
        let stream_name = format!("{}_EVENTS", self.config.stream_prefix);

        match self.jetstream.get_stream(&stream_name).await {
            Ok(stream) => {
                debug!(
                    "📦 ExecutionSagaConsumer: Using shared stream '{}'",
                    stream_name
                );
                Ok(stream)
            }
            Err(_) => {
                // Stream doesn't exist - this is an error condition
                // The main server should have created HODEI_EVENTS
                Err(DomainError::InfrastructureError {
                    message: format!(
                        "Stream '{}' does not exist. Ensure the server is running and has created the events stream.",
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
        // Consumer ID includes filter_subject to ensure uniqueness for WorkQueue streams
        let filter_suffix = self.config.job_queued_topic.replace('.', "-");
        let consumer_id = format!(
            "{}-{}-{}",
            self.config.consumer_name, self.config.consumer_group, filter_suffix
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
                // Consumer doesn't exist, create it with filter_subject
                // Consumer ID already includes filter_subject for uniqueness
                let consumer_config = PullConsumerConfig {
                    durable_name: Some(consumer_id.clone()),
                    deliver_policy: DeliverPolicy::All,
                    ack_policy: AckPolicy::Explicit,
                    ack_wait: self.config.ack_wait,
                    max_deliver: self.config.max_deliver,
                    max_ack_pending: self.config.concurrency as i64,
                    // Subscribe to job_queued_topic for triggering execution sagas
                    filter_subject: self.config.job_queued_topic.clone(),
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
    event_bus: Option<Arc<dyn EventBus + Send + Sync>>,
    command_bus: Option<DynCommandBus>,
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
            event_bus: None,
            command_bus: None,
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
            .field("event_bus", &self.event_bus.is_some())
            .field("command_bus", &self.command_bus.is_some())
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

    pub fn with_event_bus(mut self, event_bus: Arc<dyn EventBus + Send + Sync>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub fn with_command_bus(mut self, command_bus: DynCommandBus) -> Self {
        self.command_bus = Some(command_bus);
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
        let event_bus = self
            .event_bus
            .ok_or_else(|| anyhow::anyhow!("event_bus is required"))?;
        let command_bus = self
            .command_bus
            .ok_or_else(|| anyhow::anyhow!("command_bus is required"))?;

        Ok(ExecutionSagaConsumer::new(
            client,
            jetstream,
            orchestrator,
            job_repository,
            worker_registry,
            event_bus,
            command_bus,
            self.config,
        ))
    }
}

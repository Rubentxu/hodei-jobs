//! Worker Ephemeral Terminating Consumer
//!
//! This consumer handles the termination of ephemeral workers after job completion.
//! It's critical for the "Crash-Only" model where each job has its own ephemeral worker.
//!
//! # Responsibilities
//! 1. Receive WorkerEphemeralTerminating events
//! 2. Trigger infrastructure cleanup via CleanupSaga
//! 3. Publish WorkerEphemeralTerminated event after cleanup
//! 4. Update worker registry state to Terminated
//!
//! # Event Flow
//! Job completes → WorkerEphemeralTerminating → This Consumer → CleanupSaga → Infrastructure Destroyed

use async_nats::Client;
use async_nats::jetstream::Context as JetStreamContext;
use async_nats::jetstream::consumer::pull::Config as PullConsumerConfig;
use async_nats::jetstream::consumer::{AckPolicy, DeliverPolicy};
use async_nats::jetstream::stream::{Config as StreamConfig, RetentionPolicy};
use futures::StreamExt;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::outbox::OutboxEventInsert;
use hodei_server_domain::saga::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitState,
};
use hodei_server_domain::saga::{SagaOrchestrator, SagaType};
use hodei_server_domain::shared_kernel::{DomainError, JobId, ProviderId, WorkerId};
use hodei_server_domain::workers::WorkerRegistry;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use hodei_shared::event_topics::worker_topics;

// Constants for saga metadata to avoid lifetime issues
const METADATA_CLEANUP_TYPE_EPHEMERAL: &str = "ephemeral_worker";

/// Message envelope for NATS transport
#[derive(Debug, Clone, Deserialize)]
pub struct NatsMessageEnvelope {
    pub payload: String,
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub correlation_id: Option<String>,
}

/// Configuration for WorkerEphemeralTerminatingConsumer
#[derive(Debug, Clone)]
pub struct WorkerEphemeralTerminatingConsumerConfig {
    /// Consumer name identifier
    pub consumer_name: String,

    /// Stream name prefix
    pub stream_prefix: String,

    /// Topic for WorkerEphemeralTerminating events
    pub terminating_topic: String,

    /// Consumer group for load balancing
    pub consumer_group: String,

    /// Maximum concurrent cleanups
    pub concurrency: usize,

    /// Ack wait timeout
    pub ack_wait: Duration,

    /// Maximum delivery attempts
    pub max_deliver: i64,

    /// Default saga timeout
    pub saga_timeout: Duration,

    /// EPIC-85 US-05: Circuit breaker configuration
    pub circuit_breaker_failure_threshold: u64,
    pub circuit_breaker_open_duration: Duration,
    pub circuit_breaker_success_threshold: u64,
    pub circuit_breaker_call_timeout: Duration,
}

impl Default for WorkerEphemeralTerminatingConsumerConfig {
    fn default() -> Self {
        Self {
            consumer_name: "worker-ephemeral-terminating-consumer".to_string(),
            stream_prefix: "HODEI".to_string(),
            terminating_topic: worker_topics::EPHEMERAL_TERMINATING.to_string(),
            consumer_group: "worker-cleanup".to_string(),
            concurrency: 10,
            ack_wait: Duration::from_secs(60),
            max_deliver: 3,
            saga_timeout: Duration::from_secs(120),
            // EPIC-85 US-05: Circuit breaker defaults
            circuit_breaker_failure_threshold: 5,
            circuit_breaker_open_duration: Duration::from_secs(30),
            circuit_breaker_success_threshold: 2,
            circuit_breaker_call_timeout: Duration::from_secs(10),
        }
    }
}

/// Result of worker ephemeral termination processing
#[derive(Debug)]
pub enum WorkerEphemeralTerminatingResult {
    /// Cleanup triggered successfully
    CleanupTriggered,
    /// Worker not found in registry
    WorkerNotFound,
    /// Cleanup already in progress
    AlreadyCleaning,
    /// Failed to process
    Failed(String),
}

/// Worker Ephemeral Terminating Consumer
///
/// Consumes WorkerEphemeralTerminating events and triggers cleanup sagas
/// for terminating ephemeral workers after job completion.
#[derive(Clone)]
pub struct WorkerEphemeralTerminatingConsumer {
    /// NATS client
    _client: Client,

    /// NATS JetStream context
    jetstream: JetStreamContext,

    /// Saga orchestrator (trait object for dynamic dispatch)
    orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,

    /// Job repository for fetching job details
    job_repository: Arc<dyn JobRepository + Send + Sync>,

    /// Worker registry for state updates
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,

    /// Outbox repository for publishing termination events
    outbox_repository: Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync>,

    /// Consumer configuration
    config: WorkerEphemeralTerminatingConsumerConfig,

    /// EPIC-85 US-05: Circuit breaker for resilience
    circuit_breaker: Option<Arc<CircuitBreaker>>,

    /// Shutdown signal
    shutdown_tx: mpsc::Sender<()>,
}

impl std::fmt::Debug for WorkerEphemeralTerminatingConsumer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerEphemeralTerminatingConsumer")
            .field("config", &self.config)
            .field("shutdown_tx", &self.shutdown_tx)
            .finish_non_exhaustive()
    }
}

impl WorkerEphemeralTerminatingConsumer {
    /// Create a new WorkerEphemeralTerminatingConsumer
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: Client,
        jetstream: JetStreamContext,
        orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,
        job_repository: Arc<dyn JobRepository + Send + Sync>,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
        outbox_repository: Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync>,
        config: Option<WorkerEphemeralTerminatingConsumerConfig>,
    ) -> Self {
        let config = config.unwrap_or_default();
        let (shutdown_tx, _) = mpsc::channel(1);

        // EPIC-85 US-05: Initialize circuit breaker
        let circuit_breaker = Arc::new(CircuitBreaker::new(
            format!("worker-terminating-{}", config.consumer_name),
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
            outbox_repository,
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

    /// Get the stream name for this consumer
    fn stream_name(&self) -> String {
        format!("{}_WORKER_EVENTS", self.config.stream_prefix)
    }

    /// Ensure the stream exists with correct configuration
    async fn ensure_stream(&self) -> Result<(), DomainError> {
        let stream_name = self.stream_name();

        match self.jetstream.get_stream(&stream_name).await {
            Ok(_) => {
                debug!(
                    "🧹 WorkerEphemeralTerminatingConsumer: Stream {} already exists",
                    stream_name
                );
                Ok(())
            }
            Err(_) => {
                info!(
                    "🧹 WorkerEphemeralTerminatingConsumer: Creating stream {}",
                    stream_name
                );

                let _stream = self
                    .jetstream
                    .create_stream(StreamConfig {
                        name: stream_name.clone(),
                        subjects: vec![
                            worker_topics::EPHEMERAL_TERMINATING.to_string(),
                            worker_topics::EPHEMERAL_TERMINATED.to_string(),
                            worker_topics::TERMINATED.to_string(),
                        ],
                        retention: RetentionPolicy::WorkQueue,
                        max_messages: 10000,
                        max_bytes: 1024 * 1024 * 100, // 100MB
                        ..Default::default()
                    })
                    .await
                    .map_err(|e| DomainError::InfrastructureError {
                        message: format!("Failed to create stream {}: {}", stream_name, e),
                    })?;

                info!(
                    "🧹 WorkerEphemeralTerminatingConsumer: Stream {} created",
                    stream_name
                );
                Ok(())
            }
        }
    }

    /// Create or get consumer by name
    async fn create_or_get_consumer_by_name(
        &self,
        stream_name: &str,
    ) -> Result<async_nats::jetstream::consumer::PullConsumer, DomainError> {
        let consumer_name = format!(
            "{}-{}",
            self.config.consumer_name,
            Uuid::new_v4().to_string()[..8].to_string()
        );

        // Get stream to interact with consumer
        let stream = self.jetstream.get_stream(stream_name).await.map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to get stream {}: {}", stream_name, e),
            }
        })?;

        // Try to get existing consumer
        match stream
            .get_consumer::<async_nats::jetstream::consumer::pull::Config>(&consumer_name)
            .await
        {
            Ok(consumer) => {
                debug!(
                    "🧹 WorkerEphemeralTerminatingConsumer: Consumer {} already exists",
                    consumer_name
                );
                Ok(consumer)
            }
            Err(_) => {
                info!(
                    "🧹 WorkerEphemeralTerminatingConsumer: Creating consumer {}",
                    consumer_name
                );

                let consumer = self
                    .jetstream
                    .create_consumer_on_stream(
                        PullConsumerConfig {
                            name: Some(consumer_name.clone()),
                            durable_name: Some(consumer_name.clone()),
                            description: Some("Worker ephemeral terminating consumer".to_string()),
                            ack_policy: AckPolicy::Explicit,
                            deliver_policy: DeliverPolicy::All,
                            ack_wait: self.config.ack_wait,
                            max_deliver: self.config.max_deliver,
                            filter_subject: self.config.terminating_topic.clone(),
                            ..Default::default()
                        },
                        stream_name,
                    )
                    .await
                    .map_err(|e| DomainError::InfrastructureError {
                        message: format!("Failed to create consumer {}: {}", consumer_name, e),
                    })?;

                info!(
                    "🧹 WorkerEphemeralTerminatingConsumer: Consumer {} created",
                    consumer_name
                );
                Ok(consumer)
            }
        }
    }

    /// Start the consumer and begin processing events
    pub async fn start(&self) -> Result<(), DomainError> {
        info!(
            "🧹 WorkerEphemeralTerminatingConsumer: Starting consumer '{}'",
            self.config.consumer_name
        );

        // Create stream for worker events and get stream info
        let stream_name = {
            let _ = self.ensure_stream().await?;
            self.stream_name()
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
            "🧹 WorkerEphemeralTerminatingConsumer: Started consuming from stream '{}'",
            stream_name
        );

        while let Some(message_result) = messages.next().await {
            match message_result {
                Ok(message) => {
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
                                error!(
                                    "🧹 WorkerEphemeralTerminatingConsumer: Error processing message: {}",
                                    e
                                );
                                // EPIC-85 US-05: Record failure
                                if let Some(ref cb) = circuit_breaker {
                                    cb.record_failure();
                                }
                            }
                        }
                    } else {
                        // Circuit is open - log warning
                        warn!(
                            "🧹 WorkerEphemeralTerminatingConsumer: Circuit breaker open, skipping message"
                        );
                        // EPIC-85 US-05: Record failure for circuit being open
                        if let Some(ref cb) = self.circuit_breaker {
                            cb.record_failure();
                        }
                    }

                    // Ack the message
                    if let Err(e) = message.ack().await {
                        error!(
                            "🧹 WorkerEphemeralTerminatingConsumer: Failed to ack message: {}",
                            e
                        );
                    }
                }
                Err(e) => {
                    error!(
                        "🧹 WorkerEphemeralTerminatingConsumer: Message receive error: {}",
                        e
                    );
                    // EPIC-85 US-05: Record failure in circuit breaker
                    if let Some(ref cb) = self.circuit_breaker {
                        cb.record_failure();
                    }
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
            DomainEvent::WorkerEphemeralTerminating {
                worker_id,
                provider_id,
                reason,
                ..
            } => {
                info!(
                    "🧹 WorkerEphemeralTerminatingConsumer: Received termination event for worker {} (reason: {:?})",
                    worker_id, reason
                );
                self.handle_termination(worker_id, provider_id, None)
                    .await?;
            }
            _ => {
                debug!(
                    "🧹 WorkerEphemeralTerminatingConsumer: Ignoring event type '{}'",
                    event.event_type()
                );
            }
        }

        Ok(())
    }

    /// Handle WorkerEphemeralTerminating event - trigger cleanup
    async fn handle_termination(
        &self,
        worker_id: &WorkerId,
        provider_id: &ProviderId,
        job_id: Option<JobId>,
    ) -> Result<WorkerEphemeralTerminatingResult, DomainError> {
        // Check if worker exists in registry
        let worker = self.worker_registry.get(worker_id).await.map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to fetch worker {}: {}", worker_id, e),
            }
        })?;

        if worker.is_none() {
            debug!(
                "🧹 WorkerEphemeralTerminatingConsumer: Worker {} not found, skipping cleanup",
                worker_id
            );
            return Ok(WorkerEphemeralTerminatingResult::WorkerNotFound);
        }

        // Check if already being cleaned up
        let worker = worker.unwrap();
        if matches!(
            worker.state(),
            hodei_server_domain::shared_kernel::WorkerState::Terminated
        ) {
            debug!(
                "🧹 WorkerEphemeralTerminatingConsumer: Worker {} already terminated, skipping",
                worker_id
            );
            return Ok(WorkerEphemeralTerminatingResult::AlreadyCleaning);
        }

        info!(
            "🧹 WorkerEphemeralTerminatingConsumer: Triggering cleanup for worker {}",
            worker_id
        );

        // Trigger cleanup saga
        self.trigger_cleanup_saga(worker_id, provider_id, job_id)
            .await?;

        // Publish WorkerEphemeralTerminated event via outbox
        self.publish_termination_event(worker_id, provider_id)
            .await?;

        Ok(WorkerEphemeralTerminatingResult::CleanupTriggered)
    }

    /// Trigger cleanup saga for worker termination
    async fn trigger_cleanup_saga(
        &self,
        worker_id: &WorkerId,
        provider_id: &ProviderId,
        job_id: Option<JobId>,
    ) -> Result<(), DomainError> {
        // Create saga context
        let saga_id = hodei_server_domain::saga::SagaId::new();
        let worker_id_str = worker_id.to_string();
        let provider_id_str = provider_id.to_string();
        let job_id_str = job_id.as_ref().map(|j| j.to_string());
        let mut context = hodei_server_domain::saga::SagaContext::new(
            saga_id,
            SagaType::Recovery,
            Some(format!("ephemeral-cleanup-{}", worker_id.0)),
            Some("worker_ephemeral_terminating_consumer".to_string()),
        );

        context.set_metadata("worker_id", &worker_id_str).ok();
        context.set_metadata("provider_id", &provider_id_str).ok();
        if let Some(ref jid_str) = job_id_str {
            context.set_metadata("job_id", jid_str).ok();
        }
        let cleanup_type_val = "ephemeral_worker";
        context
            .set_metadata(METADATA_CLEANUP_TYPE_EPHEMERAL, &cleanup_type_val)
            .ok();

        // Create recovery saga for cleanup (using RecoverySaga for cleanup)
        let dummy_failed_worker_id = WorkerId::new();
        let saga = hodei_server_domain::saga::RecoverySaga::new(
            job_id.unwrap_or_else(JobId::new),
            dummy_failed_worker_id,
            Some(worker_id_str),
        );

        // Execute saga with timeout
        let saga_result = tokio::time::timeout(
            self.config.saga_timeout,
            self.orchestrator.execute_saga(&saga, context),
        )
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Saga timeout for worker {} cleanup: {}", worker_id, e),
        })?
        .map_err(|e| DomainError::InfrastructureError {
            message: format!(
                "Saga execution failed for worker {} cleanup: {}",
                worker_id, e
            ),
        })?;

        if saga_result.is_success() {
            info!(
                "🧹 WorkerEphemeralTerminatingConsumer: Cleanup saga completed for worker {}",
                worker_id
            );
        } else {
            let error_msg = saga_result
                .error_message
                .unwrap_or_else(|| "Unknown error".to_string());
            warn!(
                "🧹 WorkerEphemeralTerminatingConsumer: Cleanup saga failed for worker {}: {}",
                worker_id, error_msg
            );
        }

        Ok(())
    }

    /// Publish WorkerEphemeralTerminated event via outbox
    async fn publish_termination_event(
        &self,
        worker_id: &WorkerId,
        provider_id: &ProviderId,
    ) -> Result<(), DomainError> {
        let terminated_event = OutboxEventInsert::for_worker(
            worker_id.0,
            "WorkerEphemeralTerminated".to_string(),
            serde_json::json!({
                "worker_id": worker_id.0.to_string(),
                "provider_id": provider_id.0.to_string(),
                "cleanup_scheduled": true,
                "occurred_at": chrono::Utc::now().to_rfc3339()
            }),
            Some(serde_json::json!({
                "source": "WorkerEphemeralTerminatingConsumer",
                "cleanup_type": "ephemeral_worker"
            })),
            Some(format!("ephemeral-terminated-{}", worker_id.0)),
        );

        self.outbox_repository
            .insert_events(&[terminated_event])
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!(
                    "Failed to insert WorkerEphemeralTerminated event for worker {}: {}",
                    worker_id, e
                ),
            })?;

        info!(
            "🧹 WorkerEphemeralTerminatingConsumer: Published WorkerEphemeralTerminated event for worker {}",
            worker_id
        );

        Ok(())
    }
}

/// Builder for WorkerEphemeralTerminatingConsumer
#[derive(Default)]
pub struct WorkerEphemeralTerminatingConsumerBuilder {
    config: Option<WorkerEphemeralTerminatingConsumerConfig>,
    client: Option<Client>,
    jetstream: Option<JetStreamContext>,
    orchestrator: Option<Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>>,
    job_repository: Option<Arc<dyn JobRepository + Send + Sync>>,
    worker_registry: Option<Arc<dyn WorkerRegistry + Send + Sync>>,
    outbox_repository: Option<Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync>>,
}

impl std::fmt::Debug for WorkerEphemeralTerminatingConsumerBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerEphemeralTerminatingConsumerBuilder")
            .field("config", &self.config)
            .field("client", &self.client.is_some())
            .field("jetstream", &self.jetstream.is_some())
            .field("orchestrator", &self.orchestrator.is_some())
            .field("job_repository", &self.job_repository.is_some())
            .field("worker_registry", &self.worker_registry.is_some())
            .field("outbox_repository", &self.outbox_repository.is_some())
            .finish()
    }
}

impl WorkerEphemeralTerminatingConsumerBuilder {
    /// Create a new builder
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set configuration
    #[inline]
    pub fn with_config(mut self, config: WorkerEphemeralTerminatingConsumerConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Set NATS client
    #[inline]
    pub fn with_client(mut self, client: &Client) -> Self {
        self.client = Some(client.clone());
        self
    }

    /// Set JetStream context
    #[inline]
    pub fn with_jetstream(mut self, jetstream: &JetStreamContext) -> Self {
        self.jetstream = Some(jetstream.clone());
        self
    }

    /// Set saga orchestrator
    #[inline]
    pub fn with_orchestrator(
        mut self,
        orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,
    ) -> Self {
        self.orchestrator = Some(orchestrator);
        self
    }

    /// Set job repository
    #[inline]
    pub fn with_job_repository(
        mut self,
        job_repository: Arc<dyn JobRepository + Send + Sync>,
    ) -> Self {
        self.job_repository = Some(job_repository);
        self
    }

    /// Set worker registry
    #[inline]
    pub fn with_worker_registry(
        mut self,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    ) -> Self {
        self.worker_registry = Some(worker_registry);
        self
    }

    /// Set outbox repository
    #[inline]
    pub fn with_outbox_repository(
        mut self,
        outbox_repository: Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync>,
    ) -> Self {
        self.outbox_repository = Some(outbox_repository);
        self
    }

    /// Build the consumer
    pub fn build(self) -> Result<WorkerEphemeralTerminatingConsumer, String> {
        let client = self.client.ok_or("NATS client is required")?;
        let jetstream = self.jetstream.ok_or("JetStream context is required")?;
        let orchestrator = self.orchestrator.ok_or("Saga orchestrator is required")?;
        let job_repository = self.job_repository.ok_or("Job repository is required")?;
        let worker_registry = self.worker_registry.ok_or("Worker registry is required")?;
        let outbox_repository = self
            .outbox_repository
            .ok_or("Outbox repository is required")?;

        Ok(WorkerEphemeralTerminatingConsumer::new(
            client,
            jetstream,
            orchestrator,
            job_repository,
            worker_registry,
            outbox_repository,
            self.config,
        ))
    }
}

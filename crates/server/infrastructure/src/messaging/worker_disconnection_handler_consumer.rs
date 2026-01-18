//! Worker Disconnection Handler Consumer
//!
//! This consumer handles WorkerDisconnected events and manages the response
//! to unexpected worker disconnections. It monitors system health and detects
//! worker or network failures.
//!
//! # Responsibilities
//! 1. Receive WorkerDisconnected events
//! 2. Evaluate heartbeat status and active jobs
//! 3. Decide timeout action (short vs long)
//! 4. Trigger appropriate saga (TimeoutSaga, RecoverySaga)
//! 5. Handle provider health changes if needed

use async_nats::Client;
use async_nats::jetstream::Context as JetStreamContext;
use async_nats::jetstream::consumer::pull::Config as PullConsumerConfig;
use async_nats::jetstream::consumer::{AckPolicy, DeliverPolicy};
use async_nats::jetstream::stream::{Config as StreamConfig, RetentionPolicy};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::outbox::OutboxEventInsert;
use hodei_server_domain::saga::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitState,
};
use hodei_server_domain::saga::{SagaOrchestrator, SagaType};
use hodei_server_domain::shared_kernel::{DomainError, JobId, WorkerId, WorkerState};
use hodei_server_domain::workers::WorkerRegistry;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use hodei_shared::event_topics::worker_topics;

// Constants for saga metadata to avoid lifetime issues
const METADATA_TIMEOUT_TYPE: &str = "worker_disconnection";

/// Message envelope for NATS transport
#[derive(Debug, Clone, Deserialize)]
pub struct NatsMessageEnvelope {
    pub payload: String,
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub correlation_id: Option<String>,
}

/// Configuration for WorkerDisconnectionHandlerConsumer
#[derive(Debug, Clone)]
pub struct WorkerDisconnectionHandlerConsumerConfig {
    /// Consumer name identifier
    pub consumer_name: String,

    /// Stream name prefix
    pub stream_prefix: String,

    /// Topic for WorkerDisconnected events
    pub disconnected_topic: String,

    /// Consumer group for load balancing
    pub consumer_group: String,

    /// Maximum concurrent recoveries
    pub concurrency: usize,

    /// Ack wait timeout
    pub ack_wait: StdDuration,

    /// Maximum delivery attempts
    pub max_deliver: i64,

    /// Short timeout threshold (seconds) - quick reconnection attempt
    pub short_timeout_secs: u64,

    /// Long timeout threshold (seconds) - force cleanup
    pub long_timeout_secs: u64,

    /// Default saga timeout
    pub saga_timeout: StdDuration,

    /// EPIC-85 US-05: Circuit breaker configuration
    pub circuit_breaker_failure_threshold: u64,
    pub circuit_breaker_open_duration: StdDuration,
    pub circuit_breaker_success_threshold: u64,
    pub circuit_breaker_call_timeout: StdDuration,
}

impl Default for WorkerDisconnectionHandlerConsumerConfig {
    fn default() -> Self {
        Self {
            consumer_name: "worker-disconnection-handler-consumer".to_string(),
            stream_prefix: "HODEI".to_string(),
            disconnected_topic: worker_topics::DISCONNECTED.to_string(),
            consumer_group: "worker-health".to_string(),
            concurrency: 10,
            ack_wait: StdDuration::from_secs(60),
            max_deliver: 3,
            short_timeout_secs: 30, // Quick timeout for monitoring
            long_timeout_secs: 120, // Long timeout for cleanup decision
            saga_timeout: StdDuration::from_secs(180),
            // EPIC-85 US-05: Circuit breaker defaults
            circuit_breaker_failure_threshold: 5,
            circuit_breaker_open_duration: StdDuration::from_secs(30),
            circuit_breaker_success_threshold: 2,
            circuit_breaker_call_timeout: StdDuration::from_secs(10),
        }
    }
}

/// Result of disconnection handling
#[derive(Debug)]
pub enum DisconnectionHandlingResult {
    /// Worker is still heartbeat-active (no action needed)
    HeartbeatActive,
    /// Short timeout triggered (monitoring mode)
    ShortTimeout,
    /// Long timeout triggered (cleanup needed)
    LongTimeout,
    /// Worker has active job, job timeout saga triggered
    JobTimeout,
    /// Worker not found
    WorkerNotFound,
    /// Failed to process
    Failed(String),
}

/// Worker Disconnection Handler Consumer
///
/// Consumes WorkerDisconnected events and triggers appropriate recovery
/// or timeout actions based on heartbeat and job status.
#[derive(Clone)]
pub struct WorkerDisconnectionHandlerConsumer {
    /// NATS client
    _client: Client,

    /// NATS JetStream context
    jetstream: JetStreamContext,

    /// Saga orchestrator (trait object for dynamic dispatch)
    orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,

    /// Job repository for job status queries
    job_repository: Arc<dyn JobRepository + Send + Sync>,

    /// Worker registry for state queries
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,

    /// Outbox repository for publishing events
    outbox_repository: Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync>,

    /// Consumer configuration
    pub config: WorkerDisconnectionHandlerConsumerConfig,

    /// EPIC-85 US-05: Circuit breaker for resilience
    circuit_breaker: Option<Arc<CircuitBreaker>>,

    /// Shutdown signal
    shutdown_tx: mpsc::Sender<()>,
}

impl std::fmt::Debug for WorkerDisconnectionHandlerConsumer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerDisconnectionHandlerConsumer")
            .field("config", &self.config)
            .field("shutdown_tx", &self.shutdown_tx)
            .finish_non_exhaustive()
    }
}

impl WorkerDisconnectionHandlerConsumer {
    /// Create a new WorkerDisconnectionHandlerConsumer
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: Client,
        jetstream: JetStreamContext,
        orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>,
        job_repository: Arc<dyn JobRepository + Send + Sync>,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
        outbox_repository: Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync>,
        config: Option<WorkerDisconnectionHandlerConsumerConfig>,
    ) -> Self {
        let config = config.unwrap_or_default();
        let (shutdown_tx, _) = mpsc::channel(1);

        // EPIC-85 US-05: Initialize circuit breaker
        let circuit_breaker = Arc::new(CircuitBreaker::new(
            format!("worker-disconnection-{}", config.consumer_name),
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
        format!("{}_DISCONNECT_EVENTS", self.config.stream_prefix)
    }

    /// Ensure the stream exists with correct configuration
    async fn ensure_stream(&self) -> Result<(), DomainError> {
        let stream_name = self.stream_name();

        match self.jetstream.get_stream(&stream_name).await {
            Ok(_) => {
                debug!(
                    "🌐 WorkerDisconnectionHandlerConsumer: Stream {} already exists",
                    stream_name
                );
                Ok(())
            }
            Err(_) => {
                info!(
                    "🌐 WorkerDisconnectionHandlerConsumer: Creating stream {}",
                    stream_name
                );

                let _stream = self
                    .jetstream
                    .create_stream(StreamConfig {
                        name: stream_name.clone(),
                        subjects: vec![
                            worker_topics::DISCONNECTED.to_string(),
                            worker_topics::HEARTBEAT_MISSED.to_string(),
                            worker_topics::RECONNECTED.to_string(),
                        ],
                        retention: RetentionPolicy::WorkQueue,
                        max_messages: 10000,
                        max_bytes: 1024 * 1024 * 50, // 50MB
                        ..Default::default()
                    })
                    .await
                    .map_err(|e| DomainError::InfrastructureError {
                        message: format!("Failed to create stream {}: {}", stream_name, e),
                    })?;

                info!(
                    "🌐 WorkerDisconnectionHandlerConsumer: Stream {} created",
                    stream_name
                );
                Ok(())
            }
        }
    }

    /// Create or get consumer by name
    async fn create_or_get_consumer(
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
                    "🌐 WorkerDisconnectionHandlerConsumer: Consumer {} already exists",
                    consumer_name
                );
                Ok(consumer)
            }
            Err(_) => {
                info!(
                    "🌐 WorkerDisconnectionHandlerConsumer: Creating consumer {}",
                    consumer_name
                );

                let consumer = self
                    .jetstream
                    .create_consumer_on_stream(
                        PullConsumerConfig {
                            name: Some(consumer_name.clone()),
                            durable_name: Some(consumer_name.clone()),
                            description: Some("Worker disconnection handler consumer".to_string()),
                            ack_policy: AckPolicy::Explicit,
                            deliver_policy: DeliverPolicy::All,
                            ack_wait: self.config.ack_wait,
                            max_deliver: self.config.max_deliver,
                            filter_subject: self.config.disconnected_topic.clone(),
                            ..Default::default()
                        },
                        stream_name,
                    )
                    .await
                    .map_err(|e| DomainError::InfrastructureError {
                        message: format!("Failed to create consumer {}: {}", consumer_name, e),
                    })?;

                info!(
                    "🌐 WorkerDisconnectionHandlerConsumer: Consumer {} created",
                    consumer_name
                );
                Ok(consumer)
            }
        }
    }

    /// Start the consumer and begin processing events
    pub async fn start(&self) -> Result<(), DomainError> {
        info!(
            "🌐 WorkerDisconnectionHandlerConsumer: Starting consumer '{}'",
            self.config.consumer_name
        );

        // Create stream and get stream info
        let stream_name = {
            let _ = self.ensure_stream().await?;
            self.stream_name()
        };

        // Create or get consumer
        let consumer = self.create_or_get_consumer(&stream_name).await?;

        // Start consuming messages
        let mut messages =
            consumer
                .messages()
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to create consumer stream: {}", e),
                })?;

        info!(
            "🌐 WorkerDisconnectionHandlerConsumer: Started consuming from stream '{}'",
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
                                    "🌐 WorkerDisconnectionHandlerConsumer: Error processing message: {}",
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
                            "🌐 WorkerDisconnectionHandlerConsumer: Circuit breaker open, skipping message"
                        );
                        // EPIC-85 US-05: Record failure for circuit being open
                        if let Some(ref cb) = self.circuit_breaker {
                            cb.record_failure();
                        }
                    }

                    // Ack the message
                    if let Err(e) = message.ack().await {
                        error!(
                            "🌐 WorkerDisconnectionHandlerConsumer: Failed to ack message: {}",
                            e
                        );
                    }
                }
                Err(e) => {
                    error!(
                        "🌐 WorkerDisconnectionHandlerConsumer: Message receive error: {}",
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
            DomainEvent::WorkerDisconnected {
                worker_id,
                last_heartbeat,
                ..
            } => {
                info!(
                    "🌐 WorkerDisconnectionHandlerConsumer: Received disconnect event for worker {}",
                    worker_id
                );
                self.handle_disconnection(worker_id, *last_heartbeat)
                    .await?;
            }
            _ => {
                debug!(
                    "🌐 WorkerDisconnectionHandlerConsumer: Ignoring event type '{}'",
                    event.event_type()
                );
            }
        }

        Ok(())
    }

    /// Handle WorkerDisconnected event
    async fn handle_disconnection(
        &self,
        worker_id: &WorkerId,
        last_heartbeat: Option<DateTime<Utc>>,
    ) -> Result<DisconnectionHandlingResult, DomainError> {
        // Check if worker exists in registry
        let worker = self.worker_registry.get(worker_id).await.map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to fetch worker {}: {}", worker_id, e),
            }
        })?;

        if worker.is_none() {
            debug!(
                "🌐 WorkerDisconnectionHandlerConsumer: Worker {} not found, skipping",
                worker_id
            );
            return Ok(DisconnectionHandlingResult::WorkerNotFound);
        }

        let worker = worker.unwrap();

        // Check if worker is idle (no active job)
        let has_active_job = worker.current_job_id().is_some();

        // Calculate disconnection duration
        let disconnect_duration = last_heartbeat
            .map(|hb| Utc::now().signed_duration_since(hb).num_seconds() as u64)
            .unwrap_or(0);

        // Decision tree based on heartbeat and job status
        if !has_active_job {
            // Idle worker - just mark as unhealthy if disconnected long enough
            if disconnect_duration > self.config.long_timeout_secs {
                info!(
                    "🌐 WorkerDisconnectionHandlerConsumer: Idle worker {} disconnected for {}s, marking unhealthy",
                    worker_id, disconnect_duration
                );
                self.mark_worker_unhealthy(worker_id).await?;
                Ok(DisconnectionHandlingResult::LongTimeout)
            } else {
                debug!(
                    "🌐 WorkerDisconnectionHandlerConsumer: Idle worker {} recently disconnected ({}s), monitoring",
                    worker_id, disconnect_duration
                );
                Ok(DisconnectionHandlingResult::ShortTimeout)
            }
        } else {
            // Worker has active job - need to handle job timeout
            let job_id = worker.current_job_id().unwrap();

            if disconnect_duration > self.config.short_timeout_secs {
                // Short timeout exceeded, trigger job timeout
                info!(
                    "🌐 WorkerDisconnectionHandlerConsumer: Worker {} with active job {} disconnected for {}s, triggering job timeout",
                    worker_id, job_id, disconnect_duration
                );
                self.trigger_job_timeout_saga(&job_id, worker_id).await?;
                self.publish_disconnection_event(worker_id, "job_timeout")
                    .await?;
                Ok(DisconnectionHandlingResult::JobTimeout)
            } else {
                // Still within short timeout, monitor
                debug!(
                    "🌐 WorkerDisconnectionHandlerConsumer: Worker {} with active job {} recently disconnected ({}s), monitoring",
                    worker_id, job_id, disconnect_duration
                );
                Ok(DisconnectionHandlingResult::ShortTimeout)
            }
        }
    }

    /// Mark worker as unhealthy
    async fn mark_worker_unhealthy(&self, worker_id: &WorkerId) -> Result<(), DomainError> {
        self.worker_registry
            .update_state(worker_id, WorkerState::Terminated)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to mark worker {} as unhealthy: {}", worker_id, e),
            })?;

        // Publish WorkerStatusChanged event
        let event = OutboxEventInsert::for_worker(
            worker_id.0,
            "WorkerStatusChanged".to_string(),
            serde_json::json!({
                "worker_id": worker_id.0.to_string(),
                "old_status": "Ready",
                "new_status": "Failed",
                "reason": "disconnection_timeout"
            }),
            Some(serde_json::json!({
                "source": "WorkerDisconnectionHandlerConsumer",
                "state_transition": "Ready->Failed"
            })),
            Some(format!("worker-unhealthy-{}", worker_id.0)),
        );

        self.outbox_repository
            .insert_events(&[event])
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!(
                    "Failed to insert WorkerStatusChanged event for worker {}: {}",
                    worker_id, e
                ),
            })?;

        Ok(())
    }

    /// Trigger job timeout saga
    async fn trigger_job_timeout_saga(
        &self,
        job_id: &JobId,
        worker_id: &WorkerId,
    ) -> Result<(), DomainError> {
        let saga_id = hodei_server_domain::saga::SagaId::new();
        let job_id_str = job_id.to_string();
        let worker_id_str = worker_id.to_string();
        let mut context = hodei_server_domain::saga::SagaContext::new(
            saga_id,
            SagaType::Recovery,
            Some(format!("job-timeout-{}-{}", job_id.0, worker_id.0)),
            Some("worker_disconnection_handler_consumer".to_string()),
        );

        context.set_metadata("job_id", &job_id_str).ok();
        context.set_metadata("worker_id", &worker_id_str).ok();
        let timeout_type_val = "worker_disconnection";
        context
            .set_metadata(METADATA_TIMEOUT_TYPE, &timeout_type_val)
            .ok();

        // Create recovery saga for timeout handling
        let saga = hodei_server_domain::saga::RecoverySaga::new(
            job_id.clone(),
            worker_id.clone(),
            Some(worker_id_str),
        );

        // Execute saga with timeout
        let saga_result = tokio::time::timeout(
            self.config.saga_timeout,
            self.orchestrator.execute_saga(&saga, context),
        )
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Job timeout saga timeout for job {}: {}", job_id, e),
        })?
        .map_err(|e| DomainError::InfrastructureError {
            message: format!(
                "Job timeout saga execution failed for job {}: {}",
                job_id, e
            ),
        })?;

        if saga_result.is_success() {
            info!(
                "🌐 WorkerDisconnectionHandlerConsumer: Job timeout saga completed for job {}",
                job_id
            );
        } else {
            let error_msg = saga_result
                .error_message
                .unwrap_or_else(|| "Unknown error".to_string());
            warn!(
                "🌐 WorkerDisconnectionHandlerConsumer: Job timeout saga failed for job {}: {}",
                job_id, error_msg
            );
        }

        Ok(())
    }

    /// Publish disconnection handling event
    async fn publish_disconnection_event(
        &self,
        worker_id: &WorkerId,
        action: &str,
    ) -> Result<(), DomainError> {
        let event = OutboxEventInsert::for_worker(
            worker_id.0,
            "WorkerDisconnectionHandled".to_string(),
            serde_json::json!({
                "worker_id": worker_id.0.to_string(),
                "action": action,
                "occurred_at": Utc::now().to_rfc3339()
            }),
            Some(serde_json::json!({
                "source": "WorkerDisconnectionHandlerConsumer",
                "handling": true
            })),
            Some(format!("disconnection-handled-{}-{}", worker_id.0, action)),
        );

        self.outbox_repository
            .insert_events(&[event])
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!(
                    "Failed to insert WorkerDisconnectionHandled event for worker {}: {}",
                    worker_id, e
                ),
            })?;

        Ok(())
    }
}

/// Builder for WorkerDisconnectionHandlerConsumer
#[derive(Default)]
pub struct WorkerDisconnectionHandlerConsumerBuilder {
    config: Option<WorkerDisconnectionHandlerConsumerConfig>,
    client: Option<Client>,
    jetstream: Option<JetStreamContext>,
    orchestrator: Option<Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync>>,
    job_repository: Option<Arc<dyn JobRepository + Send + Sync>>,
    worker_registry: Option<Arc<dyn WorkerRegistry + Send + Sync>>,
    outbox_repository: Option<Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync>>,
}

impl std::fmt::Debug for WorkerDisconnectionHandlerConsumerBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerDisconnectionHandlerConsumerBuilder")
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

impl WorkerDisconnectionHandlerConsumerBuilder {
    /// Create a new builder
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set configuration
    #[inline]
    pub fn with_config(mut self, config: WorkerDisconnectionHandlerConsumerConfig) -> Self {
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
    pub fn build(self) -> Result<WorkerDisconnectionHandlerConsumer, String> {
        let client = self.client.ok_or("NATS client is required")?;
        let jetstream = self.jetstream.ok_or("JetStream context is required")?;
        let orchestrator = self.orchestrator.ok_or("Saga orchestrator is required")?;
        let job_repository = self.job_repository.ok_or("Job repository is required")?;
        let worker_registry = self.worker_registry.ok_or("Worker registry is required")?;
        let outbox_repository = self
            .outbox_repository
            .ok_or("Outbox repository is required")?;

        Ok(WorkerDisconnectionHandlerConsumer::new(
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

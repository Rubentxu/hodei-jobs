//! Worker Agent gRPC Service Implementation (PRD v6.0 Aligned)
//!
//! Implements the WorkerAgentService with:
//! - Register RPC with OTP token authentication
//! - WorkerStream bidirectional stream for Worker↔Server communication
//! - Actor Model integration via WorkerSupervisorHandle (EPIC-42)
//! - HeartbeatProcessor and LogIngestor for Single Responsibility (GAP-GO-01)

use dashmap::DashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{Mutex as TokioMutex, mpsc};
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, info, warn};

use hodei_jobs::{
    AckMessage, LogEntry, RegisterWorkerRequest, RegisterWorkerResponse, ServerMessage,
    UnregisterWorkerRequest, UnregisterWorkerResponse, UpdateWorkerStatusRequest,
    UpdateWorkerStatusResponse, WorkerInfo, WorkerMessage,
    server_message::Payload as ServerPayload, worker_agent_service_server::WorkerAgentService,
    worker_message::Payload as WorkerPayload,
};

use crate::grpc::heartbeat_processor::HeartbeatProcessor;
use crate::grpc::interceptors::RequestContextExt;
use crate::grpc::log_ingestor::LogIngestor;
use crate::grpc::log_stream::LogStreamService;
use chrono::Utc;
use hodei_server_application::workers::actor::WorkerSupervisorHandle;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::iam::OtpToken;
use hodei_server_domain::outbox::{OutboxEventInsert, OutboxRepository};
use hodei_server_domain::shared_kernel::{
    DomainError, JobId, JobResult, JobState, ProviderId, WorkerId, WorkerState,
};
use hodei_server_domain::workers::{ProviderType, WorkerHandle, WorkerSpec};

/// Estado interno de un worker registrado
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct RegisteredWorker {
    info: WorkerInfo,
    session_id: String,
    status: i32,
}

/// In-memory OTP state for fallback when no persistent store is configured
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct InMemoryOtpState {
    token: String,
    worker_id: String,
    created_at: std::time::Instant,
    used: bool,
}

/// Servicio gRPC para Worker Agents (PRD v6.0)
///
/// Implementa el patrón Transactional Outbox para garantizar
/// consistencia entre actualizaciones de estado y publicación de eventos.
///
/// GAP-GO-01: Delega procesamiento de heartbeats a HeartbeatProcessor
/// y procesamiento de logs a LogIngestor para Single Responsibility.
#[derive(Clone)]
pub struct WorkerAgentServiceImpl {
    /// Legacy in-memory worker registry (used when Actor is disabled)
    /// EPIC-42: Usando DashMap para concurrencia sin bloqueos
    workers: Arc<DashMap<String, RegisteredWorker>>,
    /// Legacy OTP tokens storage (used when persistent store is not available)
    /// EPIC-42: Usando DashMap para concurrencia sin bloqueos
    otp_tokens: Arc<DashMap<String, InMemoryOtpState>>,
    /// Legacy channel para enviar comandos a workers conectados
    /// EPIC-42: Usando DashMap para concurrencia sin bloqueos
    worker_channels: Arc<DashMap<String, mpsc::Sender<Result<ServerMessage, Status>>>>,
    /// Worker registry for persistent storage
    worker_registry: Option<Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>>,
    job_repository: Option<Arc<dyn hodei_server_domain::jobs::JobRepository>>,
    token_store: Option<Arc<dyn hodei_server_domain::iam::WorkerBootstrapTokenStore>>,
    /// Channel para log streaming
    log_service: Option<Arc<LogStreamService>>,
    /// Event Bus para publicar eventos de dominio (legacy, used by OutboxRelay)
    event_bus: Option<Arc<dyn hodei_server_domain::event_bus::EventBus>>,
    /// Outbox Repository para patrón Transactional Outbox
    outbox_repository: Option<Arc<dyn OutboxRepository + Send + Sync>>,
    /// GAP-GO-01: HeartbeatProcessor para procesamiento dedicado de heartbeats
    heartbeat_processor: Option<Arc<HeartbeatProcessor>>,
    /// GAP-GO-01: LogIngestor para procesamiento dedicado de logs
    log_ingestor: Option<Arc<LogIngestor>>,
    /// EPIC-42: WorkerSupervisorActor handle for lock-free worker management
    /// When Some, worker operations are routed through the Actor
    supervisor_handle: Option<WorkerSupervisorHandle>,
}

impl Default for WorkerAgentServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}
impl WorkerAgentServiceImpl {
    pub fn new() -> Self {
        Self {
            workers: Arc::new(DashMap::new()),
            otp_tokens: Arc::new(DashMap::new()),
            worker_channels: Arc::new(DashMap::new()),
            worker_registry: None,
            job_repository: None,
            token_store: None,
            log_service: None,
            event_bus: None,
            outbox_repository: None,
            heartbeat_processor: None,
            log_ingestor: None,
            supervisor_handle: None,
        }
    }

    /// GAP-GO-01: Create service with HeartbeatProcessor, LogIngestor, and Actor supervisor
    pub fn with_actor_supervisor(
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
        token_store: Arc<dyn hodei_server_domain::iam::WorkerBootstrapTokenStore>,
        log_service: Arc<LogStreamService>,
        event_bus: Arc<dyn hodei_server_domain::event_bus::EventBus>,
        supervisor_handle: WorkerSupervisorHandle,
    ) -> Self {
        // GAP-GO-01: Create HeartbeatProcessor with dependencies
        let heartbeat_processor =
            HeartbeatProcessor::new(Some(worker_registry.clone()), Some(event_bus.clone()), None)
                .with_supervisor_handle(supervisor_handle.clone());

        // GAP-GO-01: Create LogIngestor
        let log_ingestor = LogIngestor::new(Some(log_service.clone()), None);

        Self {
            workers: Arc::new(DashMap::new()),
            otp_tokens: Arc::new(DashMap::new()),
            worker_channels: Arc::new(DashMap::new()),
            worker_registry: Some(worker_registry),
            job_repository: Some(job_repository),
            token_store: Some(token_store),
            log_service: Some(log_service),
            event_bus: Some(event_bus),
            outbox_repository: None,
            heartbeat_processor: Some(Arc::new(heartbeat_processor)),
            log_ingestor: Some(Arc::new(log_ingestor)),
            supervisor_handle: Some(supervisor_handle),
        }
    }

    /// EPIC-42: Check if Actor model is enabled
    pub fn is_actor_enabled(&self) -> bool {
        self.supervisor_handle.is_some()
    }

    /// EPIC-42: Get supervisor handle if available
    pub fn supervisor_handle(&self) -> Option<&WorkerSupervisorHandle> {
        self.supervisor_handle.as_ref()
    }

    pub fn with_registry(
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
        event_bus: Arc<dyn hodei_server_domain::event_bus::EventBus>,
    ) -> Self {
        Self {
            workers: Arc::new(DashMap::new()),
            otp_tokens: Arc::new(DashMap::new()),
            worker_channels: Arc::new(DashMap::new()),
            worker_registry: Some(worker_registry),
            job_repository: None,
            token_store: None,
            log_service: None,
            event_bus: Some(event_bus),
            outbox_repository: None,
            heartbeat_processor: None,
            log_ingestor: None,
            supervisor_handle: None,
        }
    }

    pub fn with_registry_and_log_service(
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
        log_service: Arc<LogStreamService>,
        event_bus: Arc<dyn hodei_server_domain::event_bus::EventBus>,
    ) -> Self {
        // GAP-GO-01: Create LogIngestor with log_service
        let log_ingestor = LogIngestor::new(Some(log_service.clone()), None);

        Self {
            workers: Arc::new(DashMap::new()),
            otp_tokens: Arc::new(DashMap::new()),
            worker_channels: Arc::new(DashMap::new()),
            worker_registry: Some(worker_registry),
            job_repository: None,
            token_store: None,
            log_service: Some(log_service),
            event_bus: Some(event_bus),
            outbox_repository: None,
            heartbeat_processor: None,
            log_ingestor: Some(Arc::new(log_ingestor)),
            supervisor_handle: None,
        }
    }

    pub fn with_token_store(
        mut self,
        token_store: Arc<dyn hodei_server_domain::iam::WorkerBootstrapTokenStore>,
    ) -> Self {
        self.token_store = Some(token_store);
        self
    }

    pub fn with_job_repository(
        mut self,
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
    ) -> Self {
        self.job_repository = Some(job_repository);
        self
    }

    /// Add outbox repository for transactional event publishing
    pub fn with_outbox_repository(
        mut self,
        outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
    ) -> Self {
        self.outbox_repository = Some(outbox_repository);
        self
    }

    pub fn with_registry_job_repository_and_log_service(
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
        log_service: Arc<LogStreamService>,
        event_bus: Arc<dyn hodei_server_domain::event_bus::EventBus>,
    ) -> Self {
        // GAP-GO-01: Create LogIngestor with log_service
        let log_ingestor = LogIngestor::new(Some(log_service.clone()), None);

        Self {
            workers: Arc::new(DashMap::new()),
            otp_tokens: Arc::new(DashMap::new()),
            worker_channels: Arc::new(DashMap::new()),
            worker_registry: Some(worker_registry),
            job_repository: Some(job_repository),
            token_store: None,
            log_service: Some(log_service),
            event_bus: Some(event_bus),
            outbox_repository: None,
            heartbeat_processor: None,
            log_ingestor: Some(Arc::new(log_ingestor)),
            supervisor_handle: None,
        }
    }

    pub fn with_registry_job_repository_token_store_and_log_service(
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
        token_store: Arc<dyn hodei_server_domain::iam::WorkerBootstrapTokenStore>,
        log_service: Arc<LogStreamService>,
        event_bus: Arc<dyn hodei_server_domain::event_bus::EventBus>,
    ) -> Self {
        // GAP-GO-01: Create LogIngestor with log_service
        let log_ingestor = LogIngestor::new(Some(log_service.clone()), None);

        Self {
            workers: Arc::new(DashMap::new()),
            otp_tokens: Arc::new(DashMap::new()),
            worker_channels: Arc::new(DashMap::new()),
            worker_registry: Some(worker_registry),
            job_repository: Some(job_repository),
            token_store: Some(token_store),
            log_service: Some(log_service),
            event_bus: Some(event_bus),
            outbox_repository: None,
            heartbeat_processor: None,
            log_ingestor: Some(Arc::new(log_ingestor)),
            supervisor_handle: None,
        }
    }

    /// Full constructor with outbox repository support (production-ready)
    /// GAP-GO-01: Also initializes HeartbeatProcessor and LogIngestor
    pub fn with_all_dependencies(
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
        token_store: Arc<dyn hodei_server_domain::iam::WorkerBootstrapTokenStore>,
        log_service: Arc<LogStreamService>,
        event_bus: Arc<dyn hodei_server_domain::event_bus::EventBus>,
        outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
    ) -> Self {
        // GAP-GO-01: Create HeartbeatProcessor and LogIngestor
        let heartbeat_processor =
            HeartbeatProcessor::new(Some(worker_registry.clone()), Some(event_bus.clone()), None);
        let log_ingestor = LogIngestor::new(Some(log_service.clone()), None);

        Self {
            workers: Arc::new(DashMap::new()),
            otp_tokens: Arc::new(DashMap::new()),
            worker_channels: Arc::new(DashMap::new()),
            worker_registry: Some(worker_registry),
            job_repository: Some(job_repository),
            token_store: Some(token_store),
            log_service: Some(log_service),
            event_bus: Some(event_bus),
            outbox_repository: Some(outbox_repository),
            heartbeat_processor: Some(Arc::new(heartbeat_processor)),
            log_ingestor: Some(Arc::new(log_ingestor)),
            supervisor_handle: None,
        }
    }

    fn parse_worker_uuid(
        worker_id: &str,
    ) -> Result<hodei_server_domain::shared_kernel::WorkerId, Status> {
        let id = uuid::Uuid::parse_str(worker_id)
            .map_err(|_| Status::invalid_argument("worker_id must be a UUID"))?;
        Ok(hodei_server_domain::shared_kernel::WorkerId(id))
    }

    fn token_store(&self) -> Option<&Arc<dyn hodei_server_domain::iam::WorkerBootstrapTokenStore>> {
        self.token_store.as_ref()
    }

    fn worker_registry(
        &self,
    ) -> Option<&Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>> {
        self.worker_registry.as_ref()
    }

    fn job_repository(&self) -> Option<&Arc<dyn hodei_server_domain::jobs::JobRepository>> {
        self.job_repository.as_ref()
    }

    fn outbox_repository(&self) -> Option<&Arc<dyn OutboxRepository + Send + Sync>> {
        self.outbox_repository.as_ref()
    }

    fn parse_job_uuid(job_id: &str) -> Result<JobId, Status> {
        let id = uuid::Uuid::parse_str(job_id)
            .map_err(|_| Status::invalid_argument("job_id must be a UUID"))?;
        Ok(JobId(id))
    }

    /// Handle worker registration using Transactional Outbox Pattern
    /// Modified to support Legacy/JIT flow: If worker validates via OTP but Actor doesn't know it,
    /// we register it immediately to let the flow continue.
    ///
    /// EPIC-42: When WorkerSupervisorActor is enabled, route through the Actor.
    /// If Actor doesn't have the worker (state inconsistency), perform JIT registration.
    ///
    /// This method:
    /// 1. Attempts heartbeat through Actor (fast path if Actor has the worker)
    /// 2. If Actor returns WorkerNotFound, performs JIT registration to heal state
    /// 3. Emits WorkerRegistered and WorkerReady events to trigger job dispatch
    ///
    /// Events emitted:
    /// - WorkerRegistered: Worker completed initial registration
    /// - WorkerReady: Worker is available for job assignment (triggers dispatch)
    async fn on_worker_registered(
        &self,
        worker_id_str: &str,
        worker_info: &WorkerInfo,
        job_id_from_token: Option<JobId>,
    ) -> Result<(), Status> {
        let worker_id = Self::parse_worker_uuid(worker_id_str)?;

        // 1. Attempt route through Actor (EPIC-42)
        if let Some(ref supervisor) = self.supervisor_handle {
            debug!(
                "EPIC-42: Routing worker registration through Actor for {}",
                worker_id
            );

            match supervisor.heartbeat(&worker_id).await {
                Ok(_) => {
                    debug!("EPIC-42: Actor updated successfully (Worker existed)");
                    // Worker is known to Actor, we're done
                    self.emit_worker_events(&worker_id).await?;
                    return Ok(());
                }
                Err(e) => {
                    // Check if error is "WorkerNotFound" (state inconsistency)
                    let error_msg = e.to_string();
                    let is_not_found = error_msg.contains("WorkerNotFound")
                        || error_msg.contains("not found")
                        || error_msg.contains("not exist");

                    if is_not_found {
                        info!(
                            "🔄 JIT Registration: Worker {} validated but missing in Actor. Registering now...",
                            worker_id
                        );

                        // A. Recover provider_id from job context or database
                        // REACTIVE FIX: No fallbacks. If provider_id cannot be recovered from
                        // a valid context, return an error. This forces state consistency.
                        let provider_id_result: Result<ProviderId, DomainError> = if let Some(
                            ref registry,
                        ) =
                            self.worker_registry
                        {
                            match registry.get(&worker_id).await {
                                Ok(Some(existing_worker)) => {
                                    info!(
                                        "✅ Found existing worker {} in database with provider_id: {}",
                                        worker_id,
                                        existing_worker.handle().provider_id
                                    );
                                    Ok(existing_worker.handle().provider_id.clone())
                                }
                                Ok(None) => {
                                    // Worker not in DB - MUST recover provider_id from job context
                                    if let Some(ref job_id) = job_id_from_token {
                                        info!(
                                            "🔄 Worker {} not in DB, recovering provider_id from job {}",
                                            worker_id, job_id
                                        );
                                        if let Some(ref job_repo) = self.job_repository {
                                            match job_repo.find_by_id(job_id).await {
                                                Ok(Some(job)) => {
                                                    job.selected_provider()
                                                        .cloned()
                                                        .ok_or_else(|| {
                                                            DomainError::WorkerProvisioningFailed {
                                                                message: format!(
                                                                    "Job {} has no selected_provider - provisioning state inconsistent",
                                                                    job_id
                                                                ),
                                                            }
                                                        })
                                                }
                                                Ok(None) => Err(DomainError::JobNotFound {
                                                    job_id: job_id.clone(),
                                                }),
                                                Err(e) => Err(DomainError::InfrastructureError {
                                                    message: format!("Failed to query job {}: {}", job_id, e),
                                                }),
                                            }
                                        } else {
                                            Err(DomainError::InfrastructureError {
                                                message: "JobRepository not available - cannot recover provider_id".to_string(),
                                            })
                                        }
                                    } else {
                                        Err(DomainError::WorkerProvisioningFailed {
                                            message: format!(
                                                "Worker {} has no job_id and no DB record - provisioning state lost",
                                                worker_id
                                            ),
                                        })
                                    }
                                }
                                Err(e) => Err(DomainError::InfrastructureError {
                                    message: format!(
                                        "Failed to query worker {} from database: {}",
                                        worker_id, e
                                    ),
                                }),
                            }
                        } else {
                            Err(DomainError::InfrastructureError {
                                message:
                                    "WorkerRegistry not available - cannot recover provider_id"
                                        .to_string(),
                            })
                        };

                        // Propagate error if provider_id cannot be recovered
                        let provider_id = match provider_id_result {
                            Ok(id) => id,
                            Err(e) => {
                                error!(
                                    "❌ Failed to recover provider_id for worker {}: {:?}",
                                    worker_id, e
                                );
                                return Err(Status::failed_precondition(format!(
                                    "Worker registration failed: {}",
                                    e
                                )));
                            }
                        };

                        // B. Build WorkerHandle from worker info
                        let resource_id = worker_info.name.clone();
                        let handle = WorkerHandle::new(
                            worker_id.clone(),
                            resource_id.clone(),
                            ProviderType::Docker,
                            provider_id.clone(),
                        );

                        // C. Build WorkerSpec from worker info
                        let mut spec = WorkerSpec::new(
                            "hodei-worker:latest".to_string(),
                            "http://localhost:50051".to_string(),
                        );
                        // Extract capacity from worker info if available
                        if let Some(cap) = &worker_info.capacity {
                            spec.resources.cpu_cores = cap.cpu_cores;
                            spec.resources.memory_bytes = cap.memory_bytes;
                        }

                        // D. Register in Actor (this unblocks the flow)
                        match supervisor
                            .register(
                                worker_id.clone(),
                                provider_id.clone(),
                                handle.clone(),
                                spec.clone(),
                            )
                            .await
                        {
                            Ok(_) => {
                                info!(
                                    "✅ JIT Registration successful for {}. Persisting to database...",
                                    worker_id
                                );

                                // BUG-001 FIX: Persist worker to PostgreSQL via WorkerRegistry
                                if let Some(ref registry) = self.worker_registry {
                                    let job_id = job_id_from_token.unwrap_or_else(JobId::new);
                                    match registry
                                        .register(handle.clone(), spec.clone(), job_id)
                                        .await
                                    {
                                        Ok(worker) => {
                                            info!(
                                                "✅ Worker {} persisted to database with ID: {}",
                                                worker_id,
                                                worker.id()
                                            );
                                        }
                                        Err(e) => {
                                            error!(
                                                "❌ Failed to persist worker {} to database: {:?}. Compensating...",
                                                worker_id, e
                                            );
                                            // Compensar: eliminar del Actor
                                            if let Err(unreg_err) =
                                                supervisor.unregister(&worker_id).await
                                            {
                                                warn!(
                                                    "Failed to compensate actor registration: {:?}",
                                                    unreg_err
                                                );
                                            }
                                            return Err(Status::internal(format!(
                                                "Failed to persist worker to database: {}",
                                                e
                                            )));
                                        }
                                    }
                                } else {
                                    warn!(
                                        "WorkerRegistry not available, worker {} not persisted to database",
                                        worker_id
                                    );
                                }
                            }
                            Err(reg_err) => {
                                error!("❌ JIT Registration failed: {:?}", reg_err);
                                return Err(Status::internal(
                                    "Failed to heal worker state in actor",
                                ));
                            }
                        }
                    } else {
                        // Real actor error (timeout, channel closed, etc.)
                        warn!(
                            "EPIC-42: Actor heartbeat failed with unexpected error: {:?}",
                            e
                        );
                        // Continue anyway - we can still emit events
                    }
                }
            }
        } else {
            // No supervisor/Actor mode - register directly in PostgreSQL
            info!(
                "🔄 No supervisor available. Registering worker {} directly in database...",
                worker_id
            );

            if let Some(ref registry) = self.worker_registry {
                // Check if worker already exists in DB
                match registry.get(&worker_id).await {
                    Ok(Some(existing_worker)) => {
                        info!(
                            "✅ Worker {} already exists in database with state: {:?}",
                            worker_id,
                            existing_worker.state()
                        );
                        // Update state to Ready if not already
                        if !matches!(existing_worker.state(), &WorkerState::Ready) {
                            if let Err(e) =
                                registry.update_state(&worker_id, WorkerState::Ready).await
                            {
                                warn!(
                                    "Failed to update worker {} state to Ready: {:?}",
                                    worker_id, e
                                );
                            } else {
                                info!("✅ Worker {} state updated to Ready", worker_id);
                            }
                        }
                    }
                    Ok(None) => {
                        // Worker not in DB - need to register it
                        info!(
                            "🔄 Worker {} not in database, registering now...",
                            worker_id
                        );

                        // Recover provider_id from job context
                        let provider_id = if let Some(ref job_id) = job_id_from_token {
                            if let Some(ref job_repo) = self.job_repository {
                                match job_repo.find_by_id(job_id).await {
                                    Ok(Some(job)) => job
                                        .selected_provider()
                                        .cloned()
                                        .unwrap_or_else(ProviderId::new),
                                    _ => ProviderId::new(),
                                }
                            } else {
                                ProviderId::new()
                            }
                        } else {
                            ProviderId::new()
                        };

                        // Build WorkerHandle from worker info
                        let resource_id = worker_info.name.clone();
                        let handle = WorkerHandle::new(
                            worker_id.clone(),
                            resource_id.clone(),
                            ProviderType::Kubernetes, // Default to Kubernetes for k8s workers
                            provider_id.clone(),
                        );

                        // Build WorkerSpec from worker info
                        let mut spec = WorkerSpec::new(
                            "hodei-worker:latest".to_string(),
                            "http://localhost:50051".to_string(),
                        );
                        if let Some(cap) = &worker_info.capacity {
                            spec.resources.cpu_cores = cap.cpu_cores;
                            spec.resources.memory_bytes = cap.memory_bytes;
                        }

                        // Register in PostgreSQL
                        let job_id = job_id_from_token.clone().unwrap_or_else(JobId::new);
                        match registry
                            .register(handle.clone(), spec.clone(), job_id.clone())
                            .await
                        {
                            Ok(worker) => {
                                info!(
                                    "✅ Worker {} registered in database with ID: {}",
                                    worker_id,
                                    worker.id()
                                );
                                // Mark as Ready immediately
                                if let Err(e) =
                                    registry.update_state(&worker_id, WorkerState::Ready).await
                                {
                                    warn!("Failed to mark worker {} as Ready: {:?}", worker_id, e);
                                } else {
                                    info!("✅ Worker {} marked as Ready in database", worker_id);
                                }
                            }
                            Err(e) => {
                                // Check if it's a duplicate key error (worker already exists)
                                let err_msg = e.to_string();
                                if err_msg.contains("duplicate")
                                    || err_msg.contains("already exists")
                                {
                                    info!(
                                        "Worker {} already registered (race condition), updating state...",
                                        worker_id
                                    );
                                    if let Err(e) =
                                        registry.update_state(&worker_id, WorkerState::Ready).await
                                    {
                                        warn!(
                                            "Failed to update worker {} state: {:?}",
                                            worker_id, e
                                        );
                                    }
                                } else {
                                    error!(
                                        "❌ Failed to register worker {} in database: {:?}",
                                        worker_id, e
                                    );
                                    return Err(Status::internal(format!(
                                        "Failed to register worker in database: {}",
                                        e
                                    )));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to query worker {} from database: {:?}",
                            worker_id, e
                        );
                    }
                }
            } else {
                warn!(
                    "WorkerRegistry not available, worker {} cannot be persisted to database",
                    worker_id
                );
            }
        }

        // 2. Emit WorkerRegistered and WorkerReady events (triggers JobDispatcher)
        self.emit_worker_events(&worker_id).await?;

        Ok(())
    }

    /// Helper to emit WorkerRegistered and WorkerReady events
    async fn emit_worker_events(&self, worker_id: &WorkerId) -> Result<(), Status> {
        if let Some(event_bus) = &self.event_bus {
            let now = Utc::now();

            // WorkerRegistered event
            let event_registered = DomainEvent::WorkerRegistered {
                worker_id: worker_id.clone(),
                provider_id: ProviderId::new(),
                occurred_at: now,
                correlation_id: None,
                actor: Some("worker-jit-registration".to_string()),
            };

            if let Err(e) = event_bus.publish(&event_registered).await {
                warn!("Failed to publish WorkerRegistered: {}", e);
            }

            // WorkerReady event - CRUCIAL: This triggers JobDispatcher to assign job
            let event_ready = DomainEvent::WorkerReady {
                worker_id: worker_id.clone(),
                provider_id: ProviderId::new(),
                ready_at: now,
                correlation_id: None,
                actor: Some("worker-jit-registration".to_string()),
            };

            if let Err(e) = event_bus.publish(&event_ready).await {
                warn!("Failed to publish WorkerReady: {}", e);
            } else {
                info!(
                    "🚀 Emitted WorkerReady for {}. JobDispatcher should pick this up.",
                    worker_id
                );
            }
        }
        Ok(())
    }

    async fn on_job_result(
        &self,
        worker_id: &str,
        result: &hodei_jobs::JobResultMessage,
    ) -> Result<(), Status> {
        let Some(registry) = self.worker_registry() else {
            return Ok(());
        };
        let Some(job_repository) = self.job_repository() else {
            return Ok(());
        };

        let worker_id = Self::parse_worker_uuid(worker_id)?;
        let job_id = Self::parse_job_uuid(&result.job_id)?;

        let mut job = job_repository
            .find_by_id(&job_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        // Capture old_state before any transitions for the event
        let old_state = job.state().clone();

        // Handle state transition from PENDING if needed
        // In case worker didn't send acknowledgment (race condition or fast job),
        // we need to transition through RUNNING state
        if matches!(job.state(), JobState::Pending) {
            info!(
                "🔄 Job {} is in PENDING state, transitioning to RUNNING first before completing",
                result.job_id
            );
            job.mark_running()
                .map_err(|e| Status::failed_precondition(e.to_string()))?;
            job_repository
                .update(&job)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
        }

        // Now complete the job
        if result.success {
            job.complete(JobResult::Success {
                exit_code: result.exit_code,
                output: String::new(),
                error_output: String::new(),
            })
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        } else {
            let msg = if result.error_message.is_empty() {
                "Worker reported failure".to_string()
            } else {
                result.error_message.clone()
            };

            if msg.starts_with("TIMEOUT:") {
                job.timeout()
                    .map_err(|e| Status::failed_precondition(e.to_string()))?;
            } else {
                job.fail(msg)
                    .map_err(|e| Status::failed_precondition(e.to_string()))?;
            }
        }

        job_repository
            .update(&job)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Publicar evento JobStatusChanged
        if let Some(event_bus) = &self.event_bus {
            let new_state = job.state().clone();

            let correlation_id = job.metadata().get("correlation_id").cloned();
            let actor = job.metadata().get("actor").cloned();

            let event = DomainEvent::JobStatusChanged {
                job_id: job.id.clone(),
                old_state, // Use actual old state (PENDING or RUNNING)
                new_state,
                occurred_at: Utc::now(),
                correlation_id,
                actor,
            };
            if let Err(e) = event_bus.publish(&event).await {
                warn!(
                    "Failed to publish JobStatusChanged event in on_job_result: {}",
                    e
                );
            } else {
                info!(
                    "📢 Published JobStatusChanged event: {} → {:?}",
                    job.id, new_state
                );
            }
        }

        // EPIC-32: Publish WorkerEphemeralTerminating event for cleanup
        // This triggers reactive worker cleanup through the event system
        if matches!(
            job.state(),
            JobState::Succeeded | JobState::Failed | JobState::Timeout | JobState::Cancelled
        ) {
            // Get worker info for cleanup event
            if let Ok(Some(worker)) = registry.get(&worker_id).await {
                let provider_id = worker.provider_id().clone();
                let reason = if *job.state() == JobState::Succeeded {
                    hodei_server_domain::events::TerminationReason::JobCompleted
                } else {
                    hodei_server_domain::events::TerminationReason::ProviderError {
                        message: format!("Job ended with state: {:?}", job.state()),
                    }
                };

                // Publish WorkerEphemeralTerminating event via outbox (if available) or event bus
                let cleanup_event = OutboxEventInsert::for_worker(
                    worker_id.0,
                    "WorkerEphemeralTerminating".to_string(),
                    serde_json::json!({
                        "worker_id": worker_id.0.to_string(),
                        "provider_id": provider_id.0.to_string(),
                        "reason": format!("{:?}", reason),
                        "job_id": job_id.0.to_string()
                    }),
                    Some(serde_json::json!({
                        "source": "WorkerAgentService",
                        "cleanup_type": "reactive_event",
                        "event": "job_completion"
                    })),
                    Some(format!(
                        "ephemeral-terminating-{}-{}",
                        worker_id.0,
                        Utc::now().timestamp()
                    )),
                );

                if let Some(ref outbox_repo) = self.outbox_repository {
                    if let Err(e) = outbox_repo.insert_events(&[cleanup_event]).await {
                        warn!(
                            "Failed to insert WorkerEphemeralTerminating event for worker {}: {}",
                            worker_id, e
                        );
                    } else {
                        info!(
                            "📤 Published WorkerEphemeralTerminating event for reactive cleanup (worker={}, job={})",
                            worker_id, job_id
                        );
                    }
                } else if let Some(event_bus) = &self.event_bus {
                    let event = DomainEvent::WorkerEphemeralTerminating {
                        worker_id: worker_id.clone(),
                        provider_id: provider_id.clone(),
                        reason,
                        occurred_at: Utc::now(),
                        correlation_id: Some(job_id.0.to_string()),
                        actor: Some("worker_agent_service".to_string()),
                    };
                    if let Err(e) = event_bus.publish(&event).await {
                        warn!("Failed to publish WorkerEphemeralTerminating event: {}", e);
                    } else {
                        info!(
                            "📤 Published WorkerEphemeralTerminating event (worker={}, job={})",
                            worker_id, job_id
                        );
                    }
                }

                // Release worker from job (clears current_job_id)
                registry
                    .release_from_job(&worker_id)
                    .await
                    .map_err(|e| Status::internal(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Handle job acknowledgment from worker using Transactional Outbox Pattern
    ///
    /// This method:
    /// 1. Updates job state to Running
    /// 2. Inserts events into outbox table (if configured)
    /// 3. Falls back to direct event publishing if outbox not configured
    ///
    /// Events emitted:
    /// - JobAccepted: Worker has accepted the job
    /// - JobDispatchAcknowledged: Transport-level confirmation
    /// - JobStatusChanged: State transition to Running
    async fn on_job_acknowledged(&self, worker_id: &str, job_id: &str) -> Result<(), Status> {
        let Some(job_repository) = self.job_repository() else {
            return Ok(());
        };

        let worker_uuid = Self::parse_worker_uuid(worker_id)?;
        let job_id = Self::parse_job_uuid(job_id)?;

        let mut job = job_repository
            .find_by_id(&job_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        // Extract metadata for events
        let correlation_id = job.metadata().get("correlation_id").cloned();
        let actor = job.metadata().get("actor").cloned();
        let old_state = job.state().clone();
        let now = Utc::now();

        // Update job state to RUNNING when worker acknowledges
        job.mark_running()
            .map_err(|e: DomainError| Status::failed_precondition(e.to_string()))?;

        job_repository
            .update(&job)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Use Transactional Outbox Pattern if configured
        if let Some(outbox_repo) = self.outbox_repository() {
            let idempotency_base = format!("ack-{}-{}", job_id.0, worker_uuid.0);

            let events = vec![
                // JobAccepted: Logical acknowledgment
                OutboxEventInsert::for_job(
                    job_id.0,
                    "JobAccepted".to_string(),
                    serde_json::json!({
                        "job_id": job_id.0.to_string(),
                        "worker_id": worker_uuid.0.to_string(),
                        "occurred_at": now.to_rfc3339(),
                        "correlation_id": correlation_id,
                        "actor": actor
                    }),
                    Some(serde_json::json!({
                        "correlation_id": correlation_id,
                        "actor": actor,
                        "source": "WorkerHandler"
                    })),
                    Some(format!("{}-accepted", idempotency_base)),
                ),
                // JobDispatchAcknowledged: Transport-level confirmation
                OutboxEventInsert::for_job(
                    job_id.0,
                    "JobDispatchAcknowledged".to_string(),
                    serde_json::json!({
                        "job_id": job_id.0.to_string(),
                        "worker_id": worker_uuid.0.to_string(),
                        "acknowledged_at": now.to_rfc3339(),
                        "correlation_id": correlation_id,
                        "actor": actor
                    }),
                    Some(serde_json::json!({
                        "correlation_id": correlation_id,
                        "actor": actor,
                        "source": "WorkerHandler"
                    })),
                    Some(format!("{}-dispatch-ack", idempotency_base)),
                ),
                // JobStatusChanged: State transition
                OutboxEventInsert::for_job(
                    job_id.0,
                    "JobStatusChanged".to_string(),
                    serde_json::json!({
                        "job_id": job_id.0.to_string(),
                        "old_state": format!("{:?}", old_state),
                        "new_state": "Running",
                        "occurred_at": now.to_rfc3339(),
                        "correlation_id": correlation_id,
                        "actor": actor
                    }),
                    Some(serde_json::json!({
                        "correlation_id": correlation_id,
                        "actor": actor,
                        "source": "WorkerHandler"
                    })),
                    Some(format!("{}-status-changed", idempotency_base)),
                ),
            ];

            if let Err(e) = outbox_repo.insert_events(&events).await {
                // Log error but don't fail - state is already updated
                warn!(
                    "Failed to insert events into outbox for job {}: {:?}",
                    job_id.0, e
                );
            } else {
                info!(
                    "📦 Inserted {} outbox events for job {} acknowledgment",
                    events.len(),
                    job_id.0
                );
            }
        } else if let Some(event_bus) = &self.event_bus {
            // Legacy: Direct event publishing (fallback when outbox not configured)
            let job_accepted = DomainEvent::JobAccepted {
                job_id: job_id.clone(),
                worker_id: worker_uuid.clone(),
                occurred_at: now,
                correlation_id: correlation_id.clone(),
                actor: actor.clone(),
            };
            if let Err(e) = event_bus.publish(&job_accepted).await {
                warn!("Failed to publish JobAccepted event: {}", e);
            }

            let dispatch_ack = DomainEvent::JobDispatchAcknowledged {
                job_id: job_id.clone(),
                worker_id: worker_uuid.clone(),
                acknowledged_at: now,
                correlation_id: correlation_id.clone(),
                actor: actor.clone(),
            };
            if let Err(e) = event_bus.publish(&dispatch_ack).await {
                warn!("Failed to publish JobDispatchAcknowledged event: {}", e);
            }

            let status_changed = DomainEvent::JobStatusChanged {
                job_id: job_id.clone(),
                old_state,
                new_state: JobState::Running,
                occurred_at: now,
                correlation_id,
                actor,
            };
            if let Err(e) = event_bus.publish(&status_changed).await {
                warn!(
                    "Failed to publish JobStatusChanged event in on_job_acknowledged: {}",
                    e
                );
            }
        }

        Ok(())
    }

    async fn on_worker_heartbeat(
        &self,
        worker_id: &str,
        running_job_ids: Vec<String>,
    ) -> Result<(), Status> {
        // GAP-GO-01: Delegate to HeartbeatProcessor if available
        if let Some(ref processor) = self.heartbeat_processor {
            debug!(
                "GAP-GO-01: Routing heartbeat through HeartbeatProcessor for {}",
                worker_id
            );
            processor
                .process_heartbeat(worker_id, running_job_ids)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
            return Ok(());
        }

        // Legacy path: direct processing
        let worker_id = Self::parse_worker_uuid(worker_id)?;

        // EPIC-42: Route through Actor if enabled for lock-free heartbeat processing
        if let Some(ref supervisor) = self.supervisor_handle {
            debug!("EPIC-42: Routing heartbeat through Actor for {}", worker_id);

            if let Err(e) = supervisor.heartbeat(&worker_id).await {
                warn!(
                    "EPIC-42: Failed to route heartbeat through Actor, falling back to legacy: {:?}",
                    e
                );
            } else {
                debug!("EPIC-42: Heartbeat routed through Actor successfully");
                return Ok(());
            }
        }

        // Legacy path: direct registry update
        let Some(registry) = self.worker_registry() else {
            return Ok(());
        };

        registry
            .heartbeat(&worker_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let worker = registry
            .get(&worker_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Worker not found in registry"))?;

        if matches!(worker.state(), WorkerState::Creating) {
            registry
                .update_state(&worker_id, WorkerState::Ready)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

            // Publicar evento WorkerStatusChanged
            if let Some(event_bus) = &self.event_bus {
                let event = DomainEvent::WorkerStatusChanged {
                    worker_id: worker.id().clone(),
                    old_status: WorkerState::Creating, // Crash-Only: viene de Creating
                    new_status: WorkerState::Ready,
                    occurred_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                };
                if let Err(e) = event_bus.publish(&event).await {
                    warn!("Failed to publish WorkerStatusChanged event: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Genera un nuevo OTP token para un worker
    pub async fn generate_otp(&self, worker_id: &str) -> Result<String, Status> {
        let worker_id = Self::parse_worker_uuid(worker_id)?;

        if let Some(store) = self.token_store() {
            let token = store
                .issue(&worker_id, std::time::Duration::from_secs(300))
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
            return Ok(token.to_string());
        }

        let token = uuid::Uuid::new_v4().to_string();
        let otp = InMemoryOtpState {
            token: token.clone(),
            worker_id: worker_id.to_string(),
            created_at: std::time::Instant::now(),
            used: false,
        };
        // EPIC-42: DashMap permite escritura concurrente sin bloqueos
        self.otp_tokens.insert(token.clone(), otp);
        Ok(token)
    }

    /// Valida un OTP token
    ///
    /// En modo desarrollo (HODEI_DEV_MODE=1), acepta tokens con prefijo "dev-"
    /// Validate OTP token (PRD v6.0)
    /// Returns worker_id
    async fn validate_otp(
        &self,
        token: &str,
        worker_id_from_request: &str,
    ) -> Result<String, Status> {
        let _worker_id = Self::parse_worker_uuid(worker_id_from_request)?;

        // Modo desarrollo: solo en debug builds (nunca en release)
        let dev_mode =
            cfg!(debug_assertions) && std::env::var("HODEI_DEV_MODE").unwrap_or_default() == "1";

        if dev_mode && (token.is_empty() || token.starts_with("dev-")) {
            warn!(
                "Development mode: accepting token for worker {}",
                worker_id_from_request
            );
            return Ok(worker_id_from_request.to_string());
        }

        if let Some(store) = self.token_store() {
            let worker_id = Self::parse_worker_uuid(worker_id_from_request)?;
            let token = token.parse::<OtpToken>().map_err(
                |e: hodei_server_domain::shared_kernel::DomainError| {
                    Status::unauthenticated(e.to_string())
                },
            )?;
            store
                .consume(&token, &worker_id)
                .await
                .map_err(|e| Status::unauthenticated(e.to_string()))?;
            return Ok(worker_id_from_request.to_string());
        }

        // EPIC-42: DashMap entry API para modificación atómica
        let mut otp =
            self.otp_tokens
                .entry(token.to_string())
                .or_insert_with(|| InMemoryOtpState {
                    token: token.to_string(),
                    worker_id: String::new(),
                    created_at: std::time::Instant::now(),
                    used: true, // Mark as used initially if not found
                });

        // Token expira en 5 minutos
        if otp.created_at.elapsed() > std::time::Duration::from_secs(300) {
            self.otp_tokens.remove(token);
            return Err(Status::unauthenticated("OTP token expired"));
        }

        if otp.used {
            return Err(Status::unauthenticated("OTP token already used"));
        }

        otp.used = true;
        Ok(otp.worker_id.clone())
    }

    /// Genera un session ID para reconexiones
    fn generate_session_id() -> String {
        format!("sess_{}", uuid::Uuid::new_v4())
    }

    /// Envía un mensaje a un worker específico
    ///
    /// BUG-001 FIX: Handles race condition where worker is registered but hasn't sent
    /// first heartbeat yet. Uses a waiting mechanism with timeout.
    pub async fn send_to_worker(
        &self,
        worker_id: &str,
        message: ServerMessage,
    ) -> Result<(), Status> {
        // Try DashMap first (fast path for connected workers)
        if let Some(sender) = self.worker_channels.get(worker_id) {
            return sender
                .send(Ok(message))
                .await
                .map_err(|e| Status::internal(format!("Failed to send message to worker: {}", e)));
        }

        // BUG-001 FIX: Worker not in DashMap, check Actor for registered workers
        // If worker is registered but not yet connected, wait for connection with timeout
        if let Some(ref supervisor) = self.supervisor_handle {
            // Parse worker_id string to WorkerId type
            let worker_id_obj = WorkerId::from_string(worker_id)
                .ok_or_else(|| Status::invalid_argument("Invalid worker ID format"))?;

            match supervisor.get_worker(&worker_id_obj).await {
                Ok(_actor_state) => {
                    // Worker is registered in Actor but stream not yet connected
                    // Wait for the worker to connect (send first heartbeat)
                    info!(
                        "BUG-001 FIX: Worker {} is registered but not yet connected. Waiting for connection...",
                        worker_id
                    );

                    // Wait up to 5 seconds for worker to connect
                    let max_wait = std::time::Duration::from_secs(5);
                    let start = std::time::Instant::now();

                    while start.elapsed() < max_wait {
                        // Check if worker connected (now in DashMap)
                        if let Some(sender) = self.worker_channels.get(worker_id) {
                            info!(
                                "BUG-001 FIX: Worker {} connected, sending message",
                                worker_id
                            );
                            return sender.send(Ok(message)).await.map_err(|e| {
                                Status::internal(format!("Failed to send message: {}", e))
                            });
                        }
                        // Wait a bit before checking again
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }

                    // Timeout - worker didn't connect in time
                    return Err(Status::deadline_exceeded(format!(
                        "Worker {} registered but failed to connect within timeout",
                        worker_id
                    )));
                }
                Err(e) => {
                    // Worker not found in Actor either - return not found
                    let error_msg = e.to_string();
                    if error_msg.contains("not found") || error_msg.contains("NotFound") {
                        return Err(Status::not_found(format!(
                            "Worker {} not registered",
                            worker_id
                        )));
                    }
                    // Other error from Actor
                    return Err(Status::internal(format!("Actor error: {}", error_msg)));
                }
            }
        }

        // No supervisor and not in DashMap - worker not found
        Err(Status::not_found(format!(
            "Worker {} not connected",
            worker_id
        )))
    }
}

#[tonic::async_trait]
impl WorkerAgentService for WorkerAgentServiceImpl {
    /// PRD v6.0: Register con OTP token authentication
    async fn register(
        &self,
        request: Request<RegisterWorkerRequest>,
    ) -> Result<Response<RegisterWorkerResponse>, Status> {
        let ctx = request.get_context();
        let req = request.into_inner();

        let worker_info = req
            .worker_info
            .ok_or_else(|| Status::invalid_argument("worker_info is required"))?;

        // Obtener worker_id del request para validación
        let worker_id_from_request = worker_info
            .worker_id
            .clone()
            .map(|id| id.value)
            .unwrap_or_else(|| format!("worker-{}", uuid::Uuid::new_v4()));

        info!(
            "🔐 WorkerAgentService::register: Worker {} attempting to register",
            worker_id_from_request
        );
        info!(
            "🔐 WorkerAgentService::register: Auth token provided: {}, Session ID: {}",
            if req.auth_token.is_empty() {
                "NONE"
            } else {
                "YES"
            },
            if req.session_id.is_empty() {
                "NONE"
            } else {
                &req.session_id
            }
        );

        // Check for session recovery FIRST (before OTP validation)
        let session_id_req = req.session_id.clone();
        let (worker_id, session_id, needs_otp_validation, is_reconnection, recovery_failed) =
            if !session_id_req.is_empty() {
                // Attempt to recover existing session
                if let Some(existing) = self.workers.get(&worker_id_from_request) {
                    if existing.session_id == session_id_req {
                        // Valid session found - skip OTP validation
                        info!(
                            "✅ WorkerAgentService::register: Session recovery SUCCESSFUL for worker {}",
                            worker_id_from_request
                        );
                        (
                            worker_id_from_request.clone(),
                            session_id_req.clone(),
                            false,
                            true,
                            false,
                        )
                    } else {
                        // Session mismatch - need OTP validation
                        info!(
                            "⚠️ WorkerAgentService::register: Session mismatch for worker {} (req={}, current={})",
                            worker_id_from_request, session_id_req, existing.session_id
                        );
                        (
                            worker_id_from_request.clone(),
                            Self::generate_session_id(),
                            true,
                            false,
                            true,
                        )
                    }
                } else {
                    // Worker not found in memory - need OTP validation
                    info!(
                        "⚠️ WorkerAgentService::register: Worker {} not found in memory, requiring OTP validation",
                        worker_id_from_request
                    );
                    (
                        worker_id_from_request.clone(),
                        Self::generate_session_id(),
                        true,
                        false,
                        true,
                    )
                }
            } else {
                // New registration - need OTP validation
                (
                    worker_id_from_request.clone(),
                    Self::generate_session_id(),
                    true,
                    false,
                    false,
                )
            };

        // Only validate OTP if needed (no valid session found)
        if needs_otp_validation {
            info!(
                "🔐 WorkerAgentService::register: Validating OTP token for worker {}...",
                worker_id
            );
            let validated_worker_id = self.validate_otp(&req.auth_token, &worker_id).await?;

            info!(
                "✅ WorkerAgentService::register: OTP validation SUCCESSFUL for worker {}",
                validated_worker_id
            );

            // Extract job_id from request (passed by worker via env var)
            let job_id_from_request = if let Some(ref jid_str) = req.job_id {
                if !jid_str.is_empty() {
                    match Self::parse_job_uuid(jid_str) {
                        Ok(jid) => {
                            info!("📋 Extracted job_id from request: {}", jid);
                            Some(jid)
                        }
                        Err(e) => {
                            warn!("Invalid job_id format in request: {}", e);
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            info!(
                "✅ WorkerAgentService::register: Worker {} validated, proceeding with registration (job_id: {:?})",
                validated_worker_id, job_id_from_request
            );

            // Pass worker_info and job_id for JIT registration if needed
            self.on_worker_registered(&validated_worker_id, &worker_info, job_id_from_request)
                .await?;
        } else {
            info!(
                "✅ WorkerAgentService::register: Skipping OTP validation for worker {} (valid session)",
                worker_id
            );
        }

        if is_reconnection {
            info!(
                "Worker {} reconnected with session {}",
                worker_id, session_id
            );
            // Update status if needed (e.g. from Disconnected -> Ready)
            // For now we just refresh the entry. The heartbeat will set it to Ready.

            // Publish WorkerReconnected event
            if let Some(event_bus) = &self.event_bus {
                let event = DomainEvent::WorkerReconnected {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(
                        uuid::Uuid::parse_str(&worker_id).unwrap_or_default(),
                    ),
                    session_id: session_id.clone(),
                    occurred_at: Utc::now(),
                    correlation_id: ctx.correlation_id_owned(),
                    actor: ctx.actor_owned(),
                };
                if let Err(e) = event_bus.publish(&event).await {
                    warn!("Failed to publish WorkerReconnected event: {}", e);
                }
            }
        } else {
            // New Session or Recovery Failed
            info!(
                "Worker {} registered with new session {}",
                worker_id, session_id
            );

            let registered = RegisteredWorker {
                info: worker_info,
                session_id: session_id.clone(),
                status: 0,
            };

            self.workers.insert(worker_id.clone(), registered);

            if let Some(event_bus) = &self.event_bus {
                if recovery_failed {
                    let event = DomainEvent::WorkerRecoveryFailed {
                        worker_id: hodei_server_domain::shared_kernel::WorkerId(
                            uuid::Uuid::parse_str(&worker_id).unwrap_or_default(),
                        ),
                        invalid_session_id: session_id_req, // The one requested that failed
                        occurred_at: Utc::now(),
                        correlation_id: ctx.correlation_id_owned(),
                        actor: ctx.actor_owned(),
                    };
                    if let Err(e) = event_bus.publish(&event).await {
                        warn!("Failed to publish WorkerRecoveryFailed event: {}", e);
                    }
                }

                // Always publish Registered for new sessions (consistent with current design)
                // Or should we only publish Registered for clean starts?
                // The requirements imply we want to track "Recovery Failed" distinct from "Registered".
                // Let's keep publishing Registered so systems relying on it to know a worker is UP still work.

                // Get provider_id from registry to ensure correct provider_id in event
                let provider_id = if let Some(registry) = self.worker_registry() {
                    if let Ok(Some(worker)) = registry
                        .get(&WorkerId(
                            uuid::Uuid::parse_str(&worker_id).unwrap_or_default(),
                        ))
                        .await
                    {
                        Some(worker.handle().provider_id.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };

                let event = DomainEvent::WorkerRegistered {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(
                        uuid::Uuid::parse_str(&worker_id).unwrap_or_default(),
                    ),
                    provider_id: provider_id
                        .unwrap_or_else(|| hodei_server_domain::shared_kernel::ProviderId::new()),
                    occurred_at: Utc::now(),
                    correlation_id: ctx.correlation_id_owned(),
                    actor: ctx.actor_owned(),
                };
                if let Err(e) = event_bus.publish(&event).await {
                    warn!("Failed to publish WorkerRegistered event: {}", e);
                }
            }
        }

        Ok(Response::new(RegisterWorkerResponse {
            worker_id: Some(hodei_jobs::WorkerId { value: worker_id }),
            success: true,
            message: if is_reconnection {
                "Worker reconnected successfully".to_string()
            } else {
                "Worker registered successfully".to_string()
            },
            session_id: session_id.clone(),
            registration_time: Some(prost_types::Timestamp::from(std::time::SystemTime::now())),
        }))
    }

    /// PRD v6.0: WorkerStream - Canal bidireccional principal
    type WorkerStreamStream = Pin<Box<dyn Stream<Item = Result<ServerMessage, Status>> + Send>>;

    async fn worker_stream(
        &self,
        request: Request<Streaming<WorkerMessage>>,
    ) -> Result<Response<Self::WorkerStreamStream>, Status> {
        let mut inbound = request.into_inner();
        let _workers = self.workers.clone();
        let worker_channels = self.worker_channels.clone();
        let log_service = self.log_service.clone();

        // Canal para enviar mensajes al worker (Result para compatibilidad con tonic)
        let (tx, rx) = mpsc::channel::<Result<ServerMessage, Status>>(100);

        // Variable para trackear el worker_id de esta conexión
        let worker_id_holder: Arc<TokioMutex<Option<String>>> = Arc::new(TokioMutex::new(None));
        let worker_id_for_cleanup = worker_id_holder.clone();
        let worker_id_for_result = worker_id_holder.clone();

        // Spawn task para procesar mensajes entrantes
        let tx_clone = tx.clone();
        let registry_service = self.clone();
        tokio::spawn(async move {
            while let Some(message_result) = inbound.next().await {
                info!("🔍 Received message from worker stream");
                match message_result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload {
                            info!(
                                "📦 Worker payload type: {:?}",
                                std::mem::discriminant(&payload)
                            );
                            match payload {
                                WorkerPayload::Heartbeat(hb) => {
                                    if let Some(ref wid) = hb.worker_id {
                                        // Actualizar worker_id si es el primer mensaje
                                        let mut holder = worker_id_holder.lock().await;
                                        if holder.is_none() {
                                            *holder = Some(wid.value.clone());
                                            // EPIC-42: DashMap para inserción concurrente
                                            worker_channels
                                                .insert(wid.value.clone(), tx_clone.clone());
                                        }

                                        // Enviar ACK
                                        let ack = ServerMessage {
                                            payload: Some(ServerPayload::Ack(AckMessage {
                                                message_id: uuid::Uuid::new_v4().to_string(),
                                                success: true,
                                                worker_id: wid.value.clone(),
                                            })),
                                        };
                                        let _ = tx_clone.send(Ok(ack)).await;

                                        // GAP-GO-01: Pass running_job_ids to heartbeat processor
                                        let _ = registry_service
                                            .on_worker_heartbeat(
                                                &wid.value,
                                                hb.running_job_ids.clone(),
                                            )
                                            .await;

                                        // EPIC-29: Publish WorkerHeartbeat event
                                        if let Some(ref event_bus) = registry_service.event_bus {
                                            let worker_id_obj = WorkerId::from(wid.value.clone());

                                            let heartbeat_event = DomainEvent::WorkerHeartbeat {
                                                worker_id: worker_id_obj,
                                                state: WorkerState::Ready,
                                                load_average: None,
                                                memory_usage_mb: None,
                                                current_job_id: if !hb.running_job_ids.is_empty() {
                                                    hb.running_job_ids.first().map(|id| {
                                                        JobId(
                                                            uuid::Uuid::parse_str(id)
                                                                .unwrap_or_default(),
                                                        )
                                                    })
                                                } else {
                                                    None
                                                },
                                                occurred_at: Utc::now(),
                                                correlation_id: None,
                                                actor: Some("worker:heartbeat".to_string()),
                                            };

                                            if let Err(e) =
                                                event_bus.publish(&heartbeat_event).await
                                            {
                                                error!(
                                                    "Failed to publish WorkerHeartbeat event: {}",
                                                    e
                                                );
                                            }
                                        }
                                    }
                                }
                                WorkerPayload::Log(log) => {
                                    // GAP-GO-01: Delegate to LogIngestor for dedicated log processing
                                    // NOTE: Logs are NOT logged to server console to avoid scalability issues
                                    // with 100+ workers. Logs are streamed to clients and persisted to storage.
                                    let entry = LogEntry {
                                        job_id: log.job_id.clone(),
                                        line: log.line,
                                        is_stderr: log.is_stderr,
                                        timestamp: log.timestamp,
                                    };

                                    if let Some(ref ingestor) = registry_service.log_ingestor {
                                        let _ = ingestor.ingest_log(entry).await;
                                    } else if let Some(ref svc) = log_service {
                                        svc.append_log(entry).await;
                                    }
                                }
                                WorkerPayload::LogBatch(batch) => {
                                    // GAP-GO-01: Delegate to LogIngestor for efficient batch processing
                                    info!(
                                        "Received log batch for job {}: {} entries",
                                        batch.job_id,
                                        batch.entries.len()
                                    );

                                    // GAP-GO-01: Use LogIngestor for efficient batch processing
                                    if let Some(ref ingestor) = registry_service.log_ingestor {
                                        // Convert proto entries to LogEntry
                                        let entries: Vec<LogEntry> = batch
                                            .entries
                                            .into_iter()
                                            .map(|e| LogEntry {
                                                job_id: e.job_id,
                                                line: e.line,
                                                is_stderr: e.is_stderr,
                                                timestamp: e.timestamp,
                                            })
                                            .collect();
                                        let batch = crate::grpc::log_ingestor::LogBatch::new(
                                            batch.job_id,
                                            entries,
                                        );
                                        let _ = ingestor.ingest_log_batch(batch).await;
                                    } else if let Some(ref svc) = log_service {
                                        for entry in batch.entries {
                                            let log_entry = LogEntry {
                                                job_id: entry.job_id,
                                                line: entry.line,
                                                is_stderr: entry.is_stderr,
                                                timestamp: entry.timestamp,
                                            };
                                            svc.append_log(log_entry).await;
                                        }
                                    }
                                }
                                WorkerPayload::Result(result) => {
                                    info!(
                                        "✅ Job {} completed: exit_code={}, success={}",
                                        result.job_id, result.exit_code, result.success
                                    );

                                    if let Some(wid) = worker_id_for_result.lock().await.clone() {
                                        if let Err(e) =
                                            registry_service.on_job_result(&wid, &result).await
                                        {
                                            error!("Failed to persist job result: {}", e);
                                        }
                                    }

                                    // GAP-GO-01: Finalize logs using LogIngestor or legacy path
                                    if let Some(ref ingestor) = registry_service.log_ingestor {
                                        match ingestor.finalize_job_logs(&result.job_id).await {
                                            Ok(result) => {
                                                if result.persisted {
                                                    info!(
                                                        "✅ Job {} log finalized and persisted: {} bytes",
                                                        result.job_id, result.size_bytes
                                                    );
                                                } else {
                                                    info!(
                                                        "✅ Job {} completed (no persistent log)",
                                                        result.job_id
                                                    );
                                                }
                                            }
                                            Err(e) => {
                                                warn!(
                                                    "⚠️ Failed to finalize log for job {}: {}",
                                                    result.job_id, e
                                                );
                                            }
                                        }
                                    } else if let Some(ref svc) = log_service {
                                        match svc.finalize_job_log(&result.job_id).await {
                                            Ok(Some(log_ref)) => {
                                                info!(
                                                    "✅ Job {} log finalized and persisted: {} bytes",
                                                    result.job_id, log_ref.size_bytes
                                                );
                                                // TODO: Store log_ref in database via callback
                                            }
                                            Ok(None) => {
                                                info!(
                                                    "✅ Job {} completed (no persistent log)",
                                                    result.job_id
                                                );
                                            }
                                            Err(e) => {
                                                warn!(
                                                    "⚠️ Failed to finalize log for job {}: {}",
                                                    result.job_id, e
                                                );
                                            }
                                        }
                                    }
                                }
                                WorkerPayload::Stats(stats) => {
                                    info!(
                                        "Worker stats: cpu={}%, mem={} bytes",
                                        stats.cpu_percent, stats.memory_bytes
                                    );
                                }
                                WorkerPayload::Status(status) => {
                                    info!(
                                        "Worker status: {:?}, reason: {}",
                                        status.status, status.reason
                                    );
                                }
                                WorkerPayload::Ack(ack) => {
                                    info!(
                                        "📬 Received acknowledgment: message_id={}, success={}, worker_id={}",
                                        ack.message_id, ack.success, ack.worker_id
                                    );

                                    // If this is a job acknowledgment, update job state to RUNNING
                                    if ack.message_id.starts_with("job-") {
                                        let job_id = ack.message_id.trim_start_matches("job-");
                                        let worker_id = ack.worker_id;

                                        // Publish RunJobReceived event (worker confirmed receipt)
                                        if let (Some(event_bus), Some(job_repo)) = (
                                            &registry_service.event_bus,
                                            &registry_service.job_repository,
                                        ) {
                                            if let Ok(job_uuid) = uuid::Uuid::parse_str(job_id) {
                                                let job_id_parsed =
                                                    hodei_server_domain::shared_kernel::JobId(
                                                        job_uuid,
                                                    );
                                                if let Ok(Some(job)) =
                                                    job_repo.find_by_id(&job_id_parsed).await
                                                {
                                                    let correlation_id = job
                                                        .metadata()
                                                        .get("correlation_id")
                                                        .cloned();
                                                    let actor =
                                                        job.metadata().get("actor").cloned();

                                                    let run_received_event = DomainEvent::RunJobReceived {
                                                    job_id: job.id.clone(),
                                                    worker_id: hodei_server_domain::shared_kernel::WorkerId::from_string(&worker_id).unwrap_or_default(),
                                                    received_at: Utc::now(),
                                                    correlation_id,
                                                    actor,
                                                };

                                                    if let Err(e) =
                                                        event_bus.publish(&run_received_event).await
                                                    {
                                                        warn!(
                                                            "Failed to publish RunJobReceived event: {}",
                                                            e
                                                        );
                                                    } else {
                                                        info!(
                                                            "📢 RunJobReceived event published for job {}",
                                                            job_id
                                                        );
                                                    }
                                                }
                                            }
                                        }

                                        info!(
                                            "🔄 Processing job acknowledgment for {} (from worker {})",
                                            job_id, worker_id
                                        );

                                        if !worker_id.is_empty() {
                                            if let Err(e) = registry_service
                                                .on_job_acknowledged(&worker_id, job_id)
                                                .await
                                            {
                                                error!(
                                                    "❌ Failed to update job state on acknowledgment: {}",
                                                    e
                                                );
                                            } else {
                                                info!("✅ Job {} acknowledgment processed", job_id);
                                            }
                                        } else {
                                            warn!("Acknowledgment received without worker_id");
                                        }
                                    }
                                }
                                WorkerPayload::SelfTerminate(term) => {
                                    info!(
                                        "🛑 Worker {} self-terminating: last_job={}, waited={}ms (expected={}ms), reason={}",
                                        term.worker_id,
                                        term.last_job_id,
                                        term.actual_wait_ms,
                                        term.expected_cleanup_ms,
                                        term.reason
                                    );

                                    // Parse worker_id
                                    if let Ok(worker_uuid) = uuid::Uuid::parse_str(&term.worker_id)
                                    {
                                        let worker_id_obj = WorkerId(worker_uuid);
                                        let provider_id_obj = ProviderId::new(); // Will be resolved from registry

                                        // Parse last_job_id if present
                                        let last_job_id_obj = if !term.last_job_id.is_empty() {
                                            term.last_job_id.parse::<uuid::Uuid>().ok().map(JobId)
                                        } else {
                                            None
                                        };

                                        // Publish WorkerSelfTerminated event
                                        if let Some(ref event_bus) = registry_service.event_bus {
                                            let event = DomainEvent::WorkerSelfTerminated {
                                                worker_id: worker_id_obj.clone(),
                                                provider_id: provider_id_obj,
                                                last_job_id: last_job_id_obj,
                                                expected_cleanup_ms: term.expected_cleanup_ms,
                                                actual_wait_ms: term.actual_wait_ms,
                                                worker_state: WorkerState::Terminated, // Crash-Only: no hay Draining
                                                occurred_at: Utc::now(),
                                                correlation_id: None,
                                                actor: Some("worker:self_terminate".to_string()),
                                            };

                                            if let Err(e) = event_bus.publish(&event).await {
                                                error!(
                                                    "Failed to publish WorkerSelfTerminated event: {}",
                                                    e
                                                );
                                            } else {
                                                info!(
                                                    "📢 WorkerSelfTerminated event published for worker {}",
                                                    term.worker_id
                                                );
                                            }
                                        }

                                        // EPIC-42: DashMap para eliminación concurrente
                                        worker_channels.remove(&term.worker_id);

                                        // Note: The actual worker termination (cleanup) is handled by the provider
                                        // when it detects the worker process has exited
                                        warn!(
                                            "Worker {} initiated self-termination (expected after {}ms, actual wait: {}ms)",
                                            term.worker_id,
                                            term.expected_cleanup_ms,
                                            term.actual_wait_ms
                                        );
                                    } else {
                                        warn!(
                                            "Invalid worker_id in SelfTerminate message: {}",
                                            term.worker_id
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Stream error: {}", e);
                        break;
                    }
                }
            }

            // Cleanup cuando el worker se desconecta
            if let Some(wid) = worker_id_for_cleanup.lock().await.clone() {
                warn!("Worker {} disconnected", wid);
                // EPIC-42: DashMap para eliminación concurrente sin bloqueos
                worker_channels.remove(&wid);

                // Publish WorkerDisconnected event
                if let Some(event_bus) = &registry_service.event_bus {
                    let worker_id = hodei_server_domain::shared_kernel::WorkerId(
                        uuid::Uuid::parse_str(&wid).unwrap_or_default(),
                    );
                    let event = DomainEvent::WorkerDisconnected {
                        worker_id,
                        last_heartbeat: None,
                        occurred_at: Utc::now(),
                        correlation_id: None,
                        actor: None,
                    };
                    if let Err(e) = event_bus.publish(&event).await {
                        warn!("Failed to publish WorkerDisconnected event: {}", e);
                    }
                }
            }
        });

        // Stream de salida
        let output = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output)))
    }

    /// Legacy: Update worker status
    async fn update_worker_status(
        &self,
        request: Request<UpdateWorkerStatusRequest>,
    ) -> Result<Response<UpdateWorkerStatusResponse>, Status> {
        let req = request.into_inner();
        let worker_id = req
            .worker_id
            .ok_or_else(|| Status::invalid_argument("worker_id is required"))?
            .value;

        // EPIC-42: DashMap para acceso concurrente sin bloqueos
        if let Some(mut worker) = self.workers.get_mut(&worker_id) {
            worker.status = req.status;
            Ok(Response::new(UpdateWorkerStatusResponse {
                success: true,
                message: "Status updated".to_string(),
                timestamp: None,
            }))
        } else {
            Err(Status::not_found(format!("Worker {} not found", worker_id)))
        }
    }

    /// Legacy: Unregister worker
    async fn unregister_worker(
        &self,
        request: Request<UnregisterWorkerRequest>,
    ) -> Result<Response<UnregisterWorkerResponse>, Status> {
        let req = request.into_inner();
        let worker_id = req
            .worker_id
            .ok_or_else(|| Status::invalid_argument("worker_id is required"))?
            .value;

        info!("Unregistering worker: {}", worker_id);

        // EPIC-42: DashMap para remoción concurrente sin bloqueos
        let removed = self.workers.remove(&worker_id);
        self.worker_channels.remove(&worker_id);

        // Publish WorkerTerminated event if worker was found
        if removed.is_some() {
            if let Some(event_bus) = &self.event_bus {
                let wid = hodei_server_domain::shared_kernel::WorkerId(
                    uuid::Uuid::parse_str(&worker_id).unwrap_or_default(),
                );
                let event = DomainEvent::WorkerTerminated {
                    worker_id: wid,
                    provider_id: hodei_server_domain::shared_kernel::ProviderId::new(),
                    reason: hodei_server_domain::events::TerminationReason::Unregistered,
                    occurred_at: Utc::now(),
                    correlation_id: None,
                    actor: None,
                };
                if let Err(e) = event_bus.publish(&event).await {
                    warn!("Failed to publish WorkerTerminated event: {}", e);
                }
            }
        }

        Ok(Response::new(UnregisterWorkerResponse {
            success: removed.is_some(),
            message: if removed.is_some() {
                "Unregistered".to_string()
            } else {
                "Not found".to_string()
            },
            jobs_migrated: 0,
        }))
    }
}

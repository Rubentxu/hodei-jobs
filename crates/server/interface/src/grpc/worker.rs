//! Worker Agent gRPC Service Implementation (PRD v6.0 Aligned)
//!
//! Implements the WorkerAgentService with:
//! - Register RPC with OTP token authentication
//! - WorkerStream bidirectional stream for Worker↔Server communication

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status, Streaming};
use tracing::{error, info, warn};

use hodei_jobs::{
    AckMessage, LogEntry, RegisterWorkerRequest, RegisterWorkerResponse, ServerMessage,
    UnregisterWorkerRequest, UnregisterWorkerResponse, UpdateWorkerStatusRequest,
    UpdateWorkerStatusResponse, WorkerInfo, WorkerMessage,
    server_message::Payload as ServerPayload, worker_agent_service_server::WorkerAgentService,
    worker_message::Payload as WorkerPayload,
};

use crate::grpc::interceptors::RequestContextExt;
use crate::grpc::log_stream::LogStreamService;
use chrono::Utc;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::iam::OtpToken;
use hodei_server_domain::outbox::{OutboxError, OutboxEventInsert, OutboxRepository};
use hodei_server_domain::shared_kernel::{
    DomainError, JobId, JobResult, JobState, ProviderId, WorkerId, WorkerState,
};
use hodei_server_domain::workers::registry::WorkerRegistry;

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
#[derive(Clone)]
pub struct WorkerAgentServiceImpl {
    workers: Arc<RwLock<HashMap<String, RegisteredWorker>>>,
    otp_tokens: Arc<RwLock<HashMap<String, InMemoryOtpState>>>,
    /// Channel para enviar comandos a workers conectados
    worker_channels: Arc<RwLock<HashMap<String, mpsc::Sender<Result<ServerMessage, Status>>>>>,
    worker_registry: Option<Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>>,
    job_repository: Option<Arc<dyn hodei_server_domain::jobs::JobRepository>>,
    token_store: Option<Arc<dyn hodei_server_domain::iam::WorkerBootstrapTokenStore>>,
    /// Channel para log streaming
    log_service: Option<Arc<LogStreamService>>,
    /// Event Bus para publicar eventos de dominio (legacy, used by OutboxRelay)
    event_bus: Option<Arc<dyn hodei_server_domain::event_bus::EventBus>>,
    /// Outbox Repository para patrón Transactional Outbox
    outbox_repository: Option<Arc<dyn OutboxRepository<Error = OutboxError> + Send + Sync>>,
}

impl Default for WorkerAgentServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}
impl WorkerAgentServiceImpl {
    pub fn new() -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            otp_tokens: Arc::new(RwLock::new(HashMap::new())),
            worker_channels: Arc::new(RwLock::new(HashMap::new())),
            worker_registry: None,
            job_repository: None,
            token_store: None,
            log_service: None,
            event_bus: None,
            outbox_repository: None,
        }
    }

    pub fn with_registry(
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
        event_bus: Arc<dyn hodei_server_domain::event_bus::EventBus>,
    ) -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            otp_tokens: Arc::new(RwLock::new(HashMap::new())),
            worker_channels: Arc::new(RwLock::new(HashMap::new())),
            worker_registry: Some(worker_registry),
            job_repository: None,
            token_store: None,
            log_service: None,
            event_bus: Some(event_bus),
            outbox_repository: None,
        }
    }

    pub fn with_registry_and_log_service(
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
        log_service: Arc<LogStreamService>,
        event_bus: Arc<dyn hodei_server_domain::event_bus::EventBus>,
    ) -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            otp_tokens: Arc::new(RwLock::new(HashMap::new())),
            worker_channels: Arc::new(RwLock::new(HashMap::new())),
            worker_registry: Some(worker_registry),
            job_repository: None,
            token_store: None,
            log_service: Some(log_service),
            event_bus: Some(event_bus),
            outbox_repository: None,
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
        outbox_repository: Arc<dyn OutboxRepository<Error = OutboxError> + Send + Sync>,
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
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            otp_tokens: Arc::new(RwLock::new(HashMap::new())),
            worker_channels: Arc::new(RwLock::new(HashMap::new())),
            worker_registry: Some(worker_registry),
            job_repository: Some(job_repository),
            token_store: None,
            log_service: Some(log_service),
            event_bus: Some(event_bus),
            outbox_repository: None,
        }
    }

    pub fn with_registry_job_repository_token_store_and_log_service(
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
        token_store: Arc<dyn hodei_server_domain::iam::WorkerBootstrapTokenStore>,
        log_service: Arc<LogStreamService>,
        event_bus: Arc<dyn hodei_server_domain::event_bus::EventBus>,
    ) -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            otp_tokens: Arc::new(RwLock::new(HashMap::new())),
            worker_channels: Arc::new(RwLock::new(HashMap::new())),
            worker_registry: Some(worker_registry),
            job_repository: Some(job_repository),
            token_store: Some(token_store),
            log_service: Some(log_service),
            event_bus: Some(event_bus),
            outbox_repository: None,
        }
    }

    /// Full constructor with outbox repository support (production-ready)
    pub fn with_all_dependencies(
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
        token_store: Arc<dyn hodei_server_domain::iam::WorkerBootstrapTokenStore>,
        log_service: Arc<LogStreamService>,
        event_bus: Arc<dyn hodei_server_domain::event_bus::EventBus>,
        outbox_repository: Arc<dyn OutboxRepository<Error = OutboxError> + Send + Sync>,
    ) -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            otp_tokens: Arc::new(RwLock::new(HashMap::new())),
            worker_channels: Arc::new(RwLock::new(HashMap::new())),
            worker_registry: Some(worker_registry),
            job_repository: Some(job_repository),
            token_store: Some(token_store),
            log_service: Some(log_service),
            event_bus: Some(event_bus),
            outbox_repository: Some(outbox_repository),
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

    fn outbox_repository(
        &self,
    ) -> Option<&Arc<dyn OutboxRepository<Error = OutboxError> + Send + Sync>> {
        self.outbox_repository.as_ref()
    }

    fn parse_job_uuid(job_id: &str) -> Result<JobId, Status> {
        let id = uuid::Uuid::parse_str(job_id)
            .map_err(|_| Status::invalid_argument("job_id must be a UUID"))?;
        Ok(JobId(id))
    }

    /// Handle worker registration using Transactional Outbox Pattern
    ///
    /// This method:
    /// 1. Updates worker state to Ready in registry
    /// 2. Inserts events into outbox table (if configured)
    /// 3. Falls back to direct event publishing if outbox not configured
    ///
    /// Events emitted:
    /// - WorkerStatusChanged: State transition to Ready
    /// - WorkerReady: Worker availability for job assignment
    async fn on_worker_registered(&self, worker_id: &str) -> Result<(), Status> {
        let Some(registry) = self.worker_registry() else {
            return Ok(());
        };
        let worker_id = Self::parse_worker_uuid(worker_id)?;

        // Worker may not exist in registry if it's registering directly without provisioning
        // This is valid for workers that connect independently (e.g., external workers)
        let worker = match registry.get(&worker_id).await {
            Ok(Some(w)) => w,
            Ok(None) => {
                info!(
                    "Worker {} not in registry, allowing direct registration",
                    worker_id
                );
                return Ok(());
            }
            Err(e) => return Err(Status::internal(e.to_string())),
        };

        // Update state in registry FIRST, then publish event
        // This ensures consistency: when event handlers receive the event,
        // the worker is already in the correct state in the registry
        if matches!(
            worker.state(),
            WorkerState::Creating | WorkerState::Connecting
        ) {
            let old_state = worker.state().clone();
            let now = chrono::Utc::now();
            let provider_id = worker.handle().provider_id.clone();

            // 1. First update state in registry (persistence)
            registry
                .update_state(&worker_id, WorkerState::Ready)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

            // 2. Use Transactional Outbox Pattern if configured
            if let Some(outbox_repo) = self.outbox_repository() {
                let idempotency_base = format!("reg-{}", worker_id.0);

                let events = vec![
                    // WorkerStatusChanged: State transition
                    OutboxEventInsert::for_worker(
                        worker_id.0,
                        "WorkerStatusChanged".to_string(),
                        serde_json::json!({
                            "worker_id": worker_id.0.to_string(),
                            "old_status": format!("{:?}", old_state),
                            "new_status": "Ready",
                            "occurred_at": now.to_rfc3339()
                        }),
                        Some(serde_json::json!({
                            "source": "WorkerHandler"
                        })),
                        Some(format!("{}-status-changed", idempotency_base)),
                    ),
                    // WorkerReady: Availability notification
                    OutboxEventInsert::for_worker(
                        worker_id.0,
                        "WorkerReady".to_string(),
                        serde_json::json!({
                            "worker_id": worker_id.0.to_string(),
                            "provider_id": provider_id.0.to_string(),
                            "ready_at": now.to_rfc3339(),
                            "actor": "worker-agent"
                        }),
                        Some(serde_json::json!({
                            "source": "WorkerHandler",
                            "actor": "worker-agent"
                        })),
                        Some(format!("{}-ready", idempotency_base)),
                    ),
                ];

                if let Err(e) = outbox_repo.insert_events(&events).await {
                    warn!(
                        "Failed to insert events into outbox for worker {}: {:?}",
                        worker_id.0, e
                    );
                } else {
                    info!(
                        "📦 Inserted {} outbox events for worker {} registration",
                        events.len(),
                        worker_id.0
                    );
                }
            } else if let Some(event_bus) = &self.event_bus {
                // Legacy: Direct event publishing (fallback)
                let status_event = DomainEvent::WorkerStatusChanged {
                    worker_id: worker_id.clone(),
                    old_status: old_state,
                    new_status: WorkerState::Ready,
                    occurred_at: now,
                    correlation_id: None,
                    actor: None,
                };

                event_bus.publish(&status_event).await.map_err(|e| {
                    Status::internal(format!(
                        "Failed to publish WorkerStatusChanged event: {}",
                        e
                    ))
                })?;

                let ready_event = DomainEvent::WorkerReady {
                    worker_id: worker_id.clone(),
                    provider_id,
                    ready_at: now,
                    correlation_id: None,
                    actor: Some("worker-agent".to_string()),
                };

                event_bus.publish(&ready_event).await.map_err(|e| {
                    Status::internal(format!("Failed to publish WorkerReady event: {}", e))
                })?;
            }

            info!(
                "Worker {} state updated to Ready and events published",
                worker_id
            );
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
                old_state: JobState::Running, // Asumimos que estaba running
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
            }
        }

        if matches!(
            job.state(),
            JobState::Succeeded | JobState::Failed | JobState::Timeout | JobState::Cancelled
        ) {
            registry
                .release_from_job(&worker_id)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
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

    async fn on_worker_heartbeat(&self, worker_id: &str) -> Result<(), Status> {
        let Some(registry) = self.worker_registry() else {
            return Ok(());
        };
        let worker_id = Self::parse_worker_uuid(worker_id)?;

        registry
            .heartbeat(&worker_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let worker = registry
            .get(&worker_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Worker not found in registry"))?;

        if matches!(
            worker.state(),
            WorkerState::Creating | WorkerState::Connecting
        ) {
            registry
                .update_state(&worker_id, WorkerState::Ready)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

            // Publicar evento WorkerStatusChanged
            if let Some(event_bus) = &self.event_bus {
                let event = DomainEvent::WorkerStatusChanged {
                    worker_id: worker.id().clone(),
                    old_status: WorkerState::Connecting, // Asumimos que venía de Connecting/Creating
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
        self.otp_tokens.write().await.insert(token.clone(), otp);
        Ok(token)
    }

    /// Valida un OTP token
    ///
    /// En modo desarrollo (HODEI_DEV_MODE=1), acepta tokens con prefijo "dev-"
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
            info!(
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

        let mut tokens = self.otp_tokens.write().await;

        let otp = tokens.get_mut(token).ok_or_else(|| {
            if dev_mode {
                warn!("Invalid OTP token in dev mode. Use 'dev-<any>' or set HODEI_DEV_MODE=1");
            }
            Status::unauthenticated("Invalid OTP token")
        })?;

        // Token expira en 5 minutos
        if otp.created_at.elapsed() > std::time::Duration::from_secs(300) {
            tokens.remove(token);
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
    pub async fn send_to_worker(
        &self,
        worker_id: &str,
        message: ServerMessage,
    ) -> Result<(), Status> {
        let channels = self.worker_channels.read().await;
        let sender = channels
            .get(worker_id)
            .ok_or_else(|| Status::not_found(format!("Worker {} not connected", worker_id)))?;

        sender
            .send(Ok(message))
            .await
            .map_err(|e| Status::internal(format!("Failed to send message to worker: {}", e)))
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
                let workers = self.workers.read().await;
                if let Some(existing) = workers.get(&worker_id_from_request) {
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
            let validated_worker_id = match self.validate_otp(&req.auth_token, &worker_id).await {
                Ok(id) => {
                    info!(
                        "✅ WorkerAgentService::register: OTP validation SUCCESSFUL for worker {}",
                        id
                    );
                    id
                }
                Err(e) => {
                    error!(
                        "❌ WorkerAgentService::register: OTP validation FAILED for worker {}: {}",
                        worker_id, e
                    );
                    return Err(e);
                }
            };

            info!(
                "✅ WorkerAgentService::register: Worker {} validated, proceeding with registration",
                validated_worker_id
            );

            self.on_worker_registered(&validated_worker_id).await?;
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

            self.workers
                .write()
                .await
                .insert(worker_id.clone(), registered);

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
                let event = DomainEvent::WorkerRegistered {
                    worker_id: hodei_server_domain::shared_kernel::WorkerId(
                        uuid::Uuid::parse_str(&worker_id).unwrap_or_default(),
                    ),
                    provider_id: hodei_server_domain::shared_kernel::ProviderId::new(),
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
        let worker_id_holder: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(None));
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
                                        let mut holder = worker_id_holder.write().await;
                                        if holder.is_none() {
                                            *holder = Some(wid.value.clone());
                                            // Registrar canal
                                            worker_channels
                                                .write()
                                                .await
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

                                        let _ =
                                            registry_service.on_worker_heartbeat(&wid.value).await;
                                    }
                                }
                                WorkerPayload::Log(log) => {
                                    // Forward to LogStreamService for client subscribers and persistent storage
                                    // NOTE: Logs are NOT logged to server console to avoid scalability issues
                                    // with 100+ workers. Logs are streamed to clients and persisted to storage.
                                    if let Some(ref svc) = log_service {
                                        let entry = LogEntry {
                                            job_id: log.job_id,
                                            line: log.line,
                                            is_stderr: log.is_stderr,
                                            timestamp: log.timestamp,
                                        };
                                        svc.append_log(entry).await;
                                    }
                                }
                                WorkerPayload::LogBatch(batch) => {
                                    // Process batched logs efficiently
                                    info!(
                                        "Received log batch for job {}: {} entries",
                                        batch.job_id,
                                        batch.entries.len()
                                    );

                                    // Forward to LogStreamService for client subscribers
                                    if let Some(ref svc) = log_service {
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

                                    if let Some(wid) = worker_id_for_result.read().await.clone() {
                                        if let Err(e) =
                                            registry_service.on_job_result(&wid, &result).await
                                        {
                                            error!("Failed to persist job result: {}", e);
                                        }
                                    }

                                    // Finalize and persist log file for completed job
                                    if let Some(ref svc) = log_service {
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
            if let Some(wid) = worker_id_for_cleanup.read().await.clone() {
                warn!("Worker {} disconnected", wid);
                worker_channels.write().await.remove(&wid);

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

        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(&worker_id) {
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

        // Remover worker y su canal
        let removed = self.workers.write().await.remove(&worker_id);
        self.worker_channels.write().await.remove(&worker_id);

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

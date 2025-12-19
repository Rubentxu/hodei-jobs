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
use hodei_server_domain::shared_kernel::{
    JobId, JobResult, JobState, ProviderId, WorkerId, WorkerState,
};
use hodei_server_domain::workers::registry::WorkerRegistry;
use hodei_server_domain::workers::{ProviderType, WorkerHandle, WorkerSpec};

#[cfg(test)]
use hodei_server_domain::jobs::{ExecutionContext, Job, JobRepository, JobSpec};

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
    log_service: Option<LogStreamService>,
    /// Event Bus para publicar eventos de dominio
    event_bus: Option<Arc<dyn EventBus>>,
}

impl Default for WorkerAgentServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::jobs::{ExecutionContext, Job, JobSpec};
    use hodei_server_domain::shared_kernel::{ProviderId, WorkerId};
    use hodei_server_domain::workers::{ProviderType, WorkerHandle, WorkerSpec};
    use hodei_server_infrastructure::persistence::{DatabaseConfig, PostgresWorkerRegistry};
    use hodei_server_infrastructure::repositories::{
        InMemoryJobRepository, InMemoryWorkerRegistry,
    };
    use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
    use testcontainers_modules::postgres::Postgres;
    use tokio::sync::OnceCell;

    struct PostgresTestContext {
        _container: ContainerAsync<Postgres>,
        connection_string: String,
    }
    static POSTGRES_CONTEXT: OnceCell<PostgresTestContext> = OnceCell::const_new();

    use futures::stream::BoxStream;
    use hodei_server_domain::DomainEvent;
    use hodei_server_domain::event_bus::{EventBus, EventBusError};
    use std::sync::Mutex;

    struct MockEventBus {
        published: Arc<Mutex<Vec<DomainEvent>>>,
    }
    impl MockEventBus {
        fn new() -> Self {
            Self {
                published: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }
    #[tonic::async_trait]
    impl EventBus for MockEventBus {
        async fn publish(&self, event: &DomainEvent) -> std::result::Result<(), EventBusError> {
            self.published.lock().unwrap().push(event.clone());
            Ok(())
        }
        async fn subscribe(
            &self,
            _topic: &str,
        ) -> std::result::Result<
            BoxStream<'static, std::result::Result<DomainEvent, EventBusError>>,
            EventBusError,
        > {
            Err(EventBusError::SubscribeError(
                "Mock not implemented".to_string(),
            ))
        }
    }

    async fn get_postgres_context() -> &'static PostgresTestContext {
        POSTGRES_CONTEXT
            .get_or_init(|| async {
                let container = Postgres::default()
                    .with_tag("16-alpine")
                    .start()
                    .await
                    .expect("Failed to start Postgres container");

                let host = container.get_host().await.expect("Failed to get host");
                let port = container
                    .get_host_port_ipv4(5432)
                    .await
                    .expect("Failed to get port");

                let connection_string =
                    format!("postgres://postgres:postgres@{}:{}/postgres", host, port);

                PostgresTestContext {
                    _container: container,
                    connection_string,
                }
            })
            .await
    }

    async fn create_worker_registry() -> PostgresWorkerRegistry {
        let ctx = get_postgres_context().await;
        let cfg = DatabaseConfig {
            url: ctx.connection_string.clone(),
            max_connections: 5,
            connection_timeout: std::time::Duration::from_secs(30),
        };

        let reg = PostgresWorkerRegistry::connect(&cfg)
            .await
            .expect("Failed to connect to Postgres");
        reg.run_migrations()
            .await
            .expect("Failed to run migrations");
        reg
    }

    #[tokio::test]
    #[ignore = "Requires Docker with PostgreSQL"]
    async fn hu_6_1_register_and_heartbeat_updates_registry() {
        let registry = create_worker_registry().await;

        let worker_id_uuid = uuid::Uuid::new_v4();
        let worker_id = WorkerId(worker_id_uuid);
        let provider_id = ProviderId::new();
        let handle = WorkerHandle::new(
            worker_id.clone(),
            "resource".to_string(),
            ProviderType::Docker,
            provider_id,
        );

        let mut spec = WorkerSpec::new(
            "hodei-jobs-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        spec.worker_id = worker_id.clone();

        registry
            .register(handle, spec)
            .await
            .expect("Failed to register worker in registry");

        let bus = Arc::new(MockEventBus::new());
        let service = WorkerAgentServiceImpl::with_registry(Arc::new(registry), bus);
        let otp = service
            .generate_otp(&worker_id_uuid.to_string())
            .await
            .expect("generate_otp should succeed");

        let req = RegisterWorkerRequest {
            auth_token: otp,
            worker_info: Some(WorkerInfo {
                worker_id: Some(hodei_jobs::WorkerId {
                    value: worker_id_uuid.to_string(),
                }),
                ..Default::default()
            }),
            session_id: String::new(),
        };

        service
            .register(Request::new(req))
            .await
            .expect("register should succeed");

        let reg = service.worker_registry().expect("registry should exist");
        let w = reg
            .get(&worker_id)
            .await
            .expect("registry get")
            .expect("worker exists");
        assert!(matches!(
            w.state(),
            WorkerState::Creating | WorkerState::Connecting
        ));

        service
            .on_worker_heartbeat(&worker_id_uuid.to_string())
            .await
            .expect("heartbeat should succeed");

        let w = reg
            .get(&worker_id)
            .await
            .expect("registry get")
            .expect("worker exists");
        assert!(matches!(w.state(), WorkerState::Ready));
    }

    #[tokio::test]
    async fn hu_6_5_job_result_updates_job_repository() {
        let job_repository: Arc<dyn JobRepository> = Arc::new(InMemoryJobRepository::new());
        let worker_registry: Arc<dyn WorkerRegistry> = Arc::new(InMemoryWorkerRegistry::new());
        let log_service = LogStreamService::new();

        let bus = Arc::new(MockEventBus::new());
        let service = WorkerAgentServiceImpl::with_registry_job_repository_and_log_service(
            worker_registry.clone(),
            job_repository.clone(),
            log_service,
            bus,
        );

        let worker_uuid = uuid::Uuid::new_v4();
        let worker_id = WorkerId(worker_uuid);
        let provider_id = ProviderId::new();
        let handle = WorkerHandle::new(
            worker_id.clone(),
            "resource".to_string(),
            ProviderType::Docker,
            provider_id,
        );
        let mut spec = WorkerSpec::new(
            "hodei-jobs-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        spec.worker_id = worker_id.clone();

        worker_registry
            .register(handle, spec)
            .await
            .expect("Failed to register worker");
        worker_registry
            .update_state(&worker_id, WorkerState::Connecting)
            .await
            .expect("Failed to set worker connecting");
        worker_registry
            .update_state(&worker_id, WorkerState::Ready)
            .await
            .expect("Failed to set worker ready");

        let job_uuid = uuid::Uuid::new_v4();
        let job_id = JobId(job_uuid);
        let mut job = Job::new(
            job_id.clone(),
            JobSpec::new(vec!["echo".to_string(), "hi".to_string()]),
        );
        let exec_ctx = ExecutionContext::new(
            job_id.clone(),
            ProviderId::new(),
            uuid::Uuid::new_v4().to_string(),
        );
        job.submit_to_provider(ProviderId::new(), exec_ctx)
            .expect("submit_to_provider");
        job.mark_running().expect("mark_running");
        job_repository.save(&job).await.expect("save job");

        worker_registry
            .assign_to_job(&worker_id, job_id.clone())
            .await
            .expect("assign_to_job");

        let msg = hodei_jobs::JobResultMessage {
            job_id: job_uuid.to_string(),
            exit_code: 0,
            success: true,
            error_message: String::new(),
            completed_at: None,
        };
        service
            .on_job_result(&worker_uuid.to_string(), &msg)
            .await
            .expect("on_job_result");

        let job = job_repository
            .find_by_id(&job_id)
            .await
            .expect("find_by_id")
            .expect("job exists");
        assert!(matches!(job.state(), JobState::Succeeded));

        let worker = worker_registry
            .get(&worker_id)
            .await
            .expect("get worker")
            .expect("worker exists");
        assert!(matches!(worker.state(), WorkerState::Ready));
        assert!(worker.current_job_id().is_none());
    }
    #[tokio::test]
    async fn test_job_result_publishes_event_with_correlation_id() {
        let job_repository: Arc<dyn JobRepository> = Arc::new(InMemoryJobRepository::new());
        let worker_registry: Arc<dyn WorkerRegistry> = Arc::new(InMemoryWorkerRegistry::new());
        let log_service = LogStreamService::new();
        let bus = Arc::new(MockEventBus::new());

        let service = WorkerAgentServiceImpl::with_registry_job_repository_and_log_service(
            worker_registry.clone(),
            job_repository.clone(),
            log_service,
            bus.clone(),
        );

        // 1. Setup Worker
        let worker_uuid = uuid::Uuid::new_v4();
        let worker_id = WorkerId(worker_uuid);
        let handle = WorkerHandle::new(
            worker_id.clone(),
            "resource".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut spec = WorkerSpec::new(
            "hodei-jobs-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        spec.worker_id = worker_id.clone();
        worker_registry
            .register(handle, spec)
            .await
            .expect("Failed to register worker");
        worker_registry
            .update_state(&worker_id, WorkerState::Connecting)
            .await
            .expect("Failed to set worker connecting");
        worker_registry
            .update_state(&worker_id, WorkerState::Ready)
            .await
            .expect("Failed to set worker ready");

        // 2. Setup Job with Metadata (Correlation ID)
        let job_uuid = uuid::Uuid::new_v4();
        let job_id = JobId(job_uuid);
        let mut job = Job::new(job_id.clone(), JobSpec::new(vec!["echo".to_string()]));
        job.metadata_mut()
            .insert("correlation_id".to_string(), "CORRl-123".to_string());
        job.metadata_mut()
            .insert("actor".to_string(), "ACTOR-XYZ".to_string());

        let exec_ctx = ExecutionContext::new(
            job_id.clone(),
            ProviderId::new(),
            uuid::Uuid::new_v4().to_string(),
        );
        job.submit_to_provider(ProviderId::new(), exec_ctx)
            .expect("submit");
        job.mark_running().expect("mark_running");
        job_repository.save(&job).await.expect("save job");

        // Ensure worker is assigned to job in registry so it can be released
        worker_registry
            .assign_to_job(&worker_id, job_id.clone())
            .await
            .expect("Failed to assign worker");

        // 3. Process Result
        let msg = hodei_jobs::JobResultMessage {
            job_id: job_uuid.to_string(),
            exit_code: 0,
            success: true,
            error_message: String::new(),
            completed_at: None,
        };
        service
            .on_job_result(&worker_uuid.to_string(), &msg)
            .await
            .expect("on_job_result");

        // 4. Verify Event
        let events = bus.published.lock().unwrap();
        let success_event = events.iter().find(|e| {
            matches!(
                e,
                DomainEvent::JobStatusChanged {
                    new_state: JobState::Succeeded,
                    ..
                }
            )
        });
        assert!(success_event.is_some());

        if let Some(DomainEvent::JobStatusChanged {
            correlation_id,
            actor,
            ..
        }) = success_event
        {
            assert_eq!(correlation_id.as_deref(), Some("CORRl-123"));
            assert_eq!(actor.as_deref(), Some("ACTOR-XYZ"));
        }
    }

    #[tokio::test]
    async fn test_unregister_worker_publishes_event() {
        let worker_registry: Arc<dyn WorkerRegistry> = Arc::new(InMemoryWorkerRegistry::new());
        let bus = Arc::new(MockEventBus::new());

        let service = WorkerAgentServiceImpl::with_registry(worker_registry.clone(), bus.clone());

        // 1. Setup Worker
        let worker_uuid = uuid::Uuid::new_v4();
        let worker_id = WorkerId(worker_uuid);
        let handle = WorkerHandle::new(
            worker_id.clone(),
            "resource".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut spec = WorkerSpec::new(
            "hodei-jobs-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        spec.worker_id = worker_id.clone();

        worker_registry
            .register(handle, spec)
            .await
            .expect("Failed to register worker");

        // Manually insert into service.workers map as register() does
        {
            let mut workers = service.workers.write().await;
            workers.insert(
                worker_uuid.to_string(),
                RegisteredWorker {
                    info: WorkerInfo::default(),
                    session_id: "sess-1".to_string(),
                    status: 0,
                },
            );
        }

        // 2. Unregister Not Found (should not publish event)
        let req = Request::new(UnregisterWorkerRequest {
            worker_id: Some(hodei_jobs::WorkerId {
                value: uuid::Uuid::new_v4().to_string(),
            }), // Random ID
            reason: "test".to_string(),
            force: false,
        });
        service
            .unregister_worker(req)
            .await
            .expect("unregister should succeed");

        {
            let events = bus.published.lock().unwrap();
            assert!(events.is_empty());
        }

        // 3. Unregister Existing Worker
        let req = Request::new(UnregisterWorkerRequest {
            worker_id: Some(hodei_jobs::WorkerId {
                value: worker_uuid.to_string(),
            }),
            reason: "Graceful Shutdown".to_string(),
            force: true,
        });
        let resp = service
            .unregister_worker(req)
            .await
            .expect("unregister should succeed");

        // Check Response
        assert!(resp.get_ref().success);
        assert_eq!(resp.get_ref().jobs_migrated, 0);

        // Check Event
        let events = bus.published.lock().unwrap();
        assert_eq!(events.len(), 1);
        if let Some(DomainEvent::WorkerTerminated {
            worker_id: wid,
            reason,
            ..
        }) = events.first()
        {
            assert_eq!(wid, &worker_id);
            assert!(matches!(
                reason,
                hodei_server_domain::events::TerminationReason::Unregistered
            ));
        } else {
            panic!("Expected WorkerTerminated event");
        }

        // Check Registry/Map cleanup (Map should be empty, registry logic depends on implementation but map is local)
        {
            let workers = service.workers.read().await;
            assert!(!workers.contains_key(&worker_uuid.to_string()));
        }
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
        }
    }

    pub fn with_registry_and_log_service(
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
        log_service: LogStreamService,
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
        }
    }

    pub fn with_registry_job_repository_and_log_service(
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
        log_service: LogStreamService,
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
        }
    }

    pub fn with_registry_job_repository_token_store_and_log_service(
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
        token_store: Arc<dyn hodei_server_domain::iam::WorkerBootstrapTokenStore>,
        log_service: LogStreamService,
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

    fn parse_job_uuid(job_id: &str) -> Result<JobId, Status> {
        let id = uuid::Uuid::parse_str(job_id)
            .map_err(|_| Status::invalid_argument("job_id must be a UUID"))?;
        Ok(JobId(id))
    }

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

        if matches!(worker.state(), WorkerState::Creating) {
            registry
                .update_state(&worker_id, WorkerState::Connecting)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
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
            job.fail(msg)
                .map_err(|e| Status::failed_precondition(e.to_string()))?;
        }

        job_repository
            .update(&job)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Publicar evento JobStatusChanged
        if let Some(event_bus) = &self.event_bus {
            let new_state = if result.success {
                JobState::Succeeded
            } else {
                JobState::Failed
            };

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

        // Validar OTP token
        let validated_worker_id = self
            .validate_otp(&req.auth_token, &worker_id_from_request)
            .await?;

        let worker_id = validated_worker_id;

        self.on_worker_registered(&worker_id).await?;

        // Check for session recovery
        let session_id_req = req.session_id.clone();
        let (session_id, is_reconnection, recovery_failed) = if !session_id_req.is_empty() {
            // Attempt to recover existing session
            let workers = self.workers.read().await;
            if let Some(existing) = workers.get(&worker_id) {
                if existing.session_id == session_id_req {
                    // Valid session found
                    (session_id_req.clone(), true, false)
                } else {
                    // Session mismatch (shouldn't happen for same worker_id usually unless restarted)
                    info!(
                        "Worker {} recovery failed: session mismatch (req={}, current={})",
                        worker_id, session_id_req, existing.session_id
                    );
                    (Self::generate_session_id(), false, true)
                }
            } else {
                // Worker not found in memory (restart/crash?)
                info!(
                    "Worker {} recovery failed: session {} not found",
                    worker_id, session_id_req
                );
                (Self::generate_session_id(), false, true)
            }
        } else {
            // New registration
            (Self::generate_session_id(), false, false)
        };

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
                match message_result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload {
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
                                            })),
                                        };
                                        let _ = tx_clone.send(Ok(ack)).await;

                                        let _ =
                                            registry_service.on_worker_heartbeat(&wid.value).await;
                                    }
                                }
                                WorkerPayload::Log(log) => {
                                    // Log to console
                                    if log.is_stderr {
                                        info!("[{}] stderr: {}", log.job_id, log.line);
                                    } else {
                                        info!("[{}] stdout: {}", log.job_id, log.line);
                                    }

                                    // Forward to LogStreamService for client subscribers
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

                                    // Close log subscribers when job completes
                                    if let Some(ref svc) = log_service {
                                        svc.close_subscribers(&result.job_id).await;
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

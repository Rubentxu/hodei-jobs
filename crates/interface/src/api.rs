// REST API con Axum
// Endpoints para gestión de jobs y providers

use axum::{
    Router,
    extract::{Json, Path, Query, State},
    http::StatusCode,
    response::Json as AxumJson,
    routing::{get, post},
};
use hodei_jobs_application::{
    ProviderRegistry,
    job_execution_usecases::{
        CancelJobResponse, CancelJobUseCase, CreateJobRequest, CreateJobResponse, CreateJobUseCase,
        GetJobStatusUseCase, TrackJobResponse,
    },
    provider_usecases::{
        GetProviderResponse, GetProviderUseCase, ListProvidersRequest, ListProvidersResponse,
        ListProvidersUseCase, RegisterProviderRequest, RegisterProviderResponse,
        RegisterProviderUseCase,
    },
};
use hodei_jobs_domain::provider_config::ProviderTypeConfig;
use hodei_jobs_infrastructure::event_bus::postgres::PostgresEventBus;
use hodei_jobs_infrastructure::persistence::{
    PostgresJobQueue, PostgresJobRepository, PostgresProviderConfigRepository,
};
use hodei_jobs_infrastructure::repositories::PostgresAuditRepository;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};

/// Trait para use case de crear job
#[async_trait::async_trait]
pub trait CreateJobUseCaseTrait: Send + Sync {
    async fn execute(
        &self,
        request: CreateJobRequest,
    ) -> Result<CreateJobResponse, hodei_jobs_domain::shared_kernel::DomainError>;
}

#[async_trait::async_trait]
impl RegisterProviderUseCaseTrait
    for hodei_jobs_application::provider_usecases::RegisterProviderUseCase
{
    async fn execute(
        &self,
        request: RegisterProviderRequest,
    ) -> Result<RegisterProviderResponse, hodei_jobs_domain::shared_kernel::DomainError> {
        self.execute(request).await
    }
}

#[async_trait::async_trait]
impl ListProvidersUseCaseTrait for hodei_jobs_application::provider_usecases::ListProvidersUseCase {
    async fn execute(
        &self,
        request: ListProvidersRequest,
    ) -> Result<ListProvidersResponse, hodei_jobs_domain::shared_kernel::DomainError> {
        self.execute(request).await
    }
}

#[async_trait::async_trait]
impl GetProviderUseCaseTrait for hodei_jobs_application::provider_usecases::GetProviderUseCase {
    async fn execute(
        &self,
        provider_id: hodei_jobs_domain::shared_kernel::ProviderId,
    ) -> Result<GetProviderResponse, hodei_jobs_domain::shared_kernel::DomainError> {
        self.execute(provider_id).await
    }
}

#[async_trait::async_trait]
pub trait GetJobStatusUseCaseTrait: Send + Sync {
    async fn execute(
        &self,
        job_id: hodei_jobs_domain::shared_kernel::JobId,
    ) -> Result<TrackJobResponse, hodei_jobs_domain::shared_kernel::DomainError>;
}

#[async_trait::async_trait]
pub trait CancelJobUseCaseTrait: Send + Sync {
    async fn execute(
        &self,
        job_id: hodei_jobs_domain::shared_kernel::JobId,
    ) -> Result<CancelJobResponse, hodei_jobs_domain::shared_kernel::DomainError>;
}

#[async_trait::async_trait]
pub trait RegisterProviderUseCaseTrait: Send + Sync {
    async fn execute(
        &self,
        request: RegisterProviderRequest,
    ) -> Result<RegisterProviderResponse, hodei_jobs_domain::shared_kernel::DomainError>;
}

#[async_trait::async_trait]
pub trait ListProvidersUseCaseTrait: Send + Sync {
    async fn execute(
        &self,
        request: ListProvidersRequest,
    ) -> Result<ListProvidersResponse, hodei_jobs_domain::shared_kernel::DomainError>;
}

#[async_trait::async_trait]
pub trait GetProviderUseCaseTrait: Send + Sync {
    async fn execute(
        &self,
        provider_id: hodei_jobs_domain::shared_kernel::ProviderId,
    ) -> Result<GetProviderResponse, hodei_jobs_domain::shared_kernel::DomainError>;
}

// NOTE: RegisterProviderUseCaseTrait será reimplementado en la épica 2
// con el nuevo modelo de ProviderConfig

/// Estado de la aplicación
#[derive(Clone)]
pub struct AppState {
    pub create_job_usecase: std::sync::Arc<dyn CreateJobUseCaseTrait>,
    pub get_job_status_usecase: std::sync::Arc<dyn GetJobStatusUseCaseTrait>,
    pub cancel_job_usecase: std::sync::Arc<dyn CancelJobUseCaseTrait>,
    pub register_provider_usecase: std::sync::Arc<dyn RegisterProviderUseCaseTrait>,
    pub list_providers_usecase: std::sync::Arc<dyn ListProvidersUseCaseTrait>,
    pub get_provider_usecase: std::sync::Arc<dyn GetProviderUseCaseTrait>,
    pub get_audit_logs_usecase: std::sync::Arc<dyn GetAuditLogsUseCaseTrait>,
}

#[async_trait::async_trait]
pub trait GetAuditLogsUseCaseTrait: Send + Sync {
    async fn execute(
        &self,
        correlation_id: String,
    ) -> Result<
        Vec<hodei_jobs_domain::audit::AuditLog>,
        hodei_jobs_domain::shared_kernel::DomainError,
    >;
}

#[async_trait::async_trait]
impl GetAuditLogsUseCaseTrait for hodei_jobs_application::audit_usecases::AuditService {
    async fn execute(
        &self,
        correlation_id: String,
    ) -> Result<
        Vec<hodei_jobs_domain::audit::AuditLog>,
        hodei_jobs_domain::shared_kernel::DomainError,
    > {
        self.get_logs_by_correlation_id(&correlation_id).await
    }
}

/// Tipos de respuesta API
#[derive(Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub timestamp: String,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// DTO para crear job vía API
#[derive(Serialize, Deserialize)]
pub struct CreateJobApiRequest {
    pub spec: JobSpecApiRequest,
    pub correlation_id: Option<String>,
    pub job_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct JobSpecApiRequest {
    pub command: Vec<String>,
    pub image: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    pub working_dir: Option<String>,
    pub cpu_cores: Option<f64>,
    pub memory_bytes: Option<i64>,
    pub disk_bytes: Option<i64>,
}

#[derive(Serialize, Deserialize)]
pub struct RegisterProviderApiRequest {
    pub name: String,
    pub provider_type: String,
    pub type_config: ProviderTypeConfig,
    pub priority: Option<i32>,
    pub max_workers: Option<u32>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

/// DTO para tracking de job
#[derive(Serialize, Deserialize)]
pub struct TrackJobApiResponse {
    pub job_id: String,
    pub status: String,
    pub result: Option<String>,
    pub created_at: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

fn parse_job_id(job_id: &str) -> Result<hodei_jobs_domain::shared_kernel::JobId, StatusCode> {
    let uuid = uuid::Uuid::parse_str(job_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(hodei_jobs_domain::shared_kernel::JobId(uuid))
}

fn parse_provider_id(
    provider_id: &str,
) -> Result<hodei_jobs_domain::shared_kernel::ProviderId, StatusCode> {
    let uuid = uuid::Uuid::parse_str(provider_id).map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(hodei_jobs_domain::shared_kernel::ProviderId(uuid))
}

/// Crear router de la API
pub fn create_router() -> Router<AppState> {
    Router::new()
        // Jobs endpoints
        .route("/api/v1/jobs", post(create_job))
        .route("/api/v1/jobs/{job_id}", get(get_job_status))
        .route("/api/v1/jobs/{job_id}/cancel", post(cancel_job))
        // Providers endpoints
        .route(
            "/api/v1/providers",
            post(register_provider).get(list_providers),
        )
        .route("/api/v1/providers/{provider_id}", get(get_provider))
        // Audit endpoints
        .route("/api/v1/audit-logs", get(get_audit_logs))
        // Health check
        .route("/health", get(health_check))
        // CORS
        .layer(CorsLayer::new().allow_methods(Any).allow_headers(Any))
}

/// Handler para crear job
async fn create_job(
    State(state): State<AppState>,
    Json(request): Json<CreateJobApiRequest>,
) -> Result<AxumJson<ApiResponse<CreateJobResponse>>, StatusCode> {
    info!("Creating new job");

    // Convertir DTO de API a DTO de aplicación
    let app_request = CreateJobRequest {
        spec: hodei_jobs_application::job_execution_usecases::JobSpecRequest {
            command: request.spec.command,
            image: request.spec.image,
            env: request.spec.env,
            timeout_ms: request.spec.timeout_ms,
            working_dir: request.spec.working_dir,
            cpu_cores: request.spec.cpu_cores,
            memory_bytes: request.spec.memory_bytes,
            disk_bytes: request.spec.disk_bytes,
        },
        correlation_id: request.correlation_id,
        actor: None, // API currently does not have authentication
        job_id: request.job_id,
    };

    // Ejecutar use case
    match state.create_job_usecase.execute(app_request).await {
        Ok(response) => Ok(AxumJson(ApiResponse::success(response))),
        Err(error) => {
            warn!("Failed to create job: {}", error);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

/// Handler para obtener estado de job
async fn get_job_status(
    Path(job_id): Path<String>,
    State(state): State<AppState>,
) -> Result<AxumJson<ApiResponse<TrackJobApiResponse>>, StatusCode> {
    info!("Getting job status for: {}", job_id);

    let job_id = parse_job_id(&job_id)?;
    match state.get_job_status_usecase.execute(job_id).await {
        Ok(track) => Ok(AxumJson(ApiResponse::success(TrackJobApiResponse {
            job_id: track.job_id,
            status: track.status,
            result: track.result,
            created_at: track.created_at,
            started_at: track.started_at,
            completed_at: track.completed_at,
        }))),
        Err(error) => {
            warn!("Failed to get job status: {}", error);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

/// Handler para cancelar job
async fn cancel_job(
    Path(job_id): Path<String>,
    State(state): State<AppState>,
) -> Result<AxumJson<ApiResponse<CancelJobResponse>>, StatusCode> {
    info!("Cancelling job: {}", job_id);

    let job_id = parse_job_id(&job_id)?;
    match state.cancel_job_usecase.execute(job_id).await {
        Ok(response) => Ok(AxumJson(ApiResponse::success(response))),
        Err(error) => {
            warn!("Failed to cancel job: {}", error);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

async fn register_provider(
    State(state): State<AppState>,
    Json(request): Json<RegisterProviderApiRequest>,
) -> Result<AxumJson<ApiResponse<RegisterProviderResponse>>, StatusCode> {
    info!("Registering provider: {}", request.name);

    let app_request = RegisterProviderRequest {
        name: request.name,
        provider_type: request.provider_type,
        type_config: request.type_config,
        priority: request.priority,
        max_workers: request.max_workers,
        tags: request.tags,
        metadata: request.metadata,
    };

    match state.register_provider_usecase.execute(app_request).await {
        Ok(response) => Ok(AxumJson(ApiResponse::success(response))),
        Err(error) => {
            warn!("Failed to register provider: {}", error);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

#[derive(Debug, Deserialize)]
struct ListProvidersQuery {
    only_enabled: Option<bool>,
    only_with_capacity: Option<bool>,
    provider_type: Option<String>,
}

async fn list_providers(
    State(state): State<AppState>,
    Query(query): Query<ListProvidersQuery>,
) -> Result<AxumJson<ApiResponse<ListProvidersResponse>>, StatusCode> {
    let req = ListProvidersRequest {
        only_enabled: query.only_enabled.unwrap_or(false),
        only_with_capacity: query.only_with_capacity.unwrap_or(false),
        provider_type: query.provider_type,
    };

    match state.list_providers_usecase.execute(req).await {
        Ok(response) => Ok(AxumJson(ApiResponse::success(response))),
        Err(error) => {
            warn!("Failed to list providers: {}", error);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_provider(
    Path(provider_id): Path<String>,
    State(state): State<AppState>,
) -> Result<AxumJson<ApiResponse<GetProviderResponse>>, StatusCode> {
    let provider_id = parse_provider_id(&provider_id)?;
    match state.get_provider_usecase.execute(provider_id).await {
        Ok(response) => {
            if response.provider.is_some() {
                Ok(AxumJson(ApiResponse::success(response)))
            } else {
                Err(StatusCode::NOT_FOUND)
            }
        }
        Err(error) => {
            warn!("Failed to get provider: {}", error);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

#[derive(Deserialize)]
pub struct GetAuditLogsQuery {
    pub correlation_id: String,
}

async fn get_audit_logs(
    State(state): State<AppState>,
    Query(query): Query<GetAuditLogsQuery>,
) -> Result<AxumJson<ApiResponse<Vec<hodei_jobs_domain::audit::AuditLog>>>, StatusCode> {
    match state
        .get_audit_logs_usecase
        .execute(query.correlation_id)
        .await
    {
        Ok(logs) => Ok(AxumJson(ApiResponse::success(logs))),
        Err(e) => {
            warn!("Failed to get audit logs: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Handler para health check
async fn health_check() -> Result<AxumJson<ApiResponse<String>>, StatusCode> {
    Ok(AxumJson(ApiResponse::success("healthy".to_string())))
}

#[derive(Clone)]
struct ApiDatabaseSettings {
    url: String,
    max_connections: u32,
    connection_timeout: Duration,
}

impl ApiDatabaseSettings {
    fn from_env() -> anyhow::Result<Self> {
        Self::from_env_with(|key| env::var(key).ok())
    }

    fn from_env_with<F>(get: F) -> anyhow::Result<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        let url = get("HODEI_DATABASE_URL")
            .or_else(|| get("DATABASE_URL"))
            .ok_or_else(|| {
                anyhow::anyhow!("Missing database url (HODEI_DATABASE_URL or DATABASE_URL)")
            })?;

        let max_connections = get("HODEI_DB_MAX_CONNECTIONS")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(10);

        let connection_timeout = get("HODEI_DB_CONNECTION_TIMEOUT_SECS")
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(30));

        Ok(Self {
            url,
            max_connections,
            connection_timeout,
        })
    }
}

async fn create_app_state_from_env() -> anyhow::Result<AppState> {
    let settings = ApiDatabaseSettings::from_env()?;

    let pool = PgPoolOptions::new()
        .max_connections(settings.max_connections)
        .acquire_timeout(settings.connection_timeout)
        .connect(&settings.url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database: {}", e))?;

    let job_repository = PostgresJobRepository::new(pool.clone());
    job_repository.run_migrations().await?;

    let job_queue = PostgresJobQueue::new(pool.clone());
    job_queue.run_migrations().await?;

    let provider_repository = PostgresProviderConfigRepository::new(pool.clone());
    provider_repository.run_migrations().await?;

    let event_bus = std::sync::Arc::new(PostgresEventBus::new(pool.clone()));

    let provider_repository = std::sync::Arc::new(provider_repository)
        as std::sync::Arc<dyn hodei_jobs_domain::provider_config::ProviderConfigRepository>;
    let provider_registry = std::sync::Arc::new(ProviderRegistry::new(provider_repository));

    let job_repository = std::sync::Arc::new(job_repository)
        as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobRepository>;
    let job_queue = std::sync::Arc::new(job_queue)
        as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobQueue>;

    let create_job_usecase =
        CreateJobUseCase::new(job_repository.clone(), job_queue, event_bus.clone());
    let get_job_status_usecase = GetJobStatusUseCase::new(job_repository.clone());
    let cancel_job_usecase = CancelJobUseCase::new(job_repository, event_bus.clone());

    let register_provider_usecase =
        RegisterProviderUseCase::new(provider_registry.clone(), event_bus.clone());
    let list_providers_usecase = ListProvidersUseCase::new(provider_registry.clone());
    let get_provider_usecase = GetProviderUseCase::new(provider_registry);

    let audit_repository = PostgresAuditRepository::new(pool.clone());
    let audit_service = hodei_jobs_application::audit_usecases::AuditService::new(
        std::sync::Arc::new(audit_repository),
    );

    Ok(AppState {
        create_job_usecase: std::sync::Arc::new(create_job_usecase),
        get_job_status_usecase: std::sync::Arc::new(get_job_status_usecase),
        cancel_job_usecase: std::sync::Arc::new(cancel_job_usecase),
        register_provider_usecase: std::sync::Arc::new(register_provider_usecase),
        list_providers_usecase: std::sync::Arc::new(list_providers_usecase),
        get_provider_usecase: std::sync::Arc::new(get_provider_usecase),
        get_audit_logs_usecase: std::sync::Arc::new(audit_service),
    })
}

/// Función para iniciar el servidor HTTP
pub async fn start_server(addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting HTTP server on {}", addr);

    let state = create_app_state_from_env().await?;
    let router = create_router().with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}

/// Implementaciones de traits para use cases
#[async_trait::async_trait]
impl CreateJobUseCaseTrait for hodei_jobs_application::job_execution_usecases::CreateJobUseCase {
    async fn execute(
        &self,
        request: CreateJobRequest,
    ) -> Result<CreateJobResponse, hodei_jobs_domain::shared_kernel::DomainError> {
        self.execute(request).await
    }
}

#[async_trait::async_trait]
impl GetJobStatusUseCaseTrait
    for hodei_jobs_application::job_execution_usecases::GetJobStatusUseCase
{
    async fn execute(
        &self,
        job_id: hodei_jobs_domain::shared_kernel::JobId,
    ) -> Result<TrackJobResponse, hodei_jobs_domain::shared_kernel::DomainError> {
        self.execute(job_id).await
    }
}

#[async_trait::async_trait]
impl CancelJobUseCaseTrait for hodei_jobs_application::job_execution_usecases::CancelJobUseCase {
    async fn execute(
        &self,
        job_id: hodei_jobs_domain::shared_kernel::JobId,
    ) -> Result<CancelJobResponse, hodei_jobs_domain::shared_kernel::DomainError> {
        self.execute(job_id).await
    }
}

// NOTE: RegisterProviderUseCaseTrait impl será reimplementado en la épica 2

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use futures::stream::BoxStream;
    use hodei_jobs_domain::event_bus::{EventBus, EventBusError};
    use hodei_jobs_domain::events::DomainEvent;
    use hodei_jobs_infrastructure::persistence::{FileBasedPersistence, PersistenceConfig};
    use http_body_util::BodyExt;
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    struct MockEventBusWithAudit {
        published: Arc<Mutex<Vec<DomainEvent>>>,
        audit_logs: Arc<Mutex<Vec<hodei_jobs_domain::audit::AuditLog>>>,
    }
    impl MockEventBusWithAudit {
        fn new(audit_logs: Arc<Mutex<Vec<hodei_jobs_domain::audit::AuditLog>>>) -> Self {
            Self {
                published: Arc::new(Mutex::new(Vec::new())),
                audit_logs,
            }
        }
    }
    #[async_trait::async_trait]
    impl EventBus for MockEventBusWithAudit {
        async fn publish(&self, event: &DomainEvent) -> std::result::Result<(), EventBusError> {
            self.published.lock().unwrap().push(event.clone());
            // Simulate AuditService behavior: save event as audit log
            let audit_log = hodei_jobs_domain::audit::AuditLog::new(
                event.event_type().to_string(),
                serde_json::to_value(event).unwrap_or_default(),
                event.correlation_id(),
                event.actor().or_else(|| Some("system".to_string())),
            );
            self.audit_logs.lock().unwrap().push(audit_log);
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

    fn create_app_state_for_tests() -> AppState {
        let job_repository = std::sync::Arc::new(
            hodei_jobs_infrastructure::repositories::InMemoryJobRepository::new(),
        )
            as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobRepository>;
        let job_queue =
            std::sync::Arc::new(hodei_jobs_infrastructure::repositories::InMemoryJobQueue::new())
                as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobQueue>;

        let temp_dir = tempfile::tempdir().expect("tempdir");
        let data_directory = temp_dir.keep().to_string_lossy().to_string();

        let persistence = FileBasedPersistence::new(PersistenceConfig {
            data_directory,
            backup_enabled: false,
            auto_compact: false,
        });
        let provider_repo =
            hodei_jobs_infrastructure::persistence::FileBasedProviderConfigRepository::new(
                persistence,
            );
        let provider_repo = std::sync::Arc::new(provider_repo)
            as std::sync::Arc<dyn hodei_jobs_domain::provider_config::ProviderConfigRepository>;
        let provider_registry = std::sync::Arc::new(ProviderRegistry::new(provider_repo));

        let shared_audit_logs = Arc::new(Mutex::new(Vec::new()));
        let event_bus = std::sync::Arc::new(MockEventBusWithAudit::new(shared_audit_logs.clone()));

        let create_job_usecase =
            CreateJobUseCase::new(job_repository.clone(), job_queue, event_bus.clone());
        let get_job_status_usecase = GetJobStatusUseCase::new(job_repository.clone());
        let cancel_job_usecase = CancelJobUseCase::new(job_repository, event_bus.clone());

        let register_provider_usecase =
            RegisterProviderUseCase::new(provider_registry.clone(), event_bus.clone());
        let list_providers_usecase = ListProvidersUseCase::new(provider_registry.clone());
        let get_provider_usecase = GetProviderUseCase::new(provider_registry);

        AppState {
            create_job_usecase: std::sync::Arc::new(create_job_usecase),
            get_job_status_usecase: std::sync::Arc::new(get_job_status_usecase),
            cancel_job_usecase: std::sync::Arc::new(cancel_job_usecase),
            register_provider_usecase: std::sync::Arc::new(register_provider_usecase),
            list_providers_usecase: std::sync::Arc::new(list_providers_usecase),
            get_provider_usecase: std::sync::Arc::new(get_provider_usecase),
            get_audit_logs_usecase: std::sync::Arc::new(
                hodei_jobs_application::audit_usecases::AuditService::new(std::sync::Arc::new(
                    MockAuditRepository::new_with_logs(shared_audit_logs),
                )),
            ),
        }
    }

    struct MockAuditRepository {
        pub saved_logs: Arc<Mutex<Vec<hodei_jobs_domain::audit::AuditLog>>>,
    }

    impl MockAuditRepository {
        fn new_with_logs(logs: Arc<Mutex<Vec<hodei_jobs_domain::audit::AuditLog>>>) -> Self {
            Self { saved_logs: logs }
        }
    }

    #[async_trait::async_trait]
    impl hodei_jobs_domain::audit::AuditRepository for MockAuditRepository {
        async fn save(
            &self,
            log: &hodei_jobs_domain::audit::AuditLog,
        ) -> Result<(), hodei_jobs_domain::shared_kernel::DomainError> {
            self.saved_logs.lock().unwrap().push(log.clone());
            Ok(())
        }

        async fn find_by_correlation_id(
            &self,
            id: &str,
        ) -> Result<
            Vec<hodei_jobs_domain::audit::AuditLog>,
            hodei_jobs_domain::shared_kernel::DomainError,
        > {
            let logs = self.saved_logs.lock().unwrap();
            Ok(logs
                .iter()
                .filter(|l| l.correlation_id.as_deref() == Some(id))
                .cloned()
                .collect())
        }

        async fn find_by_event_type(
            &self,
            event_type: &str,
            limit: i64,
            offset: i64,
        ) -> Result<
            hodei_jobs_domain::audit::AuditQueryResult,
            hodei_jobs_domain::shared_kernel::DomainError,
        > {
            let logs = self.saved_logs.lock().unwrap();
            let filtered: Vec<_> = logs
                .iter()
                .filter(|l| l.event_type == event_type)
                .cloned()
                .collect();
            let total = filtered.len() as i64;
            let result: Vec<_> = filtered
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect();
            Ok(hodei_jobs_domain::audit::AuditQueryResult {
                logs: result.clone(),
                total_count: total,
                has_more: (offset + result.len() as i64) < total,
            })
        }

        async fn find_by_date_range(
            &self,
            start: chrono::DateTime<chrono::Utc>,
            end: chrono::DateTime<chrono::Utc>,
            limit: i64,
            offset: i64,
        ) -> Result<
            hodei_jobs_domain::audit::AuditQueryResult,
            hodei_jobs_domain::shared_kernel::DomainError,
        > {
            let logs = self.saved_logs.lock().unwrap();
            let filtered: Vec<_> = logs
                .iter()
                .filter(|l| l.occurred_at >= start && l.occurred_at <= end)
                .cloned()
                .collect();
            let total = filtered.len() as i64;
            let result: Vec<_> = filtered
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect();
            Ok(hodei_jobs_domain::audit::AuditQueryResult {
                logs: result.clone(),
                total_count: total,
                has_more: (offset + result.len() as i64) < total,
            })
        }

        async fn find_by_actor(
            &self,
            actor: &str,
            limit: i64,
            offset: i64,
        ) -> Result<
            hodei_jobs_domain::audit::AuditQueryResult,
            hodei_jobs_domain::shared_kernel::DomainError,
        > {
            let logs = self.saved_logs.lock().unwrap();
            let filtered: Vec<_> = logs
                .iter()
                .filter(|l| l.actor.as_deref() == Some(actor))
                .cloned()
                .collect();
            let total = filtered.len() as i64;
            let result: Vec<_> = filtered
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect();
            Ok(hodei_jobs_domain::audit::AuditQueryResult {
                logs: result.clone(),
                total_count: total,
                has_more: (offset + result.len() as i64) < total,
            })
        }

        async fn query(
            &self,
            query: hodei_jobs_domain::audit::AuditQuery,
        ) -> Result<
            hodei_jobs_domain::audit::AuditQueryResult,
            hodei_jobs_domain::shared_kernel::DomainError,
        > {
            let logs = self.saved_logs.lock().unwrap();
            let mut filtered: Vec<_> = logs.iter().cloned().collect();
            if let Some(ref et) = query.event_type {
                filtered.retain(|l| &l.event_type == et);
            }
            if let Some(ref a) = query.actor {
                filtered.retain(|l| l.actor.as_deref() == Some(a.as_str()));
            }
            if let Some(start) = query.start_time {
                filtered.retain(|l| l.occurred_at >= start);
            }
            if let Some(end) = query.end_time {
                filtered.retain(|l| l.occurred_at <= end);
            }
            let total = filtered.len() as i64;
            let limit = query.limit.unwrap_or(100) as usize;
            let offset = query.offset.unwrap_or(0) as usize;
            let result: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();
            Ok(hodei_jobs_domain::audit::AuditQueryResult {
                logs: result.clone(),
                total_count: total,
                has_more: (offset + result.len()) < total as usize,
            })
        }

        async fn count_by_event_type(
            &self,
        ) -> Result<
            Vec<hodei_jobs_domain::audit::EventTypeCount>,
            hodei_jobs_domain::shared_kernel::DomainError,
        > {
            let logs = self.saved_logs.lock().unwrap();
            let mut counts: std::collections::HashMap<String, i64> =
                std::collections::HashMap::new();
            for log in logs.iter() {
                *counts.entry(log.event_type.clone()).or_insert(0) += 1;
            }
            Ok(counts
                .into_iter()
                .map(
                    |(event_type, count)| hodei_jobs_domain::audit::EventTypeCount {
                        event_type,
                        count,
                    },
                )
                .collect())
        }

        async fn delete_before(
            &self,
            before: chrono::DateTime<chrono::Utc>,
        ) -> Result<u64, hodei_jobs_domain::shared_kernel::DomainError> {
            let mut logs = self.saved_logs.lock().unwrap();
            let original_len = logs.len();
            logs.retain(|l| l.occurred_at >= before);
            Ok((original_len - logs.len()) as u64)
        }
    }

    #[tokio::test]
    async fn test_register_provider_then_list_then_get() {
        let app = create_router().with_state(create_app_state_for_tests());

        let register_body = serde_json::json!({
            "name": "docker-local",
            "provider_type": "docker",
            "type_config": {
                "type": "docker",
                "socket_path": "/var/run/docker.sock",
                "default_image": "hodei-jobs-worker:latest"
            },
            "priority": 10,
            "max_workers": 5,
            "tags": ["prod"],
            "metadata": {"region": "local"}
        });

        let register_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/providers")
                    .header("content-type", "application/json")
                    .body(Body::from(register_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(register_response.status(), StatusCode::OK);
        let register_bytes = register_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let register_api: ApiResponse<RegisterProviderResponse> =
            serde_json::from_slice(&register_bytes).unwrap();
        assert!(register_api.success);
        let provider_id = register_api.data.as_ref().unwrap().provider.id.clone();

        let list_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/providers")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(list_response.status(), StatusCode::OK);
        let list_bytes = list_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let list_api: ApiResponse<ListProvidersResponse> =
            serde_json::from_slice(&list_bytes).unwrap();
        assert!(list_api.success);
        assert!(
            list_api
                .data
                .as_ref()
                .unwrap()
                .providers
                .iter()
                .any(|p| p.id == provider_id)
        );

        let get_response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/providers/{}", provider_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(get_response.status(), StatusCode::OK);
        let get_bytes = get_response.into_body().collect().await.unwrap().to_bytes();
        let get_api: ApiResponse<GetProviderResponse> = serde_json::from_slice(&get_bytes).unwrap();
        assert!(get_api.success);
        assert_eq!(
            get_api.data.as_ref().unwrap().provider.as_ref().unwrap().id,
            provider_id
        );
    }

    #[tokio::test]
    async fn test_get_audit_logs() {
        let app = create_router().with_state(create_app_state_for_tests());
        let correlation_id = "test-corr-id";

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/v1/audit-logs?correlation_id={}",
                        correlation_id
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let api_response: ApiResponse<Vec<hodei_jobs_domain::audit::AuditLog>> =
            serde_json::from_slice(&bytes).unwrap();

        assert!(api_response.success);
        assert!(api_response.data.is_some());
    }

    #[tokio::test]
    async fn test_create_job() {
        let app = create_router().with_state(create_app_state_for_tests());
        let correlation_id = "test-corr-create-job";

        let job_request = serde_json::json!({
            "spec": {
                "command": ["echo", "hello"],
                "image": "alpine:latest",
                "timeout_ms": 1000
            },
            "correlation_id": correlation_id
        });

        // 1. Create Job
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/jobs")
                    .header("content-type", "application/json")
                    .body(Body::from(job_request.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // 2. Verify Audit Log
        let audit_response = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/v1/audit-logs?correlation_id={}",
                        correlation_id
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(audit_response.status(), StatusCode::OK);
        let audit_bytes = audit_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let audit_logs: ApiResponse<Vec<hodei_jobs_domain::audit::AuditLog>> =
            serde_json::from_slice(&audit_bytes).unwrap();

        assert!(audit_logs.success);
        let logs = audit_logs.data.unwrap();
        assert!(!logs.is_empty());
        assert_eq!(logs[0].event_type, "JobCreated");
        assert_eq!(logs[0].correlation_id.as_deref(), Some(correlation_id));
    }
    #[tokio::test]
    async fn test_health_check() {
        let app = create_router().with_state(create_app_state_for_tests());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn test_db_settings_missing_url_fails() {
        let result = ApiDatabaseSettings::from_env_with(|_key| None);
        assert!(result.is_err());
    }

    #[test]
    fn test_db_settings_parses_defaults() {
        let result = ApiDatabaseSettings::from_env_with(|key| match key {
            "HODEI_DATABASE_URL" => {
                Some("postgres://postgres:postgres@localhost:5432/postgres".to_string())
            }
            _ => None,
        })
        .expect("expected settings");

        assert_eq!(result.max_connections, 10);
        assert_eq!(result.connection_timeout, Duration::from_secs(30));
        assert_eq!(
            result.url,
            "postgres://postgres:postgres@localhost:5432/postgres"
        );
    }

    #[test]
    fn test_db_settings_parses_overrides() {
        let result = ApiDatabaseSettings::from_env_with(|key| match key {
            "DATABASE_URL" => {
                Some("postgres://postgres:postgres@localhost:5432/postgres".to_string())
            }
            "HODEI_DB_MAX_CONNECTIONS" => Some("42".to_string()),
            "HODEI_DB_CONNECTION_TIMEOUT_SECS" => Some("5".to_string()),
            _ => None,
        })
        .expect("expected settings");

        assert_eq!(result.max_connections, 42);
        assert_eq!(result.connection_timeout, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_create_job_then_get_status_then_cancel() {
        let app = create_router().with_state(create_app_state_for_tests());

        let create_body = serde_json::json!({
            "spec": {
                "command": ["echo", "hello"],
                "image": null,
                "env": null,
                "timeout_ms": null,
                "working_dir": null
            },
            "correlation_id": null
        });

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/jobs")
                    .header("content-type", "application/json")
                    .body(Body::from(create_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(create_response.status(), StatusCode::OK);

        let create_bytes = create_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let create_api: ApiResponse<CreateJobResponse> =
            serde_json::from_slice(&create_bytes).unwrap();
        assert!(create_api.success);
        let job_id = create_api.data.unwrap().job_id;

        let status_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/jobs/{}", job_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(status_response.status(), StatusCode::OK);
        let status_bytes = status_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let status_api: ApiResponse<TrackJobApiResponse> =
            serde_json::from_slice(&status_bytes).unwrap();
        assert!(status_api.success);
        assert_eq!(status_api.data.as_ref().unwrap().status, "PENDING");

        let cancel_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/jobs/{}/cancel", job_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(cancel_response.status(), StatusCode::OK);
        let cancel_bytes = cancel_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let cancel_api: ApiResponse<CancelJobResponse> =
            serde_json::from_slice(&cancel_bytes).unwrap();
        assert!(cancel_api.success);
        assert_eq!(cancel_api.data.as_ref().unwrap().status, "CANCELLED");

        let status_response2 = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/jobs/{}", job_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(status_response2.status(), StatusCode::OK);
        let status_bytes2 = status_response2
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let status_api2: ApiResponse<TrackJobApiResponse> =
            serde_json::from_slice(&status_bytes2).unwrap();
        assert!(status_api2.success);
        assert_eq!(status_api2.data.as_ref().unwrap().status, "CANCELLED");
    }
}

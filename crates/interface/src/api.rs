// REST API con Axum
// Endpoints para gestión de jobs y providers

use axum::{
    extract::{Path, State, Json, Query},
    http::StatusCode,
    response::Json as AxumJson,
    routing::{get, post},
    Router,
};
use hodei_jobs_application::{
    job_execution_usecases::{
        CancelJobResponse, CancelJobUseCase, CreateJobUseCase, CreateJobRequest, CreateJobResponse,
        GetJobStatusUseCase, TrackJobResponse,
    },
    provider_usecases::{
        GetProviderResponse, GetProviderUseCase, ListProvidersRequest, ListProvidersResponse,
        ListProvidersUseCase, RegisterProviderRequest, RegisterProviderResponse,
        RegisterProviderUseCase,
    },
    ProviderRegistry,
};
use hodei_jobs_infrastructure::persistence::{
    DatabaseConfig, PostgresJobQueue, PostgresJobRepository, PostgresProviderConfigRepository,
};
use hodei_jobs_domain::provider_config::ProviderTypeConfig;
use serde::{Deserialize, Serialize};
use std::env;
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::cors::{CorsLayer, Any};
use tracing::{info, warn};

/// Trait para use case de crear job
#[async_trait::async_trait]
pub trait CreateJobUseCaseTrait: Send + Sync {
    async fn execute(&self, request: CreateJobRequest) -> Result<CreateJobResponse, hodei_jobs_domain::shared_kernel::DomainError>;
}

#[async_trait::async_trait]
impl RegisterProviderUseCaseTrait for hodei_jobs_application::provider_usecases::RegisterProviderUseCase {
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
}

#[derive(Serialize, Deserialize)]
pub struct JobSpecApiRequest {
    pub command: Vec<String>,
    pub image: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    pub working_dir: Option<String>,
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
        .route("/api/v1/providers", post(register_provider).get(list_providers))
        .route("/api/v1/providers/{provider_id}", get(get_provider))
        
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
        },
        correlation_id: request.correlation_id,
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
            .ok_or_else(|| anyhow::anyhow!("Missing database url (HODEI_DATABASE_URL or DATABASE_URL)"))?;

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

    fn to_database_config(&self) -> DatabaseConfig {
        DatabaseConfig {
            url: self.url.clone(),
            max_connections: self.max_connections,
            connection_timeout: self.connection_timeout,
        }
    }
}

async fn create_app_state_from_env() -> anyhow::Result<AppState> {
    let settings = ApiDatabaseSettings::from_env()?;
    let config = settings.to_database_config();

    let job_repository = PostgresJobRepository::connect(&config).await?;
    job_repository.run_migrations().await?;

    let job_queue = PostgresJobQueue::connect(&config).await?;
    job_queue.run_migrations().await?;

    let provider_repository = PostgresProviderConfigRepository::connect(&config).await?;
    provider_repository.run_migrations().await?;

    let provider_repository = std::sync::Arc::new(provider_repository)
        as std::sync::Arc<dyn hodei_jobs_domain::provider_config::ProviderConfigRepository>;
    let provider_registry = std::sync::Arc::new(ProviderRegistry::new(provider_repository));

    let job_repository = std::sync::Arc::new(job_repository)
        as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobRepository>;
    let job_queue = std::sync::Arc::new(job_queue)
        as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobQueue>;

    let create_job_usecase = CreateJobUseCase::new(job_repository.clone(), job_queue);
    let get_job_status_usecase = GetJobStatusUseCase::new(job_repository.clone());
    let cancel_job_usecase = CancelJobUseCase::new(job_repository);

    let register_provider_usecase = RegisterProviderUseCase::new(provider_registry.clone());
    let list_providers_usecase = ListProvidersUseCase::new(provider_registry.clone());
    let get_provider_usecase = GetProviderUseCase::new(provider_registry);

    Ok(AppState {
        create_job_usecase: std::sync::Arc::new(create_job_usecase),
        get_job_status_usecase: std::sync::Arc::new(get_job_status_usecase),
        cancel_job_usecase: std::sync::Arc::new(cancel_job_usecase),
        register_provider_usecase: std::sync::Arc::new(register_provider_usecase),
        list_providers_usecase: std::sync::Arc::new(list_providers_usecase),
        get_provider_usecase: std::sync::Arc::new(get_provider_usecase),
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
    async fn execute(&self, request: CreateJobRequest) -> Result<CreateJobResponse, hodei_jobs_domain::shared_kernel::DomainError> {
        self.execute(request).await
    }
}

#[async_trait::async_trait]
impl GetJobStatusUseCaseTrait for hodei_jobs_application::job_execution_usecases::GetJobStatusUseCase {
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
    use hodei_jobs_infrastructure::persistence::{FileBasedPersistence, PersistenceConfig};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn create_app_state_for_tests() -> AppState {
        let job_repository = std::sync::Arc::new(hodei_jobs_infrastructure::repositories::InMemoryJobRepository::new())
            as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobRepository>;
        let job_queue = std::sync::Arc::new(hodei_jobs_infrastructure::repositories::InMemoryJobQueue::new())
            as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobQueue>;

        let temp_dir = tempfile::tempdir().expect("tempdir");
        let data_directory = temp_dir.keep().to_string_lossy().to_string();

        let persistence = FileBasedPersistence::new(PersistenceConfig {
            data_directory,
            backup_enabled: false,
            auto_compact: false,
        });
        let provider_repo = hodei_jobs_infrastructure::persistence::FileBasedProviderConfigRepository::new(persistence);
        let provider_repo = std::sync::Arc::new(provider_repo)
            as std::sync::Arc<dyn hodei_jobs_domain::provider_config::ProviderConfigRepository>;
        let provider_registry = std::sync::Arc::new(ProviderRegistry::new(provider_repo));

        let create_job_usecase = CreateJobUseCase::new(job_repository.clone(), job_queue);
        let get_job_status_usecase = GetJobStatusUseCase::new(job_repository.clone());
        let cancel_job_usecase = CancelJobUseCase::new(job_repository);

        let register_provider_usecase = RegisterProviderUseCase::new(provider_registry.clone());
        let list_providers_usecase = ListProvidersUseCase::new(provider_registry.clone());
        let get_provider_usecase = GetProviderUseCase::new(provider_registry);

        AppState {
            create_job_usecase: std::sync::Arc::new(create_job_usecase),
            get_job_status_usecase: std::sync::Arc::new(get_job_status_usecase),
            cancel_job_usecase: std::sync::Arc::new(cancel_job_usecase),
            register_provider_usecase: std::sync::Arc::new(register_provider_usecase),
            list_providers_usecase: std::sync::Arc::new(list_providers_usecase),
            get_provider_usecase: std::sync::Arc::new(get_provider_usecase),
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
                "default_image": "hodei-worker:latest"
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
        let list_api: ApiResponse<ListProvidersResponse> = serde_json::from_slice(&list_bytes).unwrap();
        assert!(list_api.success);
        assert!(list_api
            .data
            .as_ref()
            .unwrap()
            .providers
            .iter()
            .any(|p| p.id == provider_id));

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
        let get_bytes = get_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let get_api: ApiResponse<GetProviderResponse> = serde_json::from_slice(&get_bytes).unwrap();
        assert!(get_api.success);
        assert_eq!(
            get_api.data.as_ref().unwrap().provider.as_ref().unwrap().id,
            provider_id
        );
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
        assert_eq!(result.url, "postgres://postgres:postgres@localhost:5432/postgres");
    }

    #[test]
    fn test_db_settings_parses_overrides() {
        let result = ApiDatabaseSettings::from_env_with(|key| match key {
            "DATABASE_URL" => Some("postgres://postgres:postgres@localhost:5432/postgres".to_string()),
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
        let create_api: ApiResponse<CreateJobResponse> = serde_json::from_slice(&create_bytes).unwrap();
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
        let status_api: ApiResponse<TrackJobApiResponse> = serde_json::from_slice(&status_bytes).unwrap();
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
        let cancel_api: ApiResponse<CancelJobResponse> = serde_json::from_slice(&cancel_bytes).unwrap();
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
        let status_api2: ApiResponse<TrackJobApiResponse> = serde_json::from_slice(&status_bytes2).unwrap();
        assert!(status_api2.success);
        assert_eq!(status_api2.data.as_ref().unwrap().status, "CANCELLED");
    }
}
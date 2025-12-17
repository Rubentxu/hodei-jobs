#![allow(dead_code)]
//! Common test infrastructure for E2E tests
//!
//! # Patrón Single Instance + Resource Pooling
//!
//! Esta implementación optimiza el uso de recursos computacionales mediante:
//!
//! 1. **Single Instance**: Un único container de Postgres compartido entre todos los tests
//!    - Se inicializa una sola vez usando `OnceCell`
//!    - El container persiste durante toda la ejecución de la suite de tests
//!    - Evita el overhead de crear/destruir containers por cada test
//!
//! 2. **Resource Pooling**: Bases de datos aisladas por test
//!    - Cada test obtiene su propia base de datos con nombre único
//!    - Las migraciones se ejecutan una vez por base de datos
//!    - Al finalizar el test, la base de datos se elimina automáticamente
//!
//! 3. **Connection Pooling**: Pool de conexiones reutilizables
//!    - Cada TestServer mantiene su propio pool de conexiones
//!    - Las conexiones se reutilizan dentro del mismo test
//!
//! ## Beneficios
//!
//! - **Velocidad**: ~10x más rápido que crear un container por test
//! - **Recursos**: Un solo container consume ~50MB vs ~50MB * N tests
//! - **Aislamiento**: Cada test tiene su propia base de datos limpia
//! - **Limpieza**: Las bases de datos se eliminan automáticamente
//!
//! ## Uso
//!
//! ```rust,ignore
//! #[tokio::test]
//! async fn my_test() {
//!     let stack = TestStack::without_provider().await.unwrap();
//!     // El stack tiene su propia base de datos aislada
//!     // Al salir del scope, la base de datos se elimina
//! }
//! ```
//!
//! Provides:
//! - PostgresTestDatabase: Testcontainers-based Postgres with database isolation
//! - TestServer: Full gRPC server with all services
//! - TestStack: Complete E2E stack (Postgres + Server + Provider)

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tokio::sync::{OnceCell, RwLock, oneshot};
use tonic::transport::{Channel, Server};

use sqlx::{Connection, PgConnection};

use hodei_jobs::{
    job_execution_service_client::JobExecutionServiceClient,
    job_execution_service_server::JobExecutionServiceServer,
    log_stream_service_server::LogStreamServiceServer,
    scheduler_service_client::SchedulerServiceClient,
    scheduler_service_server::SchedulerServiceServer,
    worker_agent_service_client::WorkerAgentServiceClient,
    worker_agent_service_server::WorkerAgentServiceServer,
};

use hodei_jobs_application::job_controller::JobController;
use hodei_jobs_application::job_execution_usecases::{CancelJobUseCase, CreateJobUseCase};
use hodei_jobs_application::smart_scheduler::SchedulerConfig;
use hodei_jobs_application::worker_provisioning_impl::{
    DefaultWorkerProvisioningService, ProvisioningConfig,
};
use hodei_jobs_domain::shared_kernel::ProviderId;
use hodei_jobs_domain::worker_provider::WorkerProvider;
use hodei_jobs_grpc::services::{
    JobExecutionServiceImpl, LogStreamService, LogStreamServiceGrpc, SchedulerServiceImpl,
    WorkerAgentServiceImpl,
};
use hodei_jobs_grpc::worker_command_sender::GrpcWorkerCommandSender;
use hodei_jobs_infrastructure::persistence::{
    DatabaseConfig, PostgresJobQueue, PostgresJobRepository, PostgresWorkerBootstrapTokenStore,
    PostgresWorkerRegistry,
};
use hodei_jobs_infrastructure::providers::{DockerProvider, TestWorkerProvider};

use hodei_jobs_domain::event_bus::{EventBus, EventBusError};

pub struct MockEventBus;

#[async_trait::async_trait]
impl EventBus for MockEventBus {
    async fn publish(
        &self,
        _event: &hodei_jobs_domain::events::DomainEvent,
    ) -> std::result::Result<(), EventBusError> {
        Ok(())
    }

    async fn subscribe(
        &self,
        _event_type: &str,
    ) -> std::result::Result<
        futures::stream::BoxStream<
            'static,
            std::result::Result<hodei_jobs_domain::events::DomainEvent, EventBusError>,
        >,
        EventBusError,
    > {
        // Return empty stream or unimplemented for now as we don't test subscription here yet
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

// =============================================================================
// Single Instance Pattern: Shared Postgres Container
// =============================================================================

/// Shared context for the single Postgres container instance.
/// This container is created once and reused across all tests.
struct SharedPostgresContext {
    /// The container handle - kept alive to prevent container shutdown
    _container: ContainerAsync<Postgres>,
    /// Admin connection string for creating/dropping databases
    admin_connection_string: String,
    /// Container host (usually localhost or Docker host)
    host: String,
    /// Mapped port for Postgres
    port: u16,
}

/// Global singleton for the Postgres container.
/// Uses tokio's OnceCell for thread-safe lazy initialization.
static POSTGRES_CONTEXT: OnceCell<SharedPostgresContext> = OnceCell::const_new();

/// Counter for unique database names (atomic for thread safety)
static DB_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// =============================================================================
// Resource Pooling Pattern: Isolated Database per Test
// =============================================================================

/// An isolated test database within the shared Postgres container.
///
/// Each test gets its own database with a unique name, providing:
/// - Complete isolation between tests
/// - Clean state for each test
/// - Automatic cleanup on drop
pub struct PostgresTestDatabase {
    /// Connection string for this specific database
    pub connection_string: String,
    /// Unique database name (e.g., "test_1234567890_0")
    db_name: String,
    /// Admin connection string for cleanup
    admin_connection_string: String,
}

impl PostgresTestDatabase {
    /// Get the database name
    pub fn db_name(&self) -> &str {
        &self.db_name
    }
}

impl Drop for PostgresTestDatabase {
    fn drop(&mut self) {
        let db_name = self.db_name.clone();
        let admin_connection_string = self.admin_connection_string.clone();

        // Spawn cleanup task in the current runtime
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };

        handle.spawn(async move {
            // Connect to admin database
            let Ok(mut conn) = PgConnection::connect(&admin_connection_string).await else {
                tracing::warn!("Failed to connect for cleanup of database: {}", db_name);
                return;
            };

            // Terminate all connections to this database
            let _ = sqlx::query(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1 AND pid <> pg_backend_pid();",
            )
            .bind(&db_name)
            .execute(&mut conn)
            .await;

            // Drop the database
            let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS {}", db_name))
                .execute(&mut conn)
                .await;

            tracing::debug!("Cleaned up test database: {}", db_name);
        });
    }
}

/// Get or create a Postgres test database.
///
/// This function implements the Single Instance + Resource Pooling pattern:
///
/// 1. **Single Instance**: The Postgres container is created only once (first call)
///    and reused for all subsequent calls. This is achieved using `OnceCell`.
///
/// 2. **Resource Pooling**: Each call creates a new isolated database within
///    the shared container. The database name is unique using an atomic counter.
///
/// # Performance
///
/// - First call: ~2-5 seconds (container startup)
/// - Subsequent calls: ~50-100ms (just database creation)
///
/// # Example
///
/// ```rust,ignore
/// let db1 = get_postgres_context().await?; // Creates container + db "test_0"
/// let db2 = get_postgres_context().await?; // Reuses container, creates "test_1"
/// // Both databases are isolated and can run tests in parallel
/// ```
pub async fn get_postgres_context() -> anyhow::Result<PostgresTestDatabase> {
    // Single Instance: Get or create the shared container
    let ctx = POSTGRES_CONTEXT
        .get_or_try_init(|| async {
            tracing::info!("Starting shared Postgres container (Single Instance pattern)...");

            let container = Postgres::default()
                .with_tag("16-alpine")
                .start()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to start Postgres container: {e}"))?;

            let host = container
                .get_host()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get host: {e}"))?;
            let host = host.to_string();
            let port = container
                .get_host_port_ipv4(5432)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get port: {e}"))?;

            let admin_connection_string =
                format!("postgres://postgres:postgres@{}:{}/postgres", host, port);

            tracing::info!("Postgres container ready at {}:{}", host, port);

            Ok::<SharedPostgresContext, anyhow::Error>(SharedPostgresContext {
                _container: container,
                admin_connection_string,
                host,
                port,
            })
        })
        .await?;

    // Resource Pooling: Create a unique database for this test
    // Use atomic counter for fast, unique names (faster than UUID)
    let db_id = DB_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let db_name = format!("test_{}_{}", timestamp, db_id);

    let mut conn = PgConnection::connect(&ctx.admin_connection_string)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to postgres admin db: {e}"))?;

    sqlx::query(&format!("CREATE DATABASE {}", db_name))
        .execute(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create test database: {e}"))?;

    let connection_string = format!(
        "postgres://postgres:postgres@{}:{}/{}",
        ctx.host, ctx.port, db_name
    );

    Ok(PostgresTestDatabase {
        connection_string,
        db_name,
        admin_connection_string: ctx.admin_connection_string.clone(),
    })
}

// =============================================================================
// TestServer - Full gRPC Server for E2E Tests
// =============================================================================

/// Configuration for TestServer
#[derive(Clone)]
pub struct TestServerConfig {
    pub db_url: String,
    pub enable_docker_provider: bool,
    pub enable_test_provider: bool,
    pub enable_job_controller: bool,
    pub worker_image: String,
    pub worker_binary_path: String,
    pub dev_mode: bool,
}

impl TestServerConfig {
    pub fn new(db_url: String) -> Self {
        Self {
            db_url,
            enable_docker_provider: false,
            enable_test_provider: false,
            enable_job_controller: true,
            worker_image: "hodei-jobs-worker:e2e-test".to_string(),
            worker_binary_path: "target/release/worker".to_string(),
            dev_mode: true,
        }
    }

    pub fn with_docker_provider(mut self) -> Self {
        self.enable_docker_provider = true;
        self
    }

    pub fn with_test_provider(mut self) -> Self {
        self.enable_test_provider = true;
        self
    }

    pub fn with_worker_image(mut self, image: String) -> Self {
        self.worker_image = image;
        self
    }

    pub fn with_worker_binary_path(mut self, path: String) -> Self {
        self.worker_binary_path = path;
        self
    }

    pub fn with_job_controller(mut self) -> Self {
        self.enable_job_controller = true;
        self
    }

    pub fn without_job_controller(mut self) -> Self {
        self.enable_job_controller = false;
        self
    }
}

/// A running test server with all services
pub struct TestServer {
    pub addr: SocketAddr,
    pub worker_service: WorkerAgentServiceImpl,
    shutdown_tx: Option<oneshot::Sender<()>>,
    controller_handle: Option<tokio::task::JoinHandle<()>>,
}

impl TestServer {
    /// Start a new test server with the given configuration
    pub async fn start(config: TestServerConfig) -> anyhow::Result<Self> {
        let db_config = DatabaseConfig {
            url: config.db_url.clone(),
            max_connections: 5,
            connection_timeout: Duration::from_secs(30),
        };

        // Connect to Postgres and run migrations
        let job_repository = PostgresJobRepository::connect(&db_config).await?;
        job_repository.run_migrations().await?;

        let job_queue = PostgresJobQueue::connect(&db_config).await?;
        job_queue.run_migrations().await?;

        let worker_registry = PostgresWorkerRegistry::connect(&db_config).await?;
        worker_registry.run_migrations().await?;

        let token_store = PostgresWorkerBootstrapTokenStore::connect(&db_config).await?;
        token_store.run_migrations().await?;

        // Arc wrappers
        let job_repository =
            Arc::new(job_repository) as Arc<dyn hodei_jobs_domain::job_execution::JobRepository>;
        let job_queue = Arc::new(job_queue) as Arc<dyn hodei_jobs_domain::job_execution::JobQueue>;
        let worker_registry = Arc::new(worker_registry)
            as Arc<dyn hodei_jobs_domain::worker_registry::WorkerRegistry>;
        let token_store = Arc::new(token_store)
            as Arc<dyn hodei_jobs_domain::otp_token_store::WorkerBootstrapTokenStore>;

        // Create use cases
        let event_bus = Arc::new(MockEventBus);
        let create_job_usecase =
            CreateJobUseCase::new(job_repository.clone(), job_queue.clone(), event_bus.clone());
        let cancel_job_usecase = CancelJobUseCase::new(job_repository.clone(), event_bus.clone());

        // Create log stream service
        let log_stream_service = LogStreamService::new();

        // Create worker service
        let worker_service =
            WorkerAgentServiceImpl::with_registry_job_repository_token_store_and_log_service(
                worker_registry.clone(),
                job_repository.clone(),
                token_store.clone(),
                log_stream_service.clone(),
                event_bus.clone(),
            );

        // Create job execution service
        let create_job_usecase2 =
            CreateJobUseCase::new(job_repository.clone(), job_queue.clone(), event_bus.clone());
        let job_service = JobExecutionServiceImpl::new(
            Arc::new(create_job_usecase2),
            Arc::new(cancel_job_usecase),
            job_repository.clone(),
            worker_registry.clone(),
        );

        // Find available port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        drop(listener);

        // Create provisioning service (with provider)
        let provisioning_service: Option<
            Arc<dyn hodei_jobs_application::worker_provisioning::WorkerProvisioningService>,
        > = if config.enable_docker_provider || config.enable_test_provider {
            // Initialize provider (Docker or Test)
            let mut providers: HashMap<ProviderId, Arc<dyn WorkerProvider>> = HashMap::new();

            if config.enable_docker_provider {
                let docker_provider = DockerProvider::new().await?;
                let provider_id = docker_provider.provider_id().clone();
                providers.insert(
                    provider_id,
                    Arc::new(docker_provider) as Arc<dyn WorkerProvider>,
                );
            }

            #[cfg(test)]
            if config.enable_test_provider {
                let test_provider =
                    TestWorkerProvider::with_worker_binary(config.worker_binary_path.clone())
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to create test provider: {}", e))?;
                let provider_id = test_provider.provider_id().clone();
                providers.insert(
                    provider_id,
                    Arc::new(test_provider) as Arc<dyn WorkerProvider>,
                );
            }

            let providers = Arc::new(RwLock::new(providers));

            // Server address for workers to connect back
            let server_address = if config.enable_test_provider {
                // Test provider connects directly to localhost
                format!("http://127.0.0.1:{}", addr.port())
            } else {
                // Docker provider needs host.docker.internal
                format!("http://host.docker.internal:{}", addr.port())
            };

            let provisioning_config = ProvisioningConfig::new(server_address)
                .with_default_image(config.worker_image.clone());

            Some(Arc::new(DefaultWorkerProvisioningService::new(
                worker_registry.clone(),
                token_store.clone(),
                providers,
                provisioning_config,
            ))
                as Arc<
                    dyn hodei_jobs_application::worker_provisioning::WorkerProvisioningService,
                >)
        } else {
            None
        };

        // Create scheduler service (with or without provisioning)
        let scheduler_service = if let Some(ref prov_svc) = provisioning_service {
            SchedulerServiceImpl::with_provisioning(
                Arc::new(create_job_usecase),
                job_repository.clone(),
                job_queue.clone(),
                worker_registry.clone(),
                SchedulerConfig::default(),
                prov_svc.clone(),
            )
        } else {
            SchedulerServiceImpl::new(
                Arc::new(create_job_usecase),
                job_repository.clone(),
                job_queue.clone(),
                worker_registry.clone(),
                SchedulerConfig::default(),
            )
        };

        let log_grpc_service = LogStreamServiceGrpc::new(log_stream_service);

        // Shutdown channel
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        // Clone for server
        let worker_service_clone = worker_service.clone();

        // Start gRPC server
        tokio::spawn(async move {
            let _ = Server::builder()
                .add_service(WorkerAgentServiceServer::new(worker_service_clone))
                .add_service(JobExecutionServiceServer::new(job_service))
                .add_service(SchedulerServiceServer::new(scheduler_service))
                .add_service(LogStreamServiceServer::new(log_grpc_service))
                .serve_with_shutdown(addr, async {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        // Start JobController if enabled
        let controller_handle = if config.enable_job_controller {
            let sender = Arc::new(GrpcWorkerCommandSender::new(worker_service.clone()))
                as Arc<dyn hodei_jobs_application::worker_command_sender::WorkerCommandSender>;

            let controller = Arc::new(JobController::new(
                job_queue,
                job_repository,
                worker_registry,
                SchedulerConfig::default(),
                sender,
                event_bus.clone(),
                provisioning_service.clone(),
            ));

            let handle = tokio::spawn(async move {
                loop {
                    if let Err(e) = controller.run_once().await {
                        tracing::debug!("JobController run_once: {}", e);
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            });
            Some(handle)
        } else {
            None
        };

        // Wait for server to be ready
        tokio::time::sleep(Duration::from_millis(100)).await;

        Ok(Self {
            addr,
            worker_service,
            shutdown_tx: Some(shutdown_tx),
            controller_handle,
        })
    }

    /// Get the server endpoint URL
    pub fn endpoint(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Create a WorkerAgentService client
    pub async fn worker_client(&self) -> anyhow::Result<WorkerAgentServiceClient<Channel>> {
        let channel = Channel::from_shared(self.endpoint())?.connect().await?;
        Ok(WorkerAgentServiceClient::new(channel))
    }

    /// Create a JobExecutionService client
    pub async fn job_client(&self) -> anyhow::Result<JobExecutionServiceClient<Channel>> {
        let channel = Channel::from_shared(self.endpoint())?.connect().await?;
        Ok(JobExecutionServiceClient::new(channel))
    }

    /// Create a SchedulerService client
    pub async fn scheduler_client(&self) -> anyhow::Result<SchedulerServiceClient<Channel>> {
        let channel = Channel::from_shared(self.endpoint())?.connect().await?;
        Ok(SchedulerServiceClient::new(channel))
    }

    /// Generate an OTP token for a worker
    pub async fn generate_otp(&self, worker_id: &str) -> anyhow::Result<String> {
        self.worker_service
            .generate_otp(worker_id)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to generate OTP: {}", e))
    }

    /// Shutdown the server
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.controller_handle.take() {
            handle.abort();
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.controller_handle.take() {
            handle.abort();
        }
    }
}

// =============================================================================
// TestStack - Complete E2E Stack
// =============================================================================

/// Complete E2E test stack: Postgres + Server + optional Provider
pub struct TestStack {
    pub db: PostgresTestDatabase,
    pub server: TestServer,
}

impl TestStack {
    /// Create a new test stack with Docker provider enabled (slow but realistic)
    pub async fn with_docker_provider() -> anyhow::Result<Self> {
        let db = get_postgres_context().await?;
        let config = TestServerConfig::new(db.connection_string.clone()).with_docker_provider();
        let server = TestServer::start(config).await?;
        Ok(Self { db, server })
    }

    /// Create a new test stack with TestWorkerProvider (FAST - ~10s)
    /// This is the preferred option for E2E tests as it's much faster
    pub async fn with_test_provider() -> anyhow::Result<Self> {
        let db = get_postgres_context().await?;

        // Calculate absolute path to worker binary
        let current_dir = std::env::current_dir()?;
        let project_root = current_dir
            .ancestors()
            .nth(2) // Go up from crates/grpc/tests to project root
            .unwrap_or(&current_dir);
        let worker_binary_path = project_root.join("target/release/worker");

        tracing::info!("Using worker binary at: {}", worker_binary_path.display());

        let config = TestServerConfig::new(db.connection_string.clone())
            .with_test_provider()
            .with_worker_binary_path(worker_binary_path.to_string_lossy().to_string());
        let server = TestServer::start(config).await?;
        Ok(Self { db, server })
    }

    /// Create a new test stack without any provider (manual worker registration)
    pub async fn without_provider() -> anyhow::Result<Self> {
        let db = get_postgres_context().await?;
        let config = TestServerConfig::new(db.connection_string.clone());
        let server = TestServer::start(config).await?;
        Ok(Self { db, server })
    }

    /// Create a new test stack with JobController enabled (for job dispatch tests)
    pub async fn with_job_controller() -> anyhow::Result<Self> {
        let db = get_postgres_context().await?;
        let config = TestServerConfig::new(db.connection_string.clone()).with_job_controller();
        let server = TestServer::start(config).await?;
        Ok(Self { db, server })
    }

    /// Shutdown the stack
    pub async fn shutdown(self) {
        self.server.shutdown().await;
        // db will be dropped and cleaned up automatically
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Check if Docker is available
pub async fn is_docker_available() -> bool {
    match DockerProvider::new().await {
        Ok(provider) => {
            matches!(
                provider.health_check().await,
                Ok(hodei_jobs_domain::worker_provider::HealthStatus::Healthy)
            )
        }
        Err(_) => false,
    }
}

/// Skip test if Docker is not available
#[macro_export]
macro_rules! skip_if_no_docker {
    () => {
        if !common::is_docker_available().await {
            eprintln!("Skipping test: Docker not available");
            return;
        }
    };
}

// =============================================================================
// Docker Verification Helpers
// =============================================================================

use std::process::Command;

/// Helper para verificar Docker en tests E2E
///
/// Usa el comando `docker` CLI directamente para evitar problemas de
/// permisos con bollard/hyper en algunos entornos.
pub struct DockerVerifier;

/// Información de un container Docker
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
}

impl ContainerInfo {
    pub fn is_running(&self) -> bool {
        self.state.to_lowercase().contains("running")
            || self.status.to_lowercase().starts_with("up")
    }
}

impl DockerVerifier {
    /// Crear un nuevo verificador de Docker
    pub fn new() -> anyhow::Result<Self> {
        // Verificar que docker CLI está disponible
        let output = Command::new("docker")
            .arg("version")
            .output()
            .map_err(|e| anyhow::anyhow!("Docker CLI not available: {}", e))?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Docker daemon not running"));
        }

        Ok(Self)
    }

    /// Verificar que Docker está disponible
    pub fn is_available(&self) -> bool {
        Command::new("docker")
            .args(["info"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Listar containers que coinciden con un patrón de nombre
    pub fn find_containers(&self, name_pattern: &str) -> anyhow::Result<Vec<ContainerInfo>> {
        let output = Command::new("docker")
            .args([
                "ps",
                "-a",
                "--filter",
                &format!("name={}", name_pattern),
                "--format",
                "{{.ID}}|{{.Names}}|{{.Image}}|{{.Status}}|{{.State}}",
            ])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to run docker ps: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("docker ps failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let containers: Vec<ContainerInfo> = stdout
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 5 {
                    Some(ContainerInfo {
                        id: parts[0].to_string(),
                        name: parts[1].to_string(),
                        image: parts[2].to_string(),
                        status: parts[3].to_string(),
                        state: parts[4].to_string(),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(containers)
    }

    /// Buscar containers de workers Hodei
    pub fn find_hodei_workers(&self) -> anyhow::Result<Vec<ContainerInfo>> {
        self.find_containers("hodei-jobs-worker")
    }

    /// Verificar si un container específico está corriendo
    pub fn is_container_running(&self, name_pattern: &str) -> anyhow::Result<bool> {
        let containers = self.find_containers(name_pattern)?;
        Ok(containers.iter().any(|c| c.is_running()))
    }

    /// Obtener todos los containers activos
    pub fn list_all_containers(&self) -> anyhow::Result<Vec<ContainerInfo>> {
        let output = Command::new("docker")
            .args([
                "ps",
                "-a",
                "--format",
                "{{.ID}}|{{.Names}}|{{.Image}}|{{.Status}}|{{.State}}",
            ])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to run docker ps: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("docker ps failed: {}", stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let containers: Vec<ContainerInfo> = stdout
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 5 {
                    Some(ContainerInfo {
                        id: parts[0].to_string(),
                        name: parts[1].to_string(),
                        image: parts[2].to_string(),
                        status: parts[3].to_string(),
                        state: parts[4].to_string(),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(containers)
    }
}

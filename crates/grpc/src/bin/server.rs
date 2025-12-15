//! Hodei Jobs gRPC Server
//!
//! Main entry point for the gRPC server.

use hodei_jobs::{
    FILE_DESCRIPTOR_SET, job_execution_service_server::JobExecutionServiceServer,
    log_stream_service_server::LogStreamServiceServer,
    metrics_service_server::MetricsServiceServer,
    providers::provider_management_service_server::ProviderManagementServiceServer,
    scheduler_service_server::SchedulerServiceServer,
    worker_agent_service_server::WorkerAgentServiceServer,
};
use hodei_jobs_application::job_controller::JobController;
use hodei_jobs_application::job_execution_usecases::{CancelJobUseCase, CreateJobUseCase};
use hodei_jobs_application::provider_registry::ProviderRegistry;
use hodei_jobs_application::smart_scheduler::SchedulerConfig;
use hodei_jobs_application::worker_provisioning_impl::{
    DefaultWorkerProvisioningService, ProvisioningConfig,
};
use hodei_jobs_domain::shared_kernel::ProviderId;
use hodei_jobs_domain::worker_provider::WorkerProvider;
use hodei_jobs_grpc::services::{
    JobExecutionServiceImpl, LogStreamService, LogStreamServiceGrpc, MetricsServiceImpl,
    ProviderManagementServiceImpl, SchedulerServiceImpl, WorkerAgentServiceImpl,
};
use hodei_jobs_infrastructure::persistence::{
    DatabaseConfig, PostgresJobQueue, PostgresJobRepository, PostgresProviderConfigRepository,
    PostgresWorkerBootstrapTokenStore, PostgresWorkerRegistry,
};
use hodei_jobs_infrastructure::providers::{
    DockerProvider, FirecrackerConfig, FirecrackerProvider, KubernetesConfig, KubernetesProvider,
};
use std::collections::HashMap;
use std::env;
use std::time::Duration;
use tokio::sync::RwLock;
use tonic::transport::Server;
use tonic_reflection::server::Builder as ReflectionBuilder;
use tonic_web::GrpcWebLayer;
use tower_http::cors::{Any, CorsLayer};
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

use hodei_jobs_grpc::worker_command_sender::GrpcWorkerCommandSender;

#[derive(Clone)]
struct GrpcDatabaseSettings {
    url: String,
    max_connections: u32,
    connection_timeout: Duration,
}

impl GrpcDatabaseSettings {
    fn from_env() -> anyhow::Result<Self> {
        let url = env::var("HODEI_DATABASE_URL")
            .or_else(|_| env::var("DATABASE_URL"))
            .map_err(|_| {
                anyhow::anyhow!("Missing database url (HODEI_DATABASE_URL or DATABASE_URL)")
            })?;

        let max_connections = env::var("HODEI_DB_MAX_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(10);

        let connection_timeout = env::var("HODEI_DB_CONNECTION_TIMEOUT_SECS")
            .ok()
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .with_target(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Get port from environment or default
    let port = env::var("GRPC_PORT").unwrap_or_else(|_| "50051".to_string());
    let addr = format!("0.0.0.0:{}", port).parse()?;

    // Check dev mode
    let dev_mode = env::var("HODEI_DEV_MODE").unwrap_or_default() == "1";

    info!("╔═══════════════════════════════════════════════════════════════╗");
    info!("║           Hodei Jobs Platform - gRPC Server                   ║");
    info!("╚═══════════════════════════════════════════════════════════════╝");
    info!("Starting server on {}", addr);
    if dev_mode {
        info!("🔓 Development mode ENABLED (accepting dev-* tokens)");
    }

    // Create shared log stream service
    let log_stream_service = LogStreamService::new();

    let db_settings = GrpcDatabaseSettings::from_env()?;
    let db_config = db_settings.to_database_config();

    let job_repository = PostgresJobRepository::connect(&db_config).await?;
    job_repository.run_migrations().await?;

    let job_queue = PostgresJobQueue::connect(&db_config).await?;
    job_queue.run_migrations().await?;

    let worker_registry = PostgresWorkerRegistry::connect(&db_config).await?;
    worker_registry.run_migrations().await?;

    let token_store = PostgresWorkerBootstrapTokenStore::connect(&db_config).await?;
    token_store.run_migrations().await?;

    let provider_config_repo = PostgresProviderConfigRepository::connect(&db_config).await?;
    provider_config_repo.run_migrations().await?;

    let job_repository = std::sync::Arc::new(job_repository)
        as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobRepository>;
    let job_queue = std::sync::Arc::new(job_queue)
        as std::sync::Arc<dyn hodei_jobs_domain::job_execution::JobQueue>;
    let worker_registry = std::sync::Arc::new(worker_registry)
        as std::sync::Arc<dyn hodei_jobs_domain::worker_registry::WorkerRegistry>;
    let token_store = std::sync::Arc::new(token_store)
        as std::sync::Arc<dyn hodei_jobs_domain::otp_token_store::WorkerBootstrapTokenStore>;
    let provider_config_repo = std::sync::Arc::new(provider_config_repo)
        as std::sync::Arc<dyn hodei_jobs_domain::provider_config::ProviderConfigRepository>;

    let provider_registry =
        std::sync::Arc::new(ProviderRegistry::new(provider_config_repo.clone()));

    let create_job_usecase = CreateJobUseCase::new(job_repository.clone(), job_queue.clone());
    let cancel_job_usecase = CancelJobUseCase::new(job_repository.clone());

    let scheduler_job_repository = job_repository.clone();
    let scheduler_job_queue = job_queue.clone();
    let scheduler_worker_registry = worker_registry.clone();
    let scheduler_create_job_usecase = CreateJobUseCase::new(
        scheduler_job_repository.clone(),
        scheduler_job_queue.clone(),
    );

    // Create services with shared log stream
    let worker_service =
        WorkerAgentServiceImpl::with_registry_job_repository_token_store_and_log_service(
            worker_registry.clone(),
            job_repository.clone(),
            token_store.clone(),
            log_stream_service.clone(),
        );
    let worker_service_for_controller = worker_service.clone();
    let job_repository_for_controller = job_repository.clone();
    let job_queue_for_controller = job_queue.clone();
    let worker_registry_for_controller = worker_registry.clone();
    let job_service = JobExecutionServiceImpl::new(
        std::sync::Arc::new(create_job_usecase),
        std::sync::Arc::new(cancel_job_usecase),
        job_repository,
        worker_registry,
    );
    let metrics_service = MetricsServiceImpl::new();
    let provider_management_service = ProviderManagementServiceImpl::new(provider_registry.clone());
    // Create provisioning service with Docker provider (HU-6.6)
    let provisioning_enabled =
        env::var("HODEI_PROVISIONING_ENABLED").unwrap_or_else(|_| "1".to_string()) == "1";

    let scheduler_service = if provisioning_enabled {
        // Initialize providers map
        let mut providers: HashMap<ProviderId, std::sync::Arc<dyn WorkerProvider>> = HashMap::new();

        // Initialize Docker provider
        let docker_enabled =
            env::var("HODEI_DOCKER_ENABLED").unwrap_or_else(|_| "1".to_string()) == "1";

        if docker_enabled {
            match DockerProvider::new().await {
                Ok(provider) => {
                    info!("  ✓ Docker provider initialized");
                    let provider_id = provider.provider_id().clone();
                    providers.insert(
                        provider_id,
                        std::sync::Arc::new(provider) as std::sync::Arc<dyn WorkerProvider>,
                    );
                }
                Err(e) => {
                    tracing::warn!("Docker provider not available: {}", e);
                }
            }
        }

        // Initialize Kubernetes provider (HU-7.8)
        let k8s_enabled = env::var("HODEI_K8S_ENABLED").unwrap_or_else(|_| "0".to_string()) == "1";

        if k8s_enabled {
            let k8s_config = KubernetesConfig::from_env().unwrap_or_default();
            match KubernetesProvider::with_config(k8s_config).await {
                Ok(provider) => {
                    info!(
                        "  ✓ Kubernetes provider initialized (namespace: {})",
                        env::var("HODEI_K8S_NAMESPACE")
                            .unwrap_or_else(|_| "hodei-workers".to_string())
                    );
                    let provider_id = provider.provider_id().clone();
                    providers.insert(
                        provider_id,
                        std::sync::Arc::new(provider) as std::sync::Arc<dyn WorkerProvider>,
                    );
                }
                Err(e) => {
                    tracing::warn!("Kubernetes provider not available: {}", e);
                }
            }
        }

        // Initialize Firecracker provider (HU-8.12)
        let fc_enabled = env::var("HODEI_FC_ENABLED").unwrap_or_else(|_| "0".to_string()) == "1";

        if fc_enabled {
            let fc_config = FirecrackerConfig::from_env().unwrap_or_default();
            match FirecrackerProvider::with_config(fc_config).await {
                Ok(provider) => {
                    info!(
                        "  ✓ Firecracker provider initialized (data_dir: {})",
                        env::var("HODEI_FC_DATA_DIR")
                            .unwrap_or_else(|_| "/var/lib/hodei/firecracker".to_string())
                    );
                    let provider_id = provider.provider_id().clone();
                    providers.insert(
                        provider_id,
                        std::sync::Arc::new(provider) as std::sync::Arc<dyn WorkerProvider>,
                    );
                }
                Err(e) => {
                    tracing::warn!("Firecracker provider not available: {}", e);
                }
            }
        }

        if !providers.is_empty() {
            let providers = std::sync::Arc::new(RwLock::new(providers));

            let server_address = format!(
                "http://{}:{}",
                env::var("HODEI_SERVER_HOST")
                    .unwrap_or_else(|_| "host.docker.internal".to_string()),
                port
            );
            let provisioning_config = ProvisioningConfig::new(server_address).with_default_image(
                env::var("HODEI_WORKER_IMAGE")
                    .unwrap_or_else(|_| "hodei-worker:latest".to_string()),
            );

            let provisioning_service = std::sync::Arc::new(DefaultWorkerProvisioningService::new(
                scheduler_worker_registry.clone(),
                token_store.clone(),
                providers,
                provisioning_config,
            ))
                as std::sync::Arc<
                    dyn hodei_jobs_application::worker_provisioning::WorkerProvisioningService,
                >;

            info!("  ✓ WorkerProvisioningService configured");

            SchedulerServiceImpl::with_provisioning(
                std::sync::Arc::new(scheduler_create_job_usecase),
                scheduler_job_repository,
                scheduler_job_queue,
                scheduler_worker_registry,
                SchedulerConfig::default(),
                provisioning_service,
            )
        } else {
            tracing::warn!("No providers available. Provisioning disabled.");
            SchedulerServiceImpl::new(
                std::sync::Arc::new(scheduler_create_job_usecase),
                scheduler_job_repository,
                scheduler_job_queue,
                scheduler_worker_registry,
                SchedulerConfig::default(),
            )
        }
    } else {
        info!("  ⚠ Provisioning disabled (HODEI_PROVISIONING_ENABLED != 1)");
        SchedulerServiceImpl::new(
            std::sync::Arc::new(scheduler_create_job_usecase),
            scheduler_job_repository,
            scheduler_job_queue,
            scheduler_worker_registry,
            SchedulerConfig::default(),
        )
    };
    let log_grpc_service = LogStreamServiceGrpc::new(log_stream_service);

    // Create reflection service
    let reflection_service = ReflectionBuilder::configure()
        .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
        .build_v1()?;

    info!("Services initialized:");
    info!("  ✓ WorkerAgentService (OTP auth)");
    info!("  ✓ JobExecutionService");
    info!("  ✓ MetricsService");
    info!("  ✓ SchedulerService");
    info!("  ✓ ProviderManagementService");
    info!("  ✓ LogStreamService (PRD v6.0)");
    info!("  ✓ Reflection Service");

    // JobController loop (HU-6.3+6.4)
    let controller_enabled =
        env::var("HODEI_JOB_CONTROLLER_ENABLED").unwrap_or_else(|_| "1".to_string()) == "1";
    let controller_interval_ms = env::var("HODEI_JOB_CONTROLLER_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(500);

    if controller_enabled {
        info!(
            "Starting JobController loop (interval={}ms)",
            controller_interval_ms
        );

        let sender =
            std::sync::Arc::new(GrpcWorkerCommandSender::new(worker_service_for_controller))
                as std::sync::Arc<
                    dyn hodei_jobs_application::worker_command_sender::WorkerCommandSender,
                >;

        let controller = std::sync::Arc::new(JobController::new(
            job_queue_for_controller,
            job_repository_for_controller,
            worker_registry_for_controller,
            SchedulerConfig::default(),
            sender,
        ));

        let interval = Duration::from_millis(controller_interval_ms);
        tokio::spawn(async move {
            loop {
                if let Err(e) = controller.run_once().await {
                    tracing::error!("JobController run_once failed: {}", e);
                }
                tokio::time::sleep(interval).await;
            }
        });
    } else {
        info!("JobController loop disabled (HODEI_JOB_CONTROLLER_ENABLED != 1)");
    }

    // Configure CORS for gRPC-Web (US-12.1.4)
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers(Any)
        .allow_methods(Any)
        .expose_headers(Any);

    info!("  ✓ gRPC-Web support enabled (CORS configured)");

    // Build and start server with gRPC-Web support
    Server::builder()
        .accept_http1(true)
        .layer(cors)
        .layer(GrpcWebLayer::new())
        .add_service(reflection_service)
        .add_service(WorkerAgentServiceServer::new(worker_service))
        .add_service(JobExecutionServiceServer::new(job_service))
        .add_service(MetricsServiceServer::new(metrics_service))
        .add_service(SchedulerServiceServer::new(scheduler_service))
        .add_service(ProviderManagementServiceServer::new(
            provider_management_service,
        ))
        .add_service(LogStreamServiceServer::new(log_grpc_service))
        .serve(addr)
        .await?;

    Ok(())
}

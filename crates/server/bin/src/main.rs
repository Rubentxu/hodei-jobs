//! Hodei Jobs gRPC Server
//!
//! Main entry point for the gRPC server with full provisioning support.

mod config;

use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use tokio::sync::RwLock;
use tonic::transport::Server;
use tonic_reflection::server::Builder as ReflectionBuilder;
use tonic_web::GrpcWebLayer;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

use hodei_jobs::{
    FILE_DESCRIPTOR_SET, job_execution_service_server::JobExecutionServiceServer,
    log_stream_service_server::LogStreamServiceServer,
    metrics_service_server::MetricsServiceServer,
    providers::provider_management_service_server::ProviderManagementServiceServer,
    scheduler_service_server::SchedulerServiceServer,
    worker_agent_service_server::WorkerAgentServiceServer,
};

use hodei_server_application::jobs::{CancelJobUseCase, CreateJobUseCase, JobController};
use hodei_server_application::providers::ProviderRegistry;
use hodei_server_application::scheduling::smart_scheduler::SchedulerConfig;
use hodei_server_application::workers::{DefaultWorkerProvisioningService, ProvisioningConfig};

use hodei_server_domain::shared_kernel::ProviderId;
use hodei_server_domain::workers::WorkerProvider;

use hodei_server_infrastructure::messaging::postgres::PostgresEventBus;
use hodei_server_infrastructure::persistence::postgres::{
    PostgresJobQueue, PostgresJobRepository, PostgresProviderConfigRepository,
    PostgresWorkerBootstrapTokenStore, PostgresWorkerRegistry,
};
use hodei_server_infrastructure::providers::docker::DockerProviderBuilder;

use hodei_server_interface::grpc::{
    GrpcWorkerCommandSender, JobExecutionServiceImpl, LogStreamService, LogStreamServiceGrpc,
    MetricsServiceImpl, ProviderManagementServiceImpl, SchedulerServiceImpl,
    WorkerAgentServiceImpl, context_interceptor,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging with env filter
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
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
        info!("🔓 Development mode ENABLED");
    }

    // Database configuration
    let db_url = env::var("SERVER_DATABASE_URL")
        .or_else(|_| env::var("HODEI_DATABASE_URL"))
        .or_else(|_| env::var("DATABASE_URL"))
        .map_err(|_| anyhow::anyhow!("Missing database URL"))?;

    let max_connections = env::var("HODEI_DB_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(10);

    let connection_timeout = env::var("HODEI_DB_CONNECTION_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(30));

    // Create shared Postgres Pool
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(connection_timeout)
        .connect(&db_url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database: {}", e))?;

    info!("Connected to database");

    // Create shared log stream service
    let log_stream_service = LogStreamService::new();

    // Create repositories and run migrations
    let job_repository_impl = PostgresJobRepository::new(pool.clone());
    job_repository_impl.run_migrations().await?;
    let job_repository = Arc::new(job_repository_impl);

    let job_queue_impl = PostgresJobQueue::new(pool.clone());
    job_queue_impl.run_migrations().await?;
    let job_queue = Arc::new(job_queue_impl);

    let worker_registry_impl = PostgresWorkerRegistry::new(pool.clone());
    worker_registry_impl.run_migrations().await?;
    let worker_registry = Arc::new(worker_registry_impl);

    let token_store_impl = PostgresWorkerBootstrapTokenStore::new(pool.clone());
    token_store_impl.run_migrations().await?;
    let token_store = Arc::new(token_store_impl);

    let provider_config_repo_impl = PostgresProviderConfigRepository::new(pool.clone());
    provider_config_repo_impl.run_migrations().await?;
    let provider_config_repo = Arc::new(provider_config_repo_impl);

    info!("Database migrations completed");

    // Create Event Bus
    let event_bus = Arc::new(PostgresEventBus::new(pool.clone()));

    // Create Provider Registry
    let provider_registry = Arc::new(ProviderRegistry::new(provider_config_repo.clone()));

    // Create Use Cases
    let create_job_usecase = Arc::new(CreateJobUseCase::new(
        job_repository.clone(),
        event_bus.clone(),
    ));
    let cancel_job_usecase = Arc::new(CancelJobUseCase::new(
        job_repository.clone(),
        event_bus.clone(),
    ));

    // Create gRPC services
    let worker_service = WorkerAgentServiceImpl::with_registry_and_log_service(
        worker_registry.clone(),
        log_stream_service.clone(),
        event_bus.clone(),
    );
    let worker_service_for_controller = worker_service.clone();

    let job_service = JobExecutionServiceImpl::new(
        create_job_usecase.clone(),
        cancel_job_usecase,
        job_repository.clone(),
        worker_registry.clone(),
    );

    let metrics_service = MetricsServiceImpl::new();
    let provider_management_service = ProviderManagementServiceImpl::new(provider_registry.clone());

    // Create provisioning service with Docker provider
    let provisioning_enabled =
        env::var("HODEI_PROVISIONING_ENABLED").unwrap_or_else(|_| "1".to_string()) == "1";

    // Initialize provisioning service (shared between SchedulerService and JobController)
    let provisioning_service: Option<Arc<DefaultWorkerProvisioningService>> =
        if provisioning_enabled {
            // Initialize providers map
            let mut providers: HashMap<ProviderId, Arc<dyn WorkerProvider>> = HashMap::new();

            // Load Docker provider config from DB or use default
            let docker_enabled =
                env::var("HODEI_DOCKER_ENABLED").unwrap_or_else(|_| "1".to_string()) == "1";

            if docker_enabled {
                // Try to find existing Docker provider in DB
                use hodei_server_domain::providers::ProviderConfigRepository;
                let docker_config: Option<hodei_server_domain::providers::ProviderConfig> =
                    provider_config_repo
                        .find_by_name("Docker")
                        .await
                        .ok()
                        .flatten();

                let provider_id = docker_config
                    .as_ref()
                    .map(|c| c.id.clone())
                    .unwrap_or_else(ProviderId::new);

                info!("Using Docker provider with ID: {}", provider_id);

                // Build DockerProvider with the DB provider_id
                match DockerProviderBuilder::new()
                    .with_provider_id(provider_id.clone())
                    .build()
                    .await
                {
                    Ok(provider) => {
                        info!("  ✓ Docker provider initialized (id: {})", provider_id);
                        providers
                            .insert(provider_id, Arc::new(provider) as Arc<dyn WorkerProvider>);
                    }
                    Err(e) => {
                        tracing::warn!("Docker provider not available: {}", e);
                    }
                }
            }

            if !providers.is_empty() {
                let providers = Arc::new(RwLock::new(providers));

                let server_address = format!(
                    "http://{}:{}",
                    env::var("HODEI_SERVER_HOST")
                        .unwrap_or_else(|_| "host.docker.internal".to_string()),
                    port
                );
                let provisioning_config = ProvisioningConfig::new(server_address)
                    .with_default_image(
                        env::var("HODEI_WORKER_IMAGE")
                            .unwrap_or_else(|_| "hodei-jobs-worker:dev".to_string()),
                    );

                let service = Arc::new(DefaultWorkerProvisioningService::new(
                    worker_registry.clone(),
                    token_store.clone(),
                    providers,
                    provisioning_config,
                ));

                info!("  ✓ WorkerProvisioningService configured");
                Some(service)
            } else {
                tracing::warn!("No providers available. Provisioning disabled.");
                None
            }
        } else {
            info!("  ⚠ Provisioning disabled (HODEI_PROVISIONING_ENABLED != 1)");
            None
        };

    // Create SchedulerService with or without provisioning
    let scheduler_service = if let Some(ref prov) = provisioning_service {
        SchedulerServiceImpl::with_provisioning(
            create_job_usecase.clone(),
            job_repository.clone(),
            job_queue.clone(),
            worker_registry.clone(),
            SchedulerConfig::default(),
            prov.clone(),
        )
    } else {
        SchedulerServiceImpl::new(
            create_job_usecase.clone(),
            job_repository.clone(),
            job_queue.clone(),
            worker_registry.clone(),
            SchedulerConfig::default(),
        )
    };

    let log_grpc_service = LogStreamServiceGrpc::new(log_stream_service);

    // Create reflection service
    let reflection_service = ReflectionBuilder::configure()
        .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
        .build_v1()?;

    info!("Services initialized:");
    info!("  ✓ WorkerAgentService");
    info!("  ✓ JobExecutionService");
    info!("  ✓ MetricsService");
    info!("  ✓ SchedulerService");
    info!("  ✓ ProviderManagementService");
    info!("  ✓ LogStreamService");
    info!("  ✓ Reflection Service");

    // JobController loop
    let controller_enabled =
        env::var("HODEI_JOB_CONTROLLER_ENABLED").unwrap_or_else(|_| "1".to_string()) == "1";
    if controller_enabled {
        info!("Starting JobController");

        let sender = Arc::new(GrpcWorkerCommandSender::new(worker_service_for_controller));

        // Pass provisioning_service to JobController for auto-provisioning workers
        let controller_provisioning = provisioning_service
            .clone()
            .map(|p| p as Arc<dyn hodei_server_application::workers::WorkerProvisioningService>);

        let controller = Arc::new(JobController::new(
            job_queue.clone(),
            job_repository.clone(),
            worker_registry.clone(),
            SchedulerConfig::default(),
            sender,
            event_bus.clone(),
            controller_provisioning,
        ));

        if let Err(e) = controller.clone().subscribe_to_events().await {
            tracing::error!("Failed to subscribe JobController to events: {}", e);
        }

        // Run initial sweep to process any pending jobs
        let sweep_controller = controller.clone();
        tokio::spawn(async move {
            info!("Running initial JobController sweep");
            loop {
                match sweep_controller.run_once().await {
                    Ok(0) => break, // Queue empty
                    Ok(count) => info!("Initial sweep processed {} jobs", count),
                    Err(e) => {
                        tracing::error!("Initial sweep failed: {}", e);
                        break;
                    }
                }
            }
        });
    } else {
        info!("JobController loop disabled (HODEI_JOB_CONTROLLER_ENABLED != 1)");
    }

    // Provider Manager (Auto-scaling & Health)
    if let Some(ref prov) = provisioning_service {
        use hodei_server_application::providers::ProviderManager;
        info!("Starting ProviderManager for auto-scaling and health monitoring");

        let manager = ProviderManager::new(event_bus.clone(), prov.clone(), job_queue.clone());

        if let Err(e) = manager.subscribe_to_events().await {
            tracing::error!("Failed to subscribe ProviderManager to events: {}", e);
        }
    }

    // Configure CORS for gRPC-Web
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers(Any)
        .allow_methods(Any)
        .expose_headers(Any);

    info!("  ✓ gRPC-Web support enabled");
    info!("  ✓ Context interceptor enabled");

    // Build and start server
    Server::builder()
        .accept_http1(true)
        .layer(cors)
        .layer(GrpcWebLayer::new())
        .add_service(reflection_service)
        .add_service(WorkerAgentServiceServer::with_interceptor(
            worker_service,
            context_interceptor,
        ))
        .add_service(JobExecutionServiceServer::with_interceptor(
            job_service,
            context_interceptor,
        ))
        .add_service(MetricsServiceServer::new(metrics_service))
        .add_service(SchedulerServiceServer::with_interceptor(
            scheduler_service,
            context_interceptor,
        ))
        .add_service(ProviderManagementServiceServer::with_interceptor(
            provider_management_service,
            context_interceptor,
        ))
        .add_service(LogStreamServiceServer::new(log_grpc_service))
        .serve(addr)
        .await?;

    Ok(())
}

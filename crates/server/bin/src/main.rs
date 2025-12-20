//! Hodei Jobs gRPC Server
//!
//! Main entry point for the gRPC server with full provisioning support.

mod config;
#[cfg(test)]
mod tests_integration;

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
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
use uuid::Uuid;

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
    LogStorageRepository, PostgresJobQueue, PostgresJobRepository,
    PostgresProviderConfigRepository, PostgresWorkerBootstrapTokenStore, PostgresWorkerRegistry,
};
use hodei_server_infrastructure::providers::docker::DockerProviderBuilder;

use hodei_server_interface::grpc::{
    GrpcWorkerCommandSender, JobExecutionServiceImpl, LogStreamService, LogStreamServiceGrpc,
    MetricsServiceImpl, ProviderManagementServiceImpl, SchedulerServiceImpl,
    WorkerAgentServiceImpl, context_interceptor,
};
use hodei_server_interface::log_persistence::{
    LocalStorageConfig, LogPersistenceConfig, LogStorage, LogStorageFactory, LogStorageRef,
    StorageBackend,
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

    // Initialize log persistence configuration
    let server_config =
        config::ServerConfig::new().map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;
    let persistence_config = server_config.to_log_persistence_config();

    // Create storage backend (agnostic - local, S3, etc.)
    let storage_backend: Box<dyn LogStorage> = LogStorageFactory::create(&persistence_config);

    // Create log storage repository
    let log_storage_repo = Arc::new(LogStorageRepository::new(pool.clone()));

    // Create callback to save log references to database
    let log_repo_for_callback = log_storage_repo.clone();
    let log_ttl_hours = persistence_config.ttl_hours;
    let on_log_finalized = Arc::new(move |log_ref: LogStorageRef| {
        let repo = log_repo_for_callback.clone();
        let ttl_hours = log_ttl_hours;

        tokio::spawn(async move {
            use hodei_server_domain::shared_kernel::JobId;
            let job_id = JobId(Uuid::parse_str(&log_ref.job_id).unwrap_or_default());

            let log_storage_ref =
                hodei_server_infrastructure::persistence::LogStorageReference::new(
                    job_id,
                    log_ref.storage_uri,
                    log_ref.size_bytes,
                    log_ref.entry_count,
                    ttl_hours,
                );

            if let Err(e) = repo.save(&log_storage_ref).await {
                tracing::error!("Failed to save log storage reference to database: {}", e);
            } else {
                tracing::info!(
                    "✅ Log storage reference saved to database: {} ({} bytes, {} entries)",
                    log_ref.job_id,
                    log_ref.size_bytes,
                    log_ref.entry_count
                );
            }
        });
    });

    // Create shared log stream service with persistence
    let log_stream_service: Arc<LogStreamService> = if persistence_config.enabled {
        info!(
            "📝 Log persistence enabled with storage backend: {}",
            match &persistence_config.storage_backend {
                StorageBackend::Local(_) => "local",
                // StorageBackend::S3(_) => "s3",
                _ => "unknown",
            }
        );

        Arc::new(LogStreamService::with_storage(
            storage_backend,
            Some(on_log_finalized),
        ))
    } else {
        info!("⚠️ Log persistence disabled");
        Arc::new(LogStreamService::new())
    };

    // Create background log cleanup service
    if persistence_config.enabled {
        let cleanup_interval_hours = env::var("LOG_CLEANUP_INTERVAL_HOURS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(24); // Default: run cleanup every 24 hours

        let storage_for_cleanup = LogStorageFactory::create(&persistence_config);
        let repo_for_cleanup = log_storage_repo.clone();
        let cleanup_ttl_hours = persistence_config.ttl_hours;

        // Spawn background cleanup task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
                cleanup_interval_hours * 3600,
            ));

            // Run cleanup immediately on startup
            info!(
                "🧹 Running initial log cleanup (TTL: {} hours)",
                cleanup_ttl_hours
            );
            if let Err(e) = repo_for_cleanup.cleanup_expired_with_uris().await {
                tracing::error!("Failed to cleanup expired logs from database: {}", e);
            }
            if let Err(e) = storage_for_cleanup
                .cleanup_old_logs(cleanup_ttl_hours)
                .await
            {
                tracing::error!("Failed to cleanup old logs from storage: {}", e);
            }

            // Then run periodically
            loop {
                interval.tick().await;
                info!(
                    "🧹 Running scheduled log cleanup (TTL: {} hours)",
                    cleanup_ttl_hours
                );

                // Cleanup database references
                if let Err(e) = repo_for_cleanup.cleanup_expired_with_uris().await {
                    tracing::error!("Failed to cleanup expired logs from database: {}", e);
                }

                // Cleanup storage
                if let Err(e) = storage_for_cleanup
                    .cleanup_old_logs(cleanup_ttl_hours)
                    .await
                {
                    tracing::error!("Failed to cleanup old logs from storage: {}", e);
                }
            }
        });

        info!(
            "🧹 Log cleanup service scheduled (interval: {} hours, TTL: {} hours)",
            cleanup_interval_hours, persistence_config.ttl_hours
        );
    }

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
        job_queue.clone(),
        event_bus.clone(),
    ));
    let cancel_job_usecase = Arc::new(CancelJobUseCase::new(
        job_repository.clone(),
        event_bus.clone(),
    ));

    // Create gRPC services
    let worker_service =
        WorkerAgentServiceImpl::with_registry_job_repository_token_store_and_log_service(
            worker_registry.clone(),
            job_repository.clone(),
            token_store.clone(),
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

                // Get existing provider ID or create new one
                let provider_id = docker_config
                    .as_ref()
                    .map(|c| c.id.clone())
                    .unwrap_or_else(ProviderId::new);

                info!("Using Docker provider with ID: {}", provider_id);

                // Save provider to database if it doesn't exist
                if docker_config.is_none() {
                    use hodei_server_domain::ProviderType;
                    use hodei_server_domain::providers::{DockerConfig, ProviderTypeConfig};

                    // IMPORTANT: Use the same provider_id for the config
                    let docker_provider_config =
                        hodei_server_domain::providers::ProviderConfig::with_id(
                            provider_id.clone(),
                            "Docker".to_string(),
                            ProviderType::Docker,
                            ProviderTypeConfig::Docker(DockerConfig::default()),
                        );

                    if let Err(e) = provider_config_repo.save(&docker_provider_config).await {
                        tracing::warn!("Failed to save Docker provider to database: {}", e);
                    } else {
                        info!("  ✓ Docker provider saved to database");
                    }
                }

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
                            .unwrap_or_else(|_| "hodei-jobs-worker:latest".to_string()),
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

        let sender = Arc::new(GrpcWorkerCommandSender::new(
            worker_service_for_controller.clone(),
        ));

        // Pass provisioning_service to JobController for auto-provisioning workers
        let controller_provisioning = provisioning_service
            .clone()
            .map(|p| p as Arc<dyn hodei_server_application::workers::WorkerProvisioningService>);

        let controller = Arc::new(tokio::sync::Mutex::new(JobController::new(
            job_queue.clone(),
            job_repository.clone(),
            worker_registry.clone(),
            provider_registry.clone(),
            SchedulerConfig::default(),
            sender,
            event_bus.clone(),
            controller_provisioning,
        )));

        // Keep the controller alive for the entire server lifetime
        let controller_guard = Arc::clone(&controller);

        // Start the JobController (starts continuous processing loop)
        tokio::spawn(async move {
            info!("Starting JobController processing loop");
            {
                let mut controller = controller_guard.lock().await;
                if let Err(e) = controller.start().await {
                    tracing::error!("Failed to start JobController: {}", e);
                }
            }
            info!("JobController processing loop ended");
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

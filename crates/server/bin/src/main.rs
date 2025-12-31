//! Hodei Jobs gRPC Server
//!
//! Main entry point for the gRPC server with full provisioning support.

mod config;
#[cfg(test)]
mod tests_integration;

use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
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
use hodei_server_application::saga::dispatcher_saga::DynExecutionSagaDispatcher;
use hodei_server_application::saga::provisioning_saga::DynProvisioningSagaCoordinator;
use hodei_server_application::saga::recovery_saga::DynRecoverySagaCoordinator;
use hodei_server_application::scheduling::smart_scheduler::SchedulerConfig;
use hodei_server_application::workers::{
    DefaultWorkerProvisioningService, ProvisioningConfig, WorkerLifecycleConfig,
    WorkerLifecycleManager,
};
use hodei_server_domain::saga::{SagaOrchestrator, SagaOrchestratorConfig};
use hodei_server_infrastructure::persistence::postgres::saga_repository::{
    PostgresSagaOrchestrator, PostgresSagaOrchestratorConfig,
};

use hodei_server_domain::providers::ProviderConfigRepository;
use hodei_server_domain::shared_kernel::ProviderId;
use hodei_server_domain::workers::WorkerProvider;

use hodei_server_infrastructure::messaging::OutboxEventBus;
use hodei_server_infrastructure::messaging::outbox_relay::OutboxRelay;
use hodei_server_infrastructure::messaging::postgres::PostgresEventBus;
use hodei_server_infrastructure::persistence::outbox::PostgresOutboxRepository;
use hodei_server_infrastructure::persistence::postgres::PostgresSagaRepository;
use hodei_server_infrastructure::persistence::postgres::{
    LogStorageRepository, PostgresJobQueue, PostgresJobRepository,
    PostgresProviderConfigRepository, PostgresWorkerBootstrapTokenStore, PostgresWorkerRegistry,
};
use hodei_server_infrastructure::providers::docker::DockerProviderBuilder;
use hodei_server_infrastructure::providers::kubernetes::{
    KubernetesConfig, KubernetesConfigBuilder, KubernetesProviderBuilder,
};

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
        let backend_name = if let StorageBackend::Local(_) = &persistence_config.storage_backend {
            "local"
        } else {
            "unknown"
        };
        info!(
            "📝 Log persistence enabled with storage backend: {}",
            backend_name
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

    // Create Real Event Bus (Postgres/Notify)
    let real_event_bus_impl = PostgresEventBus::new(pool.clone());
    real_event_bus_impl
        .run_migrations()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run event bus migrations: {}", e))?;
    let real_event_bus = Arc::new(real_event_bus_impl);

    // Create Outbox Repository
    // Transactional Outbox Pattern enabled
    let outbox_repository_impl: PostgresOutboxRepository =
        PostgresOutboxRepository::new(pool.clone());
    outbox_repository_impl
        .run_migrations()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run outbox migrations: {}", e))?;
    let outbox_repository = Arc::new(outbox_repository_impl);

    // Start OutboxRelay background service
    // Reads from OutboxRepo and publishes to RealEventBus
    let outbox_relay = OutboxRelay::new_with_defaults(pool.clone(), real_event_bus.clone());
    let outbox_relay = Arc::new(outbox_relay);

    let outbox_relay_clone = outbox_relay.clone();
    tokio::spawn(async move {
        if let Err(e) = outbox_relay_clone.run().await {
            tracing::error!("OutboxRelay stopped with error: {}", e);
        }
    });

    info!("  ✓ OutboxRelay started (Transactional Outbox Pattern enabled)");

    // Create App Event Bus (Outbox Adapter) and SHADOW 'event_bus' variable
    // All downstream services will use this Outbox-aware bus
    let event_bus = Arc::new(OutboxEventBus::new(
        outbox_repository.clone(),
        real_event_bus.clone(),
    ));

    // Cleanup orphaned workers from previous runs
    // Remove workers in TERMINATING or TERMINATED state
    info!("  Cleaning up terminated workers from previous runs...");
    sqlx::query("DELETE FROM workers WHERE state = 'TERMINATING' OR state = 'TERMINATED'")
        .execute(&pool)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to cleanup terminated workers: {}", e))?;
    info!("  ✓ Terminated workers cleaned up");

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

    // EPIC-31: Create providers map early for shared access
    // This is used by JobExecutionService for worker cleanup notifications
    let providers: Arc<RwLock<HashMap<ProviderId, Arc<dyn WorkerProvider>>>> =
        Arc::new(RwLock::new(HashMap::new()));

    // EPIC-31: Use with_cleanup_support to enable JobQueued event publishing
    // The event_bus is required for reactive job processing (JobCoordinator)
    let job_service = JobExecutionServiceImpl::with_cleanup_support(
        create_job_usecase.clone(),
        cancel_job_usecase,
        job_repository.clone(),
        worker_registry.clone(),
        providers.clone(), // Empty now, will be populated when providers are initialized
        None,              // outbox_repository
        Some(event_bus.clone()), // EPIC-31 FIX: event_bus required for JobQueued events
    );

    let metrics_service = MetricsServiceImpl::new();
    let provider_management_service = ProviderManagementServiceImpl::new(provider_registry.clone());

    // Create provisioning service with Docker provider
    let provisioning_enabled =
        env::var("HODEI_PROVISIONING_ENABLED").unwrap_or_else(|_| "1".to_string()) == "1";

    // Initialize provisioning service (shared between SchedulerService and JobController)
    let provisioning_service: Option<Arc<DefaultWorkerProvisioningService>> =
        if provisioning_enabled {
            // Populate providers map (already created at line 353)
            let mut providers_map: HashMap<ProviderId, Arc<dyn WorkerProvider>> = HashMap::new();

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
                        providers_map
                            .insert(provider_id, Arc::new(provider) as Arc<dyn WorkerProvider>);
                    }
                    Err(e) => {
                        tracing::warn!("Docker provider not available: {}", e);
                    }
                }
            }

            // Initialize Kubernetes provider
            let kubernetes_enabled =
                env::var("HODEI_KUBERNETES_ENABLED").unwrap_or_else(|_| "1".to_string()) == "1";

            if kubernetes_enabled {
                use hodei_server_domain::providers::ProviderConfigRepository;
                // Try to find existing Kubernetes provider in DB
                let k8s_config: Option<hodei_server_domain::providers::ProviderConfig> =
                    provider_config_repo
                        .find_by_name("Kubernetes")
                        .await
                        .ok()
                        .flatten();

                // Get existing provider ID or create new one
                let provider_id = k8s_config
                    .as_ref()
                    .map(|c| c.id.clone())
                    .unwrap_or_else(ProviderId::new);

                info!("Using Kubernetes provider with ID: {}", provider_id);

                // Save provider to database if it doesn't exist
                if k8s_config.is_none() {
                    use hodei_server_domain::ProviderType;
                    use hodei_server_domain::providers::{
                        KubernetesConfig as DomainKubernetesConfig, ProviderTypeConfig,
                    };

                    // Load Kubernetes config from environment or use default
                    let k8s_config_for_db = std::env::var("HODEI_K8S_KUBECONFIG")
                        .map(|path| {
                            KubernetesConfig::builder()
                                .kubeconfig_path(path)
                                .namespace(
                                    std::env::var("HODEI_K8S_NAMESPACE")
                                        .unwrap_or_else(|_| "hodei-jobs-workers".to_string()),
                                )
                                .build()
                                .unwrap_or_else(|_| KubernetesConfig::default())
                        })
                        .unwrap_or_else(|_| KubernetesConfig::default());

                    // Convert to domain type
                    let domain_k8s_config = DomainKubernetesConfig {
                        kubeconfig_path: k8s_config_for_db.kubeconfig_path,
                        namespace: k8s_config_for_db.namespace,
                        service_account: k8s_config_for_db.service_account.unwrap_or_default(),
                        default_image: "hodei-jobs-worker:latest".to_string(), // Default image
                        image_pull_secrets: k8s_config_for_db.image_pull_secrets,
                        node_selector: k8s_config_for_db.node_selector,
                        tolerations: vec![], // Convert tolerations if needed
                        default_resources: None,
                    };

                    // IMPORTANT: Use the same provider_id for the config
                    let k8s_provider_config =
                        hodei_server_domain::providers::ProviderConfig::with_id(
                            provider_id.clone(),
                            "Kubernetes".to_string(),
                            ProviderType::Kubernetes,
                            ProviderTypeConfig::Kubernetes(domain_k8s_config),
                        );

                    if let Err(e) = provider_config_repo.save(&k8s_provider_config).await {
                        tracing::warn!("Failed to save Kubernetes provider to database: {}", e);
                    } else {
                        info!("  ✓ Kubernetes provider saved to database");
                    }
                }

                // Build KubernetesProvider with proper kubeconfig configuration
                let k8s_provider = {
                    // Try to load kubeconfig from environment or use default path
                    let k8s_config = std::env::var("HODEI_K8S_KUBECONFIG")
                        .map(|path| {
                            KubernetesConfig::builder()
                                .kubeconfig_path(path)
                                .namespace(
                                    std::env::var("HODEI_K8S_NAMESPACE")
                                        .unwrap_or_else(|_| "hodei-jobs-workers".to_string()),
                                )
                                .build()
                        })
                        .unwrap_or_else(|_| {
                            // Fallback to default path
                            KubernetesConfig::builder()
                                .kubeconfig_path("/home/rubentxu/.kube/config")
                                .namespace("hodei-jobs-workers".to_string())
                                .build()
                        });

                    match k8s_config {
                        Ok(config) => {
                            info!(
                                "Using Kubernetes config from: {}",
                                config
                                    .kubeconfig_path
                                    .as_deref()
                                    .unwrap_or("default inference")
                            );
                            KubernetesProviderBuilder::new()
                                .with_provider_id(provider_id.clone())
                                .with_config(config)
                                .build()
                                .await
                        }
                        Err(e) => {
                            tracing::warn!("Failed to build Kubernetes config: {}", e);
                            KubernetesProviderBuilder::new()
                                .with_provider_id(provider_id.clone())
                                .build()
                                .await
                        }
                    }
                };

                match k8s_provider {
                    Ok(provider) => {
                        info!("  ✓ Kubernetes provider initialized (id: {})", provider_id);
                        providers_map
                            .insert(provider_id, Arc::new(provider) as Arc<dyn WorkerProvider>);
                    }
                    Err(e) => {
                        tracing::warn!("Kubernetes provider not available: {}", e);
                    }
                }
            }

            if !providers_map.is_empty() {
                // EPIC-31: Populate the shared providers map (already created at line 354)
                // This map is shared between JobExecutionService and other services
                let mut providers_write = providers.write().await;
                *providers_write = providers_map;
                drop(providers_write);

                // EPIC-30: Saga Infrastructure - Disabled for now (needs full implementation)
                info!(
                    "⚠️ Saga infrastructure initialized (orchestration disabled - using legacy flow)"
                );

                // Server address for worker provisioning
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
                    providers.clone(),
                    provisioning_config,
                ));

                info!("  ✓ WorkerProvisioningService configured");

                // Lifecycle Manager without saga coordinator (legacy mode)
                let mut lifecycle_manager = WorkerLifecycleManager::new(
                    worker_registry.clone(),
                    providers.clone(),
                    WorkerLifecycleConfig::default(),
                    event_bus.clone(),
                );

                info!("  ✓ WorkerLifecycleManager started (legacy mode)");

                let lifecycle_manager = Arc::new(lifecycle_manager);

                // Spawn background cleanup task
                let cleanup_manager = lifecycle_manager.clone();
                tokio::spawn(async move {
                    // EPIC-29: Start reactive event monitoring from providers
                    cleanup_manager.start_event_monitoring().await;

                    // Optimization: Polling loop reduced to 5 minutes as safety net (reconciliation)
                    // Primary lifecycle management is now event-driven.
                    let mut interval = tokio::time::interval(Duration::from_secs(300));
                    loop {
                        interval.tick().await;
                        // Run health check (check heartbeats)
                        if let Err(e) = cleanup_manager.run_health_check().await {
                            tracing::error!("Health check failed: {}", e);
                        }
                        // Run cleanup (terminate idle/expired)
                        if let Err(e) = cleanup_manager.cleanup_workers().await {
                            tracing::error!("Worker cleanup failed: {}", e);
                        }
                    }
                });

                info!("  ✓ WorkerLifecycleManager started (saga-based provisioning enabled)");

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
    // EPIC-29: Always use with_event_bus for reactive processing
    let scheduler_service = if let Some(ref prov) = provisioning_service {
        SchedulerServiceImpl::with_event_bus(
            create_job_usecase.clone(),
            job_repository.clone(),
            job_queue.clone(),
            worker_registry.clone(),
            SchedulerConfig::default(),
            event_bus.clone(),
        )
    } else {
        SchedulerServiceImpl::with_event_bus(
            create_job_usecase.clone(),
            job_repository.clone(),
            job_queue.clone(),
            worker_registry.clone(),
            SchedulerConfig::default(),
            event_bus.clone(),
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

    // ==========================================================================
    // EPIC-31: Saga Infrastructure Initialization
    // ==========================================================================
    info!("Initializing saga infrastructure...");

    // Create Saga Repository with PostgreSQL persistence
    let saga_repository_impl = PostgresSagaRepository::new(pool.clone());
    saga_repository_impl
        .run_migrations()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run saga migrations: {}", e))?;
    let saga_repository = Arc::new(saga_repository_impl);

    // Create Saga Orchestrator Configuration from environment variables
    let saga_config = PostgresSagaOrchestratorConfig {
        max_concurrent_sagas: env::var("HODEI_SAGA_MAX_CONCURRENT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100),
        max_concurrent_steps: env::var("HODEI_SAGA_MAX_CONCURRENT_STEPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10),
        step_timeout: Duration::from_secs(
            env::var("HODEI_SAGA_STEP_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
        ),
        saga_timeout: Duration::from_secs(
            env::var("HODEI_SAGA_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
        ),
        max_retries: env::var("HODEI_SAGA_MAX_RETRIES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3),
        retry_backoff: Duration::from_secs(
            env::var("HODEI_SAGA_RETRY_BACKOFF")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1),
        ),
    };

    // Create Production-Ready Saga Orchestrator with PostgreSQL persistence
    let orchestrator: Arc<
        dyn hodei_server_domain::saga::SagaOrchestrator<
                Error = hodei_server_domain::shared_kernel::DomainError,
            > + Send
            + Sync,
    > = Arc::new(PostgresSagaOrchestrator::new(
        saga_repository.clone(),
        Some(saga_config),
    ));

    info!("  ✓ Saga orchestrator initialized with PostgreSQL repository");

    // Create Provisioning Saga Coordinator (always enabled)
    let config =
        hodei_server_application::saga::provisioning_saga::ProvisioningSagaCoordinatorConfig {
            saga_timeout: Duration::from_secs(
                env::var("HODEI_SAGA_PROVISIONING_TIMEOUT")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(300),
            ),
            step_timeout: Duration::from_secs(
                env::var("HODEI_SAGA_PROVISIONING_STEP_TIMEOUT")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60),
            ),
        };

    let provisioning_coordinator = Arc::new(DynProvisioningSagaCoordinator::new(
        orchestrator.clone(),
        provisioning_service
            .clone()
            .expect("Provisioning service is required for saga coordinator"),
        Some(config),
    ));

    info!("  ✓ ProvisioningSagaCoordinator initialized");

    // Create Execution Saga Dispatcher (always enabled)
    let config = hodei_server_application::saga::dispatcher_saga::ExecutionSagaDispatcherConfig {
        saga_timeout: Duration::from_secs(
            env::var("HODEI_SAGA_EXECUTION_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
        ),
        step_timeout: Duration::from_secs(
            env::var("HODEI_SAGA_EXECUTION_STEP_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
        ),
    };

    let execution_dispatcher = Arc::new(DynExecutionSagaDispatcher::new(
        orchestrator.clone(),
        job_repository.clone(),
        worker_registry.clone(),
        Some(config),
    ));

    info!("  ✓ ExecutionSagaDispatcher initialized");

    // Create Recovery Saga Coordinator (always enabled)
    let config = hodei_server_application::saga::recovery_saga::RecoverySagaCoordinatorConfig {
        saga_timeout: Duration::from_secs(
            env::var("HODEI_SAGA_RECOVERY_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
        ),
        step_timeout: Duration::from_secs(
            env::var("HODEI_SAGA_RECOVERY_STEP_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
        ),
    };

    let recovery_coordinator = Arc::new(DynRecoverySagaCoordinator::new(
        orchestrator.clone(),
        Some(config),
    ));

    info!("  ✓ RecoverySagaCoordinator initialized");

    info!("Saga infrastructure ready");

    // JobController loop - EPIC-31: Now with saga coordinators connected
    let controller_enabled =
        env::var("HODEI_JOB_CONTROLLER_ENABLED").unwrap_or_else(|_| "1".to_string()) == "1";

    // Try to create JobController with saga coordinators (if provisioning_service is Some)
    if controller_enabled {
        if let Some(ref prov) = provisioning_service {
            // Only start controller when saga infrastructure is available
            info!("Starting JobController with saga coordinators");

            let sender = Arc::new(GrpcWorkerCommandSender::new(
                worker_service_for_controller.clone(),
            ));

            // Pass provisioning_service to JobController for auto-provisioning workers
            let controller_provisioning = provisioning_service.clone().map(|p| {
                p as Arc<dyn hodei_server_application::workers::WorkerProvisioningService>
            });

            // Create JobController with saga coordinators
            // The provisioning_service contains the saga coordinators via inner service
            let controller = Arc::new(tokio::sync::Mutex::new(JobController::new(
                job_queue.clone(),
                job_repository.clone(),
                worker_registry.clone(),
                provider_registry.clone(),
                SchedulerConfig::default(),
                sender,
                event_bus.clone(),
                controller_provisioning,
                Some(execution_dispatcher), // EPIC-31: Now connected to saga dispatcher
                Some(provisioning_coordinator), // EPIC-31: Now connected to saga coordinator
                pool.clone(),               // EPIC-32: Database pool for reactive subscriptions
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

            // Keep controller reference alive (drop at end of main)
            let _controller_keep_alive = controller;
        } else {
            info!("JobController not started: provisioning_service is None");
        }
    } else {
        info!("JobController loop disabled (HODEI_JOB_CONTROLLER_ENABLED != 1)");
    }

    // EPIC-28: ProviderManager DESHABILITADO
    // El modelo efímero usa: 1 job = 1 worker provisioned on-demand
    // El dispatcher (JobCoordinator) aprovisiona workers bajo demanda
    // No se necesita auto-scaling basado en eventos de cola
    //
    // ProviderManager был удалён для упрощения архитектуры
    // Auto-scaling basada en eventos ya no es necesaria

    // Worker Monitor (Heartbeats & Disconnection)
    // Keep guard alive to prevent shutdown
    let _worker_monitor_guard =
        match hodei_server_application::jobs::worker_monitor::WorkerMonitor::new(
            worker_registry.clone(),
            event_bus.clone(),
        )
        .start()
        .await
        {
            Ok(guard) => {
                info!("Started WorkerMonitor");
                Some(guard)
            }
            Err(e) => {
                tracing::error!("Failed to start WorkerMonitor: {}", e);
                None
            }
        };

    // Configure CORS for gRPC-Web
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers(Any)
        .allow_methods(Any)
        .expose_headers(Any);

    info!("  ✓ gRPC-Web support enabled");
    info!("  ✓ Context interceptor enabled");

    // Build and start gRPC server
    Server::builder()
        .accept_http1(true)
        .http2_keepalive_interval(Some(Duration::from_secs(30)))
        .http2_keepalive_timeout(Some(Duration::from_secs(10)))
        // .http2_permit_keep_alive_without_calls(true)
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

//! Hodei Jobs gRPC Server
//!
//! Main entry point for the gRPC server with full provisioning support.

mod config;
#[cfg(test)]
mod tests_integration;

use dashmap::DashMap;
use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use tokio::sync::{mpsc, watch};
use tonic::transport::Server;
use tonic_reflection::server::Builder as ReflectionBuilder;
use tonic_web::GrpcWebLayer;
use tower_http::cors::{Any, CorsLayer};
use tracing::{debug, error, info, warn};
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

use hodei_server_domain::outbox::OutboxError;

use hodei_server_application::jobs::{CancelJobUseCase, CreateJobUseCase, JobController};
use hodei_server_application::providers::ProviderRegistry;
use hodei_server_application::saga::dispatcher_saga::DynExecutionSagaDispatcher;
use hodei_server_application::saga::provisioning_saga::DynProvisioningSagaCoordinator;
use hodei_server_application::saga::recovery_saga::DynRecoverySagaCoordinator;
use hodei_server_application::scheduling::smart_scheduler::SchedulerConfig;
use hodei_server_application::workers::actor::{
    WorkerSupervisorBuilder, WorkerSupervisorConfig, WorkerSupervisorHandle,
};
use hodei_server_application::workers::{
    DefaultWorkerProvisioningService, ProvisioningConfig, WorkerLifecycleConfig,
    WorkerLifecycleManager,
};
use hodei_server_domain::saga::{SagaOrchestrator, SagaOrchestratorConfig, SagaRepository};
use hodei_server_infrastructure::persistence::postgres::saga_repository::{
    PostgresSagaOrchestrator, PostgresSagaOrchestratorConfig, SagaPoller, SagaPollerConfig,
};

use hodei_server_domain::providers::ProviderConfigRepository;
use hodei_server_domain::shared_kernel::ProviderId;
use hodei_server_domain::workers::WorkerProvider;
use hodei_server_domain::command::{DynCommandBus, InMemoryErasedCommandBus};

use hodei_server_infrastructure::messaging::OutboxEventBus;
use hodei_server_infrastructure::messaging::execution_saga_consumer::{
    ExecutionSagaConsumer, ExecutionSagaConsumerBuilder, ExecutionSagaConsumerConfig,
};
use hodei_server_infrastructure::messaging::nats::{NatsConfig, NatsEventBus};
use hodei_server_infrastructure::messaging::orphan_worker_detector_consumer::{
    OrphanWorkerDetectorConsumer, OrphanWorkerDetectorConsumerBuilder,
    OrphanWorkerDetectorConsumerConfig,
};
use hodei_server_infrastructure::messaging::outbox_relay::OutboxRelay;
use hodei_server_infrastructure::messaging::postgres::PostgresEventBus;
use hodei_server_infrastructure::messaging::worker_disconnection_handler_consumer::{
    WorkerDisconnectionHandlerConsumer, WorkerDisconnectionHandlerConsumerBuilder,
    WorkerDisconnectionHandlerConsumerConfig,
};
use hodei_server_infrastructure::messaging::worker_ephemeral_terminating_consumer::{
    WorkerEphemeralTerminatingConsumer, WorkerEphemeralTerminatingConsumerBuilder,
    WorkerEphemeralTerminatingConsumerConfig,
};
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
use hodei_shared::event_topics::{job_topics, worker_topics};

use hodei_server_interface::grpc::{
    GrpcWorkerCommandSender, JobExecutionServiceImpl, LogStreamService, LogStreamServiceGrpc,
    MetricsServiceImpl, ProviderManagementServiceImpl, SchedulerServiceImpl,
    WorkerAgentServiceImpl, context_interceptor,
};

use hodei_server_interface::log_persistence::{
    LocalStorageConfig, LogPersistenceConfig, LogStorage, LogStorageFactory, LogStorageRef,
    StorageBackend,
};

// EPIC-42: Reactive Saga Processing imports
use hodei_server_infrastructure::persistence::saga::notifying_repository::{
    NotifyingRepositoryMetrics, NotifyingSagaRepository,
};
use hodei_server_infrastructure::persistence::saga::reactive_processor::{
    ReactiveSagaProcessor, ReactiveSagaProcessorConfig, ReactiveSagaProcessorMetrics,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging - default to INFO level unless RUST_LOG is set
    let env_filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => EnvFilter::try_new("info")?,
    };
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(env_filter)
        .with_target(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Get port from environment or default
    let port = env::var("GRPC_PORT").unwrap_or_else(|_| "50051".to_string());
    // Get server bind address from environment or use default
    let server_bind_host = env::var("HODEI_SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    // Get server address for workers to connect (host.docker.internal for Docker)
    let server_address_for_workers =
        env::var("HODEI_SERVER_ADDRESS").unwrap_or_else(|_| "host.docker.internal".to_string());
    let addr = format!("{}:{}", server_bind_host, port).parse()?;

    // Check dev mode
    let dev_mode = env::var("HODEI_DEV_MODE").unwrap_or_default() == "1";

    info!("╔═══════════════════════════════════════════════════════════════╗");
    info!("║           Hodei Jobs Platform - gRPC Server                   ║");
    info!("╚═══════════════════════════════════════════════════════════════╝");
    info!("Starting server on {}", addr);
    if dev_mode {
        info!("🔓 Development mode ENABLED");
    }

    info!("🔍 DEBUG: Reading database configuration...");
    // Database configuration
    let db_url = env::var("SERVER_DATABASE_URL")
        .or_else(|_| env::var("HODEI_DATABASE_URL"))
        .or_else(|_| env::var("DATABASE_URL"))
        .map_err(|_| anyhow::anyhow!("Missing database URL"))?;

    info!(
        "🔍 DEBUG: Database URL obtained: {}",
        db_url.split('@').last().unwrap_or("unknown")
    );

    let max_connections = env::var("HODEI_DB_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(50); // Increased from 10 to handle multiple NATS subscribers

    let min_connections = env::var("HODEI_DB_MIN_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(10);

    let connection_timeout = env::var("HODEI_DB_CONNECTION_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(30));

    info!(
        "🔍 DEBUG: Creating database pool (max={}, min={}, timeout={}s)...",
        max_connections,
        min_connections,
        connection_timeout.as_secs()
    );
    // Create shared Postgres Pool with optimized settings for high concurrency
    info!("🔍 DEBUG: Attempting database connection...");
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .min_connections(min_connections) // Maintain ready connections
        .acquire_timeout(connection_timeout)
        .idle_timeout(Duration::from_secs(600)) // Recycle idle connections after 10 min
        .max_lifetime(Duration::from_secs(1800)) // Recycle connections after 30 min
        .connect(&db_url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database: {}", e))?;

    info!(
        "✅ Connected to database (pool: min={}, max={})",
        min_connections, max_connections
    );

    info!("🔍 DEBUG: Creating Arc wrapper for pool...");
    // Create Arc wrapper for components that need it
    let pool_arc = Arc::new(pool.clone());

    info!("🔍 DEBUG: Initializing log persistence configuration...");

    // Initialize log persistence configuration
    let server_config =
        config::ServerConfig::new().map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;
    let persistence_config = server_config.to_log_persistence_config();

    info!("🔍 DEBUG: Creating storage backend...");

    // Create storage backend (agnostic - local, S3, etc.)
    let storage_backend: Box<dyn LogStorage> = LogStorageFactory::create(&persistence_config);

    info!("🔍 DEBUG: Creating log storage repository...");
    // Create log storage repository
    let log_storage_repo = Arc::new(LogStorageRepository::new(pool.clone()));

    info!("🔍 DEBUG: Setting up log finalization callback...");

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

    // ==========================================================================
    // Event Bus Configuration - NATS JetStream Only
    // ==========================================================================
    // EPIC-42: NATS JetStream is the production-ready event bus.
    // Provides:
    // - Durable consumers (events survive restarts)
    // - At-least-once delivery semantics
    // - Work queue pattern for multi-instance processing
    // - Automatic replay of missed events
    //
    // NATS is required. Server will fail to start if NATS is unavailable.

    // Connect to NATS
    let nats_urls =
        env::var("HODEI_NATS_URLS").unwrap_or_else(|_| "nats://localhost:4222".to_string());
    info!("Connecting to NATS at: {}", nats_urls);

    // Create NATS EventBus (handles connection internally)
    let nats_config = NatsConfig {
        urls: vec![nats_urls],
        connection_timeout_secs: 10,
        request_timeout_secs: Some(30),
        max_reconnects: Some(5),
        name: Some("hodei-server".to_string()),
        ..Default::default()
    };

    let nats_event_bus = NatsEventBus::new_with_pool(nats_config, Some(pool_arc.clone()))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize NATS EventBus: {}", e))?;

    // Get JetStream context and client from the EventBus
    let nats_jetstream = nats_event_bus.jetstream().clone();
    let nats_client = nats_event_bus.client().clone();

    let event_bus: Arc<dyn hodei_server_domain::event_bus::EventBus> = Arc::new(nats_event_bus);

    info!("✅ NATS EventBus initialized (JetStream enabled with domain_events persistence)");

    // Create Outbox Repository (Transactional Outbox Pattern)
    let outbox_repository_impl: PostgresOutboxRepository =
        PostgresOutboxRepository::new(pool.clone());
    outbox_repository_impl
        .run_migrations()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run outbox migrations: {}", e))?;
    let outbox_repository = Arc::new(outbox_repository_impl);

    // Create trait object for shared use
    let outbox_repository_dyn: Arc<
        dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync,
    > = outbox_repository.clone();

    // Start OutboxRelay background service
    // Reads from OutboxRepo and publishes to NATS EventBus
    let outbox_relay = OutboxRelay::new_with_defaults(pool.clone(), event_bus.clone());
    let outbox_relay = Arc::new(outbox_relay);

    let outbox_relay_clone = outbox_relay.clone();
    tokio::spawn(async move {
        if let Err(e) = outbox_relay_clone.run().await {
            tracing::error!("OutboxRelay stopped with error: {}", e);
        }
    });

    info!("  ✓ OutboxRelay started (Transactional Outbox Pattern enabled)");

    // Start EventArchiver (Decoupled Audit Log)
    // Archives all events from NATS to domain_events table
    use hodei_server_infrastructure::messaging::EventArchiver;
    let event_archiver = Arc::new(EventArchiver::new(
        pool.clone(),
        event_bus.clone(),
        None, // Default config (hodei.events.>)
    ));
    
    let event_archiver_clone = event_archiver.clone();
    tokio::spawn(async move {
        if let Err(e) = event_archiver_clone.run().await {
            tracing::error!("EventArchiver stopped with error: {}", e);
        }
    });

    info!("  ✓ EventArchiver started (Audit Log enabled)");

    // Create OutboxEventBus (wraps real bus with outbox pattern)
    // All downstream services use this for reliable event publishing
    let outbox_event_bus: Arc<dyn hodei_server_domain::event_bus::EventBus> =
        Arc::new(hodei_server_infrastructure::messaging::OutboxEventBus::new(
            outbox_repository.clone(),
            event_bus.clone(),
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

    // ==========================================================================
    // EPIC-42: WorkerSupervisor Actor Initialization
    // ==========================================================================
    info!("Initializing WorkerSupervisor Actor for lock-free worker management...");

    // Configure WorkerSupervisor based on environment variables
    let supervisor_config = WorkerSupervisorConfig {
        max_workers: env::var("HODEI_WORKER_SUPERVISOR_MAX_WORKERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10000),
        inbox_capacity: env::var("HODEI_WORKER_SUPERVISOR_INBOX_CAPACITY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000),
        worker_channel_capacity: env::var("HODEI_WORKER_SUPERVISOR_WORKER_CHANNEL_CAPACITY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100),
        actor_enabled: env::var("HODEI_ACTOR_MODEL_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse::<bool>()
            .unwrap_or(true),
    };

    info!(
        max_workers = supervisor_config.max_workers,
        inbox_capacity = supervisor_config.inbox_capacity,
        "WorkerSupervisor Actor configuration"
    );

    // Create WorkerSupervisor Actor
    let (supervisor_handle, supervisor, _supervisor_shutdown) = WorkerSupervisorBuilder::new()
        .with_config(supervisor_config.clone())
        .build();

    // Spawn WorkerSupervisor Actor in background (move supervisor into spawn)
    let supervisor_for_spawn = supervisor;
    tokio::spawn(async move {
        info!("🚀 WorkerSupervisor Actor: Starting actor loop");
        supervisor_for_spawn.run().await;
        info!("✅ WorkerSupervisor Actor: Actor loop ended");
    });

    info!("  ✓ WorkerSupervisor Actor started");

    // Create Use Cases
    let create_job_usecase = Arc::new(CreateJobUseCase::new(
        job_repository.clone(),
        job_queue.clone(),
        outbox_event_bus.clone(),
    ));
    let cancel_job_usecase = Arc::new(CancelJobUseCase::new(
        job_repository.clone(),
        outbox_event_bus.clone(),
    ));

    // Create gRPC services with WorkerSupervisor Actor integration (EPIC-42)
    let worker_service = if supervisor_config.actor_enabled {
        info!("🔧 Using WorkerSupervisor Actor for worker management");

        WorkerAgentServiceImpl::with_actor_supervisor(
            worker_registry.clone(),
            job_repository.clone(),
            token_store.clone(),
            log_stream_service.clone(),
            outbox_event_bus.clone(),
            supervisor_handle,
        )
    } else {
        info!("⚠️ Using legacy mode for worker management (Actor disabled)");

        WorkerAgentServiceImpl::with_registry_job_repository_token_store_and_log_service(
            worker_registry.clone(),
            job_repository.clone(),
            token_store.clone(),
            log_stream_service.clone(),
            outbox_event_bus.clone(),
        )
    };
    let worker_service_for_controller = worker_service.clone();

    // EPIC-31: Create providers map early for shared access
    // This is used by JobExecutionService for worker cleanup notifications
    // EPIC-42: DashMap for lock-free concurrency
    let providers: Arc<DashMap<ProviderId, Arc<dyn WorkerProvider>>> =
        Arc::new(DashMap::new());

    // EPIC-31: Use with_cleanup_support to enable JobQueued event publishing
    // The outbox_event_bus is required for reactive job processing (JobCoordinator)
    let job_service = JobExecutionServiceImpl::with_cleanup_support(
        create_job_usecase.clone(),
        cancel_job_usecase,
        job_repository.clone(),
        worker_registry.clone(),
        providers.clone(), // Empty now, will be populated when providers are initialized
        None,              // outbox_repository
        Some(outbox_event_bus.clone()), // EPIC-31 FIX: event_bus required for JobQueued events
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
                // EPIC-42: DashMap for lock-free concurrency - insert each entry
                for (provider_id, provider) in providers_map {
                    providers.insert(provider_id, provider);
                }

                // EPIC-30: Saga Infrastructure - Disabled for now (needs full implementation)
                info!(
                    "⚠️ Saga infrastructure initialized (orchestration disabled - using legacy flow)"
                );

                // Server address for worker provisioning
                // HODEI_SERVER_HOST: Used for binding (0.0.0.0 for all interfaces)
                // HODEI_SERVER_ADDRESS: Used by workers to connect (host.docker.internal for Docker)
                let server_host =
                    env::var("HODEI_SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
                let server_address = format!("http://{}:{}", server_address_for_workers, port);
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
                    outbox_event_bus.clone(),
                    outbox_repository_dyn.clone(),
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
    let scheduler_service = if let Some(ref _prov) = provisioning_service {
        SchedulerServiceImpl::with_event_bus(
            create_job_usecase.clone(),
            job_repository.clone(),
            job_queue.clone(),
            worker_registry.clone(),
            SchedulerConfig::default(),
            outbox_event_bus.clone(),
        )
    } else {
        SchedulerServiceImpl::with_event_bus(
            create_job_usecase.clone(),
            job_repository.clone(),
            job_queue.clone(),
            worker_registry.clone(),
            SchedulerConfig::default(),
            outbox_event_bus.clone(),
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
    // Create Production-Ready Saga Orchestrator with PostgreSQL persistence
    let orchestrator_impl = Arc::new(PostgresSagaOrchestrator::new(
        saga_repository.clone(),
        Some(saga_config.clone()),
    ));

    let orchestrator: Arc<
        dyn hodei_server_domain::saga::SagaOrchestrator<
                Error = hodei_server_domain::shared_kernel::DomainError,
            > + Send
            + Sync,
    > = orchestrator_impl.clone();

    info!("  ✓ Saga orchestrator initialized with PostgreSQL repository");

    // GAP-60-01: Create CommandBus for saga command dispatch
    let command_bus: DynCommandBus = Arc::new(InMemoryErasedCommandBus::new());

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
        // EPIC-45 Gap 1 Fix: Pass SagaServices dependencies
        worker_registry.clone(),
        outbox_event_bus.clone(),
        Some(job_repository.clone()),
        command_bus.clone(), // GAP-60-01: CommandBus for saga command dispatch
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
        command_bus.clone(), // GAP-60-01: CommandBus for saga command dispatch
        worker_registry.clone(),
        outbox_event_bus.clone(),
        Some(job_repository.clone()),
        None, // provisioning_service
        Some(config),
        None, // target_provider_id
    ));

    info!("  ✓ RecoverySagaCoordinator initialized");

    info!("Saga infrastructure ready");

    // ==========================================================================
    // EPIC-42: Reactive Saga Processing (SIMPLIFIED IMPLEMENTATION)
    // ==========================================================================
    info!("Configuring reactive saga processing...");

    // EPIC-42: Check if reactive mode is enabled
    let reactive_mode = env::var("HODEI_SAGA_REACTIVE_MODE")
        .unwrap_or_else(|_| "true".to_string())
        .parse::<bool>()
        .unwrap_or(true);

    // Create shared metrics for saga processing
    let saga_processor_metrics = Arc::new(ReactiveSagaProcessorMetrics::default());
    let notifying_metrics = Arc::new(NotifyingRepositoryMetrics::new());

    // Channel for saga notifications (signals from NotifyingSagaRepository to processor)
    let (signal_tx, mut signal_rx) =
        tokio::sync::mpsc::unbounded_channel::<hodei_server_domain::saga::SagaId>();

    // Wrap saga_repository with NotifyingSagaRepository to emit signals on save/update
    // The NotifyingSagaRepository decorates the inner repository with signal emission
    let _notifying_repository =
        NotifyingSagaRepository::new(saga_repository.clone(), signal_tx, notifying_metrics);

    // EPIC-42: Start reactive signal consumer if reactive mode is enabled
    // This provides immediate saga execution upon signal, with safety net polling
    let mut reactive_processor_guard: Option<tokio::task::JoinHandle<()>> = None;
    // EPIC-46 GAP-16: Default to reactive mode (EDA pure architecture)
    let mut use_polling = false;

    if reactive_mode {
        info!("🚀 Starting Reactive Saga Processor (signal-based execution)...");

        // Configure safety net polling interval
        let safety_polling_interval = Duration::from_secs(
            env::var("HODEI_SAGA_SAFETY_POLLING_INTERVAL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300), // 5 minutes safety net
        );

        // Create shutdown channel
        let (shutdown_tx, mut shutdown_rx) = watch::channel(());

        // Spawn reactive signal processor
        let metrics = saga_processor_metrics.clone();
        let saga_repo = saga_repository.clone();
        // Use concrete orchestrator implementation to access the execute() method
        // which handles saga reconstruction from context
        let saga_orchestrator = orchestrator_impl.clone();

        reactive_processor_guard = Some(tokio::spawn(async move {
            info!("🔄 ReactiveSagaProcessor: Waiting for saga signals...");

            let mut interval = tokio::time::interval(safety_polling_interval);

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!("✅ ReactiveSagaProcessor: Shutdown signal received");
                        break;
                    }
                    _ = interval.tick() => {
                        // Safety net: process any pending sagas via polling
                        // This handles missed events or stuck sagas (resumption)
                        metrics.record_polling_processed();

                        // Claim pending sagas using the specific Postgres repository method
                        match saga_repo.claim_pending_sagas(10, "reactive-safety-net").await {
                            Ok(contexts) => {
                                if !contexts.is_empty() {
                                    info!("🔄 ReactiveSagaProcessor: Safety net processing {} sagas", contexts.len());

                                    for ctx in contexts {
                                        let saga_id = ctx.saga_id.clone();
                                        info!(saga_id = %saga_id.0, "Derived execution via safety net");

                                        // Execute using the concrete orchestrator which knows how to rebuild sagas
                                        if let Err(e) = saga_orchestrator.execute(&ctx).await {
                                            error!(saga_id = %saga_id.0, error = ?e, "Safety net execution failed");
                                            metrics.record_error();
                                        } else {
                                            metrics.record_reactive_processed(); // Count as processed
                                            info!(saga_id = %saga_id.0, "Safety net execution completed");
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!(error = ?e, "Failed to claim pending sagas in safety net");
                                metrics.record_error();
                            }
                        }
                    }
                    signal = signal_rx.recv() => {
                        match signal {
                            Some(saga_id) => {
                                let start = std::time::Instant::now();

                                // Process saga immediately using execute method
                                if let Ok(Some(saga_ctx)) = saga_repo.find_by_id(&saga_id).await {
                                    if let Err(e) = saga_orchestrator.execute(&saga_ctx).await {
                                        error!(saga_id = %saga_id, error = ?e, "Saga execution failed");
                                        metrics.record_error();
                                    } else {
                                        metrics.record_reactive_processed();
                                        let elapsed_ms = start.elapsed().as_millis();
                                        metrics.record_latency(elapsed_ms as u64);
                                        info!(saga_id = %saga_id, elapsed_ms = elapsed_ms, "Saga executed reactively");
                                    }
                                } else {
                                    warn!(saga_id = %saga_id, "Saga not found for reactive processing");
                                }
                            }
                            None => {
                                info!("Signal channel closed");
                                break;
                            }
                        }
                    }
                }
            }

            info!(
                "✅ ReactiveSagaProcessor: Shutdown complete (reactive={}, polling={}, errors={})",
                metrics.reactive_processed(),
                metrics.polling_processed(),
                metrics.errors()
            );
        }));

        // Keep shutdown sender alive
        let _shutdown_guard = shutdown_tx;

        // Disable legacy poller in reactive mode
        use_polling = false;
        info!(
            safety_polling_interval = %safety_polling_interval.as_secs(),
            "  ✓ ReactiveSagaProcessor started (safety net active)"
        );
    } else {
        // EPIC-46 GAP-16: Polling is now fallback mode, not default
        info!("ℹ️ Reactive saga processing disabled, falling back to legacy polling mode");
        info!("  ℹ️  Enable reactive mode with: export HODEI_SAGA_REACTIVE_MODE=true");
        use_polling = true; // Enable polling as fallback when reactive mode is off
    }

    // EPIC-33/EPIC-42: Start Saga Poller (only if not using reactive mode)
    if use_polling {
        info!("Starting saga poller for pending saga execution...");

        let saga_poller = SagaPoller::new(
            saga_repository.clone(),
            Arc::new(PostgresSagaOrchestrator::new(
                saga_repository.clone(),
                Some(saga_config.clone()),
            )),
            Some(SagaPollerConfig {
                polling_interval: Duration::from_secs(5),
                max_concurrent: 10,
            }),
        );

        // Create saga factory for provisioning and execution sagas
        let poller_handle = saga_poller.start(move |saga_type, metadata| {
            // Create saga instances based on type
            match saga_type {
                hodei_server_domain::saga::SagaType::Provisioning => {
                    // Extract provider_id and worker_spec from metadata
                    let provider_id_str = metadata
                        .get("provider_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_default();

                    let provider_id = if !provider_id_str.is_empty() {
                        hodei_server_domain::shared_kernel::ProviderId::from_uuid(
                            Uuid::parse_str(&provider_id_str).unwrap_or_else(|_| Uuid::new_v4()),
                        )
                    } else {
                        hodei_server_domain::shared_kernel::ProviderId::new()
                    };

                    let worker_image = metadata
                        .get("worker_image")
                        .and_then(|v| v.as_str())
                        .unwrap_or("hodei-jobs-worker:latest");

                    let spec = hodei_server_domain::workers::WorkerSpec {
                        worker_id: hodei_server_domain::shared_kernel::WorkerId::new(),
                        image: worker_image.to_string(),
                        resources: hodei_server_domain::workers::ResourceRequirements {
                            cpu_cores: 1.0,
                            memory_bytes: 1024 * 1024 * 1024, // 1GB
                            disk_bytes: 1024 * 1024 * 1024,   // 1GB
                            gpu_count: 0,
                            gpu_type: None,
                        },
                        labels: std::collections::HashMap::new(),
                        annotations: std::collections::HashMap::new(),
                        environment: std::collections::HashMap::new(),
                        volumes: vec![],
                        server_address: format!("http://{}:{}", server_address_for_workers, port),
                        max_lifetime: Duration::from_secs(3600),
                        idle_timeout: Duration::from_secs(300),
                        ttl_after_completion: None,
                        architecture: hodei_server_domain::workers::Architecture::Amd64,
                        required_capabilities: vec![],
                        provider_config: None,
                        kubernetes: hodei_server_domain::workers::KubernetesWorkerConfig::default(),
                    };

                    Some(Box::new(hodei_server_domain::saga::ProvisioningSaga::new(
                        spec,
                        provider_id,
                    ))
                        as Box<dyn hodei_server_domain::saga::Saga>)
                }
                hodei_server_domain::saga::SagaType::Execution => {
                    // Execution sagas are handled by the dispatcher
                    None
                }
                hodei_server_domain::saga::SagaType::Recovery => {
                    // Recovery sagas - create if needed
                    None
                }
                _ => None, // Cancellation, Timeout, Cleanup - not handled by poller
            }
        });

        info!("  ✓ Saga poller started (interval: 5s)");

        // Keep poller handle alive for server lifetime
        let _saga_poller_guard = poller_handle;

        info!("Saga poller active - pending sagas will be processed every 5s");
    } else {
        info!("⚠️ Legacy saga poller DISABLED (using ReactiveSagaProcessor instead)");
    }

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
                outbox_event_bus.clone(),
                outbox_repository_dyn.clone(), // Mandatory outbox
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

    // ==========================================================================
    // EPIC-42: NATS Event-Driven Saga Consumers (Production-Ready)
    // ==========================================================================
    // Start the ExecutionSagaConsumer to process job execution events
    // reactively via NATS JetStream durable consumers.
    // This eliminates polling and provides at-least-once delivery semantics.
    info!("🚀 Starting NATS ExecutionSagaConsumer for reactive job processing");

    // Create ExecutionSagaConsumer for event-driven saga triggering
    let nats_client_clone = nats_client.clone();
    let nats_jetstream_clone = nats_jetstream.clone();
    let exec_consumer = ExecutionSagaConsumerBuilder::new()
        .with_client(nats_client_clone)
        .with_jetstream(nats_jetstream_clone)
        .with_orchestrator(orchestrator.clone())
        .with_job_repository(job_repository.clone())
        .with_worker_registry(worker_registry.clone())
        .with_config(ExecutionSagaConsumerConfig {
            consumer_name: "execution-saga-consumer".to_string(),
            stream_prefix: "HODEI".to_string(),
            job_queued_topic: job_topics::QUEUED.to_string(),
            worker_ready_topic: worker_topics::READY.to_string(),
            consumer_group: "execution-dispatchers".to_string(),
            concurrency: 10,
            ack_wait: Duration::from_secs(30),
            max_deliver: 3,
            enable_auto_dispatch: true,
            saga_timeout: Duration::from_secs(60),
        })
        .build()
        .expect("Failed to build ExecutionSagaConsumer");

    // Spawn the consumer in background
    let consumer = exec_consumer.clone();
    tokio::spawn(async move {
        info!("📦 ExecutionSagaConsumer: Starting NATS consumer");
        if let Err(e) = consumer.start().await {
            tracing::error!("ExecutionSagaConsumer error: {}", e);
        }
        info!("📦 ExecutionSagaConsumer: Stopped");
    });

    // Keep consumer alive for server lifetime
    let _nats_exec_consumer_guard = exec_consumer;

    info!("✅ NATS ExecutionSagaConsumer started (reactive mode enabled)");

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
            outbox_event_bus.clone(),
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

    // ==========================================================================
    // EPIC-NEW: Worker Lifecycle Event Consumers
    // ==========================================================================
    // These consumers handle worker lifecycle events reactively via NATS JetStream

    // Orphan Worker Detector Consumer
    // Detects and handles orphaned workers that lost communication with the server
    info!("🚀 Starting NATS OrphanWorkerDetectorConsumer");

    let orphan_detector = OrphanWorkerDetectorConsumerBuilder::new()
        .with_client(&nats_client)
        .with_jetstream(&nats_jetstream)
        .with_orchestrator(orchestrator.clone())
        .with_worker_registry(worker_registry.clone())
        .with_outbox_repository(outbox_repository.clone())
        .with_config(OrphanWorkerDetectorConsumerConfig {
            consumer_name: "orphan-worker-detector".to_string(),
            stream_prefix: "HODEI".to_string(),
            orphan_detected_topic: worker_topics::ORPHAN_DETECTED.to_string(),
            consumer_group: "worker-orphans".to_string(),
            concurrency: 5,
            ack_wait: Duration::from_secs(60),
            max_deliver: 3,
            orphan_termination_threshold_secs: 300,
            saga_timeout: Duration::from_secs(120),
        })
        .build()
        .expect("Failed to build OrphanWorkerDetectorConsumer");

    let orphan_detector_guard = orphan_detector.clone();
    tokio::spawn(async move {
        info!("🔍 OrphanWorkerDetectorConsumer: Starting NATS consumer");
        if let Err(e) = orphan_detector_guard.start().await {
            tracing::error!("OrphanWorkerDetectorConsumer error: {}", e);
        }
        info!("🔍 OrphanWorkerDetectorConsumer: Stopped");
    });

    // Worker Disconnection Handler Consumer
    // Handles unexpected worker disconnections and triggers appropriate recovery
    info!("🚀 Starting NATS WorkerDisconnectionHandlerConsumer");

    let disconnection_handler = WorkerDisconnectionHandlerConsumerBuilder::new()
        .with_client(&nats_client)
        .with_jetstream(&nats_jetstream)
        .with_orchestrator(orchestrator.clone())
        .with_job_repository(job_repository.clone())
        .with_worker_registry(worker_registry.clone())
        .with_outbox_repository(outbox_repository.clone())
        .with_config(WorkerDisconnectionHandlerConsumerConfig {
            consumer_name: "worker-disconnection-handler".to_string(),
            stream_prefix: "HODEI".to_string(),
            disconnected_topic: worker_topics::DISCONNECTED.to_string(),
            consumer_group: "worker-disconnections".to_string(),
            concurrency: 10,
            ack_wait: Duration::from_secs(60),
            max_deliver: 3,
            short_timeout_secs: 30,
            long_timeout_secs: 120,
            saga_timeout: Duration::from_secs(180),
        })
        .build()
        .expect("Failed to build WorkerDisconnectionHandlerConsumer");

    let disconnection_guard = disconnection_handler.clone();
    tokio::spawn(async move {
        info!("🌐 WorkerDisconnectionHandlerConsumer: Starting NATS consumer");
        if let Err(e) = disconnection_guard.start().await {
            tracing::error!("WorkerDisconnectionHandlerConsumer error: {}", e);
        }
        info!("🌐 WorkerDisconnectionHandlerConsumer: Stopped");
    });

    // Worker Ephemeral Terminating Consumer
    // Handles termination of ephemeral workers after job completion
    info!("🚀 Starting NATS WorkerEphemeralTerminatingConsumer");

    let ephemeral_terminating = WorkerEphemeralTerminatingConsumerBuilder::new()
        .with_client(&nats_client)
        .with_jetstream(&nats_jetstream)
        .with_orchestrator(orchestrator.clone())
        .with_job_repository(job_repository.clone())
        .with_worker_registry(worker_registry.clone())
        .with_outbox_repository(outbox_repository.clone())
        .with_config(WorkerEphemeralTerminatingConsumerConfig {
            consumer_name: "worker-ephemeral-terminating".to_string(),
            stream_prefix: "HODEI".to_string(),
            terminating_topic: worker_topics::EPHEMERAL_TERMINATING.to_string(),
            consumer_group: "worker-cleanup".to_string(),
            concurrency: 10,
            ack_wait: Duration::from_secs(60),
            max_deliver: 3,
            saga_timeout: Duration::from_secs(120),
        })
        .build()
        .expect("Failed to build WorkerEphemeralTerminatingConsumer");

    let ephemeral_guard = ephemeral_terminating.clone();
    tokio::spawn(async move {
        info!("🧹 WorkerEphemeralTerminatingConsumer: Starting NATS consumer");
        if let Err(e) = ephemeral_guard.start().await {
            tracing::error!("WorkerEphemeralTerminatingConsumer error: {}", e);
        }
        info!("🧹 WorkerEphemeralTerminatingConsumer: Stopped");
    });

    info!("✅ All worker lifecycle consumers started");

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

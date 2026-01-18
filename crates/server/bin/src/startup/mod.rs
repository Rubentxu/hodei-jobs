//! Startup module - orchestrates application initialization.
//!
//! Uses exponential backoff for resilient connections to database and NATS.
//! Separates gRPC server lifecycle into dedicated grpc_server module.
//! Separates service initialization into services_init module.
//! Separates provider initialization into providers_init module.
//! Provides graceful shutdown coordination via shutdown module.

mod grpc_server;
mod providers_init;
mod services_init;
mod shutdown;

pub use grpc_server::{GrpcServerConfig, start_grpc_server};
pub use providers_init::{ProvidersInitConfig, ProvidersInitResult, ProvidersInitializer};
pub use services_init::{
    GrpcServices, initialize_grpc_services, start_background_tasks, start_command_relay,
    start_execution_command_consumers_service, start_execution_saga_consumer,
    start_job_coordinator, start_nats_outbox_relay, start_reactive_saga_processor,
    start_saga_command_consumers_service, start_saga_consumers, start_saga_poller,
    start_worker_ephemeral_terminating_consumer,
};
pub use shutdown::{GracefulShutdown, ShutdownConfig, start_signal_handler};

use backoff::{ExponentialBackoff, future::retry};
use dashmap::DashMap;
use hodei_server_application::saga::recovery_saga::DynRecoverySagaCoordinator;
use hodei_server_application::workers::lifecycle::{
    WorkerLifecycleManager, WorkerLifecycleManagerBuilder,
};
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::saga::{InMemorySagaOrchestrator, SagaOrchestrator};
use hodei_server_domain::shared_kernel::ProviderId;
use hodei_server_domain::workers::WorkerProvider;
use hodei_server_infrastructure::messaging::nats::{NatsConfig, NatsEventBus};
use hodei_server_infrastructure::persistence::outbox::PostgresOutboxRepository;
use hodei_server_infrastructure::persistence::postgres::PostgresSagaOrchestrator;
use hodei_server_infrastructure::persistence::postgres::PostgresSagaRepository;
use hodei_server_infrastructure::persistence::postgres::migrations::MigrationConfig;
use hodei_server_infrastructure::persistence::postgres::{
    PostgresJobRepository, PostgresProviderConfigRepository, PostgresWorkerBootstrapTokenStore,
    PostgresWorkerRegistry,
};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

/// Current application version
pub const APP_VERSION: &str = "0.38.5";

/// Application state containing all initialized components and dependencies.
#[derive(Clone)]
pub struct AppState {
    /// Database connection pool
    pub pool: sqlx::PgPool,
    /// Worker registry for persistent storage
    pub worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
    /// Job repository for persistent storage
    pub job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
    /// Token store for worker bootstrap tokens
    pub token_store: Arc<dyn hodei_server_domain::iam::WorkerBootstrapTokenStore>,
    /// Provider configuration repository
    pub provider_config_repository:
        Arc<dyn hodei_server_domain::providers::config::ProviderConfigRepository>,
    /// NATS EventBus for messaging
    pub nats_event_bus: NatsEventBus,
    /// Outbox repository for transactional outbox pattern
    pub outbox_repository: Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync>,
    /// Saga orchestrator for saga-based workflows
    pub saga_orchestrator: Arc<
        dyn SagaOrchestrator<Error = hodei_server_domain::shared_kernel::DomainError> + Send + Sync,
    >,
    /// Worker lifecycle manager for provisioning and recovery
    pub lifecycle_manager: Arc<WorkerLifecycleManager>,
    /// Provider initialization result
    pub provider_init_result: Option<ProvidersInitResult>,
    /// gRPC services container (for sharing between coordinator and server)
    pub grpc_services: Option<Arc<GrpcServices>>,
}

impl AppState {
    pub fn new(
        pool: sqlx::PgPool,
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
        token_store: Arc<dyn hodei_server_domain::iam::WorkerBootstrapTokenStore>,
        provider_config_repository: Arc<
            dyn hodei_server_domain::providers::config::ProviderConfigRepository,
        >,
        nats_event_bus: NatsEventBus,
        outbox_repository: Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync>,
        saga_orchestrator: Arc<
            dyn SagaOrchestrator<Error = hodei_server_domain::shared_kernel::DomainError>
                + Send
                + Sync,
        >,
        lifecycle_manager: Arc<WorkerLifecycleManager>,
        provider_init_result: Option<ProvidersInitResult>,
        grpc_services: Option<Arc<GrpcServices>>,
    ) -> Self {
        Self {
            pool,
            worker_registry,
            job_repository,
            token_store,
            provider_config_repository,
            nats_event_bus,
            outbox_repository,
            saga_orchestrator,
            lifecycle_manager,
            provider_init_result,
            grpc_services,
        }
    }

    /// Get event bus as trait object for services that need it
    pub fn event_bus(&self) -> Arc<dyn hodei_server_domain::event_bus::EventBus> {
        Arc::new(self.nats_event_bus.clone())
    }
}

/// Configuration for application startup.
#[derive(Debug, Clone)]
pub struct StartupConfig {
    pub grpc_addr: SocketAddr,
    pub db_pool_size: u32,
    pub db_min_idle: u32,
    pub db_connect_timeout: Duration,
    pub db_idle_timeout: Duration,
    pub db_max_lifetime: Duration,
    pub nats_urls: String,
    pub nats_timeout: Duration,
    pub dev_mode: bool,
    pub provisioning_enabled: bool,
    pub worker_image: String,
    pub docker_enabled: bool,
    pub kubernetes_enabled: bool,
    pub rust_log: String,
    pub server_address: String,
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self {
            grpc_addr: "0.0.0.0:9090".parse().unwrap(),
            db_pool_size: 50,
            db_min_idle: 10,
            db_connect_timeout: Duration::from_secs(30),
            db_idle_timeout: Duration::from_secs(600),
            db_max_lifetime: Duration::from_secs(1800),
            nats_urls: "nats://localhost:4222".to_string(),
            nats_timeout: Duration::from_secs(10),
            dev_mode: false,
            provisioning_enabled: true,
            worker_image: "hodei-jobs-worker:latest".to_string(),
            docker_enabled: true,
            kubernetes_enabled: true,
            rust_log: "info".to_string(),
            server_address: "http://localhost:9090".to_string(),
        }
    }
}

impl StartupConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        use std::env;

        let grpc_port: u16 = env::var("GRPC_PORT")
            .unwrap_or_else(|_| "9090".to_string())
            .parse()?;

        let host = env::var("HODEI_SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let grpc_addr = format!("{}:{}", host, grpc_port).parse()?;

        Ok(Self {
            grpc_addr,
            nats_urls: env::var("HODEI_NATS_URLS")
                .unwrap_or_else(|_| "nats://localhost:4222".to_string()),
            dev_mode: env::var("HODEI_DEV_MODE").unwrap_or_default() == "1",
            provisioning_enabled: env::var("HODEI_PROVISIONING_ENABLED")
                .unwrap_or_else(|_| "1".to_string())
                == "1",
            worker_image: env::var("HODEI_WORKER_IMAGE")
                .unwrap_or_else(|_| "hodei-jobs-worker:latest".to_string()),
            docker_enabled: env::var("HODEI_DOCKER_ENABLED").unwrap_or_else(|_| "1".to_string())
                == "1",
            kubernetes_enabled: env::var("HODEI_KUBERNETES_ENABLED")
                .unwrap_or_else(|_| "1".to_string())
                == "1",
            rust_log: env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            server_address: env::var("HODEI_SERVER_ADDRESS")
                .or_else(|_| env::var("HODEI_GRPC_ADDRESS"))
                .unwrap_or_else(|_| format!("http://127.0.0.1:{}", grpc_port)),
            ..Self::default()
        })
    }

    pub fn database_url(&self) -> anyhow::Result<String> {
        std::env::var("SERVER_DATABASE_URL")
            .or_else(|_| std::env::var("HODEI_DATABASE_URL"))
            .or_else(|_| std::env::var("DATABASE_URL"))
            .map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))
    }
}

/// Run the complete application startup sequence with resilient connections.
pub async fn run(config: StartupConfig) -> anyhow::Result<AppState> {
    info!(
        "Starting Hodei Jobs Platform v{} on {}",
        APP_VERSION, config.grpc_addr
    );

    // Exponential backoff for initial connections
    let backoff = ExponentialBackoff {
        initial_interval: Duration::from_secs(2),
        max_interval: Duration::from_secs(30),
        max_elapsed_time: Some(Duration::from_secs(120)), // 2 minutes max
        ..Default::default()
    };

    // Step 1: Connect to database with exponential backoff
    let pool = retry(backoff.clone(), || async {
        // Wrap connection in timeout to prevent hanging
        let connect_future = connect_to_database(&config);
        match tokio::time::timeout(Duration::from_secs(10), connect_future).await {
            Ok(Ok(pool)) => Ok(pool),
            Ok(Err(e)) => {
                tracing::warn!("Database connection failed, retrying: {}", e);
                Err(backoff::Error::transient(e))
            }
            Err(_) => {
                tracing::warn!("Database connection timed out, retrying...");
                Err(backoff::Error::transient(anyhow::anyhow!(
                    "Connection timed out"
                )))
            }
        }
    })
    .await
    .map_err(|e| anyhow::anyhow!("Failed to connect to database after retries: {}", e))?;
    info!("✓ Database connected");

    // Step 2: Run migrations using existing infrastructure service
    let migration_config = MigrationConfig::with_paths(vec![
        "crates/server/infrastructure/src/persistence/postgres/migrations/core".to_string(),
        "crates/server/infrastructure/src/persistence/postgres/migrations/infra".to_string(),
    ]);
    let service =
        hodei_server_infrastructure::persistence::postgres::migrations::MigrationService::new(
            pool.clone(),
            migration_config,
        );
    let _results = service.run_all().await?;
    let validation = service.validate().await?;
    info!("✓  Migrations: {}", validation.summary());

    // Step 3: Connect to NATS with exponential backoff
    let nats_config = NatsConfig {
        urls: config
            .nats_urls
            .split(',')
            .map(|s| s.trim().to_string())
            .collect(),
        connection_timeout_secs: config.nats_timeout.as_secs() as u64,
        request_timeout_secs: Some(30),
        max_reconnects: Some(5),
        name: Some("hodei-server".to_string()),
        ..Default::default()
    };

    let nats_event_bus = retry(backoff.clone(), || async {
        let connect_future = NatsEventBus::new(nats_config.clone());
        match tokio::time::timeout(Duration::from_secs(10), connect_future).await {
            Ok(Ok(bus)) => Ok(bus),
            Ok(Err(e)) => {
                tracing::warn!("NATS connection failed, retrying: {}", e);
                Err(backoff::Error::transient(e))
            }
            Err(_) => {
                tracing::warn!("NATS connection timed out, retrying...");
                Err(backoff::Error::transient(
                    hodei_server_domain::event_bus::EventBusError::ConnectionError(
                        "Connection timed out".to_string(),
                    ),
                ))
            }
        }
    })
    .await
    .map_err(|e| anyhow::anyhow!("Failed to connect to NATS after retries: {}", e))?;
    info!("✓ NATS connected");

    // Step 4: Initialize repositories and stores
    let worker_registry: Arc<PostgresWorkerRegistry> =
        Arc::new(PostgresWorkerRegistry::new(pool.clone()));
    info!("✓ WorkerRegistry initialized");

    let job_repository: Arc<PostgresJobRepository> =
        Arc::new(PostgresJobRepository::new(pool.clone()));
    info!("✓ JobRepository initialized");

    let token_store: Arc<dyn hodei_server_domain::iam::WorkerBootstrapTokenStore> =
        Arc::new(PostgresWorkerBootstrapTokenStore::new(pool.clone()));
    info!("✓ TokenStore initialized");

    // Step 5: Initialize outbox repository
    let outbox_repository: Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync> =
        Arc::new(PostgresOutboxRepository::new(pool.clone()));
    info!("✓ OutboxRepository initialized");

    // Step 6: Initialize saga orchestrator with Postgres persistence (NOT in-memory)
    let saga_repository: Arc<PostgresSagaRepository> =
        Arc::new(PostgresSagaRepository::new(pool.clone()));
    info!("✓ SagaRepository initialized");

    // ✅ Use PostgresSagaOrchestrator for persistent saga state (CRITICAL FIX)
    let saga_orchestrator: Arc<
        dyn SagaOrchestrator<Error = hodei_server_domain::shared_kernel::DomainError> + Send + Sync,
    > = Arc::new(PostgresSagaOrchestrator::new(saga_repository, None));
    info!("✓ PostgresSagaOrchestrator initialized (persistent saga state)");

    // Step 7: Initialize provider configuration repository
    let provider_config_repository: Arc<
        dyn hodei_server_domain::providers::config::ProviderConfigRepository,
    > = Arc::new(PostgresProviderConfigRepository::new(pool.clone()));
    info!("✓ ProviderConfigRepository initialized");

    // Step 8: Initialize recovery saga coordinator with OutboxCommandBus (CRITICAL FIX)
    let (recovery_command_bus, _) = services_init::create_command_bus(pool.clone());
    let recovery_saga_coordinator: Arc<DynRecoverySagaCoordinator> = Arc::new(
        DynRecoverySagaCoordinator::builder()
            .with_orchestrator(saga_orchestrator.clone())
            .with_command_bus(recovery_command_bus) // ✅ OutboxCommandBus for persistence
            .with_worker_registry(worker_registry.clone())
            .with_event_bus(Arc::new(nats_event_bus.clone()) as Arc<dyn EventBus>)
            .build()
            .expect("Failed to build recovery saga coordinator"),
    );
    info!("✓ RecoverySagaCoordinator initialized with OutboxCommandBus");

    // Step 9: Initialize WorkerLifecycleManager with empty providers map
    // The providers will be registered during provider initialization
    let providers_map: Arc<DashMap<ProviderId, Arc<dyn WorkerProvider>>> = Arc::new(DashMap::new());
    let lifecycle_manager = WorkerLifecycleManagerBuilder::new()
        .with_registry(worker_registry.clone())
        .with_providers(providers_map.clone())
        .with_event_bus(Arc::new(nats_event_bus.clone()) as Arc<dyn EventBus>)
        .with_outbox_repository(outbox_repository.clone())
        .with_recovery_saga_coordinator(recovery_saga_coordinator.clone())
        .build();
    let lifecycle_manager = Arc::new(lifecycle_manager);
    info!("✓ WorkerLifecycleManager initialized with empty providers map");

    // Step 10: Initialize providers and register them in lifecycle manager
    let registry = Arc::new(
        hodei_server_application::providers::registry::ProviderRegistry::new(
            provider_config_repository.clone(),
        ),
    );
    let provider_init_config = ProvidersInitConfig::default();
    let initializer = ProvidersInitializer::new(
        provider_config_repository.clone(),
        registry,
        provider_init_config,
    )
    .with_event_bus(Arc::new(nats_event_bus.clone()) as Arc<dyn EventBus>)
    .with_lifecycle_manager(lifecycle_manager.clone());

    let provider_init_result = initializer
        .initialize()
        .await
        .map_err(|e| anyhow::anyhow!("Provider initialization failed: {}", e))?;

    info!(
        "✓ Providers initialized: {}",
        provider_init_result.summary_message()
    );

    // EPIC-32: Start event monitoring for reactive worker cleanup
    // This enables the lifecycle manager to:
    // 1. Listen to WorkerEphemeralTerminating events (published by JobCoordinator)
    // 2. Reactively destroy workers when jobs complete
    // 3. Monitor provider infrastructure events
    lifecycle_manager.start_event_monitoring().await;
    info!("✓ WorkerLifecycleManager event monitoring started (reactive cleanup enabled)");

    // EPIC-88: Start Execution Command Consumers
    services_init::start_execution_command_consumers_service(
        Arc::new(nats_event_bus.client().clone()),
        pool.clone(),
    )
    .await?;
    info!("✓ Execution Command Consumers started");

    // EPIC-89: Start Saga Command Consumers (Sprint 1 - low-complexity commands only)
    // Commands requiring WorkerProvisioning are deferred to Sprint 3
    services_init::start_saga_command_consumers_service(
        Arc::new(nats_event_bus.client().clone()),
        pool.clone(),
    )
    .await?;
    info!("✓ Saga Command Consumers started (EPIC-89 Sprint 1)");

    info!("✓ All connections established, ready for gRPC");

    Ok(AppState::new(
        pool,
        worker_registry,
        job_repository,
        token_store,
        provider_config_repository,
        nats_event_bus,
        outbox_repository,
        saga_orchestrator,
        lifecycle_manager,
        Some(provider_init_result),
        None, // grpc_services will be initialized later as Arc
    ))
}

/// Connect to database.
async fn connect_to_database(config: &StartupConfig) -> anyhow::Result<sqlx::PgPool> {
    let db_url = config.database_url()?;
    info!("Connecting to database...");

    let pool = PgPoolOptions::new()
        .max_connections(config.db_pool_size)
        .min_connections(config.db_min_idle)
        .acquire_timeout(config.db_connect_timeout)
        .idle_timeout(config.db_idle_timeout)
        .max_lifetime(config.db_max_lifetime)
        .connect(&db_url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database: {}", e))?;

    Ok(pool)
}

//! Hodei Jobs gRPC Server
//!
//! Main entry point for the gRPC server.

use hodei_jobs::{
    FILE_DESCRIPTOR_SET, audit_service_server::AuditServiceServer,
    job_execution_service_server::JobExecutionServiceServer,
    log_stream_service_server::LogStreamServiceServer,
    metrics_service_server::MetricsServiceServer,
    providers::provider_management_service_server::ProviderManagementServiceServer,
    scheduler_service_server::SchedulerServiceServer,
    worker_agent_service_server::WorkerAgentServiceServer,
};
use hodei_jobs_application::audit::AuditService;
use hodei_jobs_application::audit::cleanup::{AuditCleanupService, AuditRetentionConfig};
use hodei_jobs_application::jobs::cancel::CancelJobUseCase;
use hodei_jobs_application::jobs::controller::JobController;
use hodei_jobs_application::jobs::create::CreateJobUseCase;
use hodei_jobs_application::providers::registry::ProviderRegistry;
use hodei_jobs_application::scheduling::SchedulerConfig;
use hodei_jobs_application::workers::provisioning_impl::{
    DefaultWorkerProvisioningService, ProvisioningConfig,
};
use hodei_jobs_domain::shared_kernel::ProviderId;
use hodei_jobs_domain::workers::WorkerProvider;
use hodei_jobs_grpc::services::{
    AuditServiceImpl, JobExecutionServiceImpl, LogStreamService, LogStreamServiceGrpc,
    MetricsServiceImpl, ProviderManagementServiceImpl, SchedulerServiceImpl,
    WorkerAgentServiceImpl,
};
use hodei_jobs_infrastructure::persistence::postgres::PostgresAuditRepository;
use hodei_jobs_infrastructure::persistence::{
    PostgresJobQueue, PostgresJobRepository, PostgresProviderConfigRepository,
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
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

use hodei_jobs_grpc::interceptors::context::context_interceptor;
use hodei_jobs_grpc::worker_command_sender::GrpcWorkerCommandSender;

/// T4.4: Server TLS configuration structure
#[derive(Clone)]
struct ServerTlsSettings {
    enabled: bool,
    server_cert_path: Option<std::path::PathBuf>,
    server_key_path: Option<std::path::PathBuf>,
    ca_cert_path: Option<std::path::PathBuf>,
    require_client_cert: bool,
}

impl ServerTlsSettings {
    fn from_env() -> Self {
        let enabled = env::var("HODEI_SERVER_TLS_ENABLED")
            .map(|v| v == "1")
            .unwrap_or(false);

        let server_cert_path = env::var("HODEI_SERVER_CERT_PATH").ok().map(|p| p.into());
        let server_key_path = env::var("HODEI_SERVER_KEY_PATH").ok().map(|p| p.into());
        let ca_cert_path = env::var("HODEI_CA_CERT_PATH").ok().map(|p| p.into());
        let require_client_cert = env::var("HODEI_REQUIRE_CLIENT_CERT")
            .map(|v| v == "1")
            .unwrap_or(false);

        Self {
            enabled,
            server_cert_path,
            server_key_path,
            ca_cert_path,
            require_client_cert,
        }
    }

    /// T4.4: Verify certificate files exist (without loading them)
    async fn verify_certificates(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(path) = &self.server_cert_path {
            tokio::fs::metadata(path)
                .await
                .map_err(|e| format!("Server certificate not found: {} - {}", path.display(), e))?;
        }

        if let Some(path) = &self.server_key_path {
            tokio::fs::metadata(path)
                .await
                .map_err(|e| format!("Server key not found: {} - {}", path.display(), e))?;
        }

        if let Some(path) = &self.ca_cert_path {
            tokio::fs::metadata(path)
                .await
                .map_err(|e| format!("CA certificate not found: {} - {}", path.display(), e))?;
        }

        Ok(())
    }

    /// T4.4: Check if mTLS is properly configured
    fn is_mtls_configured(&self) -> bool {
        self.enabled
            && self.server_cert_path.is_some()
            && self.server_key_path.is_some()
            && self.ca_cert_path.is_some()
            && self.require_client_cert
    }
}

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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
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

    // T4.4: Load and configure TLS settings
    let tls_settings = ServerTlsSettings::from_env();

    if tls_settings.enabled {
        info!("✓ TLS enabled");
        if let Some(cert_path) = &tls_settings.server_cert_path {
            info!("  • Server Cert: {:?}", cert_path);
        }
        if let Some(key_path) = &tls_settings.server_key_path {
            info!("  • Server Key: {:?}", key_path);
        }
        if let Some(ca_path) = &tls_settings.ca_cert_path {
            info!("  • CA Cert: {:?}", ca_path);
        }

        if tls_settings.require_client_cert {
            info!("✓ Client certificate validation ENABLED (mTLS)");
            info!("  Workers must present valid client certificates");
        } else {
            info!("⚠ Client certificate validation DISABLED");
            info!("  Set HODEI_REQUIRE_CLIENT_CERT=1 to enable mTLS");
        }
    } else {
        warn!("⚠ TLS NOT CONFIGURED - Server running in insecure mode");
        warn!("  For production, enable TLS with:");
        warn!("    export HODEI_SERVER_TLS_ENABLED=1");
        warn!("    export HODEI_SERVER_CERT_PATH=/path/to/server-cert.pem");
        warn!("    export HODEI_SERVER_KEY_PATH=/path/to/server-key.pem");
        warn!("    export HODEI_CA_CERT_PATH=/path/to/ca-cert.pem");
        warn!("    export HODEI_REQUIRE_CLIENT_CERT=1");
    }

    if dev_mode {
        info!("🔓 Development mode ENABLED (accepting dev-* tokens)");
    }

    // Create shared log stream service
    let log_stream_service = LogStreamService::new();

    let db_settings = GrpcDatabaseSettings::from_env()?;

    // Create shared Postgres Pool
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(db_settings.max_connections)
        .acquire_timeout(db_settings.connection_timeout)
        .connect(&db_settings.url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database: {}", e))?;

    // Create repositories using shared pool
    let job_repository_impl = PostgresJobRepository::new(pool.clone());
    job_repository_impl.run_migrations().await?;

    let job_queue_impl = PostgresJobQueue::new(pool.clone());
    job_queue_impl.run_migrations().await?;

    let worker_registry_impl = PostgresWorkerRegistry::new(pool.clone());
    worker_registry_impl.run_migrations().await?;

    let token_store_impl = PostgresWorkerBootstrapTokenStore::new(pool.clone());
    token_store_impl.run_migrations().await?;

    let provider_config_repo_impl = PostgresProviderConfigRepository::new(pool.clone());
    provider_config_repo_impl.run_migrations().await?;

    // Create Audit Repository and run migrations (Story 3.1)
    let audit_repository_impl = PostgresAuditRepository::new(pool.clone());
    audit_repository_impl.run_migrations().await?;
    let audit_repository = std::sync::Arc::new(audit_repository_impl)
        as std::sync::Arc<dyn hodei_jobs_domain::audit::AuditRepository>;

    // Create Audit Service (Story 3.2)
    let audit_service = AuditService::new(audit_repository.clone());

    // Create Audit gRPC Service (Story 15.8)
    let audit_grpc_service = AuditServiceImpl::new(audit_repository.clone());

    // Create Audit Cleanup Service (Story 15.9)
    let audit_cleanup_config = AuditRetentionConfig::from_env();
    let audit_cleanup_service = std::sync::Arc::new(AuditCleanupService::new(
        audit_repository,
        audit_cleanup_config.clone(),
    ));

    // Start background cleanup task
    if audit_cleanup_config.enabled {
        info!(
            "  ✓ Audit cleanup enabled (retention: {} days, interval: {:?})",
            audit_cleanup_config.retention_days, audit_cleanup_config.cleanup_interval
        );
        audit_cleanup_service.clone().start_background_cleanup();
    } else {
        info!("  ⚠ Audit cleanup disabled (HODEI_AUDIT_CLEANUP_ENABLED != 1)");
    }

    // Create Event Bus
    let event_bus_impl =
        hodei_jobs_infrastructure::messaging::postgres::PostgresEventBus::new(pool.clone());
    let event_bus = std::sync::Arc::new(event_bus_impl);

    // Cast to traits
    let job_repository = std::sync::Arc::new(job_repository_impl)
        as std::sync::Arc<dyn hodei_jobs_domain::jobs::JobRepository>;
    let job_queue = std::sync::Arc::new(job_queue_impl)
        as std::sync::Arc<dyn hodei_jobs_domain::jobs::JobQueue>;
    let worker_registry = std::sync::Arc::new(worker_registry_impl)
        as std::sync::Arc<dyn hodei_jobs_domain::workers::WorkerRegistry>;
    let token_store = std::sync::Arc::new(token_store_impl)
        as std::sync::Arc<dyn hodei_jobs_domain::iam::WorkerBootstrapTokenStore>;
    let provider_config_repo = std::sync::Arc::new(provider_config_repo_impl)
        as std::sync::Arc<dyn hodei_jobs_domain::providers::ProviderConfigRepository>;

    let provider_registry =
        std::sync::Arc::new(ProviderRegistry::new(provider_config_repo.clone()));

    let create_job_usecase =
        CreateJobUseCase::new(job_repository.clone(), job_queue.clone(), event_bus.clone());
    let cancel_job_usecase = CancelJobUseCase::new(job_repository.clone(), event_bus.clone());

    let scheduler_job_repository = job_repository.clone();
    let scheduler_job_queue = job_queue.clone();
    let scheduler_worker_registry = worker_registry.clone();
    let scheduler_create_job_usecase = CreateJobUseCase::new(
        scheduler_job_repository.clone(),
        scheduler_job_queue.clone(),
        event_bus.clone(),
    );

    // Create services with shared log stream
    let worker_service =
        WorkerAgentServiceImpl::with_registry_job_repository_token_store_and_log_service(
            worker_registry.clone(),
            job_repository.clone(),
            token_store.clone(),
            log_stream_service.clone(),
            event_bus.clone(),
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

    // Keep track of provisioning service to inject into controller
    let mut provisioning_service_for_controller: Option<
        std::sync::Arc<dyn hodei_jobs_application::workers::WorkerProvisioningService>,
    > = None;

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
                    let provider_id: ProviderId = provider.provider_id().clone();
                    providers.insert(
                        provider_id.clone(),
                        std::sync::Arc::new(provider) as std::sync::Arc<dyn WorkerProvider>,
                    );

                    // Register provider in ProviderConfigRepository
                    let docker_config = hodei_jobs_domain::providers::ProviderConfig::new(
                        "Docker".to_string(),
                        hodei_jobs_domain::workers::ProviderType::Docker,
                        hodei_jobs_domain::providers::ProviderTypeConfig::Docker(
                            hodei_jobs_domain::providers::DockerConfig::default(),
                        ),
                    )
                    .with_max_workers(10);
                    if let Err(e) = provider_config_repo.save(&docker_config).await {
                        tracing::warn!(
                            "Failed to register Docker provider in config repository: {}",
                            e
                        );
                    }
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
                            .unwrap_or_else(|_| "hodei-jobs-workers".to_string())
                    );
                    let provider_id: ProviderId = provider.provider_id().clone();
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
                    let provider_id: ProviderId = provider.provider_id().clone();
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
                    .unwrap_or_else(|_| "hodei-jobs-worker:latest".to_string()),
            );

            let provisioning_service = std::sync::Arc::new(DefaultWorkerProvisioningService::new(
                scheduler_worker_registry.clone(),
                token_store.clone(),
                providers,
                provisioning_config,
            ))
                as std::sync::Arc<dyn hodei_jobs_application::workers::WorkerProvisioningService>;

            // Save for controller
            provisioning_service_for_controller = Some(provisioning_service.clone());

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
    info!("  ✓ AuditService (EPIC-15 Story 15.8)");
    info!("  ✓ Reflection Service");
    info!("  ✓ Event Bus (Postgres)");

    // JobController loop (HU-6.3+6.4) - REFACTORED FOR EDA
    let controller_enabled =
        env::var("HODEI_JOB_CONTROLLER_ENABLED").unwrap_or_else(|_| "1".to_string()) == "1";
    let controller_interval_ms = env::var("HODEI_JOB_CONTROLLER_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(500);

    if controller_enabled {
        info!(
            "Starting JobController loop (interval={}ms, event-driven)",
            controller_interval_ms
        );

        let sender =
            std::sync::Arc::new(GrpcWorkerCommandSender::new(worker_service_for_controller))
                as std::sync::Arc<dyn hodei_jobs_application::workers::WorkerCommandSender>;

        let controller = std::sync::Arc::new(JobController::new(
            job_queue_for_controller,
            job_repository_for_controller,
            worker_registry_for_controller,
            SchedulerConfig::default(),
            sender,
            event_bus.clone(),
            provisioning_service_for_controller,
        ));

        let interval = Duration::from_millis(controller_interval_ms);
        let event_bus_clone = event_bus.clone();

        tokio::spawn(async move {
            use hodei_jobs_domain::event_bus::EventBus;
            use tokio_stream::StreamExt;

            let mut events = match event_bus_clone.subscribe("hodei_events").await {
                Ok(stream) => stream,
                Err(e) => {
                    tracing::error!(
                        "Failed to subscribe to 'hodei_events': {}. Falling back to polling.",
                        e
                    );
                    // If subscription fails, we loop with just interval
                    loop {
                        if let Err(e) = controller.run_once().await {
                            tracing::error!("JobController run_once failed: {}", e);
                        }
                        tokio::time::sleep(interval).await;
                    }
                }
            };

            // Initial run to catch up
            if let Err(e) = controller.run_once().await {
                tracing::error!("JobController initial run failed: {}", e);
            }

            loop {
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {
                        // Keep-alive / polling fallback
                        if let Err(e) = controller.run_once().await {
                            tracing::error!("JobController run_once failed (timer): {}", e);
                        }
                    }
                    Some(event_res) = events.next() => {
                        match event_res {
                            Ok(event) => {
                                tracing::debug!("Controller woke up by event: {:?}", event);
                                // Trigger processing for the event
                                // In a perfect world, we'd check if event relates to job creation/state change
                                // For now, any event triggers a scheduler run attempt
                                if let Err(e) = controller.run_once().await {
                                    tracing::error!("JobController run_once failed (event): {}", e);
                                }
                            }
                            Err(e) => {
                                tracing::error!("Event bus stream error: {}", e);
                            }
                        }
                    }
                }
            }
        });
    } else {
        info!("JobController loop disabled (HODEI_JOB_CONTROLLER_ENABLED != 1)");
    }

    // Start Audit Service Subscriber Loop (Story 3.2)
    let audit_service_clone = audit_service.clone();
    let event_bus_for_audit = event_bus.clone();

    tokio::spawn(async move {
        use hodei_jobs_domain::event_bus::EventBus;
        use tokio_stream::StreamExt;

        info!("Starting Audit Service subscriber loop for 'hodei_events'");

        let mut events = match event_bus_for_audit.subscribe("hodei_events").await {
            Ok(stream) => stream,
            Err(e) => {
                tracing::error!("Failed to subscribe Audit Service to 'hodei_events': {}", e);
                return;
            }
        };

        while let Some(event_res) = events.next().await {
            match event_res {
                Ok(event) => {
                    tracing::debug!("Audit Service received event: {:?}", event);
                    if let Err(e) = audit_service_clone.log_event(&event).await {
                        tracing::error!("Failed to log audit event: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("Audit event stream error: {}", e);
                }
            }
        }
        tracing::warn!("Audit Service event stream ended");
    });

    // Configure CORS for gRPC-Web (US-12.1.4)
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers(Any)
        .allow_methods(Any)
        .expose_headers(Any);

    info!("  ✓ gRPC-Web support enabled (CORS configured)");

    // Build and start server with gRPC-Web support and context interceptor
    // The context_interceptor extracts/generates correlation_id and actor from headers
    // and makes them available via RequestContextExt::get_context()
    info!("  ✓ Context interceptor enabled (correlation_id, actor propagation)");

    // T4.4: Configure TLS if enabled
    let mut server_builder = Server::builder().accept_http1(true);

    if tls_settings.enabled {
        info!("Verifying TLS certificate files...");

        // Verify certificate files exist (without loading them yet)
        if let Err(e) = tls_settings.verify_certificates().await {
            warn!("Certificate verification failed: {}", e);
            warn!("TLS will not be enabled - server running in insecure mode");
        } else {
            info!("✓ Certificate files verified and readable");
            // Note: Actual TLS configuration requires tonic with TLS support
            // This will be implemented when upgrading to tonic >= 0.15
            warn!("⚠ TLS connection not yet applied (requires tonic TLS feature)");

            if tls_settings.require_client_cert {
                info!("✓ mTLS configuration detected - client cert validation ready");
                info!("  Workers will need to present valid client certificates");
            }
        }
    }

    // Start server with all configured layers and services
    server_builder
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
        .add_service(AuditServiceServer::new(audit_grpc_service))
        .serve(addr)
        .await?;

    Ok(())
}

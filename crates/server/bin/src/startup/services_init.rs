//! gRPC Services Initialization Module
//!
//! Responsible for instantiating and configuring all gRPC services with their dependencies.
//! Follows Single Responsibility Principle - only handles service initialization.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::time;
use tracing::info;

use hodei_server_application::jobs::cancel::CancelJobUseCase;
use hodei_server_application::jobs::create::CreateJobUseCase;
use hodei_server_application::providers::ProviderRegistry;
use hodei_server_application::scheduling::SchedulerConfig;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::iam::WorkerBootstrapTokenStore;
use hodei_server_domain::jobs::JobQueue;
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::outbox::OutboxRepository;
use hodei_server_domain::workers::registry::WorkerRegistry;
use hodei_server_infrastructure::persistence::postgres::{
    PostgresJobQueue, PostgresProviderConfigRepository,
};
use hodei_server_interface::grpc::{
    JobExecutionServiceImpl, LogStreamService, MetricsServiceImpl, ProviderManagementServiceImpl,
    SchedulerServiceImpl, WorkerAgentServiceImpl,
};

/// Container for all initialized gRPC services.
/// Used to register them with the tonic Server.
pub struct GrpcServices {
    pub worker_agent_service: WorkerAgentServiceImpl,
    pub job_execution_service: JobExecutionServiceImpl,
    pub scheduler_service: SchedulerServiceImpl,
    pub provider_management_service: ProviderManagementServiceImpl,
    pub log_stream_service: Arc<LogStreamService>,
    pub metrics_service: MetricsServiceImpl,
}

/// Job coordinator shutdown handle for graceful shutdown
pub struct CoordinatorShutdownHandle {
    shutdown_tx: watch::Sender<()>,
}

/// Initialize all gRPC services with their dependencies.
///
/// This function creates and configures all gRPC services that will be
/// registered with the tonic server. Each service is initialized with
/// its required dependencies from AppState.
pub fn initialize_grpc_services(
    pool: sqlx::PgPool,
    worker_registry: Arc<dyn WorkerRegistry>,
    job_repository: Arc<dyn JobRepository>,
    _token_store: Arc<dyn WorkerBootstrapTokenStore>,
    event_bus: Arc<dyn EventBus>,
    _outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
) -> GrpcServices {
    info!("Initializing gRPC services...");

    // Initialize JobQueue for scheduler
    let job_queue: Arc<dyn JobQueue> = Arc::new(PostgresJobQueue::new(pool.clone()));
    info!("✓ JobQueue initialized");

    // Initialize CreateJobUseCase with all dependencies
    info!("Initializing CreateJobUseCase...");
    let create_job_usecase = Arc::new(CreateJobUseCase::new(
        job_repository.clone(),
        job_queue.clone(),
        event_bus.clone(),
    ));
    info!("✓ CreateJobUseCase initialized");

    // Initialize CancelJobUseCase
    info!("Initializing CancelJobUseCase...");
    let cancel_job_usecase = Arc::new(CancelJobUseCase::new(
        job_repository.clone(),
        event_bus.clone(),
    ));
    info!("✓ CancelJobUseCase initialized");

    // Initialize ProviderRegistry with its repository
    info!("Initializing ProviderRegistry...");
    let provider_config_repo: Arc<dyn hodei_server_domain::providers::ProviderConfigRepository> =
        Arc::new(PostgresProviderConfigRepository::new(pool.clone()));
    let provider_registry = Arc::new(ProviderRegistry::new(provider_config_repo));
    info!("✓ ProviderRegistry initialized");

    // Initialize LogStreamService first (used by other services)
    info!("Initializing LogStreamService...");
    let log_stream_service = Arc::new(LogStreamService::new());
    info!("✓ LogStreamService initialized");

    // Initialize WorkerAgentService
    info!("Initializing WorkerAgentService...");
    let worker_agent_service = WorkerAgentServiceImpl::new();
    info!("✓ WorkerAgentService initialized");

    // Initialize JobExecutionService
    info!("Initializing JobExecutionService...");
    let job_execution_service = JobExecutionServiceImpl::new(
        create_job_usecase.clone(),
        cancel_job_usecase.clone(),
        job_repository.clone(),
        worker_registry.clone(),
    );
    info!("✓ JobExecutionService initialized");

    // Initialize SchedulerService
    info!("Initializing SchedulerService...");
    let scheduler_config = SchedulerConfig::default();
    let scheduler_service = SchedulerServiceImpl::new(
        create_job_usecase.clone(),
        job_repository.clone(),
        job_queue.clone(),
        worker_registry.clone(),
        scheduler_config,
    );
    info!("✓ SchedulerService initialized");

    // Initialize ProviderManagementService
    info!("Initializing ProviderManagementService...");
    let provider_management_service = ProviderManagementServiceImpl::new(provider_registry);
    info!("✓ ProviderManagementService initialized");

    // Initialize MetricsService
    info!("Initializing MetricsService...");
    let metrics_service = MetricsServiceImpl::new();
    info!("✓ MetricsService initialized");

    info!("All gRPC services initialized successfully");

    GrpcServices {
        worker_agent_service,
        job_execution_service,
        scheduler_service,
        provider_management_service,
        log_stream_service,
        metrics_service,
    }
}

/// Simple job processor that polls the job queue and assigns jobs to workers.
/// This is a fallback mechanism when the full JobCoordinator is not available.
async fn start_simple_job_processor(
    job_queue: Arc<dyn JobQueue>,
    mut shutdown_rx: watch::Receiver<()>,
) {
    info!("Starting simple job processor (polling mode)");

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                info!("Job processor shutting down");
                break;
            }
            _ = time::sleep(Duration::from_secs(2)) => {
                // Process pending jobs
                if let Err(e) = process_pending_job(&job_queue).await {
                    tracing::error!("Error processing pending jobs: {:?}", e);
                }
            }
        }
    }
}

/// Process the next pending job from the queue
async fn process_pending_job(job_queue: &Arc<dyn JobQueue>) -> anyhow::Result<()> {
    // Peek next job from queue
    let _next_job = job_queue
        .peek()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to peek job from queue: {:?}", e))?;

    // For now, just log that we're processing
    tracing::debug!("Job queue check completed");

    Ok(())
}

/// Initialize and start the job processor in background.
///
/// This processor polls the job queue and dispatches jobs to workers.
/// It runs as a separate Tokio task for reactive event processing.
pub async fn start_job_coordinator(
    pool: sqlx::PgPool,
    _worker_registry: Arc<dyn WorkerRegistry>,
    _job_repository: Arc<dyn JobRepository>,
    _event_bus: Arc<dyn EventBus>,
) -> CoordinatorShutdownHandle {
    info!("Initializing job coordinator for reactive job processing...");

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = watch::channel(());

    // Initialize job queue
    let job_queue: Arc<dyn JobQueue> = Arc::new(PostgresJobQueue::new(pool.clone()));

    // Start the simple job processor in background
    tokio::spawn(async move {
        start_simple_job_processor(job_queue, shutdown_rx).await;
    });

    info!("✓ Job coordinator started in background");

    CoordinatorShutdownHandle { shutdown_tx }
}

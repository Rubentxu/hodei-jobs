//! gRPC Services Initialization Module
//!
//! Responsible for instantiating and configuring all gRPC services with their dependencies.
//! Follows Single Responsibility Principle - only handles service initialization.
//! Also handles saga consumers and background tasks wiring.

use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::time;
use tracing::{debug, error, info, warn};

use super::StartupConfig;

use hodei_server_application::command::execution::JobExecutorImpl;
use hodei_server_application::command::execution::register_execution_command_handlers_with_executor;
use hodei_server_application::jobs::cancel::CancelJobUseCase;
use hodei_server_application::jobs::coordinator::JobCoordinator;
use hodei_server_application::jobs::create::CreateJobUseCase;
use hodei_server_application::jobs::dispatcher::JobDispatcher;
use hodei_server_application::jobs::worker_monitor::WorkerMonitor;
use hodei_server_application::providers::ProviderRegistry;
use hodei_server_application::saga::bridge::CommandBusJobExecutionPort;
use hodei_server_application::saga::dispatcher_saga::{
    DynExecutionSagaDispatcher, ExecutionSagaDispatcher,
};
use hodei_server_application::saga::sync_durable_executor::SyncDurableWorkflowExecutor;
use hodei_server_application::saga::timeout_checker::TimeoutChecker;
use hodei_server_application::saga::workflows::execution_durable::ExecutionWorkflow;
use hodei_server_application::saga::workflows::timeout_durable::TimeoutDurableWorkflow;
use hodei_server_application::scheduling::SchedulerConfig;
use hodei_server_application::workers::lifecycle::WorkerLifecycleManager;
use hodei_server_application::workers::provisioning::WorkerProvisioningService;
use hodei_server_application::workers::provisioning_impl::{
    DefaultWorkerProvisioningService, ProvisioningConfig,
};
use hodei_server_domain::command::{
    InMemoryErasedCommandBus, OutboxCommandBus, OutboxCommandBusExt,
};
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::iam::WorkerBootstrapTokenStore;
use hodei_server_domain::jobs::JobQueue;
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::outbox::OutboxRepository;
use hodei_server_domain::saga::{Saga, SagaOrchestrator, SagaType};
use hodei_server_domain::shared_kernel::WorkerId;
use hodei_server_domain::workers::registry::WorkerRegistry;
use hodei_server_infrastructure::messaging::cancellation_saga_consumer::CancellationSagaConsumer;
use hodei_server_infrastructure::messaging::cleanup_saga_consumer::CleanupSagaConsumer;
use hodei_server_infrastructure::messaging::command_dlq;
use hodei_server_infrastructure::messaging::hybrid::command_relay::CommandRelay;
use hodei_server_infrastructure::messaging::hybrid::create_command_relay;
use hodei_server_infrastructure::messaging::nats::NatsEventBus;
use hodei_server_infrastructure::messaging::nats_outbox_relay::NatsOutboxRelay;
use hodei_server_infrastructure::messaging::saga_command_consumers;
use hodei_server_infrastructure::messaging::worker_ephemeral_terminating_consumer::WorkerEphemeralTerminatingConsumer;
use hodei_server_infrastructure::persistence::command_outbox::PostgresCommandOutboxRepository;
use hodei_server_infrastructure::persistence::outbox::PostgresOutboxRepository;
use hodei_server_infrastructure::persistence::postgres::SagaPoller;
use hodei_server_infrastructure::persistence::postgres::{
    PostgresJobQueue, PostgresJobRepository, PostgresProviderConfigRepository,
    PostgresSagaOrchestrator, PostgresSagaRepository, PostgresWorkerRegistry,
};
use hodei_server_infrastructure::persistence::saga::{
    NotifyingRepositoryMetrics, NotifyingSagaRepository, ReactiveSagaProcessor,
    ReactiveSagaProcessorConfig, ReactiveSagaProcessorMetrics,
};
use hodei_server_interface::grpc::{
    JobExecutionServiceImpl, LogStreamService, MetricsServiceImpl, ProviderManagementServiceImpl,
    SchedulerServiceImpl, WorkerAgentServiceImpl, worker_command_sender::GrpcWorkerCommandSender,
};

/// Container for all initialized gRPC services.
/// Used to register them with the tonic Server.
#[derive(Clone)]
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
    token_store: Arc<dyn WorkerBootstrapTokenStore>,
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

    // Initialize WorkerAgentService with all dependencies for proper worker registration
    info!("Initializing WorkerAgentService...");
    let worker_agent_service =
        WorkerAgentServiceImpl::with_registry_job_repository_token_store_and_log_service(
            worker_registry.clone(),
            job_repository.clone(),
            token_store,
            log_stream_service.clone(),
            event_bus.clone(),
        );
    info!("✓ WorkerAgentService initialized (with worker_registry, job_repository, token_store)");

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

/// Initialize and start the job processor in background.
///
/// This processor polls the job queue and dispatches jobs to workers.
/// It runs as a separate Tokio task for reactive event processing.
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `worker_registry` - Worker registry for registration
/// * `job_repository` - Job repository for persistence
/// * `event_bus` - Event bus for publishing events
/// * `token_store` - OTP token store for worker authentication
/// * `lifecycle_manager` - Worker lifecycle manager for provisioning
/// * `saga_orchestrator` - Saga orchestrator for saga-based workflows
pub async fn start_job_coordinator(
    pool: sqlx::PgPool,
    worker_registry: Arc<dyn WorkerRegistry>,
    job_repository: Arc<dyn JobRepository>,
    event_bus: Arc<dyn EventBus>,
    token_store: Arc<dyn WorkerBootstrapTokenStore>,
    lifecycle_manager: Arc<WorkerLifecycleManager>,
    worker_agent_service: WorkerAgentServiceImpl,
    _saga_orchestrator: Arc<
        dyn hodei_server_domain::saga::SagaOrchestrator<
                Error = hodei_server_domain::shared_kernel::DomainError,
            > + Send
            + Sync,
    >,
    config: StartupConfig,
    app_state: Arc<super::AppState>,
) -> CoordinatorShutdownHandle {
    info!("Initializing job coordinator for reactive job processing...");

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = watch::channel(());

    // Initialize job queue
    let job_queue: Arc<dyn JobQueue> = Arc::new(PostgresJobQueue::new(pool.clone()));

    // Initialize provider registry
    let provider_config_repo: Arc<dyn hodei_server_domain::providers::ProviderConfigRepository> =
        Arc::new(PostgresProviderConfigRepository::new(pool.clone()));
    let provider_registry = Arc::new(ProviderRegistry::new(provider_config_repo));

    // Initialize outbox repository
    let outbox_repository: Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync> =
        Arc::new(
            hodei_server_infrastructure::persistence::outbox::PostgresOutboxRepository::new(
                pool.clone(),
            ),
        );

    // Create ProvisioningService with real implementation
    let default_image = std::env::var("HODEI_WORKER_IMAGE")
        .unwrap_or_else(|_| "localhost:31500/hodei-jobs-worker:latest".to_string());
    info!("🎯 Using worker image: {}", default_image);

    // Get the worker server address (uses telepresence DNS in dev mode)
    let worker_server_address = config.get_worker_server_address();
    info!("🌐 Worker server address: {}", worker_server_address);

    let provisioning_config =
        ProvisioningConfig::new(worker_server_address).with_default_image(default_image);
    let providers_map = lifecycle_manager.providers_map();
    let provisioning_service: Arc<dyn WorkerProvisioningService> =
        Arc::new(DefaultWorkerProvisioningService::new(
            worker_registry.clone(),
            token_store.clone(),
            providers_map,
            provider_registry.clone(), // Pass provider registry to get default_image
            provisioning_config,
        ));
    info!("✓ WorkerProvisioningService initialized");

    // Create scheduler config
    let scheduler_config = hodei_server_domain::scheduling::SchedulerConfig::default();

    // Create WorkerCommandSender with real gRPC implementation
    let grpc_command_sender = GrpcWorkerCommandSender::new(worker_agent_service);
    let worker_command_sender: Arc<
        dyn hodei_server_application::workers::commands::WorkerCommandSender,
    > = Arc::new(grpc_command_sender);
    info!("✓ GrpcWorkerCommandSender initialized");

    // EPIC-94-C: Create ExecutionSagaDispatcher for v4.0 DurableWorkflow
    // This uses the CommandBus for real command handling
    let (command_bus, _) = create_command_bus(pool.clone());
    info!("✓ CommandBus created for ExecutionSagaDispatcher");

    // Create CommandBusJobExecutionPort wrapping the command bus
    let execution_port = CommandBusJobExecutionPort::new(Some(command_bus.clone()));

    // Create ExecutionWorkflow with the port
    let execution_workflow = ExecutionWorkflow::new(Arc::new(execution_port));

    // Create SyncDurableWorkflowExecutor for the execution workflow
    let execution_executor = Arc::new(SyncDurableWorkflowExecutor::new(execution_workflow));

    // Create the ExecutionSagaDispatcher
    let execution_saga_dispatcher: Arc<
        dyn hodei_server_application::saga::dispatcher_saga::ExecutionSagaDispatcherTrait
            + Send
            + Sync,
    > = Arc::new(DynExecutionSagaDispatcher::new(
        ExecutionSagaDispatcher::new(execution_executor, worker_registry.clone()),
    ));
    info!("✓ ExecutionSagaDispatcher initialized (v4.0 DurableWorkflow)");

    // EPIC-94-C: Get workflow coordinator from AppState
    let provisioning_workflow_coordinator = app_state.provisioning_workflow_coordinator.clone();

    // Create JobDispatcher with ExecutionSagaDispatcher (v4.0)
    let job_dispatcher = Arc::new(JobDispatcher::new(
        job_queue.clone(),
        job_repository.clone(),
        worker_registry.clone(),
        provider_registry.clone(),
        scheduler_config,
        worker_command_sender,
        event_bus.clone(),
        outbox_repository.clone(),
        Some(provisioning_service),      // Now we have real provisioning!
        Some(execution_saga_dispatcher), // v4.0 ExecutionSagaDispatcher
        provisioning_workflow_coordinator, // EPIC-94-C: v4.0 DurableWorkflow coordinator
    ));
    info!("✓ JobDispatcher initialized with v4.0 ExecutionSagaDispatcher");

    // EPIC-32: Initialize WorkerMonitor for reactive worker tracking
    let worker_monitor = WorkerMonitor::new(worker_registry.clone(), event_bus.clone());
    info!("✓ WorkerMonitor initialized");

    // EPIC-32: Create the reactive JobCoordinator (not the polling loop)
    let mut job_coordinator = JobCoordinator::new(
        event_bus.clone(),
        job_dispatcher.clone(),
        Arc::new(worker_monitor),
        worker_registry.clone(),
    );
    info!("✓ JobCoordinator created (reactive mode)");

    // Keep dispatcher reference for fallback (move into async block)
    let dispatcher = job_dispatcher.clone();
    let shutdown_rx = shutdown_rx;

    // Start the reactive JobCoordinator in background
    // This will subscribe to WorkerReady and JobQueued events reactively
    // instead of polling the job queue
    tokio::spawn(async move {
        if let Err(e) = job_coordinator.start().await {
            tracing::error!("JobCoordinator failed, falling back to polling: {:?}", e);
            // Fallback to polling dispatch loop
            start_dispatch_loop(dispatcher, shutdown_rx).await;
        }
    });

    info!("✓ Job coordinator started in REACTIVE mode (EPIC-32)");

    CoordinatorShutdownHandle { shutdown_tx }
}

/// Dispatch loop that polls and dispatches jobs
///
/// @deprecated Use JobCoordinator reactive mode instead.
/// This function is only used when JobCoordinator fails.
#[deprecated(since = "0.59.1", note = "Use JobCoordinator reactive mode instead")]
async fn start_dispatch_loop(dispatcher: Arc<JobDispatcher>, mut shutdown_rx: watch::Receiver<()>) {
    info!("Starting dispatch loop");

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                info!("Dispatch loop shutting down");
                break;
            }
            _ = time::sleep(Duration::from_secs(20)) => {
                // Dispatch fallback: solo activa si el sistema reactivo falla
                // Intervalo largo para no sobrecargar cuando no hay jobs
                if let Err(e) = dispatcher.dispatch_once().await {
                    tracing::error!("Error dispatching jobs: {:?}", e);
                }
            }
        }
    }
}

/// Shutdown handle for saga consumers
pub struct SagaConsumersShutdownHandle {
    pub cancellation_consumer_stop: tokio::sync::broadcast::Sender<()>,
    pub cleanup_consumer_stop: tokio::sync::broadcast::Sender<()>,
}

/// Start all saga consumers (Cancellation, Cleanup)
///
/// This function wires up the event-driven saga consumers that listen to
/// domain events and trigger appropriate sagas for handling:
/// - Job cancellation via CancellationSaga
/// - Resource cleanup via CleanupSaga
pub async fn start_saga_consumers(
    nats_event_bus: &NatsEventBus,
    saga_orchestrator: Arc<PostgresSagaOrchestrator<PostgresSagaRepository>>,
    pool: sqlx::PgPool,
) -> SagaConsumersShutdownHandle {
    info!("🚀 Starting saga consumers...");

    // Create concrete repositories for the saga consumers
    let job_repository = Arc::new(PostgresJobRepository::new(pool.clone()));
    let worker_registry = Arc::new(PostgresWorkerRegistry::new(pool.clone()));

    let (cancellation_stop_tx, cancellation_stop_rx) = tokio::sync::broadcast::channel(1);
    let (cleanup_stop_tx, cleanup_stop_rx) = tokio::sync::broadcast::channel(1);

    // Create command bus for saga consumers
    let (command_bus, _) = create_command_bus(pool.clone());

    // Start CancellationSagaConsumer
    let cancellation_consumer = CancellationSagaConsumer::new(
        nats_event_bus.client().clone(),
        nats_event_bus.jetstream().clone(),
        saga_orchestrator.clone(),
        job_repository.clone(),
        worker_registry.clone(),
        None,
        command_bus.clone(),
    );

    tokio::spawn(async move {
        // US-18: Pass shutdown receiver for graceful drain
        if let Err(e) = cancellation_consumer
            .start(Some(cancellation_stop_rx))
            .await
        {
            tracing::error!("❌ CancellationSagaConsumer failed: {:?}", e);
        }
    });
    info!("✅ CancellationSagaConsumer started");

    // Start CleanupSagaConsumer
    let cleanup_consumer = CleanupSagaConsumer::new(
        nats_event_bus.client().clone(),
        nats_event_bus.jetstream().clone(),
        saga_orchestrator.clone(),
        job_repository.clone(),
        worker_registry.clone(),
        None,
    );

    tokio::spawn(async move {
        // US-18: Pass shutdown receiver for graceful drain
        if let Err(e) = cleanup_consumer.start(Some(cleanup_stop_rx)).await {
            tracing::error!("❌ CleanupSagaConsumer failed: {:?}", e);
        }
    });
    info!("✅ CleanupSagaConsumer started");

    SagaConsumersShutdownHandle {
        cancellation_consumer_stop: cancellation_stop_tx,
        cleanup_consumer_stop: cleanup_stop_tx,
    }
}

/// Shutdown handle for background tasks
pub struct BackgroundTasksShutdownHandle {
    pub timeout_checker_stop: tokio::sync::broadcast::Sender<()>,
}

/// Start SagaPoller for processing pending sagas
///
/// SagaPoller polls the database for pending sagas and executes them.
/// This is a fallback mechanism for sagas that weren't triggered by events.
pub async fn start_saga_poller(
    pool: sqlx::PgPool,
    saga_orchestrator: Arc<PostgresSagaOrchestrator<PostgresSagaRepository>>,
) -> tokio::sync::mpsc::Sender<()> {
    info!("🔄 Starting SagaPoller for pending saga processing...");

    let repository = Arc::new(PostgresSagaRepository::new(pool));

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel(1);

    let poller = SagaPoller::new(repository, saga_orchestrator, None);

    // Factory function to create sagas from type and metadata
    let saga_factory = |saga_type: SagaType, _metadata: Value| -> Option<Box<dyn Saga>> {
        match saga_type {
            SagaType::Execution => Some(Box::new(hodei_server_domain::saga::ExecutionSaga::new(
                hodei_server_domain::shared_kernel::JobId::new(),
            ))),
            SagaType::Cancellation => {
                Some(Box::new(hodei_server_domain::saga::CancellationSaga::new(
                    hodei_server_domain::shared_kernel::JobId::new(),
                    "pending_timeout".to_string(),
                )))
            }
            SagaType::Cleanup => Some(Box::new(hodei_server_domain::saga::CleanupSaga::new())),
            SagaType::Timeout => Some(Box::new(hodei_server_domain::saga::TimeoutSaga::new(
                hodei_server_domain::shared_kernel::JobId::new(),
                std::time::Duration::from_secs(3600),
                "timeout".to_string(),
            ))),
            SagaType::Provisioning => {
                // ProvisioningSaga requires WorkerSpec and ProviderId
                // For the poller, we use the orchestrator's factory instead
                None
            }
            SagaType::Recovery => Some(Box::new(hodei_server_domain::saga::RecoverySaga::new(
                hodei_server_domain::shared_kernel::JobId::new(),
                WorkerId::new(),
                None,
            ))),
        }
    };

    tokio::spawn(async move {
        let handle = poller.start(saga_factory);

        tokio::select! {
            _ = shutdown_rx.recv() => {
                info!("🔄 SagaPoller shutting down...");
                let _ = handle.stop().await;
            }
        }
    });

    info!("✅ SagaPoller started");
    shutdown_tx
}

/// Start NATS Outbox Relay for publishing events to NATS
///
/// The Outbox Relay reads pending events from the outbox table
/// and publishes them to NATS JetStream with retry logic.
pub async fn start_nats_outbox_relay(
    pool: sqlx::PgPool,
    nats_event_bus: NatsEventBus,
) -> Result<tokio::sync::broadcast::Sender<()>, anyhow::Error> {
    info!("📦 Starting NATS Outbox Relay for event publishing...");

    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

    // Use std::thread::spawn since run() is a blocking loop
    let _handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create runtime for NatsOutboxRelay");

        rt.block_on(async {
            let relay = NatsOutboxRelay::new(pool, nats_event_bus, None);
            let result = relay.run().await;
            match result {
                Ok(_) => info!("📦 NATS Outbox Relay completed"),
                Err(e) => error!("📦 NATS Outbox Relay error: {}", e),
            }
        });
    });

    // Handle shutdown signal in background
    let mut shutdown_rx_clone = shutdown_rx;
    tokio::spawn(async move {
        let _ = shutdown_rx_clone.recv().await;
        info!("📦 NATS Outbox Relay shutdown signal received");
    });

    info!("✅ NATS Outbox Relay started");
    Ok(shutdown_tx)
}

/// Create a command bus with transactional outbox support
///
/// This function creates a command bus that persists commands to the outbox
/// before dispatching them, implementing the Transactional Outbox pattern.
/// Commands are stored in the hodei_commands table and processed by a background relay.
pub fn create_command_bus(
    pool: sqlx::PgPool,
) -> (
    Arc<dyn hodei_server_domain::command::ErasedCommandBus + Send + Sync>,
    Arc<PostgresCommandOutboxRepository>,
) {
    let (inner_bus, outbox_repository) = create_command_bus_with_inner(pool);

    // Wrap the command bus with outbox support
    let outboxed_bus: hodei_server_domain::command::OutboxCommandBus<
        PostgresCommandOutboxRepository,
        InMemoryErasedCommandBus,
    > = hodei_server_domain::command::OutboxCommandBus::new(outbox_repository.clone(), inner_bus);

    (Arc::new(outboxed_bus), outbox_repository)
}

/// Create a command bus and return the inner bus for handler registration
///
/// This function separates the inner bus creation from the outbox wrapping,
/// allowing handlers to be registered on the inner bus before it's wrapped.
pub fn create_command_bus_with_inner(
    pool: sqlx::PgPool,
) -> (
    Arc<InMemoryErasedCommandBus>,
    Arc<PostgresCommandOutboxRepository>,
) {
    // Create the command outbox repository
    let outbox_repository = Arc::new(PostgresCommandOutboxRepository::new(pool));

    // Create the in-memory command bus as a concrete type (not wrapped)
    let inner_bus: Arc<InMemoryErasedCommandBus> = Arc::new(InMemoryErasedCommandBus::new());

    (inner_bus, outbox_repository)
}

/// Start background tasks (TimeoutChecker)
///
/// This function starts periodic background tasks that perform system maintenance:
/// - TimeoutChecker: Detects jobs that have exceeded their configured timeout
///   and triggers TimeoutSaga for graceful termination
pub async fn start_background_tasks(
    pool: sqlx::PgPool,
    saga_orchestrator: Arc<PostgresSagaOrchestrator<PostgresSagaRepository>>,
    worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
    worker_provisioning: Arc<
        dyn hodei_server_application::workers::provisioning::WorkerProvisioningService,
    >,
) -> BackgroundTasksShutdownHandle {
    info!("⏰ Starting background tasks...");

    // Create concrete job repository for timeout checker
    let job_repository = Arc::new(PostgresJobRepository::new(pool));

    let (timeout_stop_tx, _) = tokio::sync::broadcast::channel(1);

    // Start TimeoutChecker with v4.0 DurableWorkflow
    let timeout_workflow = TimeoutDurableWorkflow::new(worker_registry, worker_provisioning);
    let timeout_executor = Arc::new(SyncDurableWorkflowExecutor::new(timeout_workflow));
    let timeout_checker = TimeoutChecker::new(timeout_executor, job_repository.clone(), None);

    tokio::spawn(async move {
        timeout_checker.run().await;
    });
    info!("✅ TimeoutChecker started");

    BackgroundTasksShutdownHandle {
        timeout_checker_stop: timeout_stop_tx,
    }
}

/// Shutdown handle for ReactiveSagaProcessor
pub struct ReactiveSagaProcessorShutdownHandle {
    pub stop_tx: tokio::sync::watch::Sender<()>,
}

/// Start ReactiveSagaProcessor for reactive saga processing
///
/// The ReactiveSagaProcessor listens to saga notifications from NotifyingSagaRepository
/// and processes sagas immediately without polling. It includes a safety net polling
/// mechanism for stuck sagas (EPIC-45 Gap 6).
pub async fn start_reactive_saga_processor(
    pool: sqlx::PgPool,
    saga_orchestrator: Arc<PostgresSagaOrchestrator<PostgresSagaRepository>>,
) -> ReactiveSagaProcessorShutdownHandle {
    info!("⚡ Starting ReactiveSagaProcessor for reactive saga processing...");

    // Create the inner repository
    let inner_repository = PostgresSagaRepository::new(pool.clone());

    // Create signal channel for saga notifications
    let (signal_tx, signal_rx) = tokio::sync::mpsc::unbounded_channel();

    // Create metrics for NotifyingSagaRepository
    let notifying_metrics = Arc::new(NotifyingRepositoryMetrics::default());

    // Wrap with NotifyingSagaRepository
    let notifying_repository =
        NotifyingSagaRepository::new(inner_repository, signal_tx, notifying_metrics);

    // Create metrics for ReactiveSagaProcessor
    let metrics = Arc::new(ReactiveSagaProcessorMetrics::default());

    // Create the reactive processor with safety net polling
    let config = ReactiveSagaProcessorConfig {
        reactive_enabled: true,
        safety_polling_enabled: true,
        safety_polling_interval: std::time::Duration::from_secs(300),
        max_concurrent_sagas: 10,
        saga_timeout: std::time::Duration::from_secs(300),
        polling_batch_size: 100,
    };

    let (stop_tx, stop_rx) = tokio::sync::watch::channel(());

    let processor = ReactiveSagaProcessor::new(
        Arc::new(notifying_repository),
        signal_rx,
        stop_rx.clone(),
        Some(config),
        Some(metrics),
    );

    // Start the processor
    tokio::spawn(async move {
        processor.run().await;
        info!("⚡ ReactiveSagaProcessor stopped");
    });

    info!("✅ ReactiveSagaProcessor started (reactive + safety net polling)");
    ReactiveSagaProcessorShutdownHandle { stop_tx }
}

/// Shutdown handle for WorkerEphemeralTerminatingConsumer
pub struct WorkerEphemeralTerminatingConsumerShutdownHandle {
    pub stop_tx: tokio::sync::broadcast::Sender<()>,
}

/// Start WorkerEphemeralTerminatingConsumer for reactive worker cleanup
///
/// This consumer listens to WorkerEphemeralTerminating events from NATS
/// and triggers cleanup sagas for worker resource cleanup.
pub async fn start_worker_ephemeral_terminating_consumer(
    nats_event_bus: &NatsEventBus,
    saga_orchestrator: Arc<PostgresSagaOrchestrator<PostgresSagaRepository>>,
    pool: sqlx::PgPool,
) -> Result<WorkerEphemeralTerminatingConsumerShutdownHandle, anyhow::Error> {
    info!("🧹 Starting WorkerEphemeralTerminatingConsumer for reactive cleanup...");

    let job_repository = Arc::new(PostgresJobRepository::new(pool.clone()));
    let worker_registry = Arc::new(PostgresWorkerRegistry::new(pool.clone()));
    let outbox_repository: Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync> =
        Arc::new(
            hodei_server_infrastructure::persistence::outbox::PostgresOutboxRepository::new(
                pool.clone(),
            ),
        );

    let (stop_tx, stop_rx) = tokio::sync::broadcast::channel(1);

    let consumer = WorkerEphemeralTerminatingConsumer::new(
        nats_event_bus.client().clone(),
        nats_event_bus.jetstream().clone(),
        saga_orchestrator,
        job_repository,
        worker_registry,
        outbox_repository,
        None,
    );

    tokio::spawn(async move {
        // Note: start() uses internal shutdown channel, not external receiver
        if let Err(e) = consumer.start().await {
            error!("🧹 WorkerEphemeralTerminatingConsumer error: {}", e);
        }
    });

    info!("✅ WorkerEphemeralTerminatingConsumer started");
    Ok(WorkerEphemeralTerminatingConsumerShutdownHandle { stop_tx })
}

/// Shutdown handle for CommandRelay
pub struct CommandRelayShutdownHandle {
    pub stop_tx: tokio::sync::broadcast::Sender<()>,
}

/// Start Command Relay for publishing commands to NATS
pub async fn start_command_relay(
    pool: sqlx::PgPool,
    nats_client: Arc<async_nats::Client>,
) -> Result<CommandRelayShutdownHandle, anyhow::Error> {
    info!("🚀 Starting Command Relay for command publishing...");

    // Create the relay using the factory function
    let (relay, shutdown_signal_sender) = create_command_relay(&pool, nats_client, None)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create command relay: {}", e))?;

    // We can spawn the relay runner
    tokio::spawn(async move {
        relay.run().await;
        info!("🛑 Command Relay stopped running");
    });

    info!("✅ Command Relay started");

    // We return the sender so the caller can trigger shutdown
    Ok(CommandRelayShutdownHandle {
        stop_tx: shutdown_signal_sender,
    })
}

/// Start execution command consumers (NATS Consumers for handlers)
pub async fn start_execution_command_consumers_service(
    nats_client: Arc<async_nats::Client>,
    pool: sqlx::PgPool,
) -> anyhow::Result<()> {
    info!("🚀 Starting Execution Command Consumers...");

    let job_repository = Arc::new(PostgresJobRepository::new(pool.clone()));
    let worker_registry = Arc::new(PostgresWorkerRegistry::new(pool.clone()));
    let outbox_repository: Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync> =
        Arc::new(PostgresOutboxRepository::new(pool.clone()));

    // Create WorkerCommandSender and JobExecutor for ExecuteJobHandler
    let grpc_command_sender = GrpcWorkerCommandSender::new(WorkerAgentServiceImpl::new());
    let worker_command_sender: Arc<
        dyn hodei_server_application::workers::commands::WorkerCommandSender,
    > = Arc::new(grpc_command_sender);

    let job_executor: Arc<
        dyn hodei_server_domain::saga::commands::execution::JobExecutor + Send + Sync,
    > = Arc::new(JobExecutorImpl::new(
        job_repository.clone(),
        worker_command_sender,
        outbox_repository.clone(),
    ));

    hodei_server_infrastructure::messaging::execution_command_consumers::start_execution_command_consumers(
        nats_client.as_ref().clone(),
        pool.clone(),
        job_repository,
        worker_registry,
        Some(job_executor),
    )
    .await?;

    info!("✅ Execution Command Consumers started");
    Ok(())
}

/// Start saga command consumers (NATS Consumers for saga commands - EPIC-89)
///
/// This function starts NATS consumers for all saga command handlers (Sprint 3):
/// - Cancellation Saga: UpdateJobStateCommand, ReleaseWorkerCommand, NotifyWorkerCommand
/// - Timeout Saga: MarkJobTimedOutCommand, TerminateWorkerCommand
/// - Recovery Saga: CheckConnectivityCommand, TransferJobCommand, MarkJobForRecoveryCommand,
///                  ProvisionNewWorkerCommand, DestroyOldWorkerCommand
/// - Provisioning Saga: CreateWorkerCommand, DestroyWorkerCommand, UnregisterWorkerCommand
pub async fn start_saga_command_consumers_service<P>(
    nats_client: Arc<async_nats::Client>,
    pool: sqlx::PgPool,
    provisioning: Arc<P>,
) -> anyhow::Result<()>
where
    P: hodei_server_application::workers::provisioning::WorkerProvisioningService
        + hodei_server_domain::workers::WorkerProvisioning
        + Send
        + Sync
        + 'static,
{
    info!("🚀 Starting Saga Command Consumers (EPIC-89 Sprint 4)...");

    // Initialize DLQ configuration for Sprint 4
    let dlq_config = Some(command_dlq::CommandDlqConfig {
        stream_prefix: "HODEI".to_string(),
        dlq_subject: "hodei.dlq.commands.>".to_string(),
        max_retries: 3,
        retention_days: 7,
        enabled: true,
        retry_delay_ms: 1000,
    });

    // Cast to WorkerProvisioning trait object for the infrastructure layer
    // DefaultWorkerProvisioningService implements both traits
    let provisioning_domain: Arc<
        dyn hodei_server_domain::workers::WorkerProvisioning + Send + Sync,
    > = provisioning as _;
    saga_command_consumers::start_saga_command_consumers(
        nats_client.as_ref().clone(),
        pool,
        provisioning_domain,
        dlq_config,
    )
    .await?;

    info!("✅ Saga Command Consumers started (Sprint 4)");
    Ok(())
}

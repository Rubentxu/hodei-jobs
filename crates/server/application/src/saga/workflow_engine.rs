//!
//! # Workflow Engine (saga-engine v4.0)
//!
//! This module provides initialization and management of the saga-engine v4.0
//! workflow orchestration engine, including activity consumers that process
//! tasks from NATS task queues.
//!
//! The workflow engine uses a durable execution model where:
//! - Workflows are defined as Rust code (Workflow-as-Code pattern)
//! - Activities are dispatched to NATS for async execution
//! - State is persisted in PostgreSQL EventStore
//! - Timers are managed for timeout handling
//!
//! Without these activity consumers, the saga engine will dispatch activities
//! to NATS but no one will execute them, causing workflows to hang indefinitely.
//!

use hodei_server_domain::command::DynCommandBus;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::outbox::OutboxRepository;
use hodei_server_domain::providers::ProviderConfigRepository;
use hodei_server_domain::workers::WorkerRegistry;
use saga_engine_core::activity_registry::ActivityRegistry;
use saga_engine_core::saga_engine::{SagaEngine, SagaEngineConfig};
use saga_engine_core::worker::{Worker, WorkerConfig};
use saga_engine_core::workflow::registry::WorkflowRegistry;
use saga_engine_nats::NatsTaskQueue;
use saga_engine_pg::{PostgresEventStore, PostgresTimerStore};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::saga::bridge::CommandBusJobExecutionPort;
use crate::saga::workflows::JobExecutionPort;
use crate::saga::workflows::execution_durable::{
    CollectResultActivity, DispatchJobActivity, ExecutionWorkflow, ValidateJobActivity,
};
use crate::saga::workflows::provisioning_durable::{
    ProvisionWorkerActivity, ProvisioningWorkflow, ValidateProviderActivity,
    ValidateWorkerSpecActivity,
};
use crate::workers::provisioning::WorkerProvisioningService;

// =============================================================================
// Workflow Engine Configuration
// =============================================================================

/// Configuration for the workflow engine activity consumers
#[derive(Debug, Clone)]
pub struct WorkflowEngineConfig {
    /// Enable provisioning workflow consumer
    pub provisioning_worker_enabled: bool,
    /// Enable execution workflow consumer
    pub execution_worker_enabled: bool,
    /// Number of concurrent tasks per consumer
    pub max_concurrent: u64,
    /// Poll interval for task queues
    pub poll_interval_ms: u64,
    /// Graceful shutdown timeout
    pub shutdown_timeout_secs: u64,
}

impl Default for WorkflowEngineConfig {
    fn default() -> Self {
        Self {
            provisioning_worker_enabled: true,
            execution_worker_enabled: true,
            max_concurrent: 10,
            poll_interval_ms: 100,
            shutdown_timeout_secs: 30,
        }
    }
}

/// Result of workflow engine initialization
#[derive(Debug)]
pub struct WorkflowEngineInitResult {
    pub provisioning_consumer:
        Option<Worker<PostgresEventStore, NatsTaskQueue, PostgresTimerStore>>,
    pub execution_consumer: Option<Worker<PostgresEventStore, NatsTaskQueue, PostgresTimerStore>>,
    pub activity_registry: Arc<ActivityRegistry>,
    pub workflow_registry: Arc<WorkflowRegistry>,
}

/// Initialize the workflow engine with proper registries
///
/// This function creates and configures:
/// 1. ActivityRegistry - Registers all activity implementations
/// 2. WorkflowRegistry - Registers all workflow definitions
/// 3. Activity Consumers - One per task queue (provisioning, execution)
pub async fn initialize_workflow_engine(
    saga_engine: Arc<SagaEngine<PostgresEventStore, NatsTaskQueue, PostgresTimerStore>>,
    config: WorkflowEngineConfig,
    command_bus: Option<DynCommandBus>,
    provisioning_service: Option<Arc<dyn WorkerProvisioningService>>,
) -> WorkflowEngineInitResult {
    info!("Initializing workflow engine...");

    // Create registries
    let activity_registry = Arc::new(ActivityRegistry::new());
    let workflow_registry = Arc::new(WorkflowRegistry::new());

    // Create execution port if command_bus is provided
    // DEBT-006: CommandBusJobExecutionPort now requires CommandBus (not Optional)
    let execution_port = command_bus.map(|bus| CommandBusJobExecutionPort::new(bus));

    // Register provisioning activities
    if config.provisioning_worker_enabled {
        if let Some(service) = provisioning_service {
            register_provisioning_activities(&activity_registry, service);
            register_provisioning_workflows(&workflow_registry);
            info!("✓ Provisioning activities and workflows registered");
        } else {
            warn!("Provisioning worker enabled but no provisioning service provided");
        }
    }

    // Register execution activities with CommandBus port
    if config.execution_worker_enabled {
        if let Some(ref port) = execution_port {
            register_execution_activities_real(&activity_registry, Arc::new(port.clone()));
            register_execution_workflows(&workflow_registry, Arc::new(port.clone()));
        } else {
            // Fallback: register with mock unit port
            register_execution_activities_mock(&activity_registry);
            // Create a mock port for workflow registration
            use hodei_server_domain::command::erased::InMemoryErasedCommandBus;
            let mock_bus: DynCommandBus = Arc::new(InMemoryErasedCommandBus::new());
            let mock_port = CommandBusJobExecutionPort::new(mock_bus);
            register_execution_workflows(&workflow_registry, Arc::new(mock_port));
        }
        info!("✓ Execution activities and workflows registered");
    }

    // Create activity consumers
    let provisioning_consumer = if config.provisioning_worker_enabled {
        let config = WorkerConfig::new()
            .with_consumer_name("saga-provisioning-worker".to_string())
            .with_queue_group("saga-workers".to_string())
            .with_max_concurrent(config.max_concurrent)
            .with_poll_interval(Duration::from_millis(config.poll_interval_ms));

        let worker = Worker::new(
            config,
            saga_engine.clone(),
            activity_registry.clone(),
            workflow_registry.clone(),
        );

        Some(worker)
    } else {
        None
    };

    let execution_consumer = if config.execution_worker_enabled {
        let config = WorkerConfig::new()
            .with_consumer_name("saga-execution-worker".to_string())
            .with_queue_group("saga-workers".to_string())
            .with_max_concurrent(config.max_concurrent)
            .with_poll_interval(Duration::from_millis(config.poll_interval_ms));

        let worker = Worker::new(
            config,
            saga_engine.clone(),
            activity_registry.clone(),
            workflow_registry.clone(),
        );

        Some(worker)
    } else {
        None
    };

    info!(
        "✓ Workflow engine initialized (provisioning: {}, execution: {})",
        config.provisioning_worker_enabled, config.execution_worker_enabled
    );

    WorkflowEngineInitResult {
        provisioning_consumer,
        execution_consumer,
        activity_registry,
        workflow_registry,
    }
}

/// Start all workflow engine activity consumers in background tasks
///
/// Returns handles for shutdown management.
pub async fn start_workflow_engine(
    result: WorkflowEngineInitResult,
) -> WorkflowEngineShutdownHandles {
    let mut handles = WorkflowEngineShutdownHandles::default();

    if let Some(consumer) = result.provisioning_consumer {
        let handle = tokio::spawn(async move {
            if let Err(e) = consumer.start().await {
                error!("Provisioning consumer failed: {:?}", e);
            }
        });
        *handles.provisioning_consumer.lock().await = Some(handle);
        info!("✓ Provisioning consumer started");
    }

    if let Some(consumer) = result.execution_consumer {
        let handle = tokio::spawn(async move {
            if let Err(e) = consumer.start().await {
                error!("Execution consumer failed: {:?}", e);
            }
        });
        *handles.execution_consumer.lock().await = Some(handle);
        info!("✓ Execution consumer started");
    }

    handles
}

/// Shutdown handles for workflow engine activity consumers
#[derive(Debug, Clone)]
pub struct WorkflowEngineShutdownHandles {
    pub provisioning_consumer: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    pub execution_consumer: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl Default for WorkflowEngineShutdownHandles {
    fn default() -> Self {
        Self {
            provisioning_consumer: Arc::new(Mutex::new(None)),
            execution_consumer: Arc::new(Mutex::new(None)),
        }
    }
}

impl WorkflowEngineShutdownHandles {
    /// Wait for all consumers to shutdown
    pub async fn shutdown_all(&self) {
        if let Some(handle) = self.provisioning_consumer.lock().await.take() {
            let _ = handle.await;
            debug!("Provisioning consumer shut down");
        }

        if let Some(handle) = self.execution_consumer.lock().await.take() {
            let _ = handle.await;
            debug!("Execution consumer shut down");
        }
    }
}

// =============================================================================
// Activity Registration
// =============================================================================

/// Register all provisioning workflow activities
fn register_provisioning_activities(
    registry: &Arc<ActivityRegistry>,
    provisioning_service: Arc<dyn WorkerProvisioningService>,
) {
    registry.register_activity(ValidateProviderActivity::new(provisioning_service.clone()));
    registry.register_activity(ValidateWorkerSpecActivity::new(
        provisioning_service.clone(),
    ));
    registry.register_activity(ProvisionWorkerActivity::new(provisioning_service));
}

/// Register all execution workflow activities
fn register_execution_activities_real(
    registry: &Arc<ActivityRegistry>,
    port: Arc<CommandBusJobExecutionPort>,
) {
    registry.register_activity(ValidateJobActivity::new(port.clone()));
    registry.register_activity(DispatchJobActivity::new(port.clone()));
    registry.register_activity(CollectResultActivity::new(port));
}

/// Register all execution workflow activities (mock for testing)
fn register_execution_activities_mock(registry: &Arc<ActivityRegistry>) {
    use hodei_server_domain::command::erased::InMemoryErasedCommandBus;

    let mock_command_bus: DynCommandBus = Arc::new(InMemoryErasedCommandBus::new());
    let mock_port = CommandBusJobExecutionPort::new(mock_command_bus);

    registry.register_activity(ValidateJobActivity::new(Arc::new(mock_port.clone())));
    registry.register_activity(DispatchJobActivity::new(Arc::new(mock_port.clone())));
    registry.register_activity(CollectResultActivity::new(Arc::new(mock_port)));
}

// =============================================================================
// Workflow Registration
// =============================================================================

/// Register all provisioning workflow definitions
fn register_provisioning_workflows(registry: &Arc<WorkflowRegistry>) {
    registry.register_workflow(ProvisioningWorkflow::default());
    debug!("Registered workflow: provisioning");
}

/// Register all execution workflow definitions
fn register_execution_workflows(
    registry: &Arc<WorkflowRegistry>,
    port: Arc<CommandBusJobExecutionPort>,
) {
    let workflow = ExecutionWorkflow::new(port);
    registry.register_workflow_with_name("execution", workflow);
    debug!("Registered workflow: execution");
}

// =============================================================================
// Health Check
// =============================================================================

/// Check if workflow engine activities are registered
pub async fn check_workflow_engine_health(
    activity_registry: &Arc<ActivityRegistry>,
    expected_activities: &[&str],
) -> bool {
    let mut all_registered = true;

    for activity_type in expected_activities {
        if !activity_registry.has_activity(activity_type) {
            error!("Missing activity: {}", activity_type);
            all_registered = false;
        }
    }

    if all_registered {
        debug!(
            "All {} expected activities registered",
            expected_activities.len()
        );
    }

    all_registered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_workflow_engine_config_defaults() {
        let config = WorkflowEngineConfig::default();
        assert!(config.provisioning_worker_enabled);
        assert!(config.execution_worker_enabled);
        assert_eq!(config.max_concurrent, 10);
    }

    #[tokio::test]
    async fn test_activity_registration() {
        let registry = Arc::new(ActivityRegistry::new());
        let provisioning_service = Arc::new(
            crate::workers::provisioning::MockProvisioningService::new(vec![]),
        );

        register_provisioning_activities(&registry, provisioning_service);
        register_execution_activities_mock(&registry);

        assert!(registry.has_activity("provisioning-validate-provider"));
        assert!(registry.has_activity("provisioning-validate-spec"));
        assert!(registry.has_activity("provisioning-provision-worker"));
        assert!(registry.has_activity("execution-validate-job"));
        assert!(registry.has_activity("execution-dispatch-job"));
        assert!(registry.has_activity("execution-collect-result"));
        assert!(registry.len() >= 5);
    }

    #[tokio::test]
    async fn test_workflow_engine_shutdown_handles() {
        let handles = WorkflowEngineShutdownHandles::default();
        handles.shutdown_all().await;
    }
}

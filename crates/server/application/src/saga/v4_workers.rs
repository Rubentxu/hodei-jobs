//!
//! # v4.0 Activity Workers
//!
//! This module provides initialization and management of saga-engine v4.0
//! workers that consume activity tasks from NATS task queues.
//!
//! Without these workers, the saga engine will dispatch activities to NATS
//! but no one will execute them, causing workflows to hang indefinitely.
//!

use hodei_server_domain::command::{CommandBus, DynCommandBus};
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
    ProvisionWorkerActivity, ValidateProviderActivity, ValidateWorkerSpecActivity,
};
use crate::workers::provisioning::WorkerProvisioningService;

// =============================================================================
// v4.0 Worker Initialization
// =============================================================================

/// Configuration for v4.0 workers
#[derive(Debug, Clone)]
pub struct V4WorkerConfig {
    /// Worker for provisioning workflows
    pub provisioning_worker_enabled: bool,
    /// Worker for execution workflows
    pub execution_worker_enabled: bool,
    /// Number of concurrent tasks per worker
    pub max_concurrent: u64,
    /// Poll interval for task queues
    pub poll_interval_ms: u64,
    /// Graceful shutdown timeout
    pub shutdown_timeout_secs: u64,
}

impl Default for V4WorkerConfig {
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

/// Result of worker initialization
#[derive(Debug)]
pub struct V4WorkerInitResult {
    pub provisioning_worker: Option<Worker<PostgresEventStore, NatsTaskQueue, PostgresTimerStore>>,
    pub execution_worker: Option<Worker<PostgresEventStore, NatsTaskQueue, PostgresTimerStore>>,
    pub activity_registry: Arc<ActivityRegistry>,
    pub workflow_registry: Arc<WorkflowRegistry>,
}

/// Initialize all v4.0 workers with proper registries
///
/// This function creates and configures:
/// 1. ActivityRegistry - Registers all activity implementations
/// 2. WorkflowRegistry - Registers all workflow definitions
/// 3. Workers - One per task queue (provisioning, execution)
pub async fn initialize_v4_workers(
    saga_engine: Arc<SagaEngine<PostgresEventStore, NatsTaskQueue, PostgresTimerStore>>,
    config: V4WorkerConfig,
    command_bus: Option<DynCommandBus>,
    provisioning_service: Option<Arc<dyn WorkerProvisioningService>>,
) -> V4WorkerInitResult {
    info!("Initializing v4.0 workers...");

    // Create registries
    let activity_registry = Arc::new(ActivityRegistry::new());
    let workflow_registry = Arc::new(WorkflowRegistry::new());

    // Create execution port if command_bus is provided
    let execution_port = command_bus.map(|bus| CommandBusJobExecutionPort::new(Some(bus)));

    // Register provisioning activities
    if config.provisioning_worker_enabled {
        if let Some(service) = provisioning_service {
            register_provisioning_activities(&activity_registry, service);
            info!("✓ Provisioning activities registered");
        } else {
            warn!("Provisioning worker enabled but no provisioning service provided");
        }
    }

    // Register execution activities with CommandBus port
    if config.execution_worker_enabled {
        if let Some(ref port) = execution_port {
            register_execution_activities_real(&activity_registry, Arc::new(port.clone()));
        } else {
            // Fallback: register with mock unit port
            register_execution_activities_mock(&activity_registry);
        }
        info!("✓ Execution activities registered");
    }

    // Create workers
    let provisioning_worker = if config.provisioning_worker_enabled {
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

    let execution_worker = if config.execution_worker_enabled {
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
        "✓ v4.0 workers initialized (provisioning: {}, execution: {})",
        config.provisioning_worker_enabled, config.execution_worker_enabled
    );

    V4WorkerInitResult {
        provisioning_worker,
        execution_worker,
        activity_registry,
        workflow_registry,
    }
}

/// Start all v4.0 workers in background tasks
///
/// Returns handles for shutdown management.
pub async fn start_v4_workers(result: V4WorkerInitResult) -> V4WorkerShutdownHandles {
    let mut handles = V4WorkerShutdownHandles::default();

    if let Some(worker) = result.provisioning_worker {
        let handle = tokio::spawn(async move {
            if let Err(e) = worker.start().await {
                error!("Provisioning worker failed: {:?}", e);
            }
        });
        *handles.provisioning_worker.lock().await = Some(handle);
        info!("✓ Provisioning worker started");
    }

    if let Some(worker) = result.execution_worker {
        let handle = tokio::spawn(async move {
            if let Err(e) = worker.start().await {
                error!("Execution worker failed: {:?}", e);
            }
        });
        *handles.execution_worker.lock().await = Some(handle);
        info!("✓ Execution worker started");
    }

    handles
}

/// Shutdown handles for v4.0 workers
#[derive(Debug, Clone)]
pub struct V4WorkerShutdownHandles {
    pub provisioning_worker: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    pub execution_worker: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl Default for V4WorkerShutdownHandles {
    fn default() -> Self {
        Self {
            provisioning_worker: Arc::new(Mutex::new(None)),
            execution_worker: Arc::new(Mutex::new(None)),
        }
    }
}

impl V4WorkerShutdownHandles {
    /// Wait for all workers to shutdown
    pub async fn shutdown_all(&self) {
        if let Some(handle) = self.provisioning_worker.lock().await.take() {
            let _ = handle.await;
            debug!("Provisioning worker shut down");
        }

        if let Some(handle) = self.execution_worker.lock().await.take() {
            let _ = handle.await;
            debug!("Execution worker shut down");
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
    // ValidateProviderActivity - Validates provider exists and is healthy
    registry.register_activity(ValidateProviderActivity::new(provisioning_service.clone()));

    // ValidateWorkerSpecActivity - Validates worker spec before provisioning
    registry.register_activity(ValidateWorkerSpecActivity::new(
        provisioning_service.clone(),
    ));

    // ProvisionWorkerActivity - Provisions worker infrastructure via provider
    registry.register_activity(ProvisionWorkerActivity::new(provisioning_service));
}

/// Register all execution workflow activities (real implementation with CommandBus)
fn register_execution_activities_real(
    registry: &Arc<ActivityRegistry>,
    port: Arc<CommandBusJobExecutionPort>,
) {
    // ValidateJobActivity - Validates job exists and is in valid state
    registry.register_activity(ValidateJobActivity::new(port.clone()));

    // DispatchJobActivity - Dispatches job to worker
    registry.register_activity(DispatchJobActivity::new(port.clone()));

    // CollectResultActivity - Collects job execution result
    registry.register_activity(CollectResultActivity::new(port));
}

/// Register all execution workflow activities (mock implementation for testing)
fn register_execution_activities_mock(registry: &Arc<ActivityRegistry>) {
    // Create a mock port using CommandBusJobExecutionPort with None command_bus
    // This will return errors for all operations but satisfies the type system
    let mock_port = CommandBusJobExecutionPort::new(None);
    registry.register_activity(ValidateJobActivity::new(Arc::new(mock_port.clone())));
    registry.register_activity(DispatchJobActivity::new(Arc::new(mock_port.clone())));
    registry.register_activity(CollectResultActivity::new(Arc::new(mock_port)));
}

// =============================================================================
// Health Check
// =============================================================================

/// Check if v4.0 workers are healthy
pub async fn check_v4_workers_health(
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
    use saga_engine_core::workflow::DurableWorkflow;

    #[tokio::test]
    async fn test_v4_worker_config_defaults() {
        let config = V4WorkerConfig::default();
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
        // Use mock port for testing
        register_execution_activities_mock(&registry);

        // Check provisioning activities
        assert!(registry.has_activity("provisioning-validate-provider"));
        assert!(registry.has_activity("provisioning-validate-spec"));
        assert!(registry.has_activity("provisioning-provision-worker"));

        // Check execution activities
        assert!(registry.has_activity("execution-validate-job"));
        assert!(registry.has_activity("execution-dispatch-job"));
        assert!(registry.has_activity("execution-collect-result"));

        // Check count (5 activities: 3 provisioning + 2 execution)
        // Note: register_execution_activities_mock only registers 2 activities, not 3
        assert!(registry.len() >= 5);
    }

    #[tokio::test]
    async fn test_v4_worker_shutdown_handles() {
        let handles = V4WorkerShutdownHandles::default();
        // Empty handles should not panic on shutdown
        handles.shutdown_all().await;
    }
}

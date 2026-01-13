//! Execution Command Handlers Bootstrap Module
//!
//! Registers saga command handlers for job execution operations.
//! This module is used during application startup to register handlers
//! with the CommandBus for saga-based job execution.

use hodei_server_domain::command::InMemoryErasedCommandBus;
use hodei_server_domain::saga::commands::execution::{
    AssignWorkerCommand, AssignWorkerHandler,
    ValidateJobCommand, ValidateJobHandler,
};
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::workers::WorkerRegistry;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

/// Configuration for execution command handlers
#[derive(Clone)]
pub struct ExecutionCommandBusConfig {
    /// Job repository for ValidateJob/AssignWorker handlers
    pub job_repository: Arc<dyn JobRepository + Send + Sync>,
    /// Worker registry for AssignWorker handler
    pub worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl ExecutionCommandBusConfig {
    /// Create a new configuration
    pub fn new(
        job_repository: Arc<dyn JobRepository + Send + Sync>,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    ) -> Self {
        Self {
            job_repository,
            worker_registry,
        }
    }
}

/// Wrapper for Arc<dyn JobRepository> that implements Debug
#[derive(Clone)]
pub struct JobRepositoryWrapper(pub Arc<dyn JobRepository + Send + Sync>);

impl Debug for JobRepositoryWrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobRepositoryWrapper").finish()
    }
}

#[async_trait::async_trait]
impl JobRepository for JobRepositoryWrapper {
    async fn save(&self, job: &hodei_server_domain::jobs::Job) -> hodei_server_domain::shared_kernel::Result<()> {
        self.0.save(job).await
    }

    async fn find_by_id(&self, id: &hodei_server_domain::shared_kernel::JobId) -> hodei_server_domain::shared_kernel::Result<Option<hodei_server_domain::jobs::Job>> {
        self.0.find_by_id(id).await
    }

    async fn update(&self, job: &hodei_server_domain::jobs::Job) -> hodei_server_domain::shared_kernel::Result<()> {
        self.0.update(job).await
    }

    async fn delete(&self, id: &hodei_server_domain::shared_kernel::JobId) -> hodei_server_domain::shared_kernel::Result<()> {
        self.0.delete(id).await
    }

    async fn find_by_state(&self, state: hodei_server_domain::shared_kernel::JobState) -> hodei_server_domain::shared_kernel::Result<Vec<hodei_server_domain::jobs::Job>> {
        self.0.find_by_state(state).await
    }
}

/// Wrapper for Arc<dyn WorkerRegistry> that implements Debug
#[derive(Clone)]
pub struct WorkerRegistryWrapper(pub Arc<dyn WorkerRegistry + Send + Sync>);

impl Debug for WorkerRegistryWrapper {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerRegistryWrapper").finish()
    }
}

#[async_trait::async_trait]
impl WorkerRegistry for WorkerRegistryWrapper {
    async fn register(&self, worker: &hodei_server_domain::workers::Worker) -> hodei_server_domain::shared_kernel::Result<()> {
        self.0.register(worker).await
    }

    async fn get(&self, id: &hodei_server_domain::shared_kernel::WorkerId) -> hodei_server_domain::shared_kernel::Result<Option<hodei_server_domain::workers::Worker>> {
        self.0.get(id).await
    }

    async fn update_state(&self, id: &hodei_server_domain::shared_kernel::WorkerId, state: hodei_server_domain::shared_kernel::WorkerState) -> hodei_server_domain::shared_kernel::Result<()> {
        self.0.update_state(id, state).await
    }

    async fn find(&self, filter: &hodei_server_domain::workers::WorkerFilter) -> hodei_server_domain::shared_kernel::Result<Vec<hodei_server_domain::workers::Worker>> {
        self.0.find(filter).await
    }

    async fn remove(&self, id: &hodei_server_domain::shared_kernel::WorkerId) -> hodei_server_domain::shared_kernel::Result<()> {
        self.0.remove(id).await
    }

    async fn update_heartbeat(&self, id: &hodei_server_domain::shared_kernel::WorkerId) -> hodei_server_domain::shared_kernel::Result<()> {
        self.0.update_heartbeat(id).await
    }
}

/// Register all execution command handlers with the provided CommandBus.
///
/// This function registers:
/// - ValidateJobHandler: Handles ValidateJobCommand for job validation
/// - AssignWorkerHandler: Handles AssignWorkerCommand for worker assignment
///
/// # Arguments
/// * `command_bus` - The InMemoryErasedCommandBus to register handlers with
/// * `config` - Configuration containing the job repository and worker registry
pub async fn register_execution_command_handlers(
    command_bus: &InMemoryErasedCommandBus,
    config: ExecutionCommandBusConfig,
) {
    tracing::info!("Registering execution command handlers...");

    // Create handler for ValidateJobCommand
    let validate_job_handler = ValidateJobHandler::new(JobRepositoryWrapper(config.job_repository.clone()));

    // Register ValidateJobHandler
    command_bus
        .register::<ValidateJobCommand, ValidateJobHandler<JobRepositoryWrapper>>(
            validate_job_handler,
        )
        .await;

    tracing::info!("  ✓ ValidateJobHandler registered");

    // Create handler for AssignWorkerCommand
    let assign_worker_handler = AssignWorkerHandler::new(
        JobRepositoryWrapper(config.job_repository),
        WorkerRegistryWrapper(config.worker_registry),
    );

    // Register AssignWorkerHandler
    command_bus
        .register::<AssignWorkerCommand, AssignWorkerHandler<JobRepositoryWrapper, WorkerRegistryWrapper>>(
            assign_worker_handler,
        )
        .await;

    tracing::info!("  ✓ AssignWorkerHandler registered");

    tracing::info!("Execution command handlers registered successfully");
}

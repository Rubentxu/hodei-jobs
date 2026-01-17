//! Execution Command Handlers Bootstrap Module
//!
//! Registers saga command handlers for job execution operations.
//! This module is used during application startup to register handlers
//! with the CommandBus for saga-based job execution.

use hodei_server_domain::command::InMemoryErasedCommandBus;
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::outbox::OutboxRepository;
use hodei_server_domain::saga::commands::execution::{
    AssignWorkerCommand, AssignWorkerHandler, CompleteJobCommand, CompleteJobHandler,
    ExecuteJobCommand, ExecuteJobHandler, ValidateJobCommand, ValidateJobHandler,
};
use hodei_server_domain::workers::WorkerRegistry;
use std::sync::Arc;

mod job_executor;
pub use job_executor::JobExecutorImpl;

/// Register all execution command handlers with the provided CommandBus.
///
/// This function registers:
/// - ValidateJobHandler: Handles ValidateJobCommand for job validation
/// - AssignWorkerHandler: Handles AssignWorkerCommand for worker assignment
/// - ExecuteJobHandler: Handles ExecuteJobCommand for job execution initiation
/// - CompleteJobHandler: Handles CompleteJobCommand for job completion
///
/// # Arguments
/// * `command_bus` - The InMemoryErasedCommandBus to register handlers with
/// * `job_repository` - Job repository trait object
/// * `worker_registry` - Worker registry trait object
pub async fn register_execution_command_handlers(
    command_bus: &InMemoryErasedCommandBus,
    job_repository: Arc<dyn JobRepository + Send + Sync>,
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
) {
    register_execution_command_handlers_with_executor(
        command_bus,
        job_repository,
        worker_registry,
        None,
        None,
    )
    .await;
}

/// Register execution command handlers with optional JobExecutor
///
/// # Arguments
/// * `command_bus` - The InMemoryErasedCommandBus to register handlers with
/// * `job_repository` - Job repository trait object
/// * `worker_registry` - Worker registry trait object
/// * `job_executor` - Optional JobExecutor for RUN_JOB dispatch
/// * `outbox_repository` - Optional OutboxRepository for transactional events
pub async fn register_execution_command_handlers_with_executor(
    command_bus: &InMemoryErasedCommandBus,
    job_repository: Arc<dyn JobRepository + Send + Sync>,
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    job_executor: Option<
        Arc<dyn hodei_server_domain::saga::commands::execution::JobExecutor + Send + Sync>,
    >,
    _outbox_repository: Option<Arc<dyn OutboxRepository + Send + Sync>>,
) {
    tracing::info!("Registering execution command handlers...");

    // Create handler for ValidateJobCommand
    let validate_job_handler = ValidateJobHandler::new(job_repository.clone());

    // Register ValidateJobHandler
    command_bus
        .register::<ValidateJobCommand, _>(validate_job_handler)
        .await;

    tracing::info!("  ✓ ValidateJobHandler registered");

    // Create handler for AssignWorkerCommand
    let assign_worker_handler =
        AssignWorkerHandler::new(job_repository.clone(), worker_registry.clone());

    // Register AssignWorkerHandler
    command_bus
        .register::<AssignWorkerCommand, _>(assign_worker_handler)
        .await;

    tracing::info!("  ✓ AssignWorkerHandler registered");

    // Create handler for ExecuteJobCommand with optional JobExecutor
    let execute_job_handler = ExecuteJobHandler::new(
        job_repository.clone(),
        worker_registry.clone(),
        job_executor,
    );

    // Register ExecuteJobHandler
    command_bus
        .register::<ExecuteJobCommand, _>(execute_job_handler)
        .await;

    tracing::info!("  ✓ ExecuteJobHandler registered");

    // Create handler for CompleteJobCommand
    let complete_job_handler = CompleteJobHandler::new(job_repository, worker_registry);

    // Register CompleteJobHandler
    command_bus
        .register::<CompleteJobCommand, _>(complete_job_handler)
        .await;

    tracing::info!("  ✓ CompleteJobHandler registered");

    tracing::info!("Execution command handlers registered successfully");
}

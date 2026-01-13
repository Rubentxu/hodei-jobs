//! Execution Command Handlers Bootstrap Module
//!
//! Registers saga command handlers for job execution operations.
//! This module is used during application startup to register handlers
//! with the CommandBus for saga-based job execution.

use hodei_server_domain::command::InMemoryErasedCommandBus;
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::saga::commands::execution::{
    AssignWorkerCommand, AssignWorkerHandler, ValidateJobCommand, ValidateJobHandler,
};
use hodei_server_domain::workers::WorkerRegistry;
use std::sync::Arc;

/// Register all execution command handlers with the provided CommandBus.
///
/// This function registers:
/// - ValidateJobHandler: Handles ValidateJobCommand for job validation
/// - AssignWorkerHandler: Handles AssignWorkerCommand for worker assignment
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
    tracing::info!("Registering execution command handlers...");

    // Create handler for ValidateJobCommand
    let validate_job_handler = ValidateJobHandler::new(job_repository.clone());

    // Register ValidateJobHandler
    command_bus
        .register::<ValidateJobCommand, _>(validate_job_handler)
        .await;

    tracing::info!("  ✓ ValidateJobHandler registered");

    // Create handler for AssignWorkerCommand
    let assign_worker_handler = AssignWorkerHandler::new(job_repository, worker_registry);

    // Register AssignWorkerHandler
    command_bus
        .register::<AssignWorkerCommand, _>(assign_worker_handler)
        .await;

    tracing::info!("  ✓ AssignWorkerHandler registered");

    tracing::info!("Execution command handlers registered successfully");
}

//! Execution Command Consumers
//!
//! This module implements NATS consumers for execution-related commands.
//! It subscribes to NATS JetStream subjects for commands and delegates processing
//! to the appropriate domain handlers.
//!
//! Commands handled:
//! - AssignWorkerCommand: `hodei.commands.worker.assignworker`
//! - ExecuteJobCommand: `hodei.commands.worker.executejob`
//! - CompleteJobCommand: `hodei.commands.job.completejob`
//! - ValidateJobCommand: `hodei.commands.job.validatejob`

use async_nats::jetstream::{self, stream};
use futures::StreamExt;
use hodei_server_domain::command::{Command, CommandHandler, CommandTargetType};
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::saga::commands::execution::{
    AssignWorkerCommand, AssignWorkerHandler, CompleteJobCommand, CompleteJobHandler,
    ExecuteJobCommand, ExecuteJobHandler, JobExecutor, ValidateJobCommand, ValidateJobHandler,
};
use hodei_server_domain::workers::WorkerRegistry;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{error, info, instrument, warn};

use crate::messaging::consumer_utils::spawn_idempotent_consumer;

/// Start consumers for execution commands
pub async fn start_execution_command_consumers(
    nats: async_nats::Client,
    pool: PgPool,
    job_repository: Arc<dyn JobRepository + Send + Sync>,
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    job_executor: Option<Arc<dyn JobExecutor + Send + Sync>>,
) -> anyhow::Result<()> {
    let jetstream = async_nats::jetstream::new(nats);

    // 1. AssignWorkerCommand Consumer -> Idempotent
    let assign_worker_handler = Arc::new(AssignWorkerHandler::new(
        job_repository.clone(),
        worker_registry.clone(),
    ));
    spawn_idempotent_consumer::<AssignWorkerCommand, _>(
        pool.clone(),
        jetstream.clone(),
        "AssignWorker",
        "hodei.commands.worker.assignworker",
        assign_worker_handler,
    )
    .await?;

    // 2. ExecuteJobCommand Consumer -> Idempotent
    let execute_job_handler = Arc::new(ExecuteJobHandler::new(
        job_repository.clone(),
        worker_registry.clone(),
        job_executor,
    ));
    spawn_idempotent_consumer::<ExecuteJobCommand, _>(
        pool.clone(),
        jetstream.clone(),
        "ExecuteJob",
        "hodei.commands.worker.executejob",
        execute_job_handler,
    )
    .await?;

    // 3. CompleteJobCommand Consumer -> Idempotent
    let complete_job_handler = Arc::new(CompleteJobHandler::new(
        job_repository.clone(),
        worker_registry.clone(),
    ));
    spawn_idempotent_consumer::<CompleteJobCommand, _>(
        pool.clone(),
        jetstream.clone(),
        "CompleteJob",
        "hodei.commands.job.completejob",
        complete_job_handler,
    )
    .await?;

    // 4. ValidateJobCommand Consumer -> Idempotent
    let validate_job_handler = Arc::new(ValidateJobHandler::new(job_repository.clone()));
    spawn_idempotent_consumer::<ValidateJobCommand, _>(
        pool.clone(),
        jetstream.clone(),
        "ValidateJob",
        "hodei.commands.job.validatejob",
        validate_job_handler,
    )
    .await?;

    info!("🚀 Execution command consumers started");
    Ok(())
}

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
use std::sync::Arc;
use tracing::{error, info, instrument, warn};

/// Start consumers for execution commands
pub async fn start_execution_command_consumers(
    nats: async_nats::Client,
    job_repository: Arc<dyn JobRepository + Send + Sync>,
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    job_executor: Option<Arc<dyn JobExecutor + Send + Sync>>,
) -> anyhow::Result<()> {
    let jetstream = async_nats::jetstream::new(nats);

    // Ensure stream exists (created by CommandRelay usually, but good to be safe)
    // We assume stream "HODEI_COMMANDS" exists with subject "hodei.commands.>"

    // 1. AssignWorkerCommand Consumer
    let assign_worker_handler = Arc::new(AssignWorkerHandler::new(
        job_repository.clone(),
        worker_registry.clone(),
    ));
    spawn_consumer(
        jetstream.clone(),
        "AssignWorker",
        "hodei.commands.worker.assignworker",
        assign_worker_handler,
    )
    .await?;

    // 2. ExecuteJobCommand Consumer
    let execute_job_handler = Arc::new(ExecuteJobHandler::new(
        job_repository.clone(),
        worker_registry.clone(),
        job_executor,
    ));
    spawn_consumer(
        jetstream.clone(),
        "ExecuteJob",
        "hodei.commands.worker.executejob",
        execute_job_handler,
    )
    .await?;

    // 3. CompleteJobCommand Consumer
    let complete_job_handler = Arc::new(CompleteJobHandler::new(
        job_repository.clone(),
        worker_registry.clone(),
    ));
    spawn_consumer(
        jetstream.clone(),
        "CompleteJob",
        "hodei.commands.job.completejob",
        complete_job_handler,
    )
    .await?;

    // 4. ValidateJobCommand Consumer
    let validate_job_handler = Arc::new(ValidateJobHandler::new(job_repository.clone()));
    spawn_consumer(
        jetstream.clone(),
        "ValidateJob",
        "hodei.commands.job.validatejob",
        validate_job_handler,
    )
    .await?;

    info!("🚀 Execution command consumers started");
    Ok(())
}

/// Helper method to spawn a consumer for a specific command type
async fn spawn_consumer<C, H>(
    jetstream: jetstream::Context,
    consumer_name_base: &'static str,
    subject: &'static str,
    handler: Arc<H>,
) -> anyhow::Result<()>
where
    C: Command + serde::de::DeserializeOwned + Send + Sync + 'static,
    H: CommandHandler<C> + Send + Sync + 'static,
{
    let consumer_name = format!("cmd-consumer-{}", consumer_name_base.to_lowercase());

    // Create/Update Consumer configuration
    let consumer = jetstream
        .get_or_create_consumer(
            "HODEI_COMMANDS",
            jetstream::consumer::pull::Config {
                durable_name: Some(consumer_name.clone()),
                filter_subject: subject.to_string(),
                // Ensure we process commands in order per subject if needed,
                // but concurrency helps.
                // For commands updates to same aggregate (Job) might conflict if parallel,
                // but ID locking in DB usually handles it.
                ..Default::default()
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create consumer {}: {}", consumer_name, e))?;

    let mut messages = consumer
        .messages()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get messages: {}", e))?;

    tokio::spawn(async move {
        info!("Started NATS consumer: {} -> {}", consumer_name, subject);

        // Process messages
        while let Some(msg_result) = messages.next().await {
            match msg_result {
                Ok(msg) => {
                    let payload = msg.payload.clone();

                    // Decode command
                    // Note: The payload in Outbox is wrapped in `CommandOutboxInsert`.
                    // But CommandRelay publishes the `payload` field directly?
                    // Let's check CommandRelay.
                    // CommandRelay::process_command deserializes payload to `MarkJobFailedPayload`?
                    // No, for `MarkJobFailed` it has specific logic.
                    // For generic commands in ExecuteJob, we assume the payload json IS the command struct.
                    // IF CommandOutboxRelay inserts `serde_json::to_value(cmd)` as payload.
                    // We need to ensure serialization format matches.

                    match serde_json::from_slice::<C>(&payload) {
                        Ok(command) => {
                            info!(
                                consumer = consumer_name,
                                command_type = %command.command_name(),
                                "📥 Received command"
                            );

                            match handler.handle(command).await {
                                Ok(_) => {
                                    if let Err(e) = msg.ack().await {
                                        warn!(
                                            consumer = consumer_name,
                                            "Failed to ack message: {}", e
                                        );
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        consumer = consumer_name,
                                        error = ?e,
                                        "❌ Failed to handle command"
                                    );
                                    // Nak with delay? Or term?
                                    // For now, let it timeout and retry (default NATS behavior)
                                    // or explicit nak.
                                    if let Err(e) = msg.ack().await {
                                        // Acking failed message essentially drops it if we don't handle DLQ here.
                                        // TODO: Implement DLQ or Retry logic
                                        warn!(
                                            consumer = consumer_name,
                                            "Failed to ack (failed) message: {}", e
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!(
                                consumer = consumer_name,
                                error = %e,
                                "❌ Failed to deserialize command payload"
                            );
                            // Cannot process this message, term it.
                            if let Err(e) = msg.ack().await {
                                // Ack to remove poison message
                                warn!(
                                    consumer = consumer_name,
                                    "Failed to term poison message: {}", e
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(consumer = consumer_name, "Error receiving message: {}", e);
                }
            }
        }
    });

    Ok(())
}

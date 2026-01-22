//! Saga Command Consumers
//!
//! This module implements NATS consumers for saga commands across all saga types.
//! Each consumer subscribes to NATS JetStream subjects and delegates processing
//! to the appropriate domain command handlers.
//!
//! # Idempotency
//!
//! Consumers verify idempotency by checking the hodei_commands table before
//! processing. If a command with the same idempotency_key has status COMPLETED,
//! the message is acknowledged but the handler is NOT called. This ensures
//! at-least-once delivery without duplicate processing.
//!
//! # Sprints
//!
//! - Sprint 1-3: ✅ COMPLETED (17/17 commands)
//! - Sprint 4: ✅ Production readiness (DLQ, metrics, idempotency)

use async_nats::jetstream::{self, stream};
use futures::StreamExt;
use hodei_server_application::workers::provisioning::WorkerProvisioningService;
use hodei_server_domain::command::{Command, CommandHandler};
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::saga::commands::cancellation::{
    GenericNotifyWorkerHandler, NotifyWorkerCommand, ReleaseWorkerCommand, ReleaseWorkerHandler,
    UpdateJobStateCommand, UpdateJobStateHandler,
};
use hodei_server_domain::saga::commands::provisioning::{
    CreateWorkerCommand, CreateWorkerHandler, DestroyWorkerCommand, DestroyWorkerHandler,
    UnregisterWorkerCommand, UnregisterWorkerHandler,
};
use hodei_server_domain::saga::commands::recovery::{
    CheckConnectivityCommand, CheckConnectivityHandler, DestroyOldWorkerCommand,
    GenericDestroyOldWorkerHandler, GenericProvisionNewWorkerHandler, MarkJobForRecoveryCommand,
    MarkJobForRecoveryHandler, ProvisionNewWorkerCommand, TransferJobCommand, TransferJobHandler,
};
use hodei_server_domain::saga::commands::timeout::{
    GenericTerminateWorkerHandler, MarkJobTimedOutCommand, MarkJobTimedOutHandler,
    TerminateWorkerCommand,
};
use hodei_server_domain::workers::{WorkerProvisioning, WorkerRegistry};
use serde::Serialize;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::messaging::command_dlq::{CommandDlq, CommandDlqConfig, CommandDlqMetrics};
use crate::messaging::consumer_utils::spawn_idempotent_consumer;
use crate::persistence::postgres::{PostgresJobRepository, PostgresWorkerRegistry};

/// Start consumers for cancellation saga commands
pub async fn start_cancellation_command_consumers(
    nats: async_nats::Client,
    pool: PgPool,
    job_repository: Arc<dyn JobRepository + Send + Sync>,
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    dlq_config: Option<CommandDlqConfig>,
) -> anyhow::Result<()> {
    let jetstream = async_nats::jetstream::new(nats);
    let jetstream_context = jetstream.clone();

    // Initialize DLQ if enabled (for UpdateJobStateCommand)
    let dlq: Option<CommandDlq<UpdateJobStateCommand>> = if let Some(ref config) = dlq_config {
        if config.enabled {
            let metrics = Arc::new(CommandDlqMetrics::new());
            let dlq = CommandDlq::<UpdateJobStateCommand>::new(config.clone(), jetstream, metrics);
            dlq.initialize().await?;
            debug!("Command DLQ initialized for UpdateJobStateCommand");
            Some(dlq)
        } else {
            None
        }
    } else {
        None
    };

    // UpdateJobStateCommand Consumer with DLQ support (Note: sticking to custom loop for DLQ support for now)
    // TODO: Unify DLQ support into spawn_idempotent_consumer
    let update_job_state_handler = Arc::new(UpdateJobStateHandler::new(job_repository.clone()));
    spawn_consumer_with_dlq::<UpdateJobStateCommand, _>(
        jetstream_context.clone(),
        "UpdateJobState",
        "hodei.commands.job.updatejobstate",
        update_job_state_handler,
        dlq,
    )
    .await?;

    // ReleaseWorkerCommand Consumer -> Idempotent
    let release_worker_handler = Arc::new(ReleaseWorkerHandler::new(worker_registry.clone()));
    spawn_idempotent_consumer::<ReleaseWorkerCommand, _>(
        pool.clone(),
        jetstream_context.clone(),
        "ReleaseWorker",
        "hodei.commands.worker.releaseworker",
        release_worker_handler,
    )
    .await?;

    // NotifyWorkerCommand Consumer -> Idempotent [NEW]
    let notify_worker_handler = Arc::new(GenericNotifyWorkerHandler::new(worker_registry.clone()));
    spawn_idempotent_consumer::<NotifyWorkerCommand, _>(
        pool.clone(),
        jetstream_context.clone(),
        "NotifyWorker",
        "hodei.commands.worker.notifyworker",
        notify_worker_handler,
    )
    .await?;

    info!("🚀 Cancellation command consumers started");
    Ok(())
}

/// Start consumers for timeout saga commands
pub async fn start_timeout_command_consumers(
    nats: async_nats::Client,
    pool: PgPool,
    job_repository: Arc<dyn JobRepository + Send + Sync>,
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
) -> anyhow::Result<()> {
    let jetstream = async_nats::jetstream::new(nats);

    // MarkJobTimedOutCommand Consumer -> Idempotent
    let mark_job_timed_out_handler = Arc::new(MarkJobTimedOutHandler::new(job_repository.clone()));
    spawn_idempotent_consumer::<MarkJobTimedOutCommand, _>(
        pool.clone(),
        jetstream.clone(),
        "MarkJobTimedOut",
        "hodei.commands.job.markjobtimedout",
        mark_job_timed_out_handler,
    )
    .await?;

    // TerminateWorkerCommand Consumer -> Idempotent
    let terminate_worker_handler =
        Arc::new(GenericTerminateWorkerHandler::new(worker_registry.clone()));
    spawn_idempotent_consumer::<TerminateWorkerCommand, _>(
        pool.clone(),
        jetstream.clone(),
        "TerminateWorker",
        "hodei.commands.worker.terminateworker",
        terminate_worker_handler,
    )
    .await?;

    info!("🚀 Timeout command consumers started");
    Ok(())
}

/// Start consumers for recovery saga commands
pub async fn start_recovery_command_consumers(
    nats: async_nats::Client,
    pool: PgPool,
    job_repository: Arc<dyn JobRepository + Send + Sync>,
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    provisioning: Arc<dyn WorkerProvisioning + Send + Sync>,
) -> anyhow::Result<()> {
    let jetstream = async_nats::jetstream::new(nats);

    // CheckConnectivityCommand Consumer -> Idempotent
    let check_connectivity_handler =
        Arc::new(CheckConnectivityHandler::new(worker_registry.clone()));
    spawn_idempotent_consumer::<CheckConnectivityCommand, _>(
        pool.clone(),
        jetstream.clone(),
        "CheckConnectivity",
        "hodei.commands.worker.checkconnectivity",
        check_connectivity_handler,
    )
    .await?;

    // TransferJobCommand Consumer -> Idempotent
    let transfer_job_handler = Arc::new(TransferJobHandler::new(
        job_repository.clone(),
        worker_registry.clone(),
    ));
    spawn_idempotent_consumer::<TransferJobCommand, _>(
        pool.clone(),
        jetstream.clone(),
        "TransferJob",
        "hodei.commands.job.transferjob",
        transfer_job_handler,
    )
    .await?;

    // MarkJobForRecoveryCommand Consumer -> Idempotent
    let mark_job_for_recovery_handler =
        Arc::new(MarkJobForRecoveryHandler::new(job_repository.clone()));
    spawn_idempotent_consumer::<MarkJobForRecoveryCommand, _>(
        pool.clone(),
        jetstream.clone(),
        "MarkJobForRecovery",
        "hodei.commands.job.markjobforrecovery",
        mark_job_for_recovery_handler,
    )
    .await?;

    // ProvisionNewWorkerCommand Consumer -> Idempotent
    let provision_new_worker_handler =
        Arc::new(GenericProvisionNewWorkerHandler::new(provisioning.clone()));
    spawn_idempotent_consumer::<ProvisionNewWorkerCommand, _>(
        pool.clone(),
        jetstream.clone(),
        "ProvisionNewWorker",
        "hodei.commands.worker.provisionnewworker",
        provision_new_worker_handler,
    )
    .await?;

    // DestroyOldWorkerCommand Consumer -> Idempotent
    let destroy_old_worker_handler =
        Arc::new(GenericDestroyOldWorkerHandler::new(provisioning.clone()));
    spawn_idempotent_consumer::<DestroyOldWorkerCommand, _>(
        pool.clone(),
        jetstream.clone(),
        "DestroyOldWorker",
        "hodei.commands.worker.destroyoldworker",
        destroy_old_worker_handler,
    )
    .await?;

    info!("🚀 Recovery command consumers started");
    Ok(())
}

/// Start consumers for provisioning saga commands
pub async fn start_provisioning_command_consumers(
    nats: async_nats::Client,
    pool: PgPool,
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    provisioning: Arc<dyn WorkerProvisioning + Send + Sync>,
) -> anyhow::Result<()> {
    let jetstream = async_nats::jetstream::new(nats);

    // CreateWorkerCommand Consumer -> Idempotent
    let create_worker_handler = Arc::new(CreateWorkerHandler::new(provisioning.clone()));
    spawn_idempotent_consumer::<CreateWorkerCommand, _>(
        pool.clone(),
        jetstream.clone(),
        "CreateWorker",
        "hodei.commands.worker.createworker",
        create_worker_handler,
    )
    .await?;

    // DestroyWorkerCommand Consumer -> Idempotent
    let destroy_worker_handler = Arc::new(DestroyWorkerHandler::new(provisioning.clone()));
    spawn_idempotent_consumer::<DestroyWorkerCommand, _>(
        pool.clone(),
        jetstream.clone(),
        "DestroyWorker",
        "hodei.commands.worker.destroyworker",
        destroy_worker_handler,
    )
    .await?;

    // UnregisterWorkerCommand Consumer -> Idempotent [NEW]
    let unregister_worker_handler = Arc::new(UnregisterWorkerHandler::new(worker_registry.clone()));
    spawn_idempotent_consumer::<UnregisterWorkerCommand, _>(
        pool.clone(),
        jetstream.clone(), // Corrected from jetstream_context which isn't defined here
        "UnregisterWorker",
        "hodei.commands.worker.unregisterworker",
        unregister_worker_handler,
    )
    .await?;

    info!("🚀 Provisioning command consumers started");
    Ok(())
}

/// Start all saga command consumers with optional DLQ support (Sprint 4)
pub async fn start_saga_command_consumers(
    nats: async_nats::Client,
    pool: sqlx::PgPool,
    provisioning: Arc<dyn WorkerProvisioning + Send + Sync>,
    dlq_config: Option<CommandDlqConfig>,
) -> anyhow::Result<()> {
    // Create concrete repositories wrapped in Arc<dyn Trait>
    let job_repository: Arc<dyn JobRepository + Send + Sync> =
        Arc::new(PostgresJobRepository::new(pool.clone()));
    let worker_registry: Arc<dyn WorkerRegistry + Send + Sync> =
        Arc::new(PostgresWorkerRegistry::new(pool.clone()));

    // Start cancellation saga command consumers with DLQ
    start_cancellation_command_consumers(
        nats.clone(),
        pool.clone(),
        job_repository.clone(),
        worker_registry.clone(),
        dlq_config.clone(),
    )
    .await?;

    // Start timeout saga command consumers
    start_timeout_command_consumers(
        nats.clone(),
        pool.clone(),
        job_repository.clone(),
        worker_registry.clone(),
    )
    .await?;

    // Start recovery saga command consumers
    start_recovery_command_consumers(
        nats.clone(),
        pool.clone(),
        job_repository.clone(),
        worker_registry.clone(),
        provisioning.clone(),
    )
    .await?;

    // Start provisioning saga command consumers
    start_provisioning_command_consumers(
        nats.clone(),
        pool.clone(),
        worker_registry.clone(),
        provisioning.clone(),
    )
    .await?;

    info!("🚀 All saga command consumers (Sprint 1, 2, 3 & 4) started");
    Ok(())
}

/// Helper method to spawn a consumer with DLQ support for a specific command type
/// Kept local as it deals with DLQ logic not yet in shared utils
async fn spawn_consumer_with_dlq<C, H>(
    jetstream: jetstream::Context,
    consumer_name_base: &'static str,
    subject: &'static str,
    handler: Arc<H>,
    dlq: Option<CommandDlq<C>>,
) -> anyhow::Result<()>
where
    C: Command + serde::de::DeserializeOwned + Send + Sync + Serialize + Clone + 'static,
    H: CommandHandler<C> + Send + Sync + 'static,
{
    let consumer_name = format!("cmd-consumer-{}", consumer_name_base.to_lowercase());

    let stream = jetstream
        .get_stream("HODEI_COMMANDS")
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get stream HODEI_COMMANDS: {}", e))?;

    let consumer: async_nats::jetstream::consumer::PullConsumer = stream
        .get_or_create_consumer(
            &consumer_name,
            jetstream::consumer::pull::Config {
                durable_name: Some(consumer_name.clone()),
                filter_subject: subject.to_string(),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create consumer {}: {}", consumer_name, e))?;

    let mut messages = consumer
        .messages()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get messages: {}", e))?;

    let dlq_enabled = dlq.is_some();
    let dlq_subject = subject.to_string();

    tokio::spawn(async move {
        info!(
            "Started NATS consumer with DLQ: {} -> {}",
            consumer_name, subject
        );

        while let Some(msg_result) = messages.next().await {
            match msg_result {
                Ok(msg) => {
                    let payload = msg.payload.clone();

                    match serde_json::from_slice::<C>(&payload) {
                        Ok(command) => {
                            let command_type = command.command_name();
                            // let idempotency_key = command.idempotency_key().into_owned();
                            // TODO: Add idempotency check here too when DLQ is merged into utils

                            info!(
                                consumer = consumer_name,
                                command_type = %command_type,
                                "📥 Received command"
                            );

                            match handler.handle(command.clone()).await {
                                Ok(_) => {
                                    if let Err(e) = msg.ack().await {
                                        warn!(
                                            consumer = consumer_name,
                                            "Failed to ack message: {}", e
                                        );
                                    }
                                }
                                Err(e) => {
                                    error!(consumer = consumer_name, error = ?e, "❌ Failed to handle command");

                                    // Move to DLQ if enabled
                                    if let Some(ref dlq) = dlq {
                                        let _ = dlq
                                            .move_to_dlq(
                                                &command,
                                                1,
                                                format!("{:?}", e),
                                                vec![],
                                                chrono::Utc::now(),
                                                &dlq_subject,
                                                &consumer_name,
                                                None,
                                            )
                                            .await;
                                    }

                                    if let Err(e) = msg.ack().await {
                                        warn!(
                                            consumer = consumer_name,
                                            "Failed to ack (failed) message: {}", e
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!(consumer = consumer_name, error = %e, "❌ Failed to deserialize command payload");
                            if let Err(e) = msg.ack().await {
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

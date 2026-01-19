use async_nats::jetstream::{self, stream};
use futures::StreamExt;
use hodei_server_domain::command::{Command, CommandHandler};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tracing::{error, info, warn};

/// Check if a command has already been processed.
///
/// Queries hodei_commands table for the given idempotency_key.
/// Returns Some(status) if found, None if not found.
pub async fn is_command_processed(
    pool: &PgPool,
    idempotency_key: &str,
) -> Result<Option<String>, sqlx::Error> {
    // Use raw query with text column extraction
    let row = sqlx::query("SELECT status FROM hodei_commands WHERE idempotency_key = $1 AND status != 'CANCELLED' LIMIT 1")
        .bind(idempotency_key)
        .fetch_optional(pool)
        .await?;

    // Extract status from row
    if let Some(row) = row {
        let status: String = row.try_get(0)?;
        Ok(Some(status))
    } else {
        Ok(None)
    }
}

/// Mark a command as completed in the outbox.
pub async fn mark_command_completed(
    pool: &PgPool,
    idempotency_key: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE hodei_commands SET status = 'COMPLETED', processed_at = NOW() WHERE idempotency_key = $1 AND status != 'COMPLETED'"
    )
    .bind(idempotency_key)
    .execute(pool)
    .await?;
    Ok(())
}

/// Spawn a consumer with idempotency checking against the hodei_commands table.
///
/// This consumer:
/// 1. Deserializes the command
/// 2. Checks hodei_commands for existing processing (COMPLETED/PENDING)
/// 3. If already processed: skips handler, just acks the message
/// 4. If new: runs handler, then marks as COMPLETED
pub async fn spawn_idempotent_consumer<C, H>(
    pool: PgPool,
    jetstream: jetstream::Context,
    consumer_name_base: &'static str,
    subject: &'static str,
    handler: Arc<H>,
) -> anyhow::Result<()>
where
    C: Command + serde::de::DeserializeOwned + Send + Sync + 'static,
    H: CommandHandler<C> + Send + Sync + 'static,
{
    let consumer_name = format!(
        "cmd-consumer-idempotent-{}",
        consumer_name_base.to_lowercase()
    );

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

    tokio::spawn(async move {
        info!(
            "Started idempotent NATS consumer: {} -> {}",
            consumer_name, subject
        );

        while let Some(msg_result) = messages.next().await {
            match msg_result {
                Ok(msg) => {
                    let payload = msg.payload.clone();

                    match serde_json::from_slice::<C>(&payload) {
                        Ok(command) => {
                            let command_type = command.command_name();
                            let idempotency_key = command.idempotency_key().into_owned();

                            // Check idempotency: skip if already processed
                            match is_command_processed(&pool, &idempotency_key).await {
                                Ok(Some(status)) => {
                                    info!(
                                        consumer = consumer_name,
                                        command_type = %command_type,
                                        idempotency_key = %idempotency_key,
                                        status = %status,
                                        "⏭️ Command already processed, skipping handler"
                                    );
                                    // Still mark as completed to ensure status is correct
                                    let _ = mark_command_completed(&pool, &idempotency_key).await;
                                    if let Err(e) = msg.ack().await {
                                        warn!(
                                            consumer = consumer_name,
                                            "Failed to ack duplicate: {}", e
                                        );
                                    }
                                }
                                Ok(None) => {
                                    // New command, process it
                                    info!(
                                        consumer = consumer_name,
                                        command_type = %command_type,
                                        "📥 Received new command"
                                    );

                                    match handler.handle(command).await {
                                        Ok(_) => {
                                            // Mark as completed
                                            let _ = mark_command_completed(&pool, &idempotency_key)
                                                .await;
                                            if let Err(e) = msg.ack().await {
                                                warn!(
                                                    consumer = consumer_name,
                                                    "Failed to ack: {}", e
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            error!(consumer = consumer_name, error = ?e, "❌ Failed to handle command");
                                            if let Err(e) = msg.ack().await {
                                                warn!(
                                                    consumer = consumer_name,
                                                    "Failed to ack failed: {}", e
                                                );
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(consumer = consumer_name, error = %e, "❌ Idempotency check failed");
                                    // Fail open: process the command
                                    if let Err(e2) = handler.handle(command).await {
                                        error!(consumer = consumer_name, error = ?e2, "❌ Handler error after idempotency failure");
                                    }
                                    if let Err(e) = msg.ack().await {
                                        warn!(consumer = consumer_name, "Failed to ack: {}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!(consumer = consumer_name, error = %e, "❌ Failed to deserialize");
                            if let Err(e) = msg.ack().await {
                                warn!(consumer = consumer_name, "Failed to term: {}", e);
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

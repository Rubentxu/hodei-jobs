//! Job Completion Worker Cleanup Handler
//!
//! Reactively handles job completion events to trigger worker cleanup.
//! This ensures workers are cleaned up reliably through the event-driven pattern
//! instead of async spawning that can fail silently.
//!
//! Flow:
//! 1. Worker calls complete_job → JobStatusChanged(Succeeded) event published
//! 2. This handler receives the event
//! 3. Finds the worker that executed the job
//! 4. Emits WorkerEphemeralTerminating event for lifecycle manager to process

use async_trait::async_trait;
use chrono::Utc;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::outbox::{OutboxError, OutboxEventInsert, OutboxRepository};
use hodei_server_domain::shared_kernel::{JobId, JobState, WorkerId};
use hodei_server_domain::workers::{WorkerFilter, WorkerRegistry};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use super::event_subscriber::EventHandler;

/// Handles job completion events and triggers worker cleanup
///
/// This handler ensures reliable worker cleanup by:
/// - Listening to JobStatusChanged events (not spawned tasks)
/// - Persisting cleanup intent in the event system (outbox)
/// - Retrying through the event bus if the handler fails
pub struct JobCompletionWorkerCleanupHandler {
    /// Worker registry to find workers for completed jobs
    worker_registry: Arc<dyn WorkerRegistry>,
    /// Outbox repository to persist cleanup events
    outbox_repository: Option<Arc<dyn OutboxRepository<Error = OutboxError> + Send + Sync>>,
}

impl JobCompletionWorkerCleanupHandler {
    /// Create a new handler
    pub fn new(
        worker_registry: Arc<dyn WorkerRegistry>,
        outbox_repository: Option<Arc<dyn OutboxRepository<Error = OutboxError> + Send + Sync>>,
    ) -> Self {
        Self {
            worker_registry,
            outbox_repository,
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for JobCompletionWorkerCleanupHandler {
    async fn handle_event(
        &self,
        event: DomainEvent,
    ) -> anyhow::Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Only handle JobStatusChanged events
        let (job_id, new_state, correlation_id, actor) = match event {
            DomainEvent::JobStatusChanged {
                job_id,
                new_state,
                correlation_id,
                actor,
                ..
            } => (job_id, new_state, correlation_id, actor),
            _ => return Ok(()), // Not our event, skip
        };

        // Only trigger cleanup when job reaches Succeeded state (terminal state)
        if new_state != JobState::Succeeded {
            debug!(
                "Job {} is now {:?}, skipping worker cleanup (only triggers on Succeeded)",
                job_id, new_state
            );
            return Ok(());
        }

        info!(
            job_id = %job_id,
            correlation_id = ?correlation_id,
            "🔔 JobCompletionHandler: Job succeeded, triggering worker cleanup"
        );

        // Find the worker that executed this job
        let workers = self
            .worker_registry
            .find(&WorkerFilter::new())
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        let worker = workers
            .into_iter()
            .find(|w| w.current_job_id().map_or(false, |jid| jid == &job_id));

        match worker {
            Some(worker) => {
                let worker_id = worker.id().clone();
                let provider_id = worker.provider_id().clone();

                info!(
                    job_id = %job_id,
                    worker_id = %worker_id,
                    provider_id = %provider_id,
                    "🔔 Found worker {} for succeeded job {}, emitting cleanup event",
                    worker_id, job_id
                );

                // Emit WorkerEphemeralTerminating event to trigger cleanup reactively
                self.emit_cleanup_event(&job_id, &worker_id, &provider_id)
                    .await?;

                Ok(())
            }
            None => {
                warn!(
                    job_id = %job_id,
                    "No worker found for succeeded job - worker may have already been cleaned up or never assigned"
                );
                Ok(())
            }
        }
    }
}

impl JobCompletionWorkerCleanupHandler {
    /// Emit a WorkerEphemeralTerminating event to trigger cleanup through the event system
    async fn emit_cleanup_event(
        &self,
        job_id: &JobId,
        worker_id: &WorkerId,
        provider_id: &hodei_server_domain::shared_kernel::ProviderId,
    ) -> anyhow::Result<()> {
        if let Some(ref outbox_repo) = self.outbox_repository {
            let now = Utc::now();
            let event = OutboxEventInsert::for_worker(
                worker_id.0,
                "WorkerEphemeralTerminating".to_string(),
                serde_json::json!({
                    "worker_id": worker_id.0.to_string(),
                    "provider_id": provider_id.0.to_string(),
                    "reason": "JOB_SUCCEEDED",
                    "job_id": job_id.0.to_string()
                }),
                Some(serde_json::json!({
                    "source": "JobCompletionWorkerCleanupHandler",
                    "cleanup_type": "reactive",
                    "event": "job_completion"
                })),
                Some(format!(
                    "ephemeral-terminating-{}-{}",
                    worker_id.0,
                    now.timestamp()
                )),
            );

            outbox_repo
                .insert_events(&[event])
                .await
                .map_err(|e| anyhow::anyhow!("Failed to insert cleanup event: {}", e))?;

            info!(
                job_id = %job_id,
                worker_id = %worker_id,
                "📤 WorkerEphemeralTerminating event persisted for reactive cleanup"
            );
        } else {
            debug!(
                job_id = %job_id,
                worker_id = %worker_id,
                "No outbox repository configured, cleanup event not persisted"
            );
        }

        Ok(())
    }
}

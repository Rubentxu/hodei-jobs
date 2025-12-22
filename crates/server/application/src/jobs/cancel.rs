// Job Execution Use Cases
// UC-XXX: Cancel Job

use chrono::Utc;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::{DomainEvent, EventMetadata};
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::request_context::RequestContext;
use hodei_server_domain::shared_kernel::{DomainError, JobId, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelJobResponse {
    pub job_id: String,
    pub status: String,
    pub message: String,
}

pub struct CancelJobUseCase {
    job_repository: Arc<dyn JobRepository>,
    event_bus: Arc<dyn EventBus>,
}

impl CancelJobUseCase {
    pub fn new(job_repository: Arc<dyn JobRepository>, event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            job_repository,
            event_bus,
        }
    }

    pub async fn execute(&self, job_id: JobId) -> Result<CancelJobResponse> {
        self.execute_with_context(job_id, None).await
    }

    pub async fn execute_with_context(
        &self,
        job_id: JobId,
        ctx: Option<&RequestContext>,
    ) -> Result<CancelJobResponse> {
        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await?
            .ok_or_else(|| DomainError::JobNotFound {
                job_id: job_id.clone(),
            })?;

        let old_state = job.state().clone();
        job.cancel()?;
        self.job_repository.update(&job).await?;

        // Refactoring: Use EventMetadata to centralize audit info extraction
        // This handles the fallback from context to job metadata automatically
        let correlation_from_ctx = ctx.map(|c| c.correlation_id().to_string());
        let actor_from_ctx = ctx.and_then(|c| c.actor_owned());

        let metadata = if correlation_from_ctx.is_some() || actor_from_ctx.is_some() {
            // If context provides audit info, use it
            EventMetadata::new(correlation_from_ctx, actor_from_ctx)
        } else {
            // Otherwise, fall back to job metadata
            EventMetadata::from_job_metadata(job.metadata(), &job.id)
        };

        // Publicar evento JobStatusChanged (Cancelled)
        let event = DomainEvent::JobStatusChanged {
            job_id: job.id.clone(),
            old_state,
            new_state: hodei_server_domain::shared_kernel::JobState::Cancelled,
            occurred_at: Utc::now(),
            correlation_id: metadata.correlation_id.clone(),
            actor: metadata.actor.clone(),
        };

        if let Err(e) = self.event_bus.publish(&event).await {
            tracing::error!(
                "Failed to publish JobStatusChanged (Cancelled) event: {}",
                e
            );
        }

        // Publicar evento explícito JobCancelled
        // Refactoring: Reuse EventMetadata from JobStatusChanged event
        let cancelled_event = DomainEvent::JobCancelled {
            job_id: job.id.clone(),
            reason: Some("User requested cancellation".to_string()),
            occurred_at: Utc::now(),
            correlation_id: metadata.correlation_id.clone(),
            actor: metadata.actor.clone(),
        };

        if let Err(e) = self.event_bus.publish(&cancelled_event).await {
            tracing::error!("Failed to publish JobCancelled event: {}", e);
        }

        Ok(CancelJobResponse {
            job_id: job.id.to_string(),
            status: job.state().to_string(),
            message: "Job cancellation requested".to_string(),
        })
    }
}

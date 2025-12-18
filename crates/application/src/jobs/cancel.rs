// Job Execution Use Cases
// UC-XXX: Cancel Job

use chrono::Utc;
use hodei_jobs_domain::event_bus::EventBus;
use hodei_jobs_domain::events::DomainEvent;
use hodei_jobs_domain::jobs::JobRepository;
use hodei_jobs_domain::request_context::RequestContext;
use hodei_jobs_domain::shared_kernel::{DomainError, JobId, Result};
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

        let correlation_id = ctx.map(|c| c.correlation_id().to_string());
        let actor = ctx.and_then(|c| c.actor_owned());

        // Publicar evento JobStatusChanged (Cancelled)
        let event = DomainEvent::JobStatusChanged {
            job_id: job.id.clone(),
            old_state,
            new_state: hodei_jobs_domain::shared_kernel::JobState::Cancelled,
            occurred_at: Utc::now(),
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        };

        if let Err(e) = self.event_bus.publish(&event).await {
            tracing::error!(
                "Failed to publish JobStatusChanged (Cancelled) event: {}",
                e
            );
        }

        // Publicar evento explícito JobCancelled
        let cancelled_event = DomainEvent::JobCancelled {
            job_id: job.id.clone(),
            reason: Some("User requested cancellation".to_string()),
            occurred_at: Utc::now(),
            correlation_id,
            actor,
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

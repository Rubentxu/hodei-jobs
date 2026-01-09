//! Fail Job Use Case
use chrono::Utc;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::events::JobStatusChanged;
use hodei_server_domain::outbox::OutboxEventInsert;
use hodei_server_domain::shared_kernel::{JobId, WorkerId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailJobCommand {
    pub job_id: JobId,
    pub execution_id: String,
    pub error_message: String,
    pub exit_code: Option<i32>,
}

#[async_trait::async_trait]
pub trait WorkerCleanupPort: Send + Sync {
    async fn trigger_cleanup(&self, worker_id: &WorkerId) -> Result<(), CleanupError>;
}

#[derive(Debug, thiserror::Error)]
pub enum CleanupError {
    #[error("Worker not found: {0}")]
    WorkerNotFound(WorkerId),
    #[error("Cleanup failed: {0}")]
    Failed(String),
}

#[async_trait::async_trait]
pub trait JobFailurePorts: Send + Sync {
    async fn find_job_by_id(&self, job_id: &JobId) -> Result<Option<Job>, FailureError>;
    async fn save_job(&self, job: &Job) -> Result<(), FailureError>;
}

#[async_trait::async_trait]
pub trait EventPublishingPort: Send + Sync {
    async fn publish_event(&self, event: &DomainEvent) -> Result<(), FailureError>;
    async fn insert_outbox_events(&self, events: &[OutboxEventInsert]) -> Result<(), FailureError>;
}

pub struct FailJobUseCase {
    job_repo: Arc<dyn JobFailurePorts>,
    event_port: Arc<dyn EventPublishingPort>,
    cleanup_port: Arc<dyn WorkerCleanupPort>,
}

impl FailJobUseCase {
    pub fn new(
        job_repo: Arc<dyn JobFailurePorts>,
        event_port: Arc<dyn EventPublishingPort>,
        cleanup_port: Arc<dyn WorkerCleanupPort>,
    ) -> Self {
        Self {
            job_repo,
            event_port,
            cleanup_port,
        }
    }

    pub async fn execute(&self, command: FailJobCommand) -> Result<(), FailureError> {
        let mut job = self
            .job_repo
            .find_job_by_id(&command.job_id)
            .await?
            .ok_or_else(|| FailureError::JobNotFound(command.job_id.clone()))?;

        job.fail(command.error_message.clone())
            .map_err(|e| FailureError::FailureFailed(e.to_string()))?;

        self.job_repo.save_job(&job).await?;

        // EPIC-65 Phase 3: Using modular event type
        let status_changed_event = JobStatusChanged {
            job_id: job.id.clone(),
            old_state: hodei_server_domain::shared_kernel::JobState::Running,
            new_state: hodei_server_domain::shared_kernel::JobState::Failed,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        self.event_port.publish_event(&status_changed_event.into()).await?;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FailureError {
    #[error("Job not found: {0}")]
    JobNotFound(JobId),
    #[error("Job failure failed: {0}")]
    FailureFailed(String),
}

pub use hodei_server_domain::jobs::Job;

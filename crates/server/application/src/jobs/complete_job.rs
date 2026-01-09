//! Complete Job Use Case
use chrono::Utc;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::events::JobStatusChanged;
use hodei_server_domain::outbox::OutboxEventInsert;
use hodei_server_domain::shared_kernel::{DomainError, JobId, JobResult, WorkerId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteJobCommand {
    pub job_id: JobId,
    pub execution_id: String,
    pub exit_code: i32,
    pub output: String,
    pub error_output: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JobCompletionResult {
    pub job_id: JobId,
    pub worker_id: Option<WorkerId>,
    pub worker_terminated: bool,
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
pub trait JobCompletionPorts: Send + Sync {
    async fn find_job_by_id(&self, job_id: &JobId) -> Result<Option<Job>, CompletionError>;
    async fn save_job(&self, job: &Job) -> Result<(), CompletionError>;
}

#[async_trait::async_trait]
pub trait EventPublishingPort: Send + Sync {
    async fn publish_event(&self, event: &DomainEvent) -> Result<(), CompletionError>;
    async fn insert_outbox_events(
        &self,
        events: &[OutboxEventInsert],
    ) -> Result<(), CompletionError>;
}

pub struct CompleteJobUseCase {
    job_repo: Arc<dyn JobCompletionPorts>,
    event_port: Arc<dyn EventPublishingPort>,
    cleanup_port: Arc<dyn WorkerCleanupPort>,
}

impl CompleteJobUseCase {
    pub fn new(
        job_repo: Arc<dyn JobCompletionPorts>,
        event_port: Arc<dyn EventPublishingPort>,
        cleanup_port: Arc<dyn WorkerCleanupPort>,
    ) -> Self {
        Self {
            job_repo,
            event_port,
            cleanup_port,
        }
    }

    pub async fn execute(
        &self,
        command: CompleteJobCommand,
    ) -> Result<JobCompletionResult, CompletionError> {
        let mut job = self
            .job_repo
            .find_job_by_id(&command.job_id)
            .await?
            .ok_or_else(|| CompletionError::JobNotFound(command.job_id.clone()))?;

        let job_result = JobResult::Success {
            exit_code: command.exit_code,
            output: command.output,
            error_output: command.error_output.unwrap_or_default(),
        };
        job.complete(job_result)
            .map_err(|e| CompletionError::CompletionFailed(e.to_string()))?;

        self.job_repo.save_job(&job).await?;

        // EPIC-65 Phase 3: Using modular event type
        let status_changed_event = JobStatusChanged {
            job_id: job.id.clone(),
            old_state: hodei_server_domain::shared_kernel::JobState::Running,
            new_state: hodei_server_domain::shared_kernel::JobState::Succeeded,
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: None,
        };

        self.event_port.publish_event(&status_changed_event.into()).await?;

        Ok(JobCompletionResult {
            job_id: command.job_id,
            worker_id: None,
            worker_terminated: false,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CompletionError {
    #[error("Job not found: {0}")]
    JobNotFound(JobId),
    #[error("Job completion failed: {0}")]
    CompletionFailed(String),
}

impl From<DomainError> for CompletionError {
    fn from(e: DomainError) -> Self {
        CompletionError::CompletionFailed(e.to_string())
    }
}

pub use hodei_server_domain::jobs::Job;

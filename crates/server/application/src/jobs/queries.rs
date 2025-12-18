// Job Execution Use Cases
// Queries: Get Job Status

use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::shared_kernel::{DomainError, JobId, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackJobResponse {
    pub job_id: String,
    pub status: String,
    pub result: Option<String>,
    pub created_at: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

pub struct GetJobStatusUseCase {
    job_repository: Arc<dyn JobRepository>,
}

impl GetJobStatusUseCase {
    pub fn new(job_repository: Arc<dyn JobRepository>) -> Self {
        Self { job_repository }
    }

    pub async fn execute(&self, job_id: JobId) -> Result<TrackJobResponse> {
        let job = self
            .job_repository
            .find_by_id(&job_id)
            .await?
            .ok_or(DomainError::JobNotFound { job_id })?;

        Ok(TrackJobResponse {
            job_id: job.id.to_string(),
            status: job.state().to_string(),
            result: job.result().as_ref().map(|r| r.to_string()),
            created_at: Some(job.created_at().to_rfc3339()),
            started_at: job.started_at().map(|t| t.to_rfc3339()),
            completed_at: job.completed_at().map(|t| t.to_rfc3339()),
        })
    }
}

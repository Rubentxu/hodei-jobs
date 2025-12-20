use crate::ids::{JobId, ProviderId, WorkerId};

#[derive(thiserror::Error, Debug)]
pub enum SharedError {
    #[error("Job not found: {job_id}")]
    JobNotFound { job_id: JobId },

    #[error("Provider not found: {provider_id}")]
    ProviderNotFound { provider_id: ProviderId },

    #[error("Worker not found: {worker_id}")]
    WorkerNotFound { worker_id: WorkerId },

    #[error("Invalid state transition")]
    InvalidStateTransition,
}

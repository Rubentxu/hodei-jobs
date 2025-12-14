// Job Execution Use Cases
// UC-001: Create Job
// UC-002: Execute Next Job

use hodei_jobs_domain::job_execution::{Job, JobRepository, JobQueue, JobSpec};
use hodei_jobs_domain::shared_kernel::{Result, DomainError, JobId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// DTOs para Create Job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobRequest {
    pub spec: JobSpecRequest,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSpecRequest {
    pub command: Vec<String>,
    pub image: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobResponse {
    pub job_id: String,
    pub status: String,
    pub message: String,
}

/// DTOs para Execute Next Job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteNextJobResponse {
    pub job_id: String,
    pub provider_id: String,
    pub status: String,
    pub message: String,
}

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
            .ok_or_else(|| DomainError::JobNotFound { job_id })?;

        Ok(TrackJobResponse {
            job_id: job.id.to_string(),
            status: job.state.to_string(),
            result: job.result.as_ref().map(|r| r.to_string()),
            created_at: Some(job.created_at.to_rfc3339()),
            started_at: job.started_at.map(|t| t.to_rfc3339()),
            completed_at: job.completed_at.map(|t| t.to_rfc3339()),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelJobResponse {
    pub job_id: String,
    pub status: String,
    pub message: String,
}

pub struct CancelJobUseCase {
    job_repository: Arc<dyn JobRepository>,
}

impl CancelJobUseCase {
    pub fn new(job_repository: Arc<dyn JobRepository>) -> Self {
        Self { job_repository }
    }

    pub async fn execute(&self, job_id: JobId) -> Result<CancelJobResponse> {
        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await?
            .ok_or_else(|| DomainError::JobNotFound { job_id: job_id.clone() })?;

        job.cancel()?;
        self.job_repository.update(&job).await?;

        Ok(CancelJobResponse {
            job_id: job.id.to_string(),
            status: job.state.to_string(),
            message: "Job cancellation requested".to_string(),
        })
    }
}

/// Use Case: Create Job (UC-001)
pub struct CreateJobUseCase {
    job_repository: Arc<dyn JobRepository>,
    job_queue: Arc<dyn JobQueue>,
}

impl CreateJobUseCase {
    pub fn new(
        job_repository: Arc<dyn JobRepository>,
        job_queue: Arc<dyn JobQueue>,
    ) -> Self {
        Self {
            job_repository,
            job_queue,
        }
    }

    pub async fn execute(&self, request: CreateJobRequest) -> Result<CreateJobResponse> {
        // 1. Convertir request a JobSpec
        let job_spec = self.convert_to_job_spec(request.spec)?;
        
        // 2. Generar JobId
        let job_id = JobId::new();
        
        // 3. Crear Job
        let job = Job::new(job_id.clone(), job_spec);
        
        // 4. Validar Job
        self.validate_job(&job)?;
        
        // 5. Guardar en repositorio
        self.job_repository.save(&job).await?;
        
        // 6. Encolar Job
        let mut queued_job = job;
        queued_job.queue()?;
        self.job_queue.enqueue(queued_job).await?;
        
        Ok(CreateJobResponse {
            job_id: job_id.to_string(),
            status: "PENDING".to_string(),
            message: "Job created and queued successfully".to_string(),
        })
    }

    fn convert_to_job_spec(&self, request: JobSpecRequest) -> Result<JobSpec> {
        let mut spec = JobSpec::new(request.command);
        
        if let Some(image) = request.image {
            spec.image = Some(image);
        }
        
        if let Some(env) = request.env {
            spec.env = env;
        }
        
        if let Some(timeout) = request.timeout_ms {
            spec.timeout_ms = timeout;
        }
        
        if let Some(working_dir) = request.working_dir {
            spec.working_dir = Some(working_dir);
        }
        
        Ok(spec)
    }

    fn validate_job(&self, job: &Job) -> Result<()> {
        // Validar que el comando tenga contenido
        let cmd_vec = job.spec.command_vec();
        if cmd_vec.is_empty() || cmd_vec.iter().all(|s| s.is_empty()) {
            return Err(DomainError::InfrastructureError {
                message: "Job command cannot be empty".to_string(),
            });
        }
        
        if job.spec.timeout_ms == 0 {
            return Err(DomainError::InfrastructureError {
                message: "Job timeout must be greater than 0".to_string(),
            });
        }
        
        Ok(())
    }
}

/// Use Case: Execute Next Job (UC-002)
pub struct ExecuteNextJobUseCase {
    job_queue: Arc<dyn JobQueue>,
    job_repository: Arc<dyn JobRepository>,
    // Aquí iría el coordinador de ejecución
}

impl ExecuteNextJobUseCase {
    pub fn new(
        job_queue: Arc<dyn JobQueue>,
        job_repository: Arc<dyn JobRepository>,
    ) -> Self {
        Self {
            job_queue,
            job_repository,
        }
    }

    pub async fn execute(&self) -> Result<ExecuteNextJobResponse> {
        // 1. Desencolar siguiente job
        let job = self.job_queue.dequeue().await?
            .ok_or_else(|| DomainError::InfrastructureError {
                message: "No jobs in queue".to_string(),
            })?;
        
        // 2. Marcar como en proceso
        let mut processing_job = job;
        processing_job.state = hodei_jobs_domain::shared_kernel::JobState::Scheduled;
        self.job_repository.update(&processing_job).await?;
        
        // 3. Aquí se coordinaría la ejecución con el provider
        // Por ahora, retornamos respuesta simulada
        
        Ok(ExecuteNextJobResponse {
            job_id: processing_job.id.to_string(),
            provider_id: "provider-001".to_string(), // Dummy provider
            status: "SUBMITTED".to_string(),
            message: "Job submitted for execution".to_string(),
        })
    }
}
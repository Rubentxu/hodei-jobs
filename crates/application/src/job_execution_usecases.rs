// Job Execution Use Cases
// UC-001: Create Job
// UC-002: Execute Next Job

use hodei_jobs_domain::job_execution::{Job, JobQueue, JobRepository, JobSpec};
use hodei_jobs_domain::shared_kernel::{DomainError, JobId, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// DTOs para Create Job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobRequest {
    pub spec: JobSpecRequest,
    pub correlation_id: Option<String>,
    pub actor: Option<String>,
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

        let old_state = job.state.clone();
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
            status: job.state.to_string(),
            message: "Job cancellation requested".to_string(),
        })
    }
}

use chrono::Utc;
use hodei_jobs_domain::event_bus::EventBus;
use hodei_jobs_domain::events::DomainEvent;

/// DTO para Retry Job Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryJobResponse {
    pub job_id: String,
    pub attempt: u32,
    pub status: String,
    pub message: String,
}

/// Use Case: Retry Failed Job (UC-003)
/// 
/// Reintenta un job que ha fallado o ha expirado, siempre que no haya
/// excedido el número máximo de intentos.
pub struct RetryJobUseCase {
    job_repository: Arc<dyn JobRepository>,
    job_queue: Arc<dyn JobQueue>,
    event_bus: Arc<dyn EventBus>,
}

impl RetryJobUseCase {
    pub fn new(
        job_repository: Arc<dyn JobRepository>,
        job_queue: Arc<dyn JobQueue>,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            job_repository,
            job_queue,
            event_bus,
        }
    }

    pub async fn execute(&self, job_id: JobId) -> Result<RetryJobResponse> {
        self.execute_with_context(job_id, None).await
    }

    pub async fn execute_with_context(
        &self,
        job_id: JobId,
        ctx: Option<&RequestContext>,
    ) -> Result<RetryJobResponse> {
        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await?
            .ok_or_else(|| DomainError::JobNotFound {
                job_id: job_id.clone(),
            })?;

        let max_attempts = job.max_attempts;

        // Validar y preparar retry (incrementa attempts internamente)
        job.prepare_retry()?;

        // Guardar job actualizado
        self.job_repository.update(&job).await?;

        // Encolar job para re-ejecución
        self.job_queue.enqueue(job.clone()).await?;

        let correlation_id = ctx.map(|c| c.correlation_id().to_string());
        let actor = ctx.and_then(|c| c.actor_owned());

        // Publicar evento JobRetried
        let event = DomainEvent::JobRetried {
            job_id: job.id.clone(),
            attempt: job.attempts,
            max_attempts,
            occurred_at: Utc::now(),
            correlation_id,
            actor,
        };

        if let Err(e) = self.event_bus.publish(&event).await {
            tracing::error!("Failed to publish JobRetried event: {}", e);
        }

        Ok(RetryJobResponse {
            job_id: job.id.to_string(),
            attempt: job.attempts,
            status: job.state.to_string(),
            message: format!(
                "Job retry initiated (attempt {} of {})",
                job.attempts, job.max_attempts
            ),
        })
    }
}
use hodei_jobs_domain::request_context::RequestContext;

/// Use Case: Create Job (UC-001)
pub struct CreateJobUseCase {
    job_repository: Arc<dyn JobRepository>,
    job_queue: Arc<dyn JobQueue>,
    event_bus: Arc<dyn EventBus>,
}

impl CreateJobUseCase {
    pub fn new(
        job_repository: Arc<dyn JobRepository>,
        job_queue: Arc<dyn JobQueue>,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            job_repository,
            job_queue,
            event_bus,
        }
    }

    pub async fn execute(&self, request: CreateJobRequest) -> Result<CreateJobResponse> {
        // 1. Convertir request a JobSpec
        let job_spec = self.convert_to_job_spec(request.spec)?;

        // 2. Generar JobId
        let job_id = JobId::new();

        // 3. Crear Job
        let job = Job::new(job_id.clone(), job_spec.clone());

        // 4. Validar Job
        self.validate_job(&job)?;

        // 5. Guardar en repositorio
        self.job_repository.save(&job).await?;

        // 6. Encolar Job
        let mut queued_job = job;
        queued_job.queue()?;
        self.job_queue.enqueue(queued_job).await?;

        // 7. Publicar evento
        let event = DomainEvent::JobCreated {
            job_id: job_id.clone(),
            spec: job_spec,
            occurred_at: Utc::now(),
            correlation_id: request.correlation_id,
            actor: request.actor,
        };

        if let Err(e) = self.event_bus.publish(&event).await {
            tracing::error!("Failed to publish JobCreated event: {}", e);
            // Non-blocking error, we continue as job is queued
        }

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
    event_bus: Arc<dyn EventBus>,
}

impl ExecuteNextJobUseCase {
    pub fn new(
        job_queue: Arc<dyn JobQueue>,
        job_repository: Arc<dyn JobRepository>,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            job_queue,
            job_repository,
            event_bus,
        }
    }

    pub async fn execute(&self) -> Result<ExecuteNextJobResponse> {
        // 1. Desencolar siguiente job
        let job =
            self.job_queue
                .dequeue()
                .await?
                .ok_or_else(|| DomainError::InfrastructureError {
                    message: "No jobs in queue".to_string(),
                })?;

        let old_state = job.state.clone();

        // 2. Marcar como en proceso
        let mut processing_job = job;
        processing_job.state = hodei_jobs_domain::shared_kernel::JobState::Scheduled;
        self.job_repository.update(&processing_job).await?;

        // 3. Publicar evento
        let event = DomainEvent::JobStatusChanged {
            job_id: processing_job.id.clone(),
            old_state,
            new_state: hodei_jobs_domain::shared_kernel::JobState::Scheduled,
            occurred_at: Utc::now(),
            correlation_id: None, // TODO: Retrieve from context if available
            actor: None,
        };

        if let Err(e) = self.event_bus.publish(&event).await {
            tracing::error!("Failed to publish JobStatusChanged event: {}", e);
        }

        // 4. Aquí se coordinaría la ejecución con el provider
        // Por ahora, retornamos respuesta simulada

        Ok(ExecuteNextJobResponse {
            job_id: processing_job.id.to_string(),
            provider_id: "provider-001".to_string(), // Dummy provider
            status: "SUBMITTED".to_string(),
            message: "Job submitted for execution".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::stream::BoxStream;
    use hodei_jobs_domain::event_bus::{EventBus, EventBusError};
    use hodei_jobs_domain::shared_kernel::{JobState, Result};
    use std::sync::{Arc, Mutex};

    // Mocks
    struct MockJobRepository;
    #[async_trait]
    impl JobRepository for MockJobRepository {
        async fn save(&self, _job: &Job) -> Result<()> {
            Ok(())
        }
        async fn find_by_id(&self, _id: &JobId) -> Result<Option<Job>> {
            Ok(None)
        }
        async fn find_by_state(&self, _state: &JobState) -> Result<Vec<Job>> {
            Ok(vec![])
        }
        async fn find_pending(&self) -> Result<Vec<Job>> {
            Ok(vec![])
        }
        async fn find_all(&self, _limit: usize, _offset: usize) -> Result<(Vec<Job>, usize)> {
            Ok((vec![], 0))
        }
        async fn find_by_execution_id(&self, _execution_id: &str) -> Result<Option<Job>> {
            Ok(None)
        }
        async fn delete(&self, _id: &JobId) -> Result<()> {
            Ok(())
        }
        async fn update(&self, _job: &Job) -> Result<()> {
            Ok(())
        }
    }

    struct MockJobQueue;
    #[async_trait]
    impl JobQueue for MockJobQueue {
        async fn enqueue(&self, _job: Job) -> Result<()> {
            Ok(())
        }
        async fn dequeue(&self) -> Result<Option<Job>> {
            Ok(None)
        }
        async fn peek(&self) -> Result<Option<Job>> {
            Ok(None)
        }
        async fn len(&self) -> Result<usize> {
            Ok(0)
        }
        async fn is_empty(&self) -> Result<bool> {
            Ok(true)
        }
        async fn clear(&self) -> Result<()> {
            Ok(())
        }
    }

    struct MockEventBus {
        published: Arc<Mutex<Vec<DomainEvent>>>,
    }
    impl MockEventBus {
        fn new() -> Self {
            Self {
                published: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }
    #[async_trait]
    impl EventBus for MockEventBus {
        async fn publish(&self, event: &DomainEvent) -> std::result::Result<(), EventBusError> {
            self.published.lock().unwrap().push(event.clone());
            Ok(())
        }
        async fn subscribe(
            &self,
            _topic: &str,
        ) -> std::result::Result<
            BoxStream<'static, std::result::Result<DomainEvent, EventBusError>>,
            EventBusError,
        > {
            Err(EventBusError::SubscribeError(
                "Mock not implemented".to_string(),
            ))
        }
    }

    #[tokio::test]
    async fn test_create_job_publishes_event() {
        let repo = Arc::new(MockJobRepository);
        let queue = Arc::new(MockJobQueue);
        let bus = Arc::new(MockEventBus::new());

        let use_case = CreateJobUseCase::new(repo, queue, bus.clone());

        let spec_request = JobSpecRequest {
            command: vec!["echo".to_string(), "hello".to_string()],
            image: None,
            env: None,
            timeout_ms: Some(1000),
            working_dir: None,
        };

        let request = CreateJobRequest {
            spec: spec_request,
            correlation_id: Some("test-correlation".to_string()),
            actor: Some("test-user".to_string()),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_ok());

        let events = bus.published.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::JobCreated {
                job_id: _,
                spec,
                occurred_at: _,
                correlation_id,
                actor,
            } => {
                assert_eq!(spec.command_vec()[0], "echo");
                assert_eq!(spec.command_vec()[1], "hello");
                assert_eq!(correlation_id.as_deref(), Some("test-correlation"));
                assert_eq!(actor.as_deref(), Some("test-user"));
            }
            _ => panic!("Unexpected event type"),
        }
    }

    // Mock repository that returns a failed job for retry testing
    struct MockJobRepositoryWithFailedJob {
        job: Mutex<Option<Job>>,
    }

    impl MockJobRepositoryWithFailedJob {
        fn new_with_failed_job() -> Self {
            let job_id = JobId::new();
            let spec = JobSpec::new(vec!["test".to_string()]);
            let mut job = Job::new(job_id, spec);
            job.fail("Test failure".to_string()).unwrap();
            Self {
                job: Mutex::new(Some(job)),
            }
        }
    }

    #[async_trait]
    impl JobRepository for MockJobRepositoryWithFailedJob {
        async fn save(&self, _job: &Job) -> Result<()> {
            Ok(())
        }
        async fn find_by_id(&self, _id: &JobId) -> Result<Option<Job>> {
            Ok(self.job.lock().unwrap().clone())
        }
        async fn find_by_state(&self, _state: &JobState) -> Result<Vec<Job>> {
            Ok(vec![])
        }
        async fn find_pending(&self) -> Result<Vec<Job>> {
            Ok(vec![])
        }
        async fn find_all(&self, _limit: usize, _offset: usize) -> Result<(Vec<Job>, usize)> {
            Ok((vec![], 0))
        }
        async fn find_by_execution_id(&self, _execution_id: &str) -> Result<Option<Job>> {
            Ok(None)
        }
        async fn delete(&self, _id: &JobId) -> Result<()> {
            Ok(())
        }
        async fn update(&self, job: &Job) -> Result<()> {
            *self.job.lock().unwrap() = Some(job.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_retry_job_publishes_job_retried_event() {
        let repo = Arc::new(MockJobRepositoryWithFailedJob::new_with_failed_job());
        let queue = Arc::new(MockJobQueue);
        let bus = Arc::new(MockEventBus::new());

        // Get the job_id from the mock
        let job_id = repo.job.lock().unwrap().as_ref().unwrap().id.clone();

        let use_case = RetryJobUseCase::new(repo, queue, bus.clone());

        let result = use_case.execute(job_id).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.attempt, 2); // First attempt was 1, retry increments to 2
        assert_eq!(response.status, "PENDING");

        let events = bus.published.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::JobRetried {
                job_id: _,
                attempt,
                max_attempts,
                occurred_at: _,
                correlation_id: _,
                actor: _,
            } => {
                assert_eq!(*attempt, 2);
                assert_eq!(*max_attempts, 3);
            }
            _ => panic!("Expected JobRetried event"),
        }
    }

    #[tokio::test]
    async fn test_retry_job_fails_when_max_attempts_exceeded() {
        let repo = Arc::new(MockJobRepositoryWithFailedJob::new_with_failed_job());
        
        // Set attempts to max
        {
            let mut job_guard = repo.job.lock().unwrap();
            if let Some(ref mut job) = *job_guard {
                job.attempts = 3; // max_attempts is 3
            }
        }

        let queue = Arc::new(MockJobQueue);
        let bus = Arc::new(MockEventBus::new());

        let job_id = repo.job.lock().unwrap().as_ref().unwrap().id.clone();

        let use_case = RetryJobUseCase::new(repo, queue, bus.clone());

        let result = use_case.execute(job_id).await;
        assert!(result.is_err());

        // No event should be published
        let events = bus.published.lock().unwrap();
        assert!(events.is_empty());
    }
}

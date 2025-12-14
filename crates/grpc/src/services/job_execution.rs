//! Job Execution gRPC Service Implementation

use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::info;

use hodei_jobs_application::job_execution_usecases::{
    CancelJobUseCase, CreateJobRequest, CreateJobUseCase, JobSpecRequest,
};
use hodei_jobs_domain::job_execution::JobRepository;
use hodei_jobs_domain::shared_kernel::{DomainError, JobId};
use hodei_jobs_domain::worker_registry::WorkerRegistry;
use uuid::Uuid;

use hodei_jobs::{
    job_execution_service_server::JobExecutionService,
    QueueJobRequest, QueueJobResponse,
    AssignJobRequest, AssignJobResponse,
    StartJobRequest, StartJobResponse,
    UpdateProgressRequest, UpdateProgressResponse,
    CompleteJobRequest, CompleteJobResponse,
    FailJobRequest, FailJobResponse,
    CancelJobRequest, CancelJobResponse,
    JobExecution, ExecutionId,
    JobId as GrpcJobId,
};

#[derive(Clone)]
pub struct JobExecutionServiceImpl {
    create_job_usecase: Arc<CreateJobUseCase>,
    cancel_job_usecase: Arc<CancelJobUseCase>,
    job_repository: Arc<dyn JobRepository>,
    worker_registry: Arc<dyn WorkerRegistry>,
}

impl JobExecutionServiceImpl {
    pub fn new(
        create_job_usecase: Arc<CreateJobUseCase>,
        cancel_job_usecase: Arc<CancelJobUseCase>,
        job_repository: Arc<dyn JobRepository>,
        worker_registry: Arc<dyn WorkerRegistry>,
    ) -> Self {
        Self {
            create_job_usecase,
            cancel_job_usecase,
            job_repository,
            worker_registry,
        }
    }

    fn parse_job_id(job_id: Option<GrpcJobId>) -> Result<JobId, Status> {
        let value = job_id
            .map(|id| id.value)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| Status::invalid_argument("job_id is required"))?;

        let uuid = Uuid::parse_str(&value)
            .map_err(|_| Status::invalid_argument("job_id must be a UUID"))?;
        Ok(JobId(uuid))
    }

    fn now_timestamp() -> prost_types::Timestamp {
        let now = chrono::Utc::now();
        prost_types::Timestamp {
            seconds: now.timestamp(),
            nanos: now.timestamp_subsec_nanos() as i32,
        }
    }

    fn to_status(err: DomainError) -> Status {
        match err {
            DomainError::JobNotFound { .. } => Status::not_found(err.to_string()),
            DomainError::InvalidStateTransition { .. } => Status::failed_precondition(err.to_string()),
            DomainError::WorkerNotFound { .. } => Status::not_found(err.to_string()),
            DomainError::WorkerNotAvailable { .. } => Status::failed_precondition(err.to_string()),
            _ => Status::internal(err.to_string()),
        }
    }
}

#[tonic::async_trait]
impl JobExecutionService for JobExecutionServiceImpl {
    async fn queue_job(
        &self,
        request: Request<QueueJobRequest>,
    ) -> Result<Response<QueueJobResponse>, Status> {
        let req = request.into_inner();
        let definition = req
            .job_definition
            .ok_or_else(|| Status::invalid_argument("job_definition is required"))?;

        let mut command = Vec::new();
        if !definition.command.is_empty() {
            command.push(definition.command);
        }
        command.extend(definition.arguments);

        let create_request = CreateJobRequest {
            spec: JobSpecRequest {
                command,
                image: None,
                env: Some(definition.environment),
                timeout_ms: None,
                working_dir: None,
            },
            correlation_id: None,
        };

        let result = self
            .create_job_usecase
            .execute(create_request)
            .await
            .map_err(Self::to_status)?;

        info!("Queued job via gRPC: {}", result.job_id);

        Ok(Response::new(QueueJobResponse {
            success: true,
            message: format!("Job queued: {}", result.job_id),
            queued_at: Some(Self::now_timestamp()),
        }))
    }

    async fn assign_job(
        &self,
        request: Request<AssignJobRequest>,
    ) -> Result<Response<AssignJobResponse>, Status> {
        let req = request.into_inner();

        let job_id = Self::parse_job_id(req.job_id)?;
        let worker_uuid = req
            .worker_id
            .map(|w| w.value)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| Status::invalid_argument("worker_id is required"))?;
        let worker_id = hodei_jobs_domain::shared_kernel::WorkerId(
            Uuid::parse_str(&worker_uuid)
                .map_err(|_| Status::invalid_argument("worker_id must be a UUID"))?,
        );

        let worker = self
            .worker_registry
            .get(&worker_id)
            .await
            .map_err(Self::to_status)?
            .ok_or_else(|| Status::not_found("Worker not found"))?;

        let provider_id = worker.handle().provider_id.clone();
        let provider_execution_id = Uuid::new_v4().to_string();

        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(Self::to_status)?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        let context = hodei_jobs_domain::job_execution::ExecutionContext::new(
            job_id.clone(),
            provider_id.clone(),
            provider_execution_id.clone(),
        );
        job.submit_to_provider(provider_id, context)
            .map_err(Self::to_status)?;

        self.worker_registry
            .assign_to_job(&worker_id, job_id.clone())
            .await
            .map_err(Self::to_status)?;

        self.job_repository.update(&job).await.map_err(Self::to_status)?;

        Ok(Response::new(AssignJobResponse {
            success: true,
            message: "Assigned".to_string(),
            execution_id: Some(ExecutionId {
                value: provider_execution_id,
            }),
            assigned_at: Some(Self::now_timestamp()),
        }))
    }

    async fn start_job(
        &self,
        request: Request<StartJobRequest>,
    ) -> Result<Response<StartJobResponse>, Status> {
        let req = request.into_inner();

        let job_id = Self::parse_job_id(req.job_id)?;
        let execution_id = req
            .execution_id
            .map(|e| e.value)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| Status::invalid_argument("execution_id is required"))?;

        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(Self::to_status)?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        if job
            .execution_context
            .as_ref()
            .map(|c| c.provider_execution_id.as_str())
            != Some(execution_id.as_str())
        {
            return Err(Status::failed_precondition(
                "execution_id does not match job execution context",
            ));
        }

        job.mark_running().map_err(Self::to_status)?;
        self.job_repository.update(&job).await.map_err(Self::to_status)?;

        Ok(Response::new(StartJobResponse {
            success: true,
            message: "Started".to_string(),
            started_at: Some(Self::now_timestamp()),
        }))
    }

    async fn update_progress(
        &self,
        request: Request<UpdateProgressRequest>,
    ) -> Result<Response<UpdateProgressResponse>, Status> {
        let req = request.into_inner();
        let job_id = Self::parse_job_id(req.job_id)?;
        let execution_id = req
            .execution_id
            .map(|e| e.value)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| Status::invalid_argument("execution_id is required"))?;

        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(Self::to_status)?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        if job
            .execution_context
            .as_ref()
            .map(|c| c.provider_execution_id.as_str())
            != Some(execution_id.as_str())
        {
            return Err(Status::failed_precondition(
                "execution_id does not match job execution context",
            ));
        }

        job.metadata
            .insert("progress_percentage".to_string(), req.progress_percentage.to_string());
        if !req.current_stage.is_empty() {
            job.metadata
                .insert("current_stage".to_string(), req.current_stage);
        }
        if !req.message.is_empty() {
            job.metadata.insert("progress_message".to_string(), req.message);
        }

        self.job_repository.update(&job).await.map_err(Self::to_status)?;

        Ok(Response::new(UpdateProgressResponse {
            success: true,
            message: "Updated".to_string(),
            updated_at: Some(Self::now_timestamp()),
        }))
    }

    async fn complete_job(
        &self,
        request: Request<CompleteJobRequest>,
    ) -> Result<Response<CompleteJobResponse>, Status> {
        let req = request.into_inner();
        let job_id = Self::parse_job_id(req.job_id)?;
        let execution_id = req
            .execution_id
            .map(|e| e.value)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| Status::invalid_argument("execution_id is required"))?;

        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(Self::to_status)?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        if job
            .execution_context
            .as_ref()
            .map(|c| c.provider_execution_id.as_str())
            != Some(execution_id.as_str())
        {
            return Err(Status::failed_precondition(
                "execution_id does not match job execution context",
            ));
        }

        let exit_code: i32 = req.exit_code.parse().unwrap_or_default();
        job.complete(hodei_jobs_domain::shared_kernel::JobResult::Success {
            exit_code,
            output: req.output,
            error_output: req.error_output,
        })
        .map_err(Self::to_status)?;

        self.job_repository.update(&job).await.map_err(Self::to_status)?;

        Ok(Response::new(CompleteJobResponse {
            success: true,
            message: "Completed".to_string(),
            completed_at: Some(Self::now_timestamp()),
        }))
    }

    async fn fail_job(
        &self,
        request: Request<FailJobRequest>,
    ) -> Result<Response<FailJobResponse>, Status> {
        let req = request.into_inner();
        let job_id = Self::parse_job_id(req.job_id)?;
        let execution_id = req
            .execution_id
            .map(|e| e.value)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| Status::invalid_argument("execution_id is required"))?;

        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(Self::to_status)?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        if job
            .execution_context
            .as_ref()
            .map(|c| c.provider_execution_id.as_str())
            != Some(execution_id.as_str())
        {
            return Err(Status::failed_precondition(
                "execution_id does not match job execution context",
            ));
        }

        job.fail(req.error_message).map_err(Self::to_status)?;
        self.job_repository.update(&job).await.map_err(Self::to_status)?;

        Ok(Response::new(FailJobResponse {
            success: true,
            message: "Failed".to_string(),
            failed_at: Some(Self::now_timestamp()),
        }))
    }

    async fn cancel_job(
        &self,
        request: Request<CancelJobRequest>,
    ) -> Result<Response<CancelJobResponse>, Status> {
        let req = request.into_inner();
        let job_id = Self::parse_job_id(req.job_id)?;

        let result = self
            .cancel_job_usecase
            .execute(job_id)
            .await
            .map_err(Self::to_status)?;

        Ok(Response::new(CancelJobResponse {
            success: true,
            message: result.message,
            cancelled_at: Some(Self::now_timestamp()),
        }))
    }

    type ExecutionEventStreamStream = Pin<Box<dyn Stream<Item = Result<JobExecution, Status>> + Send>>;

    async fn execution_event_stream(
        &self,
        request: Request<ExecutionId>,
    ) -> Result<Response<Self::ExecutionEventStreamStream>, Status> {
        let _exec_id = request.into_inner().value;
        Err(Status::unimplemented(
            "execution_event_stream is not implemented yet",
        ))
    }
}

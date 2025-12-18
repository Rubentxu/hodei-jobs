//! Job Execution gRPC Service Implementation

use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::info;

use hodei_jobs_application::jobs::cancel::CancelJobUseCase;
use hodei_jobs_application::jobs::create::{CreateJobRequest, CreateJobUseCase, JobSpecRequest};
use hodei_jobs_domain::shared_kernel::{DomainError, JobId};
use uuid::Uuid;

use hodei_jobs::{
    AssignJobRequest, AssignJobResponse, CancelJobRequest, CancelJobResponse, CompleteJobRequest,
    CompleteJobResponse, ExecutionId, FailJobRequest, FailJobResponse, GetJobRequest,
    GetJobResponse, JobDefinition, JobExecution, JobId as GrpcJobId, JobStatus, JobSummary,
    ListJobsRequest, ListJobsResponse, QueueJobRequest, QueueJobResponse, ResourceRequirements,
    SchedulingInfo, StartJobRequest, StartJobResponse, TimeoutConfig, UpdateProgressRequest,
    UpdateProgressResponse, job_execution_service_server::JobExecutionService,
};

#[derive(Clone)]
pub struct JobExecutionServiceImpl {
    create_job_usecase: Arc<CreateJobUseCase>,
    cancel_job_usecase: Arc<CancelJobUseCase>,
    job_repository: Arc<dyn hodei_jobs_domain::jobs::JobRepository>,
    worker_registry: Arc<dyn hodei_jobs_domain::workers::registry::WorkerRegistry>,
}

impl JobExecutionServiceImpl {
    pub fn new(
        create_job_usecase: Arc<CreateJobUseCase>,
        cancel_job_usecase: Arc<CancelJobUseCase>,
        job_repository: Arc<dyn hodei_jobs_domain::jobs::JobRepository>,
        worker_registry: Arc<dyn hodei_jobs_domain::workers::registry::WorkerRegistry>,
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
            DomainError::InvalidStateTransition { .. } => {
                Status::failed_precondition(err.to_string())
            }
            DomainError::WorkerNotFound { .. } => Status::not_found(err.to_string()),
            DomainError::WorkerNotAvailable { .. } => Status::failed_precondition(err.to_string()),
            _ => Status::internal(err.to_string()),
        }
    }

    fn to_timestamp(dt: chrono::DateTime<chrono::Utc>) -> prost_types::Timestamp {
        prost_types::Timestamp {
            seconds: dt.timestamp(),
            nanos: dt.timestamp_subsec_nanos() as i32,
        }
    }

    fn map_job_state(state: &hodei_jobs_domain::shared_kernel::JobState) -> JobStatus {
        match state {
            hodei_jobs_domain::shared_kernel::JobState::Pending => JobStatus::Pending,
            hodei_jobs_domain::shared_kernel::JobState::Scheduled => JobStatus::Assigned,
            hodei_jobs_domain::shared_kernel::JobState::Running => JobStatus::Running,
            hodei_jobs_domain::shared_kernel::JobState::Succeeded => JobStatus::Completed,
            hodei_jobs_domain::shared_kernel::JobState::Failed => JobStatus::Failed,
            hodei_jobs_domain::shared_kernel::JobState::Cancelled => JobStatus::Cancelled,
            hodei_jobs_domain::shared_kernel::JobState::Timeout => JobStatus::Timeout,
        }
    }

    fn map_job_to_summary(job: hodei_jobs_domain::jobs::Job) -> JobSummary {
        let duration = job.execution_duration().map(|d| prost_types::Duration {
            seconds: d.as_secs() as i64,
            nanos: d.subsec_nanos() as i32,
        });

        let progress = job
            .metadata
            .get("progress_percentage")
            .and_then(|p| p.parse::<i32>().ok())
            .unwrap_or(0);

        JobSummary {
            job_id: Some(GrpcJobId {
                value: job.id.to_string(),
            }),
            name: format!("Job {}", &job.id.to_string()[..8]),
            status: Self::map_job_state(&job.state) as i32,
            created_at: Some(Self::to_timestamp(job.created_at)),
            started_at: job.started_at.map(Self::to_timestamp),
            completed_at: job.completed_at.map(Self::to_timestamp),
            duration,
            progress_percentage: progress,
        }
    }

    fn map_job_to_definition(job: &hodei_jobs_domain::jobs::Job) -> JobDefinition {
        let cmd_vec = job.spec.command_vec();
        let command = cmd_vec.first().cloned().unwrap_or_default();
        let arguments = if cmd_vec.len() > 1 {
            cmd_vec[1..].to_vec()
        } else {
            vec![]
        };

        JobDefinition {
            job_id: Some(GrpcJobId {
                value: job.id.to_string(),
            }),
            name: format!("Job {}", &job.id.to_string()[..8]),
            description: "".to_string(),
            command,
            arguments,
            environment: job.spec.env.clone(),
            requirements: Some(ResourceRequirements {
                cpu_cores: job.spec.resources.cpu_cores as f64,
                memory_bytes: (job.spec.resources.memory_mb * 1024 * 1024) as i64,
                disk_bytes: (job.spec.resources.storage_mb * 1024 * 1024) as i64,
                gpu_count: if job.spec.resources.gpu_required {
                    1
                } else {
                    0
                },
                custom_required: Default::default(),
            }),
            scheduling: Some(SchedulingInfo {
                priority: 1, // Normal
                scheduler_name: "default".to_string(),
                deadline: None,
                preemption_allowed: false,
            }),
            selector: None,
            tolerations: vec![],
            timeout: Some(TimeoutConfig {
                execution_timeout: Some(prost_types::Duration {
                    seconds: (job.spec.timeout_ms / 1000) as i64,
                    nanos: ((job.spec.timeout_ms % 1000) * 1_000_000) as i32,
                }),
                heartbeat_timeout: None,
                cleanup_timeout: None,
            }),
            tags: vec![],
        }
    }
}

#[tonic::async_trait]
impl JobExecutionService for JobExecutionServiceImpl {
    async fn queue_job(
        &self,
        request: Request<QueueJobRequest>,
    ) -> Result<Response<QueueJobResponse>, Status> {
        let metadata = request.metadata();
        let correlation_id = metadata
            .get("x-correlation-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let actor = metadata
            .get("x-user-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let req = request.into_inner();
        let definition = req
            .job_definition
            .ok_or_else(|| Status::invalid_argument("job_definition is required"))?;

        let mut command = Vec::new();
        if !definition.command.is_empty() {
            command.push(definition.command);
        }
        command.extend(definition.arguments);

        // Map timeout from protobuf Duration to milliseconds
        let timeout_ms = definition.timeout.and_then(|t| {
            t.execution_timeout.and_then(|duration| {
                let seconds = duration.seconds;
                let nanos = duration.nanos;
                Some((seconds * 1000) as u64 + (nanos as u64 / 1_000_000))
            })
        });

        // Map resource requirements
        let (cpu_cores, memory_bytes, disk_bytes) =
            definition.requirements.map_or((None, None, None), |r| {
                tracing::info!(
                    "Mapping requirements: cpu_cores={}, memory_bytes={}, disk_bytes={}",
                    r.cpu_cores,
                    r.memory_bytes,
                    r.disk_bytes
                );
                (Some(r.cpu_cores), Some(r.memory_bytes), Some(r.disk_bytes))
            });

        tracing::info!(
            "Mapped requirements: cpu_cores={:?}, memory_bytes={:?}, disk_bytes={:?}",
            cpu_cores,
            memory_bytes,
            disk_bytes
        );

        // Extract job_id from request if provided (filter empty values)
        let job_id = definition
            .job_id
            .as_ref()
            .map(|id| id.value.clone())
            .filter(|v| !v.is_empty());

        tracing::info!(
            "QueueJob request - client job_id: {:?}, name: {}",
            job_id,
            definition.name
        );

        let create_request = CreateJobRequest {
            spec: JobSpecRequest {
                command,
                image: None,
                env: Some(definition.environment),
                timeout_ms,
                working_dir: None,
                cpu_cores,
                memory_bytes,
                disk_bytes,
            },
            correlation_id,
            actor,
            job_id,
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

        let context = hodei_jobs_domain::jobs::ExecutionContext::new(
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

        self.job_repository
            .update(&job)
            .await
            .map_err(Self::to_status)?;

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
        self.job_repository
            .update(&job)
            .await
            .map_err(Self::to_status)?;

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

        job.metadata.insert(
            "progress_percentage".to_string(),
            req.progress_percentage.to_string(),
        );
        if !req.current_stage.is_empty() {
            job.metadata
                .insert("current_stage".to_string(), req.current_stage);
        }
        if !req.message.is_empty() {
            job.metadata
                .insert("progress_message".to_string(), req.message);
        }

        self.job_repository
            .update(&job)
            .await
            .map_err(Self::to_status)?;

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

        self.job_repository
            .update(&job)
            .await
            .map_err(Self::to_status)?;

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
        self.job_repository
            .update(&job)
            .await
            .map_err(Self::to_status)?;

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

    async fn get_job(
        &self,
        request: Request<GetJobRequest>,
    ) -> Result<Response<GetJobResponse>, Status> {
        let req = request.into_inner();
        let job_id = Self::parse_job_id(req.job_id)?;

        let job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(Self::to_status)?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        let definition = Self::map_job_to_definition(&job);
        let status = Self::map_job_state(&job.state);

        // Create execution if exists
        let mut executions = Vec::new();
        if let Some(ctx) = &job.execution_context {
            let execution = JobExecution {
                execution_id: Some(ExecutionId {
                    value: ctx.provider_execution_id.clone(),
                }),
                job_id: Some(GrpcJobId {
                    value: job.id.to_string(),
                }),
                worker_id: None, // We don't have worker_id in execution context easily available here without lookup
                state: 0,        // Map execution state if needed
                job_status: status as i32,
                start_time: ctx.started_at.map(Self::to_timestamp),
                end_time: ctx.completed_at.map(Self::to_timestamp),
                retry_count: job.attempts as i32,
                exit_code: job
                    .result
                    .as_ref()
                    .map(|r| match r {
                        hodei_jobs_domain::shared_kernel::JobResult::Success {
                            exit_code, ..
                        } => exit_code.to_string(),
                        hodei_jobs_domain::shared_kernel::JobResult::Failed {
                            exit_code, ..
                        } => exit_code.to_string(),
                        _ => "0".to_string(),
                    })
                    .unwrap_or_default(),
                error_message: job.error_message.clone().unwrap_or_default(),
                metadata: job.metadata.clone(),
            };
            executions.push(execution);
        }

        Ok(Response::new(GetJobResponse {
            job: Some(definition),
            status: status as i32,
            executions,
        }))
    }

    async fn list_jobs(
        &self,
        request: Request<ListJobsRequest>,
    ) -> Result<Response<ListJobsResponse>, Status> {
        let req = request.into_inner();
        let limit = if req.limit <= 0 {
            10
        } else {
            req.limit as usize
        };
        let offset = if req.offset < 0 {
            0
        } else {
            req.offset as usize
        };

        let (jobs, total_count) = self
            .job_repository
            .find_all(limit, offset)
            .await
            .map_err(Self::to_status)?;

        let job_summaries = jobs.into_iter().map(Self::map_job_to_summary).collect();

        Ok(Response::new(ListJobsResponse {
            jobs: job_summaries,
            total_count: total_count as i32,
        }))
    }

    type ExecutionEventStreamStream =
        Pin<Box<dyn Stream<Item = Result<JobExecution, Status>> + Send>>;

    async fn execution_event_stream(
        &self,
        request: Request<ExecutionId>,
    ) -> Result<Response<Self::ExecutionEventStreamStream>, Status> {
        let execution_id = request.into_inner().value;
        if execution_id.is_empty() {
            return Err(Status::invalid_argument("execution_id is required"));
        }

        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let job_repository = self.job_repository.clone();
        let exec_id_clone = execution_id.clone();

        tokio::spawn(async move {
            let mut last_status = -1;
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));

            loop {
                interval.tick().await;

                match job_repository.find_by_execution_id(&exec_id_clone).await {
                    Ok(Some(job)) => {
                        let status = Self::map_job_state(&job.state) as i32;

                        // Check if we should send an update (simple change detection)
                        // In a real implementation, we might want to check more fields or use a proper event bus
                        if status != last_status {
                            last_status = status;

                            if let Some(ctx) = &job.execution_context {
                                let execution = JobExecution {
                                    execution_id: Some(ExecutionId { value: ctx.provider_execution_id.clone() }),
                                    job_id: Some(GrpcJobId { value: job.id.to_string() }),
                                    worker_id: None,
                                    state: 0,
                                    job_status: status,
                                    start_time: ctx.started_at.map(Self::to_timestamp),
                                    end_time: ctx.completed_at.map(Self::to_timestamp),
                                    retry_count: job.attempts as i32,
                                    exit_code: job.result.as_ref().map(|r| match r {
                                        hodei_jobs_domain::shared_kernel::JobResult::Success { exit_code, .. } => exit_code.to_string(),
                                        hodei_jobs_domain::shared_kernel::JobResult::Failed { exit_code, .. } => exit_code.to_string(),
                                        _ => "0".to_string(),
                                    }).unwrap_or_default(),
                                    error_message: job.error_message.clone().unwrap_or_default(),
                                    metadata: job.metadata.clone(),
                                };

                                if tx.send(Ok(execution)).await.is_err() {
                                    break; // Client disconnected
                                }
                            }
                        }

                        // Stop streaming if job is in a terminal state
                        if matches!(
                            job.state,
                            hodei_jobs_domain::shared_kernel::JobState::Succeeded
                                | hodei_jobs_domain::shared_kernel::JobState::Failed
                                | hodei_jobs_domain::shared_kernel::JobState::Cancelled
                                | hodei_jobs_domain::shared_kernel::JobState::Timeout
                        ) {
                            // Give a moment for the final message to be sent/processed if needed, then exit
                            // But we already sent the update above.
                            // Maybe we want to keep stream open for a bit?
                            // For now, let's close it after sending the terminal state.
                            break;
                        }
                    }
                    Ok(None) => {
                        // Job not found (yet?), keep polling or error?
                        // If it was found before and now not, maybe deleted?
                        // For now, just continue polling
                    }
                    Err(e) => {
                        let _ = tx.send(Err(Status::internal(e.to_string()))).await;
                        break;
                    }
                }
            }
        });

        let output_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Response::new(
            Box::pin(output_stream) as Self::ExecutionEventStreamStream
        ))
    }
}

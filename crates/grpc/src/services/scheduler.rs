//! Scheduler gRPC Service Implementation

use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::RwLock;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::info;

use hodei_jobs_application::job_execution_usecases::{
    CreateJobRequest, CreateJobUseCase, JobSpecRequest,
};
use hodei_jobs_application::smart_scheduler::{SchedulerConfig, SchedulingService};
use hodei_jobs_application::worker_provisioning::WorkerProvisioningService;
use hodei_jobs_domain::job_execution::{ExecutionContext, JobQueue, JobRepository};
use hodei_jobs_domain::job_scheduler::SchedulingContext;
use hodei_jobs_domain::shared_kernel::{DomainError, JobId, JobState, Result as DomainResult, WorkerId, WorkerState};
use hodei_jobs_domain::worker_registry::{WorkerFilter, WorkerRegistry};
use uuid::Uuid;

use hodei_jobs::{
    scheduler_service_server::SchedulerService,
    ScheduleJobRequest, ScheduleJobResponse,
    GetSchedulingDecisionRequest, GetSchedulingDecisionResponse,
    ConfigureSchedulerRequest, ConfigureSchedulerResponse,
    GetQueueStatusRequest, GetQueueStatusResponse,
    GetAvailableWorkersRequest, GetAvailableWorkersResponse,
    AvailableWorker, ExecutionId, JobId as GrpcJobId, QueueStatus,
    SchedulingDecision, WorkerId as GrpcWorkerId, WorkerSchedulingInfo,
};

#[derive(Clone)]
pub struct SchedulerServiceImpl {
    create_job_usecase: Arc<CreateJobUseCase>,
    job_repository: Arc<dyn JobRepository>,
    job_queue: Arc<dyn JobQueue>,
    worker_registry: Arc<dyn WorkerRegistry>,
    scheduling_service: Arc<RwLock<SchedulingService>>,
    /// Optional provisioning service for creating workers on-demand
    provisioning_service: Option<Arc<dyn WorkerProvisioningService>>,
}

impl SchedulerServiceImpl {
    pub fn new(
        create_job_usecase: Arc<CreateJobUseCase>,
        job_repository: Arc<dyn JobRepository>,
        job_queue: Arc<dyn JobQueue>,
        worker_registry: Arc<dyn WorkerRegistry>,
        scheduler_config: SchedulerConfig,
    ) -> Self {
        Self {
            create_job_usecase,
            job_repository,
            job_queue,
            worker_registry,
            scheduling_service: Arc::new(RwLock::new(SchedulingService::new(scheduler_config))),
            provisioning_service: None,
        }
    }

    /// Create a new SchedulerServiceImpl with provisioning support
    pub fn with_provisioning(
        create_job_usecase: Arc<CreateJobUseCase>,
        job_repository: Arc<dyn JobRepository>,
        job_queue: Arc<dyn JobQueue>,
        worker_registry: Arc<dyn WorkerRegistry>,
        scheduler_config: SchedulerConfig,
        provisioning_service: Arc<dyn WorkerProvisioningService>,
    ) -> Self {
        Self {
            create_job_usecase,
            job_repository,
            job_queue,
            worker_registry,
            scheduling_service: Arc::new(RwLock::new(SchedulingService::new(scheduler_config))),
            provisioning_service: Some(provisioning_service),
        }
    }

    fn now_timestamp() -> prost_types::Timestamp {
        let now = chrono::Utc::now();
        prost_types::Timestamp {
            seconds: now.timestamp(),
            nanos: now.timestamp_subsec_nanos() as i32,
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

    fn to_status(err: DomainError) -> Status {
        match err {
            DomainError::JobNotFound { .. } => Status::not_found(err.to_string()),
            DomainError::WorkerNotFound { .. } => Status::not_found(err.to_string()),
            DomainError::WorkerNotAvailable { .. } => Status::failed_precondition(err.to_string()),
            DomainError::InvalidStateTransition { .. } => Status::failed_precondition(err.to_string()),
            _ => Status::internal(err.to_string()),
        }
    }

    fn map_worker_status(state: &WorkerState) -> i32 {
        match state {
            WorkerState::Creating | WorkerState::Connecting => 1, // REGISTERING
            WorkerState::Ready => 2,                               // AVAILABLE
            WorkerState::Busy => 3,                                // BUSY
            WorkerState::Draining => 4,                            // DRAINING
            WorkerState::Terminating => 5,                         // TERMINATING
            WorkerState::Terminated => 0,                          // OFFLINE
        }
    }

    fn persist_decision_metadata(
        job: &mut hodei_jobs_domain::job_execution::Job,
        worker_id: &WorkerId,
        execution_id: &str,
        score: f64,
        reasons: &[String],
    ) {
        job.metadata
            .insert("scheduler.selected_worker_id".to_string(), worker_id.to_string());
        job.metadata
            .insert("scheduler.execution_id".to_string(), execution_id.to_string());
        job.metadata
            .insert("scheduler.score".to_string(), score.to_string());
        job.metadata.insert(
            "scheduler.reasons".to_string(),
            reasons.join("\n"),
        );
    }

    fn build_decision_from_job(job_id: GrpcJobId, job: &hodei_jobs_domain::job_execution::Job) -> SchedulingDecision {
        let selected_worker_id = job
            .metadata
            .get("scheduler.selected_worker_id")
            .and_then(|v| Uuid::parse_str(v).ok())
            .map(|_| GrpcWorkerId {
                value: job
                    .metadata
                    .get("scheduler.selected_worker_id")
                    .cloned()
                    .unwrap_or_default(),
            });

        let execution_id = job
            .metadata
            .get("scheduler.execution_id")
            .cloned()
            .filter(|s| !s.is_empty())
            .map(|value| ExecutionId { value });

        let score = job
            .metadata
            .get("scheduler.score")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);

        let reasons = job
            .metadata
            .get("scheduler.reasons")
            .map(|s| s.lines().map(|l| l.to_string()).collect())
            .unwrap_or_else(Vec::new);

        SchedulingDecision {
            job_id: Some(job_id),
            selected_worker_id,
            execution_id,
            score,
            reasons,
            decision_time: Some(Self::now_timestamp()),
            allocation: None,
        }
    }

    async fn build_scheduling_context(&self, job: hodei_jobs_domain::job_execution::Job) -> DomainResult<SchedulingContext> {
        let available_workers = self.worker_registry.find_available().await?;
        let pending_jobs_count = self.job_queue.len().await?;
        let stats = self.worker_registry.stats().await?;

        let system_load = if stats.total_workers > 0 {
            stats.busy_workers as f64 / stats.total_workers as f64
        } else {
            0.0
        };

        Ok(SchedulingContext {
            job,
            available_workers,
            available_providers: Vec::new(),
            pending_jobs_count,
            system_load,
        })
    }

    async fn remove_job_from_queue(&self, target_job_id: &JobId) -> DomainResult<()> {
        let original_len = self.job_queue.len().await?;
        if original_len == 0 {
            return Ok(());
        }

        for _ in 0..original_len {
            let Some(job) = self.job_queue.dequeue().await? else {
                break;
            };

            if &job.id == target_job_id {
                continue;
            }

            self.job_queue.enqueue(job).await?;
        }

        Ok(())
    }
}

#[tonic::async_trait]
impl SchedulerService for SchedulerServiceImpl {
    async fn schedule_job(
        &self,
        request: Request<ScheduleJobRequest>,
    ) -> Result<Response<ScheduleJobResponse>, Status> {
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

        let create_response = self
            .create_job_usecase
            .execute(create_request)
            .await
            .map_err(Self::to_status)?;

        info!("Scheduling job via gRPC: {}", create_response.job_id);

        let job_uuid = Uuid::parse_str(&create_response.job_id)
            .map_err(|_| Status::internal("CreateJobUseCase returned non-UUID job_id"))?;
        let job_id = JobId(job_uuid);

        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(Self::to_status)?
            .ok_or_else(|| Status::not_found("Job not found after creation"))?;

        let context = self
            .build_scheduling_context(job.clone())
            .await
            .map_err(Self::to_status)?;
        let decision = self
            .scheduling_service
            .read()
            .await
            .make_decision(context)
            .await
            .map_err(Self::to_status)?;

        match decision {
            hodei_jobs_domain::job_scheduler::SchedulingDecision::AssignToWorker { worker_id, .. } => {
                let worker = self
                    .worker_registry
                    .get(&worker_id)
                    .await
                    .map_err(Self::to_status)?
                    .ok_or_else(|| Status::not_found("Worker not found"))?;

                let provider_id = worker.handle().provider_id.clone();
                let provider_execution_id = Uuid::new_v4().to_string();
                let exec_ctx = ExecutionContext::new(
                    job_id.clone(),
                    provider_id.clone(),
                    provider_execution_id.clone(),
                );

                job.submit_to_provider(provider_id, exec_ctx)
                    .map_err(Self::to_status)?;
                self.worker_registry
                    .assign_to_job(&worker_id, job_id.clone())
                    .await
                    .map_err(Self::to_status)?;

                self.remove_job_from_queue(&job_id)
                    .await
                    .map_err(Self::to_status)?;

                let reasons = vec!["Assigned to existing worker".to_string()];
                Self::persist_decision_metadata(&mut job, &worker_id, &provider_execution_id, 1.0, &reasons);
                self.job_repository.update(&job).await.map_err(Self::to_status)?;

                Ok(Response::new(ScheduleJobResponse {
                    success: true,
                    message: "Job scheduled".to_string(),
                    decision: Some(SchedulingDecision {
                        job_id: Some(GrpcJobId { value: create_response.job_id }),
                        selected_worker_id: Some(GrpcWorkerId { value: worker_id.to_string() }),
                        execution_id: Some(ExecutionId { value: provider_execution_id }),
                        score: 1.0,
                        reasons,
                        decision_time: Some(Self::now_timestamp()),
                        allocation: None,
                    }),
                    scheduled_at: Some(Self::now_timestamp()),
                }))
            }
            hodei_jobs_domain::job_scheduler::SchedulingDecision::Enqueue { reason, .. } => {
                job.metadata
                    .insert("scheduler.reasons".to_string(), reason.clone());
                self.job_repository.update(&job).await.map_err(Self::to_status)?;

                Ok(Response::new(ScheduleJobResponse {
                    success: true,
                    message: format!("Job enqueued: {}", reason),
                    decision: Some(SchedulingDecision {
                        job_id: Some(GrpcJobId { value: create_response.job_id }),
                        selected_worker_id: None,
                        execution_id: None,
                        score: 0.0,
                        reasons: vec![reason],
                        decision_time: Some(Self::now_timestamp()),
                        allocation: None,
                    }),
                    scheduled_at: Some(Self::now_timestamp()),
                }))
            }
            hodei_jobs_domain::job_scheduler::SchedulingDecision::Reject { reason, .. } => {
                job.state = JobState::Failed;
                job.error_message = Some(reason.clone());
                self.job_repository.update(&job).await.map_err(Self::to_status)?;

                Ok(Response::new(ScheduleJobResponse {
                    success: false,
                    message: format!("Job rejected: {}", reason),
                    decision: Some(SchedulingDecision {
                        job_id: Some(GrpcJobId { value: create_response.job_id }),
                        selected_worker_id: None,
                        execution_id: None,
                        score: 0.0,
                        reasons: vec![reason],
                        decision_time: Some(Self::now_timestamp()),
                        allocation: None,
                    }),
                    scheduled_at: Some(Self::now_timestamp()),
                }))
            }
            hodei_jobs_domain::job_scheduler::SchedulingDecision::ProvisionWorker { provider_id, .. } => {
                // Check if provisioning service is configured
                let Some(provisioning_service) = &self.provisioning_service else {
                    let msg = "Worker provisioning is not configured in this scheduler instance";
                    return Ok(Response::new(ScheduleJobResponse {
                        success: false,
                        message: msg.to_string(),
                        decision: Some(SchedulingDecision {
                            job_id: Some(GrpcJobId { value: create_response.job_id }),
                            selected_worker_id: None,
                            execution_id: None,
                            score: 0.0,
                            reasons: vec![msg.to_string()],
                            decision_time: Some(Self::now_timestamp()),
                            allocation: None,
                        }),
                        scheduled_at: Some(Self::now_timestamp()),
                    }));
                };

                // Get default worker spec for the provider
                let spec = provisioning_service
                    .default_worker_spec(&provider_id)
                    .ok_or_else(|| Status::internal("No default worker spec for provider"))?;

                // Provision the worker
                info!("Provisioning worker for job {} via provider {}", job_id, provider_id);
                let provisioning_result = provisioning_service
                    .provision_worker(&provider_id, spec)
                    .await
                    .map_err(Self::to_status)?;

                let worker_id = provisioning_result.worker_id;
                info!(
                    "Worker {} provisioned with OTP, job {} enqueued for assignment",
                    worker_id, job_id
                );

                // Enqueue the job - it will be assigned when the worker connects and becomes ready
                job.metadata.insert(
                    "scheduler.provisioned_worker_id".to_string(),
                    worker_id.to_string(),
                );
                job.metadata.insert(
                    "scheduler.provisioning_provider_id".to_string(),
                    provider_id.to_string(),
                );
                self.job_repository.update(&job).await.map_err(Self::to_status)?;

                let reasons = vec![
                    format!("Provisioned new worker {} via provider {}", worker_id, provider_id),
                    "Job enqueued, will be assigned when worker becomes ready".to_string(),
                ];

                Ok(Response::new(ScheduleJobResponse {
                    success: true,
                    message: format!("Worker {} provisioned, job enqueued", worker_id),
                    decision: Some(SchedulingDecision {
                        job_id: Some(GrpcJobId { value: create_response.job_id }),
                        selected_worker_id: Some(GrpcWorkerId { value: worker_id.to_string() }),
                        execution_id: None,
                        score: 0.8,
                        reasons,
                        decision_time: Some(Self::now_timestamp()),
                        allocation: None,
                    }),
                    scheduled_at: Some(Self::now_timestamp()),
                }))
            }
        }
    }

    async fn get_scheduling_decision(
        &self,
        request: Request<GetSchedulingDecisionRequest>,
    ) -> Result<Response<GetSchedulingDecisionResponse>, Status> {
        let job_id = request
            .into_inner()
            .job_id
            .ok_or_else(|| Status::invalid_argument("job_id is required"))?;

        let domain_job_id = Self::parse_job_id(Some(job_id.clone()))?;
        let job = self
            .job_repository
            .find_by_id(&domain_job_id)
            .await
            .map_err(Self::to_status)?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        let decision = Self::build_decision_from_job(job_id, &job);

        Ok(Response::new(GetSchedulingDecisionResponse {
            decision: Some(decision),
        }))
    }

    async fn configure_scheduler(
        &self,
        _request: Request<ConfigureSchedulerRequest>,
    ) -> Result<Response<ConfigureSchedulerResponse>, Status> {
        let req = _request.into_inner();
        let cfg = req.config.ok_or_else(|| Status::invalid_argument("config is required"))?;
        let plugin_cfg = cfg.plugin_config;

        let mut scheduler_config = SchedulerConfig::default();

        if let Some(v) = plugin_cfg.get("max_queue_depth") {
            if let Ok(n) = v.parse::<usize>() {
                scheduler_config.max_queue_depth = n;
            }
        }
        if let Some(v) = plugin_cfg.get("scale_up_load_threshold") {
            if let Ok(n) = v.parse::<f64>() {
                scheduler_config.scale_up_load_threshold = n;
            }
        }
        if let Some(v) = plugin_cfg.get("prefer_existing_workers") {
            if let Ok(b) = v.parse::<bool>() {
                scheduler_config.prefer_existing_workers = b;
            }
        }

        if let Some(v) = plugin_cfg.get("worker_strategy") {
            scheduler_config.worker_strategy = match v.as_str() {
                "first_available" => hodei_jobs_domain::job_scheduler::WorkerSelectionStrategy::FirstAvailable,
                "least_loaded" => hodei_jobs_domain::job_scheduler::WorkerSelectionStrategy::LeastLoaded,
                "round_robin" => hodei_jobs_domain::job_scheduler::WorkerSelectionStrategy::RoundRobin,
                "most_capacity" => hodei_jobs_domain::job_scheduler::WorkerSelectionStrategy::MostCapacity,
                "affinity" => hodei_jobs_domain::job_scheduler::WorkerSelectionStrategy::Affinity,
                _ => scheduler_config.worker_strategy,
            };
        }

        if let Some(v) = plugin_cfg.get("provider_strategy") {
            scheduler_config.provider_strategy = match v.as_str() {
                "first_available" => hodei_jobs_domain::job_scheduler::ProviderSelectionStrategy::FirstAvailable,
                "lowest_cost" => hodei_jobs_domain::job_scheduler::ProviderSelectionStrategy::LowestCost,
                "fastest_startup" => hodei_jobs_domain::job_scheduler::ProviderSelectionStrategy::FastestStartup,
                "most_capacity" => hodei_jobs_domain::job_scheduler::ProviderSelectionStrategy::MostCapacity,
                "round_robin" => hodei_jobs_domain::job_scheduler::ProviderSelectionStrategy::RoundRobin,
                "healthiest" => hodei_jobs_domain::job_scheduler::ProviderSelectionStrategy::Healthiest,
                _ => scheduler_config.provider_strategy,
            };
        }

        *self.scheduling_service.write().await = SchedulingService::new(scheduler_config);

        Ok(Response::new(ConfigureSchedulerResponse {
            success: true,
            message: format!("Configured scheduler: {}", cfg.scheduler_name),
            configured_at: Some(Self::now_timestamp()),
        }))
    }

    async fn get_queue_status(
        &self,
        _request: Request<GetQueueStatusRequest>,
    ) -> Result<Response<GetQueueStatusResponse>, Status> {
        let pending_jobs = self.job_queue.len().await.map_err(Self::to_status)? as i32;
        let running_jobs = self
            .job_repository
            .find_by_state(&JobState::Running)
            .await
            .map_err(Self::to_status)?
            .len() as i32;
        let completed_jobs = self
            .job_repository
            .find_by_state(&JobState::Succeeded)
            .await
            .map_err(Self::to_status)?
            .len() as i32;
        let failed_jobs = self
            .job_repository
            .find_by_state(&JobState::Failed)
            .await
            .map_err(Self::to_status)?
            .len() as i32;
        let cancelled_jobs = self
            .job_repository
            .find_by_state(&JobState::Cancelled)
            .await
            .map_err(Self::to_status)?
            .len() as i32;

        Ok(Response::new(GetQueueStatusResponse {
            status: Some(QueueStatus {
                pending_jobs,
                running_jobs,
                completed_jobs,
                failed_jobs,
                cancelled_jobs,
                last_updated: Some(Self::now_timestamp()),
            }),
        }))
    }

    async fn get_available_workers(
        &self,
        _request: Request<GetAvailableWorkersRequest>,
    ) -> Result<Response<GetAvailableWorkersResponse>, Status> {
        let req = _request.into_inner();

        let criteria = req.filter;

        let mut filter = WorkerFilter::new();
        if let Some(ref criteria) = criteria {
            // WorkerStatus enum: OFFLINE=0, REGISTERING=1, AVAILABLE=2, BUSY=3, DRAINING=4, TERMINATING=5
            match criteria.required_status {
                2 => {
                    filter = filter.with_state(WorkerState::Ready);
                }
                3 => {
                    filter = filter.with_state(WorkerState::Busy);
                }
                4 => {
                    filter = filter.with_state(WorkerState::Draining);
                }
                5 => {
                    filter = filter.with_state(WorkerState::Terminating);
                }
                _ => {}
            }
        }

        let mut workers = self
            .worker_registry
            .find(&filter)
            .await
            .map_err(Self::to_status)?;

        if let Some(criteria) = criteria {
            if let Some(max_age) = criteria.max_age {
                let max_age = std::time::Duration::new(
                    max_age.seconds.max(0) as u64,
                    max_age.nanos.max(0) as u32,
                );
                let cutoff = chrono::Utc::now() - chrono::Duration::from_std(max_age).unwrap_or_default();
                workers.retain(|w| w.last_heartbeat() >= cutoff);
            }
        }

        let resp_workers = workers
            .into_iter()
            .map(|w| AvailableWorker {
                worker_id: Some(GrpcWorkerId {
                    value: w.id().to_string(),
                }),
                scheduling_info: Some(WorkerSchedulingInfo {
                    worker_id: Some(GrpcWorkerId {
                        value: w.id().to_string(),
                    }),
                    node_selector: None,
                    affinity: None,
                    tolerations: vec![],
                    score: 0.0,
                    reason: vec![],
                }),
                current_usage: None,
                status: Self::map_worker_status(w.state()),
                last_heartbeat: Some(prost_types::Timestamp {
                    seconds: w.last_heartbeat().timestamp(),
                    nanos: w.last_heartbeat().timestamp_subsec_nanos() as i32,
                }),
            })
            .collect();

        Ok(Response::new(GetAvailableWorkersResponse {
            workers: resp_workers,
        }))
    }

    type SchedulingDecisionStreamStream = Pin<Box<dyn Stream<Item = Result<SchedulingDecision, Status>> + Send>>;

    async fn scheduling_decision_stream(
        &self,
        request: Request<GrpcJobId>,
    ) -> Result<Response<Self::SchedulingDecisionStreamStream>, Status> {
        let grpc_job_id = request.into_inner();
        let domain_job_id = Self::parse_job_id(Some(grpc_job_id.clone()))?;
        let job_repository = self.job_repository.clone();

        let stream = async_stream::stream! {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
            let mut last_emitted: Option<String> = None;

            for _ in 0..60 {
                interval.tick().await;
                let Ok(Some(job)) = job_repository.find_by_id(&domain_job_id).await else {
                    continue;
                };

                let key = job.metadata.get("scheduler.execution_id").cloned();
                if key.is_some() && key != last_emitted {
                    last_emitted = key;
                    yield Ok(SchedulerServiceImpl::build_decision_from_job(grpc_job_id.clone(), &job));
                }

                if matches!(job.state, JobState::Succeeded | JobState::Failed | JobState::Cancelled | JobState::Timeout) {
                    break;
                }
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }
}

//! Job Execution gRPC Service Implementation

use dashmap::DashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::mappers::{
    error_to_status, map_job_state, map_job_to_definition, map_job_to_summary, now_timestamp,
    parse_grpc_job_id, to_timestamp,
};
use hodei_server_application::jobs::cancel::CancelJobUseCase;
use hodei_server_application::jobs::create::{CreateJobRequest, CreateJobUseCase, JobSpecRequest};
use hodei_server_domain::shared_kernel::{JobId, WorkerId};

use hodei_jobs::{
    AssignJobRequest, AssignJobResponse, CancelJobRequest, CancelJobResponse, CompleteJobRequest,
    CompleteJobResponse, ExecutionId, FailJobRequest, FailJobResponse, GetJobRequest,
    GetJobResponse, JobExecution, JobId as GrpcJobId, ListJobsRequest, ListJobsResponse,
    QueueJobRequest, QueueJobResponse, StartJobRequest, StartJobResponse, UpdateProgressRequest,
    UpdateProgressResponse, job_execution_service_server::JobExecutionService,
};

use chrono::Utc;
use hodei_server_domain::events::{DomainEvent, TerminationReason};
use hodei_server_domain::outbox::OutboxEventInsert;
use hodei_server_domain::workers::WorkerProvider;
use hodei_server_domain::workers::provider_api::ProviderError;

#[derive(Clone)]
pub struct JobExecutionServiceImpl {
    create_job_usecase: Arc<CreateJobUseCase>,
    cancel_job_usecase: Arc<CancelJobUseCase>,
    job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
    worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
    /// Providers map for immediate worker cleanup (EPIC-26 US-26.5) - DashMap for lock-free concurrency
    providers:
        Arc<DashMap<hodei_server_domain::shared_kernel::ProviderId, Arc<dyn WorkerProvider>>>,
    /// Outbox repository for emitting events (EPIC-26)
    outbox_repository: Option<Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync>>,
    /// Event bus for publishing domain events
    event_bus: Option<Arc<dyn hodei_server_domain::event_bus::EventBus>>,
}

impl JobExecutionServiceImpl {
    pub fn new(
        create_job_usecase: Arc<CreateJobUseCase>,
        cancel_job_usecase: Arc<CancelJobUseCase>,
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
    ) -> Self {
        Self {
            create_job_usecase,
            cancel_job_usecase,
            job_repository,
            worker_registry,
            providers: Arc::new(DashMap::new()),
            outbox_repository: None,
            event_bus: None,
        }
    }

    /// Create with providers for worker cleanup (EPIC-26 US-26.5)
    pub fn with_cleanup_support(
        create_job_usecase: Arc<CreateJobUseCase>,
        cancel_job_usecase: Arc<CancelJobUseCase>,
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
        worker_registry: Arc<dyn hodei_server_domain::workers::registry::WorkerRegistry>,
        providers: Arc<
            DashMap<hodei_server_domain::shared_kernel::ProviderId, Arc<dyn WorkerProvider>>,
        >,
        outbox_repository: Option<
            Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync>,
        >,
        event_bus: Option<Arc<dyn hodei_server_domain::event_bus::EventBus>>,
    ) -> Self {
        Self {
            create_job_usecase,
            cancel_job_usecase,
            job_repository,
            worker_registry,
            providers,
            outbox_repository,
            event_bus,
        }
    }

    /// Set outbox repository after construction (EPIC-26)
    pub fn set_outbox_repository(
        &mut self,
        outbox_repository: Arc<dyn hodei_server_domain::outbox::OutboxRepository + Send + Sync>,
    ) {
        self.outbox_repository = Some(outbox_repository);
    }

    /// Set event bus after construction (EPIC-26)
    pub fn set_event_bus(&mut self, event_bus: Arc<dyn hodei_server_domain::event_bus::EventBus>) {
        self.event_bus = Some(event_bus);
    }

    /// Register a provider for worker cleanup
    pub async fn register_provider(&self, provider: Arc<dyn WorkerProvider>) {
        let provider_id = provider.provider_id().clone();
        self.providers.insert(provider_id, provider);
    }

    /// Trigger immediate worker cleanup after job completion (EPIC-26 US-26.5)
    async fn trigger_worker_cleanup(
        &self,
        worker_id: &WorkerId,
        job_id: &JobId,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get worker info
        let worker = match self.worker_registry.get(worker_id).await {
            Ok(Some(w)) => w,
            Ok(None) => {
                warn!("Worker {} not found for cleanup", worker_id);
                return Ok(());
            }
            Err(e) => {
                warn!("Failed to get worker {}: {}", worker_id, e);
                return Err(e.into());
            }
        };

        let worker_state = worker.state().clone();
        let provider_id = worker.handle().provider_id.clone();

        // Skip cleanup if worker is already terminated (Crash-Only: no Terminating state)
        if matches!(
            worker_state,
            hodei_server_domain::shared_kernel::WorkerState::Terminated
        ) {
            debug!("Worker {} already terminated, skipping cleanup", worker_id);
            return Ok(());
        }

        // Emit WorkerEphemeralTerminating event (EPIC-26 US-26.3)
        let now = Utc::now();
        let event = OutboxEventInsert::for_worker(
            worker_id.0,
            "WorkerEphemeralTerminating".to_string(),
            serde_json::json!({
                "worker_id": worker_id.0.to_string(),
                "provider_id": provider_id.0.to_string(),
                "reason": "JOB_COMPLETED",
                "job_id": job_id.0.to_string()
            }),
            Some(serde_json::json!({
                "source": "JobExecutionService",
                "cleanup_type": "immediate",
                "event": "job_completion"
            })),
            Some(format!(
                "ephemeral-terminating-{}-{}",
                worker_id.0,
                now.timestamp()
            )),
        );

        // Emit the event
        if let Some(ref outbox_repo) = self.outbox_repository {
            if let Err(e) = outbox_repo.insert_events(&[event]).await {
                warn!("Failed to insert WorkerEphemeralTerminating event: {:?}", e);
            }
        }

        // Also emit to event bus if available
        if let Some(ref event_bus) = self.event_bus {
            let domain_event = DomainEvent::WorkerEphemeralTerminating {
                worker_id: worker_id.clone(),
                provider_id: provider_id.clone(),
                reason: TerminationReason::Unregistered,
                occurred_at: now,
                correlation_id: Some(job_id.to_string()),
                actor: Some("job-execution-service".to_string()),
            };
            if let Err(e) = event_bus.publish(&domain_event).await {
                warn!("Failed to publish WorkerEphemeralTerminating event: {}", e);
            }
        }

        // Update worker state to Terminated (Crash-Only Design)
        if let Err(e) = self
            .worker_registry
            .update_state(
                worker_id,
                hodei_server_domain::shared_kernel::WorkerState::Terminated,
            )
            .await
        {
            warn!("Failed to update worker {} state: {}", worker_id, e);
        }

        // Emit WorkerStateUpdated event (EPIC-26 US-26.8)
        let state_change_event = OutboxEventInsert::for_worker(
            worker_id.0,
            "WorkerStateUpdated".to_string(),
            serde_json::json!({
                "worker_id": worker_id.0.to_string(),
                "provider_id": provider_id.0.to_string(),
                "old_state": format!("{:?}", worker_state),
                "new_state": "Terminated",
                "current_job_id": job_id.0.to_string(),
                "last_heartbeat": now.to_rfc3339(),
                "transition_reason": "job_completion"
            }),
            Some(serde_json::json!({
                "source": "JobExecutionService",
                "event": "job_completion_cleanup"
            })),
            Some(format!(
                "worker-state-updated-{}-{}",
                worker_id.0,
                now.timestamp()
            )),
        );

        if let Some(ref outbox_repo) = self.outbox_repository {
            if let Err(e) = outbox_repo.insert_events(&[state_change_event]).await {
                warn!("Failed to insert WorkerStateUpdated event: {:?}", e);
            }
        }

        // Destroy worker via provider immediately (FIX: Only unregister on success)
        let destroy_result = if let Some(provider) = self.providers.get(&provider_id) {
            provider
                .value()
                .destroy_worker(worker.handle())
                .await
                .map_err(|e| {
                    warn!(
                        "Failed to destroy worker {} via provider {}: {}",
                        worker_id, provider_id, e
                    );
                    e
                })
        } else {
            warn!(
                "Provider {} not found for worker {} cleanup",
                provider_id, worker_id
            );
            Err(ProviderError::Internal("Provider not found".to_string()))
        };

        // Only unregister worker if destroy was successful (prevents orphaned containers)
        match destroy_result {
            Ok(_) => {
                info!(
                    "Worker {} destroyed successfully after job {} completion",
                    worker_id, job_id
                );
                // Unregister worker from registry
                if let Err(e) = self.worker_registry.unregister(worker_id).await {
                    warn!("Failed to unregister worker {}: {}", worker_id, e);
                }
            }
            Err(e) => {
                // BUG FIX: Don't unregister if destroy failed - keep worker for retry
                warn!(
                    "Worker {} cleanup FAILED: {}. Worker kept in Terminating state for retry.",
                    worker_id, e
                );

                // Emit ProviderCleanupFailed event (new event for tracking)
                let error_event = OutboxEventInsert::for_worker(
                    worker_id.0,
                    "ProviderCleanupFailed".to_string(),
                    serde_json::json!({
                        "worker_id": worker_id.0.to_string(),
                        "provider_id": provider_id.0.to_string(),
                        "job_id": job_id.0.to_string(),
                        "error": e.to_string(),
                        "cleanup_type": "immediate",
                        "retry_required": true
                    }),
                    Some(serde_json::json!({
                        "source": "JobExecutionService",
                        "event": "cleanup_failed"
                    })),
                    Some(format!(
                        "cleanup-failed-{}-{}",
                        worker_id.0,
                        now.timestamp()
                    )),
                );
                if let Some(ref outbox_repo) = self.outbox_repository {
                    let _ = outbox_repo.insert_events(&[error_event]).await;
                }
                // Worker remains in Terminating state for periodic cleanup retry
                return Ok(());
            }
        }

        // Emit WorkerEphemeralTerminated event (EPIC-26 US-26.3)
        let terminated_event = OutboxEventInsert::for_worker(
            worker_id.0,
            "WorkerEphemeralTerminated".to_string(),
            serde_json::json!({
                "worker_id": worker_id.0.to_string(),
                "provider_id": provider_id.0.to_string(),
                "cleanup_scheduled": false,
                "job_id": job_id.0.to_string()
            }),
            Some(serde_json::json!({
                "source": "JobExecutionService",
                "cleanup_type": "immediate",
                "event": "job_completion_complete"
            })),
            Some(format!(
                "ephemeral-terminated-{}-{}",
                worker_id.0,
                now.timestamp()
            )),
        );

        if let Some(ref outbox_repo) = self.outbox_repository {
            if let Err(e) = outbox_repo.insert_events(&[terminated_event]).await {
                warn!("Failed to insert WorkerEphemeralTerminated event: {:?}", e);
            }
        }

        Ok(())
    }
}

#[tonic::async_trait]
impl JobExecutionService for JobExecutionServiceImpl {
    async fn queue_job(
        &self,
        request: Request<QueueJobRequest>,
    ) -> Result<Response<QueueJobResponse>, Status> {
        tracing::info!("=== QUEUE_JOB HANDLER CALLED ===");
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
                // Extract scheduling preferences (BUG FIX: provider selection)
                preferred_provider: definition
                    .scheduling
                    .as_ref()
                    .map(|s| s.preferred_provider.clone())
                    .filter(|p| !p.is_empty()),
                preferred_region: definition
                    .scheduling
                    .as_ref()
                    .map(|s| s.preferred_region.clone())
                    .filter(|r| !r.is_empty()),
                required_labels: definition
                    .scheduling
                    .as_ref()
                    .map(|s| s.required_labels.clone())
                    .filter(|m| !m.is_empty()),
                required_annotations: definition
                    .scheduling
                    .as_ref()
                    .map(|s| s.required_annotations.clone())
                    .filter(|m| !m.is_empty()),
            },
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
            job_id,
        };

        let result = self
            .create_job_usecase
            .execute(create_request)
            .await
            .map_err(error_to_status)?;

        info!("Queued job via gRPC: {}", result.job_id);

        // EPIC-29: Publish JobQueued event for reactive job processing (saga trigger)
        // This enables the JobDispatcher to handle jobs via the event-driven architecture
        if let Some(ref event_bus) = self.event_bus {
            // Fetch the created job to get the full spec
            let job_id = hodei_server_domain::shared_kernel::JobId(
                Uuid::parse_str(&result.job_id).map_err(|_| {
                    error_to_status(
                        hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                            message: format!("Invalid job ID format: {}", result.job_id),
                        },
                    )
                })?,
            );

            if let Ok(Some(job)) = self.job_repository.find_by_id(&job_id).await {
                // Convert preferred_provider string to ProviderId
                let preferred_provider_id = job
                    .spec
                    .preferences
                    .preferred_provider
                    .as_ref()
                    .and_then(|s| {
                        Uuid::parse_str(s)
                            .ok()
                            .map(hodei_server_domain::shared_kernel::ProviderId::from_uuid)
                    });

                let job_queued_event = DomainEvent::JobQueued {
                    job_id: job.id.clone(),
                    preferred_provider: preferred_provider_id,
                    job_requirements: job.spec.clone(),
                    queued_at: chrono::Utc::now(),
                    correlation_id: correlation_id.or(Some("grpc:job_execution".to_string())),
                    actor: actor.map(|a| format!("grpc:{}", a)),
                };

                if let Err(e) = event_bus.publish(&job_queued_event).await {
                    error!("Failed to publish JobQueued event: {}", e);
                } else {
                    info!("Published JobQueued event for job {}", job.id);
                }
            }
        }

        Ok(Response::new(QueueJobResponse {
            success: true,
            message: format!("Job queued successfully"),
            queued_at: Some(now_timestamp()),
            job_id: Some(GrpcJobId {
                value: result.job_id.to_string(),
            }),
        }))
    }

    async fn assign_job(
        &self,
        request: Request<AssignJobRequest>,
    ) -> Result<Response<AssignJobResponse>, Status> {
        let req = request.into_inner();

        let job_id = parse_grpc_job_id(req.job_id)?;
        let worker_uuid = req
            .worker_id
            .map(|w| w.value)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| Status::invalid_argument("worker_id is required"))?;
        let worker_id = hodei_server_domain::shared_kernel::WorkerId(
            Uuid::parse_str(&worker_uuid)
                .map_err(|_| Status::invalid_argument("worker_id must be a UUID"))?,
        );

        let worker = self
            .worker_registry
            .get(&worker_id)
            .await
            .map_err(error_to_status)?
            .ok_or_else(|| Status::not_found("Worker not found"))?;

        let provider_id = worker.handle().provider_id.clone();
        let provider_execution_id = Uuid::new_v4().to_string();

        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(error_to_status)?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        let context = hodei_server_domain::jobs::ExecutionContext::new(
            job_id.clone(),
            provider_id.clone(),
            provider_execution_id.clone(),
        );
        job.submit_to_provider(provider_id, context)
            .map_err(error_to_status)?;

        self.worker_registry
            .assign_to_job(&worker_id, job_id.clone())
            .await
            .map_err(error_to_status)?;

        self.job_repository
            .update(&job)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(AssignJobResponse {
            success: true,
            message: "Assigned".to_string(),
            execution_id: Some(ExecutionId {
                value: provider_execution_id,
            }),
            assigned_at: Some(now_timestamp()),
        }))
    }

    async fn start_job(
        &self,
        request: Request<StartJobRequest>,
    ) -> Result<Response<StartJobResponse>, Status> {
        let req = request.into_inner();

        let job_id = parse_grpc_job_id(req.job_id)?;
        let execution_id = req
            .execution_id
            .map(|e| e.value)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| Status::invalid_argument("execution_id is required"))?;

        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(error_to_status)?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        if job
            .execution_context()
            .as_ref()
            .map(|c| c.provider_execution_id.as_str())
            != Some(execution_id.as_str())
        {
            return Err(Status::failed_precondition(
                "execution_id does not match job execution context",
            ));
        }

        job.mark_running().map_err(error_to_status)?;
        self.job_repository
            .update(&job)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(StartJobResponse {
            success: true,
            message: "Started".to_string(),
            started_at: Some(now_timestamp()),
        }))
    }

    async fn update_progress(
        &self,
        request: Request<UpdateProgressRequest>,
    ) -> Result<Response<UpdateProgressResponse>, Status> {
        let req = request.into_inner();
        let job_id = parse_grpc_job_id(req.job_id)?;
        let execution_id = req
            .execution_id
            .map(|e| e.value)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| Status::invalid_argument("execution_id is required"))?;

        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(error_to_status)?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        if job
            .execution_context()
            .as_ref()
            .map(|c| c.provider_execution_id.as_str())
            != Some(execution_id.as_str())
        {
            return Err(Status::failed_precondition(
                "execution_id does not match job execution context",
            ));
        }

        job.metadata_mut().insert(
            "progress_percentage".to_string(),
            req.progress_percentage.to_string(),
        );
        if !req.current_stage.is_empty() {
            job.metadata_mut()
                .insert("current_stage".to_string(), req.current_stage);
        }
        if !req.message.is_empty() {
            job.metadata_mut()
                .insert("progress_message".to_string(), req.message);
        }

        self.job_repository
            .update(&job)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(UpdateProgressResponse {
            success: true,
            message: "Updated".to_string(),
            updated_at: Some(now_timestamp()),
        }))
    }

    async fn complete_job(
        &self,
        request: Request<CompleteJobRequest>,
    ) -> Result<Response<CompleteJobResponse>, Status> {
        let req = request.into_inner();
        let job_id = parse_grpc_job_id(req.job_id)?;
        let execution_id = req
            .execution_id
            .map(|e| e.value)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| Status::invalid_argument("execution_id is required"))?;

        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(error_to_status)?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        if job
            .execution_context()
            .as_ref()
            .map(|c| c.provider_execution_id.as_str())
            != Some(execution_id.as_str())
        {
            return Err(Status::failed_precondition(
                "execution_id does not match job execution context",
            ));
        }

        let exit_code: i32 = req.exit_code.parse().unwrap_or_default();
        job.complete(hodei_server_domain::shared_kernel::JobResult::Success {
            exit_code,
            output: req.output,
            error_output: req.error_output,
        })
        .map_err(error_to_status)?;

        self.job_repository
            .update(&job)
            .await
            .map_err(error_to_status)?;

        // EPIC-32: Trigger REACTIVE worker cleanup via event publication
        // Instead of tokio::spawn (which can fail silently), we publish a
        // WorkerEphemeralTerminating event to the outbox. The OutboxRelay
        // will convert it to a DomainEvent, and WorkerLifecycleManager
        // will consume it reactively with retry guarantees.
        //
        // FIX: The worker may already be TERMINATED (with current_job_id = NULL)
        // by the time we reach here because the worker aggregate's complete_job()
        // method was called earlier. We need to find workers that recently
        // transitioned to TERMINATED state.
        let job_id_clone = job_id.clone();

        // First, try to find worker with current_job_id == job_id
        let worker_opt = if let Ok(workers) = self
            .worker_registry
            .find(&hodei_server_domain::workers::WorkerFilter::new())
            .await
        {
            workers
                .into_iter()
                .find(|w| w.current_job_id().map_or(false, |jid| jid == &job_id_clone))
        } else {
            None
        };

        // FIX: If no worker found with current_job_id, search for TERMINATED workers
        // that may have just completed this job
        let worker = if worker_opt.is_none() {
            info!(
                "No worker found with current_job_id={}, searching for recently terminated workers",
                job_id_clone
            );

            // Search for workers in TERMINATED state (they may have just completed)
            if let Ok(terminated_workers) = self
                .worker_registry
                .find(
                    &hodei_server_domain::workers::WorkerFilter::new()
                        .with_state(hodei_server_domain::shared_kernel::WorkerState::Terminated),
                )
                .await
            {
                // Find worker that executed this job by checking if termination
                // happened around the same time as job completion
                let job_completion_time = Utc::now(); // Approximate

                terminated_workers.into_iter().find(|w| {
                    // Check if worker was recently terminated (within 60 seconds)
                    let updated_at = w.updated_at();
                    let elapsed = job_completion_time.signed_duration_since(updated_at);
                    elapsed.num_seconds() < 60
                })
            } else {
                None
            }
        } else {
            worker_opt
        };

        if let Some(worker) = worker {
            let worker_id = worker.id().clone();
            let provider_id = worker.provider_id().clone();
            let now = Utc::now();

            // Publish WorkerEphemeralTerminating event to outbox (reactive cleanup)
            let cleanup_event = OutboxEventInsert::for_worker(
                worker_id.0,
                "WorkerEphemeralTerminating".to_string(),
                serde_json::json!({
                    "worker_id": worker_id.0.to_string(),
                    "provider_id": provider_id.0.to_string(),
                    "reason": "JOB_COMPLETED",
                    "job_id": job_id.0.to_string()
                }),
                Some(serde_json::json!({
                    "source": "JobExecutionService",
                    "cleanup_type": "reactive_event",
                    "event": "complete_job"
                })),
                Some(format!(
                    "ephemeral-terminating-{}-{}",
                    worker_id.0,
                    now.timestamp()
                )),
            );

            if let Some(ref outbox_repo) = self.outbox_repository {
                if let Err(e) = outbox_repo.insert_events(&[cleanup_event]).await {
                    warn!(
                        "Failed to insert WorkerEphemeralTerminating event for worker {}: {}",
                        worker_id, e
                    );
                } else {
                    info!(
                        "📤 Published WorkerEphemeralTerminating event for reactive cleanup (worker={}, job={})",
                        worker_id, job_id
                    );
                }
            }
        } else {
            debug!(
                "No worker found for job {} during cleanup event publication",
                job_id_clone
            );
        }

        Ok(Response::new(CompleteJobResponse {
            success: true,
            message: "Completed".to_string(),
            completed_at: Some(now_timestamp()),
        }))
    }

    async fn fail_job(
        &self,
        request: Request<FailJobRequest>,
    ) -> Result<Response<FailJobResponse>, Status> {
        let req = request.into_inner();
        let job_id = parse_grpc_job_id(req.job_id)?;
        let execution_id = req
            .execution_id
            .map(|e| e.value)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| Status::invalid_argument("execution_id is required"))?;

        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(error_to_status)?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        if job
            .execution_context()
            .as_ref()
            .map(|c| c.provider_execution_id.as_str())
            != Some(execution_id.as_str())
        {
            return Err(Status::failed_precondition(
                "execution_id does not match job execution context",
            ));
        }

        job.fail(req.error_message).map_err(error_to_status)?;
        self.job_repository
            .update(&job)
            .await
            .map_err(error_to_status)?;

        // EPIC-32: Trigger REACTIVE worker cleanup via event publication
        // Same pattern as complete_job - publish event instead of spawning task
        let job_id_clone = job_id.clone();
        if let Ok(workers) = self
            .worker_registry
            .find(&hodei_server_domain::workers::WorkerFilter::new())
            .await
        {
            let worker = workers
                .into_iter()
                .find(|w| w.current_job_id().map_or(false, |jid| jid == &job_id_clone));

            if let Some(worker) = worker {
                let worker_id = worker.id().clone();
                let provider_id = worker.provider_id().clone();
                let now = Utc::now();

                // Publish WorkerEphemeralTerminating event to outbox (reactive cleanup)
                let cleanup_event = OutboxEventInsert::for_worker(
                    worker_id.0,
                    "WorkerEphemeralTerminating".to_string(),
                    serde_json::json!({
                        "worker_id": worker_id.0.to_string(),
                        "provider_id": provider_id.0.to_string(),
                        "reason": "JOB_FAILED",
                        "job_id": job_id.0.to_string()
                    }),
                    Some(serde_json::json!({
                        "source": "JobExecutionService",
                        "cleanup_type": "reactive_event",
                        "event": "fail_job"
                    })),
                    Some(format!(
                        "ephemeral-terminating-{}-{}",
                        worker_id.0,
                        now.timestamp()
                    )),
                );

                if let Some(ref outbox_repo) = self.outbox_repository {
                    if let Err(e) = outbox_repo.insert_events(&[cleanup_event]).await {
                        warn!(
                            "Failed to insert WorkerEphemeralTerminating event for worker {}: {}",
                            worker_id, e
                        );
                    } else {
                        info!(
                            "📤 Published WorkerEphemeralTerminating event for reactive cleanup (worker={}, job={})",
                            worker_id, job_id
                        );
                    }
                }
            } else {
                debug!(
                    "No worker found for job {} during cleanup event publication",
                    job_id_clone
                );
            }
        } else {
            debug!(
                "Failed to query workers for job {} cleanup event",
                job_id_clone
            );
        }

        Ok(Response::new(FailJobResponse {
            success: true,
            message: "Failed".to_string(),
            failed_at: Some(now_timestamp()),
        }))
    }

    async fn cancel_job(
        &self,
        request: Request<CancelJobRequest>,
    ) -> Result<Response<CancelJobResponse>, Status> {
        let req = request.into_inner();
        let job_id = parse_grpc_job_id(req.job_id)?;

        let result = self
            .cancel_job_usecase
            .execute(job_id)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(CancelJobResponse {
            success: true,
            message: result.message,
            cancelled_at: Some(now_timestamp()),
        }))
    }

    async fn get_job(
        &self,
        request: Request<GetJobRequest>,
    ) -> Result<Response<GetJobResponse>, Status> {
        let req = request.into_inner();
        let job_id = parse_grpc_job_id(req.job_id)?;

        let job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(error_to_status)?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        let definition = map_job_to_definition(&job);
        let status = map_job_state(job.state());

        // Create execution if exists
        let mut executions = Vec::new();
        if let Some(ctx) = job.execution_context() {
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
                start_time: ctx.started_at.map(to_timestamp),
                end_time: ctx.completed_at.map(to_timestamp),
                retry_count: job.attempts() as i32,
                exit_code: job
                    .result()
                    .as_ref()
                    .map(|r| match r {
                        hodei_server_domain::shared_kernel::JobResult::Success {
                            exit_code,
                            ..
                        } => exit_code.to_string(),
                        hodei_server_domain::shared_kernel::JobResult::Failed {
                            exit_code, ..
                        } => exit_code.to_string(),
                        _ => "0".to_string(),
                    })
                    .unwrap_or_default(),
                error_message: job.error_message().cloned().unwrap_or_default(),
                metadata: job.metadata().clone(),
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
            .map_err(error_to_status)?;

        let job_summaries = jobs
            .into_iter()
            .map(|job| map_job_to_summary(&job))
            .collect();

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
                        let status = map_job_state(job.state()) as i32;

                        // Check if we should send an update (simple change detection)
                        // In a real implementation, we might want to check more fields or use a proper event bus
                        if status != last_status {
                            last_status = status;

                            if let Some(ctx) = job.execution_context() {
                                let execution = JobExecution {
                                    execution_id: Some(ExecutionId { value: ctx.provider_execution_id.clone() }),
                                    job_id: Some(GrpcJobId { value: job.id.to_string() }),
                                    worker_id: None,
                                    state: 0,
                                    job_status: status,
                                    start_time: ctx.started_at.map(to_timestamp),
                                    end_time: ctx.completed_at.map(to_timestamp),
                                    retry_count: job.attempts() as i32,
                                    exit_code: job.result().as_ref().map(|r| match r {
                                        hodei_server_domain::shared_kernel::JobResult::Success { exit_code, .. } => exit_code.to_string(),
                                        hodei_server_domain::shared_kernel::JobResult::Failed { exit_code, .. } => exit_code.to_string(),
                                        _ => "0".to_string(),
                                    }).unwrap_or_default(),
                                    error_message: job.error_message().cloned().unwrap_or_default(),
                                    metadata: job.metadata().clone(),
                                };

                                if tx.send(Ok(execution)).await.is_err() {
                                    break; // Client disconnected
                                }
                            }
                        }

                        // Stop streaming if job is in a terminal state
                        if matches!(
                            job.state(),
                            hodei_server_domain::shared_kernel::JobState::Succeeded
                                | hodei_server_domain::shared_kernel::JobState::Failed
                                | hodei_server_domain::shared_kernel::JobState::Cancelled
                                | hodei_server_domain::shared_kernel::JobState::Timeout
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

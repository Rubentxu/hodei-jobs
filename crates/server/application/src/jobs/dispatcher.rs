//! Job Dispatcher Component
//!
//! Responsible for dispatching jobs to workers and managing the job lifecycle.
//! Follows Single Responsibility Principle: only handles job dispatching logic.

use crate::providers::ProviderRegistry;
use crate::scheduling::smart_scheduler::SchedulingService;
use crate::workers::commands::WorkerCommandSender;
use crate::workers::provisioning::WorkerProvisioningService;
use chrono::Utc;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::{DomainEvent, EventMetadata};
use hodei_server_domain::jobs::{ExecutionContext, Job, JobQueue, JobRepository};
use hodei_server_domain::scheduling::{
    ProviderInfo, SchedulerConfig, SchedulingContext, SchedulingDecision,
};
use hodei_server_domain::shared_kernel::{DomainError, Result, WorkerId};
use hodei_server_domain::workers::{Worker, WorkerRegistry};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Job Dispatcher
///
/// Handles the dispatching of jobs to workers, including:
/// - Querying available workers and providers
/// - Making scheduling decisions
/// - Assigning jobs to workers
/// - Sending commands to workers via gRPC
/// - Publishing domain events
pub struct JobDispatcher {
    job_queue: Arc<dyn JobQueue>,
    job_repository: Arc<dyn JobRepository>,
    worker_registry: Arc<dyn WorkerRegistry>,
    provider_registry: Arc<ProviderRegistry>,
    scheduler: SchedulingService,
    worker_command_sender: Arc<dyn WorkerCommandSender>,
    event_bus: Arc<dyn EventBus>,
    provisioning_service: Option<Arc<dyn WorkerProvisioningService>>,
}

impl JobDispatcher {
    /// Create a new JobDispatcher
    pub fn new(
        job_queue: Arc<dyn JobQueue>,
        job_repository: Arc<dyn JobRepository>,
        worker_registry: Arc<dyn WorkerRegistry>,
        provider_registry: Arc<ProviderRegistry>,
        scheduler_config: SchedulerConfig,
        worker_command_sender: Arc<dyn WorkerCommandSender>,
        event_bus: Arc<dyn EventBus>,
        provisioning_service: Option<Arc<dyn WorkerProvisioningService>>,
    ) -> Self {
        Self {
            job_queue,
            job_repository,
            worker_registry,
            provider_registry,
            scheduler: SchedulingService::new(scheduler_config),
            worker_command_sender,
            event_bus,
            provisioning_service,
        }
    }

    /// Execute one dispatch cycle
    /// Returns the number of jobs dispatched
    pub async fn dispatch_once(&self) -> Result<usize> {
        info!("🔄 JobDispatcher: Starting dispatch cycle");

        // Step 1: Query and filter available workers
        info!("🔍 JobDispatcher: Querying available workers...");
        let available_workers = self.get_available_workers().await?;

        info!(
            "📊 JobDispatcher: Found {} available workers",
            available_workers.len()
        );

        if available_workers.is_empty() {
            info!("⚠️ JobDispatcher: No available workers");

            // Try to provision a worker if provisioning service is available
            if self.provisioning_service.is_some() {
                info!("🔧 JobDispatcher: No workers available, attempting to provision...");
                if let Err(e) = self.trigger_provisioning().await {
                    error!("❌ JobDispatcher: Failed to provision worker: {}", e);
                } else {
                    info!("✅ JobDispatcher: Provisioning triggered successfully");
                }
            } else {
                info!("⚠️ JobDispatcher: No provisioning service available");
            }

            return Ok(0);
        }

        // Step 2: Dequeue a job from the queue
        info!("📥 JobDispatcher: Dequeuing job from queue...");
        let Some(mut job) = self.job_queue.dequeue().await? else {
            info!("ℹ️ JobDispatcher: No jobs in queue");
            return Ok(0);
        };

        info!("📦 JobDispatcher: Dequeued job {} from queue", job.id);

        // Step 3: Get available providers
        let available_providers = self.get_available_providers().await?;

        // Step 4: Create scheduling context
        let queue_len = self.job_queue.len().await?;
        let ctx = SchedulingContext {
            job: job.clone(),
            available_workers: available_workers.clone(),
            available_providers,
            pending_jobs_count: queue_len,
            system_load: 0.0, // TODO: Calculate actual system load
        };

        // Step 5: Make scheduling decision
        let decision = self.scheduler.make_decision(ctx).await?;

        // Step 6: Execute scheduling decision
        match decision {
            SchedulingDecision::AssignToWorker { worker_id, .. } => {
                debug!(
                    "JobDispatcher: Assigning job {} to worker {}",
                    job.id, worker_id
                );

                // Assign and dispatch the job
                if let Err(e) = self.assign_and_dispatch(&mut job, &worker_id).await {
                    error!(
                        error = %e,
                        job_id = %job.id,
                        worker_id = %worker_id,
                        phase = "dispatch",
                        "❌ JobDispatcher: assign_and_dispatch failed"
                    );
                    // Job is already removed from queue and in ASSIGNED state
                    // It will timeout and be recovered by the coordinator
                    info!(
                        job_id = %job.id,
                        state = "ASSIGNED",
                        action = "recovery_wait",
                        "🔄 JobDispatcher: Job remains in ASSIGNED state, will timeout for recovery"
                    );
                    return Err(e);
                }

                info!(
                    job_id = %job.id,
                    worker_id = %worker_id,
                    "✅ JobDispatcher: Job dispatched successfully"
                );
                Ok(1)
            }
            decision => {
                debug!(
                    decision = ?decision,
                    job_id = %job.id,
                    "JobDispatcher: Scheduling decision made, re-enqueueing job"
                );
                // Re-enqueue job for later processing
                self.job_queue.enqueue(job).await?;
                Ok(0)
            }
        }
    }

    /// Get available workers (filtered by heartbeat)
    async fn get_available_workers(&self) -> Result<Vec<Worker>> {
        // Query all workers from registry
        debug!("🔍 JobDispatcher::get_available_workers: Querying all workers from registry...");
        let all_workers = self.worker_registry.find_available().await.map_err(|e| {
            error!(
                error = %e,
                "JobDispatcher::get_available_workers: Failed to query workers"
            );
            e
        })?;

        debug!(
            workers_count = all_workers.len(),
            "📊 JobDispatcher::get_available_workers: Found total workers in registry"
        );

        // Log each worker for debugging
        for (i, worker) in all_workers.iter().enumerate() {
            let now = chrono::Utc::now();
            let heartbeat_age = now
                .signed_duration_since(worker.last_heartbeat())
                .to_std()
                .unwrap_or(std::time::Duration::MAX);

            // Structured log for worker status
            debug!(
                index = i,
                worker_id = %worker.id(),
                state = ?worker.state(),
                heartbeat_age_sec = heartbeat_age.as_secs(),
                "🔍 Worker status check"
            );
        }

        // Filter workers by active gRPC connection using heartbeat as proxy
        let all_workers_clone = all_workers.clone();
        let connected_workers: Vec<_> = all_workers_clone
            .into_iter()
            .filter(|worker| {
                let now = chrono::Utc::now();
                let heartbeat_age = now
                    .signed_duration_since(worker.last_heartbeat())
                    .to_std()
                    .unwrap_or(std::time::Duration::MAX);

                let is_connected = heartbeat_age < std::time::Duration::from_secs(30);

                if !is_connected {
                    info!(
                        worker_id = %worker.id(),
                        heartbeat_age_sec = heartbeat_age.as_secs(),
                        threshold = 30,
                        "❌ JobDispatcher::get_available_workers: Worker EXCLUDED"
                    );
                } else {
                    debug!(
                        worker_id = %worker.id(),
                        heartbeat_age_sec = heartbeat_age.as_secs(),
                        "✅ JobDispatcher::get_available_workers: Worker INCLUDED"
                    );
                }

                is_connected
            })
            .collect();

        info!(
            connected_count = connected_workers.len(),
            total_count = all_workers.len(),
            "✅ JobDispatcher::get_available_workers: Final count"
        );
        Ok(connected_workers)
    }

    /// Get available providers with capacity
    async fn get_available_providers(&self) -> Result<Vec<ProviderInfo>> {
        let available_providers = self
            .provider_registry
            .list_providers_with_capacity()
            .await
            .unwrap_or_default();

        debug!(
            providers_count = available_providers.len(),
            "JobDispatcher: Found providers with capacity"
        );

        // Convert ProviderConfig to ProviderInfo for scheduler
        let providers_info: Vec<ProviderInfo> = available_providers
            .iter()
            .map(|p| ProviderInfo {
                provider_id: p.id.clone(),
                provider_type: p.provider_type.clone(),
                active_workers: p.active_workers as usize,
                max_workers: p.max_workers as usize,
                estimated_startup_time: std::time::Duration::from_secs(5),
                health_score: 0.9,
                cost_per_hour: 0.0,
            })
            .collect();

        Ok(providers_info)
    }

    /// Assign job to worker and dispatch
    /// Implements safe operation order: gRPC → Events → DB
    async fn assign_and_dispatch(&self, job: &mut Job, worker_id: &WorkerId) -> Result<()> {
        info!(
            job_id = %job.id,
            worker_id = %worker_id,
            "🔄 JobDispatcher: Starting assign_and_dispatch"
        );

        // Step 1: Get worker details
        let worker = self.worker_registry.get(worker_id).await?.ok_or_else(|| {
            DomainError::WorkerNotFound {
                worker_id: worker_id.clone(),
            }
        })?;

        debug!(worker_id = %worker_id, "JobDispatcher: Found worker");

        // Step 2: Create execution context and store provider assignment
        let provider_id = worker.handle().provider_id.clone();
        let context = ExecutionContext::new(
            job.id.clone(),
            provider_id.clone(),
            format!("exec-{}", Uuid::new_v4()),
        );

        // Assign provider to job (for both PENDING and ASSIGNED states)
        // ASSIGNED state comes from atomic dequeue, but provider still needs to be assigned
        if job.selected_provider().is_none() {
            job.assign_to_provider(provider_id.clone(), context)?;
            info!(
                provider_id = %provider_id,
                job_id = %job.id,
                "📌 JobDispatcher: Assigned provider to job"
            );
        }

        // Step 3: Update job in repository (BEFORE gRPC to avoid race condition)
        // Persist assignment state so it's safe for worker to update to RUNNING later
        info!(
            job_id = %job.id,
            "💾 JobDispatcher: Updating job in repository (pre-dispatch)"
        );
        if let Err(e) = self.job_repository.update(job).await {
            error!(
                error = %e,
                job_id = %job.id,
                "❌ JobDispatcher: Failed to update job"
            );
            return Err(DomainError::InfrastructureError {
                message: format!("Failed to persist job before dispatch: {}", e),
            });
        }

        // Step 4: Send RUN_JOB command to worker via gRPC
        info!(
            worker_id = %worker_id,
            job_id = %job.id,
            "📡 JobDispatcher: Sending RUN_JOB command to worker"
        );
        if let Err(e) = self
            .worker_command_sender
            .send_run_job(&worker_id, job)
            .await
        {
            error!(
                error = %e,
                worker_id = %worker_id,
                job_id = %job.id,
                "❌ JobDispatcher: Failed to send RUN_JOB"
            );
            // Note: Job is already persisted as ASSIGNED.
            // It will eventually timeout if worker doesn't pick it up, which is acceptable.
            return Err(DomainError::InfrastructureError {
                message: format!("Failed to dispatch job to worker {}: {}", worker_id, e),
            });
        }

        info!(
            worker_id = %worker_id,
            job_id = %job.id,
            "✅ JobDispatcher: RUN_JOB command sent successfully"
        );

        // Step 5: Publish JobAssigned event
        // Refactoring: Use EventMetadata to reduce Connascence of Algorithm
        let metadata = EventMetadata::from_job_metadata(job.metadata(), &job.id);

        let assigned_event = DomainEvent::JobAssigned {
            job_id: job.id.clone(),
            worker_id: worker_id.clone(),
            occurred_at: Utc::now(),
            correlation_id: metadata.correlation_id,
            actor: metadata.actor.or(Some("system:job_dispatcher".to_string())),
        };

        if let Err(e) = self.event_bus.publish(&assigned_event).await {
            error!(
                error = %e,
                job_id = %job.id,
                event = "JobAssigned",
                "❌ JobDispatcher: Failed to publish JobAssigned event"
            );
            // Continue anyway, job is already dispatched
        } else {
            debug!(
                job_id = %job.id,
                event = "JobAssigned",
                "📢 JobDispatcher: JobAssigned event published"
            );
        }

        info!(
            "✅ JobDispatcher: Job {} dispatched and persisted successfully",
            job.id
        );

        Ok(())
    }

    /// Trigger worker provisioning when no workers are available
    async fn trigger_provisioning(&self) -> Result<()> {
        info!("🔧 JobDispatcher::trigger_provisioning: Starting");

        if let Some(ref provisioning) = self.provisioning_service {
            info!("✅ JobDispatcher::trigger_provisioning: Provisioning service available");

            // Get enabled providers
            info!("🔍 JobDispatcher::trigger_provisioning: Querying enabled providers...");
            let providers = self.provider_registry.list_enabled_providers().await?;

            info!(
                "📊 JobDispatcher::trigger_provisioning: Found {} enabled providers",
                providers.len()
            );

            if providers.is_empty() {
                warn!(
                    "⚠️ JobDispatcher::trigger_provisioning: No providers available for provisioning"
                );
                return Ok(());
            }

            // Try to provision a worker using the first available provider
            let provider = &providers[0];
            info!(
                "🔧 JobDispatcher::trigger_provisioning: Provisioning worker on provider {} ({})",
                provider.id, provider.name
            );

            // Get the default worker spec for this provider
            info!("📋 JobDispatcher::trigger_provisioning: Getting default worker spec...");
            let spec = provisioning
                .default_worker_spec(&provider.id)
                .ok_or_else(|| {
                    error!(
                        "❌ JobDispatcher::trigger_provisioning: No default spec for provider {}",
                        provider.id
                    );
                    DomainError::ProviderNotFound {
                        provider_id: provider.id.clone(),
                    }
                })?;

            info!(
                "✅ JobDispatcher::trigger_provisioning: Got worker spec: {:?}",
                spec
            );

            info!("🚀 JobDispatcher::trigger_provisioning: Calling provision_worker...");
            match provisioning.provision_worker(&provider.id, spec).await {
                Ok(result) => {
                    info!(
                        "✅ JobDispatcher::trigger_provisioning: Worker provisioned successfully! Worker ID: {}, OTP: {}",
                        result.worker_id, result.otp_token
                    );
                    Ok(())
                }
                Err(e) => {
                    error!(
                        "❌ JobDispatcher::trigger_provisioning: Provisioning failed: {}",
                        e
                    );
                    Err(DomainError::InfrastructureError {
                        message: format!("Failed to provision worker: {}", e),
                    })
                }
            }
        } else {
            info!("⚠️ JobDispatcher::trigger_provisioning: No provisioning service available");
            Ok(())
        }
    }
}

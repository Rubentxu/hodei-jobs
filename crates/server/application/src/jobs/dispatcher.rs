//! Job Dispatcher Component
//!
//! Responsible for dispatching jobs to workers and managing the job lifecycle.
//! Follows Single Responsibility Principle: only handles job dispatching logic.
//!
//! ## Responsabilidades Core (3)
//! 1. **Orquestación del dispatch** - `dispatch_once`:Coordina el ciclo completo
//! 2. **Asignación de jobs** - `assign_and_dispatch`:Asigna y envía jobs a workers
//! 3. **Publicación de eventos** - `publish_job_assigned_event`:Publica eventos de asignación
//!
//! ## Responsabilidades Delegadas
//! - Filtrado de workers → SchedulingService
//! - Selección de providers → ProviderRegistry / SchedulingService
//! - Provisioning → WorkerProvisioningService

use crate::providers::ProviderRegistry;
use crate::scheduling::smart_scheduler::SchedulingService;
use crate::workers::commands::WorkerCommandSender;
use crate::workers::provisioning::WorkerProvisioningService;
use chrono::Utc;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::{DomainEvent, EventMetadata};
use hodei_server_domain::jobs::{ExecutionContext, Job, JobQueue, JobRepository};
use hodei_server_domain::outbox::{OutboxEventInsert, OutboxRepository};
use hodei_server_domain::scheduling::{
    ProviderInfo, SchedulerConfig, SchedulingContext, SchedulingDecision,
};
use hodei_server_domain::shared_kernel::{DomainError, ProviderId, WorkerId};
use hodei_server_domain::workers::health::WorkerHealthService;
use hodei_server_domain::workers::{Worker, WorkerRegistry};
use sqlx::postgres::PgPool;
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
    outbox_repository: Option<
        Arc<dyn OutboxRepository<Error = hodei_server_domain::outbox::OutboxError> + Send + Sync>,
    >,
    provisioning_service: Option<Arc<dyn WorkerProvisioningService>>,
    worker_health_service: Arc<WorkerHealthService>,
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
        outbox_repository: Option<
            Arc<
                dyn OutboxRepository<Error = hodei_server_domain::outbox::OutboxError>
                    + Send
                    + Sync,
            >,
        >,
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
            outbox_repository,
            provisioning_service,
            worker_health_service: Arc::new(WorkerHealthService::builder().build()),
        }
    }

    /// Create a JobDispatcher with outbox repository
    pub fn with_outbox_repository(
        job_queue: Arc<dyn JobQueue>,
        job_repository: Arc<dyn JobRepository>,
        worker_registry: Arc<dyn WorkerRegistry>,
        provider_registry: Arc<ProviderRegistry>,
        scheduler_config: SchedulerConfig,
        worker_command_sender: Arc<dyn WorkerCommandSender>,
        event_bus: Arc<dyn EventBus>,
        outbox_repository: Arc<
            dyn OutboxRepository<Error = hodei_server_domain::outbox::OutboxError> + Send + Sync,
        >,
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
            outbox_repository: Some(outbox_repository),
            provisioning_service,
            worker_health_service: Arc::new(WorkerHealthService::builder().build()),
        }
    }

    /// Execute one dispatch cycle
    /// Returns the number of jobs dispatched
    ///
    /// ## Responsabilidad Core #1: Orquestación
    /// Este método solo orquesta el flujo, delegando los detalles a servicios especializados.
    pub async fn dispatch_once(&self) -> anyhow::Result<usize> {
        info!("🔄 JobDispatcher: Starting dispatch cycle");

        // Step 1: Query and filter available workers (delegado a SchedulingService)
        let available_workers = self.query_healthy_workers().await?;

        info!(
            "📊 JobDispatcher: Found {} available workers",
            available_workers.len()
        );

        if available_workers.is_empty() {
            return self.handle_no_available_workers().await;
        }

        // Step 2: Dequeue a job from the queue
        let Some(mut job) = self.job_queue.dequeue().await? else {
            info!("ℹ️ JobDispatcher: No jobs in queue");
            return Ok(0);
        };

        info!("📦 JobDispatcher: Dequeued job {} from queue", job.id);

        // Step 3: Get scheduling decision (delegado a SchedulingService)
        let decision = self
            .make_scheduling_decision(&job, &available_workers)
            .await?;

        // Step 4: Execute scheduling decision
        match decision {
            SchedulingDecision::AssignToWorker { worker_id, .. } => {
                self.dispatch_job_to_worker(&mut job, &worker_id).await
            }
            decision => {
                debug!(
                    decision = ?decision,
                    job_id = %job.id,
                    "JobDispatcher: Scheduling decision made, re-enqueueing job"
                );
                self.job_queue.enqueue(job).await?;
                Ok(0)
            }
        }
    }

    /// Query healthy workers using SchedulingService
    /// Delegado desde SchedulingService
    async fn query_healthy_workers(&self) -> anyhow::Result<Vec<Worker>> {
        let all_workers = self.worker_registry.find_available().await.map_err(|e| {
            error!(error = %e, "JobDispatcher: Failed to query available workers");
            e
        })?;

        // Filter using WorkerHealthService (connascence reducida)
        let healthy_workers: Vec<_> = all_workers
            .into_iter()
            .filter(|worker| self.worker_health_service.is_healthy(worker))
            .collect();

        debug!(
            healthy_count = healthy_workers.len(),
            "JobDispatcher: Found healthy workers"
        );

        Ok(healthy_workers)
    }

    /// Handle case when no workers are available
    async fn handle_no_available_workers(&self) -> anyhow::Result<usize> {
        info!("⚠️ JobDispatcher: No available workers");

        // Delegar al método trigger_provisioning que maneja internamente
        // el caso de no disponibilidad del servicio
        match self.trigger_provisioning().await {
            Ok(()) => {
                info!("✅ JobDispatcher: Provisioning triggered successfully");
                Ok(0)
            }
            Err(e) => {
                error!("❌ JobDispatcher: Failed to provision worker: {}", e);
                Err(e)
            }
        }
    }

    /// Make scheduling decision using SchedulingService
    async fn make_scheduling_decision(
        &self,
        job: &Job,
        available_workers: &[Worker],
    ) -> anyhow::Result<SchedulingDecision> {
        let providers_info = self.get_providers_info().await?;
        let queue_len = self.job_queue.len().await?;

        let ctx = SchedulingContext {
            job: job.clone(),
            job_preferences: job.spec.preferences.clone(),
            available_workers: available_workers.to_vec(),
            available_providers: providers_info,
            pending_jobs_count: queue_len,
            system_load: 0.0,
        };

        self.scheduler.make_decision(ctx).await
    }

    /// Dispatch job to worker (responsabilidad core #2)
    async fn dispatch_job_to_worker(
        &self,
        job: &mut Job,
        worker_id: &WorkerId,
    ) -> anyhow::Result<usize> {
        debug!(
            "JobDispatcher: Assigning job {} to worker {}",
            job.id, worker_id
        );

        if let Err(e) = self.assign_and_dispatch(job, worker_id).await {
            error!(
                error = %e,
                job_id = %job.id,
                worker_id = %worker_id,
                phase = "dispatch",
                "❌ JobDispatcher: assign_and_dispatch failed"
            );
            info!(
                job_id = %job.id,
                state = "ASSIGNED",
                action = "recovery_wait",
                "🔄 JobDispatcher: Job remains in ASSIGNED state"
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

    /// Get providers info for scheduling (delegado a ProviderRegistry)
    async fn get_providers_info(&self) -> anyhow::Result<Vec<ProviderInfo>> {
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
    ///
    /// ## Responsabilidad Core #2: Asignación de Jobs
    /// Implementa el patrón de operación segura: gRPC → Events → DB
    /// Mantiene compatibilidad con el patrón Transactional Outbox.
    ///
    /// ## Pasos:
    /// 1. Obtener detalles del worker
    /// 2. Asignar provider al job
    /// 3. Persistir estado en BD
    /// 4. Enviar comando RUN_JOB via gRPC
    /// 5. Publicar evento JobAssigned
    async fn assign_and_dispatch(&self, job: &mut Job, worker_id: &WorkerId) -> anyhow::Result<()> {
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
            return Err(anyhow::anyhow!(
                "Failed to persist job before dispatch: {}",
                e
            ));
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
            return Err(anyhow::anyhow!(
                "Failed to dispatch job to worker {}: {}",
                worker_id,
                e
            ));
        }

        info!(
            worker_id = %worker_id,
            job_id = %job.id,
            "✅ JobDispatcher: RUN_JOB command sent successfully"
        );

        // Step 5: Publish JobAssigned event with idempotency key
        // Refactoring: Use EventMetadata to reduce Connascence of Algorithm
        let metadata = EventMetadata::from_job_metadata(job.metadata(), &job.id);

        // Generate idempotency key for JobAssigned to prevent duplicates
        let idempotency_key = format!("job-assigned-{}-{}", job.id.0, worker_id.0);

        // Try to publish via outbox first (if available)
        if let Some(ref outbox_repo) = self.outbox_repository {
            let event = OutboxEventInsert::for_job(
                job.id.0,
                "JobAssigned".to_string(),
                serde_json::json!({
                    "job_id": job.id.0.to_string(),
                    "worker_id": worker_id.0.to_string(),
                    "occurred_at": Utc::now().to_rfc3339()
                }),
                Some(serde_json::json!({
                    "source": "JobDispatcher",
                    "correlation_id": metadata.correlation_id,
                    "actor": metadata.actor.or(Some("system:job_dispatcher".to_string()))
                })),
                Some(idempotency_key),
            );

            if let Err(e) = outbox_repo.insert_events(&[event]).await {
                error!(
                    error = %e,
                    job_id = %job.id,
                    event = "JobAssigned",
                    "❌ JobDispatcher: Failed to insert JobAssigned event into outbox"
                );
                // Continue anyway, job is already dispatched
            } else {
                debug!(
                    job_id = %job.id,
                    event = "JobAssigned",
                    "📢 JobDispatcher: JobAssigned event inserted into outbox"
                );
            }
        } else {
            // Fallback to direct event bus publishing (legacy mode)
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
        }

        info!(
            "✅ JobDispatcher: Job {} dispatched and persisted successfully",
            job.id
        );

        Ok(())
    }

    /// Trigger worker provisioning when no workers are available
    ///
    /// ## Responsabilidad Delegada: Provisioning
    /// Delega la lógica de selección de provider y spec al WorkerProvisioningService.
    async fn trigger_provisioning(&self) -> anyhow::Result<()> {
        let provisioning =
            self.provisioning_service
                .as_ref()
                .ok_or_else(|| DomainError::InfrastructureError {
                    message: "No provisioning service available".to_string(),
                })?;

        // Get enabled providers
        let providers = self.provider_registry.list_enabled_providers().await?;

        if providers.is_empty() {
            warn!("⚠️ JobDispatcher: No providers available for provisioning");
            return Ok(());
        }

        // Select best provider using scheduler
        let provider_id = self.select_provider_for_provisioning(&providers).await?;

        // Get default spec and provision
        let spec = provisioning
            .default_worker_spec(&provider_id)
            .ok_or_else(|| anyhow::anyhow!("No default spec for provider {}", provider_id))?;

        match provisioning.provision_worker(&provider_id, spec).await {
            Ok(result) => {
                info!(
                    "✅ Worker provisioned: id={}, otp={}",
                    result.worker_id, result.otp_token
                );
                Ok(())
            }
            Err(e) => anyhow::bail!("Failed to provision worker: {}", e),
        }
    }

    /// Select provider for provisioning using scheduling strategy
    async fn select_provider_for_provisioning(
        &self,
        providers: &[hodei_server_domain::providers::ProviderConfig],
    ) -> anyhow::Result<ProviderId> {
        // Use scheduler to select best provider
        let providers_info: Vec<_> = providers
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

        let dummy_job = hodei_server_domain::jobs::Job::new(
            hodei_server_domain::shared_kernel::JobId::new(),
            hodei_server_domain::jobs::JobSpec::new(vec!["echo".to_string()]),
        );

        self.scheduler
            .select_provider_with_preferences(&dummy_job, &providers_info)
            .ok_or_else(|| anyhow::anyhow!("No provider found"))
    }

    /// Assign job atomically using Transactional Outbox Pattern
    ///
    /// Wrapper para compatibilidad futura con Transactional Outbox.
    /// Actualmente delega a `assign_and_dispatch`.
    pub async fn assign_job_idempotent(
        &self,
        _pool: &PgPool,
        job_id: &hodei_server_domain::shared_kernel::JobId,
        worker_id: &WorkerId,
        job: &mut Job,
    ) -> anyhow::Result<()> {
        info!(
            job_id = %job_id,
            worker_id = %worker_id,
            "JobDispatcher: Using assign_and_dispatch (idempotent wrapper)"
        );
        self.assign_and_dispatch(job, worker_id).await
    }
}

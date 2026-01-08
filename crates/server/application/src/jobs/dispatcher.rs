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
//! - Orquestación de saga → ExecutionSagaDispatcher

use crate::jobs::job_assignment_service::JobAssignmentService;
use crate::jobs::resource_allocator::ResourceAllocator;
use crate::providers::ProviderRegistry;
use crate::saga::dispatcher_saga::DynExecutionSagaDispatcher;
use crate::saga::provisioning_saga::DynProvisioningSagaCoordinator;
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
use hodei_shared::states::JobState;
use sqlx::postgres::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, instrument, warn};
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
    outbox_repository: Arc<
        dyn OutboxRepository + Send + Sync,
    >,
    provisioning_service: Option<Arc<dyn WorkerProvisioningService>>,
    worker_health_service: Arc<WorkerHealthService>,
    /// EPIC-28: Track recently provisioned jobs to prevent duplicate provisioning
    /// Key: JobId, Value: Instant when provisioning was triggered
    recently_provisioned: Arc<tokio::sync::Mutex<HashMap<Uuid, Instant>>>,
    /// EPIC-28: Time window to skip re-provisioning for same job
    provisioning_cooldown: Duration,
    /// EPIC-29: Saga orchestrator for execution saga coordination
    execution_saga_dispatcher: Option<Arc<DynExecutionSagaDispatcher>>,
    /// EPIC-30: Saga coordinator for provisioning saga (US-30.2)
    provisioning_saga_coordinator: Option<Arc<DynProvisioningSagaCoordinator>>,
    /// GAP-GO-03: Job assignment service for non-saga dispatch
    job_assignment_service: Option<Arc<JobAssignmentService>>,
    /// GAP-GO-04: Resource allocator for provider metrics calculations
    resource_allocator: ResourceAllocator,
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
        outbox_repository: Arc<
                dyn OutboxRepository
                    + Send
                    + Sync,
            >,
        provisioning_service: Option<Arc<dyn WorkerProvisioningService>>,
        execution_saga_dispatcher: Option<Arc<DynExecutionSagaDispatcher>>,
        provisioning_saga_coordinator: Option<Arc<DynProvisioningSagaCoordinator>>,
    ) -> Self {
        // Create JobAssignmentService for non-saga dispatch
        let job_assignment_service = Arc::new(JobAssignmentService::new(
            job_repository.clone(),
            worker_registry.clone(),
            worker_command_sender.clone(),
            event_bus.clone(),
            outbox_repository.clone(),
            None,
        ));

        // Create ResourceAllocator for provider metrics
        let resource_allocator = ResourceAllocator::new(None);

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
            recently_provisioned: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            provisioning_cooldown: Duration::from_secs(30), // 30 seconds cooldown
            execution_saga_dispatcher,
            provisioning_saga_coordinator,
            job_assignment_service: Some(job_assignment_service),
            resource_allocator,
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
            dyn OutboxRepository + Send + Sync,
        >,
        provisioning_service: Option<Arc<dyn WorkerProvisioningService>>,
        execution_saga_dispatcher: Option<Arc<DynExecutionSagaDispatcher>>,
        provisioning_saga_coordinator: Option<Arc<DynProvisioningSagaCoordinator>>,
    ) -> Self {
        // Create JobAssignmentService for non-saga dispatch
        let job_assignment_service = Arc::new(JobAssignmentService::new(
            job_repository.clone(),
            worker_registry.clone(),
            worker_command_sender.clone(),
            event_bus.clone(),
            outbox_repository.clone(),
            None,
        ));

        // Create ResourceAllocator for provider metrics
        let resource_allocator = ResourceAllocator::new(None);

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
            recently_provisioned: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            provisioning_cooldown: Duration::from_secs(30), // 30 seconds cooldown
            execution_saga_dispatcher,
            provisioning_saga_coordinator,
            job_assignment_service: Some(job_assignment_service),
            resource_allocator,
        }
    }

    /// Execute one dispatch cycle
    /// Returns the number of jobs dispatched
    ///
    /// ## Responsabilidad Core #1: Orquestación
    /// Este método solo orquesta el flujo, delegando los detalles a servicios especializados.
    #[instrument(skip(self), fields(job_id), ret)]
    pub async fn dispatch_once(&self) -> anyhow::Result<usize> {
        info!("🔄 JobDispatcher: Starting dispatch cycle");

        // Step 1: Dequeue a job from the queue first (needed for provider selection)
        let Some(mut job) = self.job_queue.dequeue().await? else {
            info!("ℹ️ JobDispatcher: No jobs in queue");
            return Ok(0);
        };

        info!("📦 JobDispatcher: Dequeued job {} from queue", job.id);

        // Step 2: Query and filter available workers (delegado a SchedulingService)
        let available_workers = self.query_healthy_workers().await?;

        info!(
            "📊 JobDispatcher: Found {} available workers",
            available_workers.len()
        );

        // Step 3: If no workers available, trigger provisioning with job preferences
        if available_workers.is_empty() {
            return self.handle_no_available_workers(&job).await;
        }

        // Step 4: Get scheduling decision (delegado a SchedulingService)
        let decision = self
            .make_scheduling_decision(&job, &available_workers)
            .await?;

        // Step 5: Execute scheduling decision
        match decision {
            SchedulingDecision::AssignToWorker { worker_id, .. } => {
                self.dispatch_job_to_worker(&mut job, &worker_id).await
            }
            SchedulingDecision::ProvisionWorker { provider_id, .. } => {
                // Save selected provider_id in job before re-enqueueing
                job.spec.preferences.preferred_provider = Some(provider_id.to_string());
                debug!(
                    job_id = %job.id,
                    provider_id = %provider_id,
                    "JobDispatcher: Saving selected_provider_id and triggering provisioning"
                );

                // CRITICAL FIX: Actually trigger provisioning!
                info!(
                    "🛠️ JobDispatcher: Triggering provisioning for job {}",
                    job.id
                );
                match self.trigger_provisioning(&job).await {
                    Ok(_) => info!(
                        "✅ JobDispatcher: Provisioning triggered successfully for job {}",
                        job.id
                    ),
                    Err(e) => error!("❌ JobDispatcher: Failed to provision worker: {}", e),
                }

                info!(
                    "📥 JobDispatcher: Re-enqueuing job {} after provisioning decision",
                    job.id
                );
                let job_id = job.id.clone();
                if let Err(e) = self.job_queue.enqueue(job).await {
                    error!("❌ JobDispatcher: Failed to re-enqueue job: {}", e);
                    return Err(e.into());
                }
                info!("✅ JobDispatcher: Job {} re-enqueued successfully", job_id);
                Ok(0)
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
    #[instrument(skip(self), fields(job_id = %job.id), ret)]
    async fn handle_no_available_workers(&self, job: &Job) -> anyhow::Result<usize> {
        info!("⚠️ JobDispatcher: No available workers for job {}", job.id);

        // EPIC-28: Check if job was recently provisioned to prevent duplicate provisioning
        let job_uuid = job.id.0;
        let should_provision = {
            let mut recently_provisioned = self.recently_provisioned.lock().await;
            let now = Instant::now();

            // Clean up old entries
            recently_provisioned.retain(|_, &mut timestamp| {
                now.duration_since(timestamp) < self.provisioning_cooldown
            });

            // Check if this job was provisioned recently
            if let Some(&provisioned_at) = recently_provisioned.get(&job_uuid) {
                let elapsed = now.duration_since(provisioned_at);
                if elapsed < self.provisioning_cooldown {
                    info!(
                        "⏭️ JobDispatcher: Skipping provisioning for job {} - {}s since last provisioning",
                        job.id,
                        elapsed.as_secs()
                    );
                    false
                } else {
                    // Entry expired, allow new provisioning
                    recently_provisioned.insert(job_uuid, now);
                    true
                }
            } else {
                // First time provisioning this job
                recently_provisioned.insert(job_uuid, now);
                true
            }
        };

        if !should_provision {
            // Just re-enqueue without provisioning
            info!(
                "📥 JobDispatcher: Re-enqueuing job {} (waiting for worker to connect)",
                job.id
            );
            self.job_queue.enqueue(job.clone()).await.map_err(|e| {
                error!("❌ JobDispatcher: Failed to re-enqueue job: {}", e);
                e
            })?;
            return Ok(0);
        }

        // Delegar al método trigger_provisioning que maneja internamente
        // el caso de no disponibilidad del servicio
        match self.trigger_provisioning(job).await {
            Ok(()) => {
                info!("✅ JobDispatcher: Provisioning triggered successfully");
                // IMPORTANT: Re-enqueue job so it's picked up when worker is ready
                // Without this, the job is stuck in ASSIGNED state and removed from queue!
                info!(
                    "📥 JobDispatcher: Re-enqueuing job {} waiting for workers",
                    job.id
                );
                self.job_queue.enqueue(job.clone()).await.map_err(|e| {
                    error!("❌ JobDispatcher: Failed to re-enqueue job: {}", e);
                    e
                })?;
                Ok(0)
            }
            Err(e) => {
                error!("❌ JobDispatcher: Failed to provision worker: {}", e);
                // Remove from recently_provisioned so we can retry
                {
                    let mut recently_provisioned = self.recently_provisioned.lock().await;
                    recently_provisioned.remove(&job_uuid);
                }
                // Also re-enqueue on failure so we can retry or fail later?
                // For now, re-enqueue to retry provisioning
                self.job_queue.enqueue(job.clone()).await.map_err(|xe| {
                    error!(
                        "❌ JobDispatcher: Failed to re-enqueue job after provisioning error: {}",
                        xe
                    );
                    xe
                })?;
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
    #[instrument(skip(self), fields(job_id = %job.id, worker_id = %worker_id), ret)]
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
    ///
    /// ## US-27.1: Métricas Reales
    /// Este método ahora extrae métricas reales desde ProviderConfig en lugar de valores hardcodeados:
    /// - estimated_startup_time: de capabilities.max_execution_time o cálculo basado en tipo
    /// - health_score: calculado desde capacidades del provider
    /// - cost_per_hour: calculado desde type_config basado en el tipo de provider
    ///
    /// ## GAP-GO-04: Delegado a ResourceAllocator
    /// Los cálculos de métricas están delegados a ResourceAllocator para mantener
    /// Single Responsibility Principle.
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

        // Delegar cálculos de métricas a ResourceAllocator
        let providers_info: Vec<ProviderInfo> = self
            .resource_allocator
            .configs_to_provider_infos_ref(&available_providers);

        Ok(providers_info)
    }

    /// Assign job to worker and dispatch
    ///
    /// ## EPIC-30: Integración con ExecutionSaga
    /// Si el ExecutionSagaDispatcher está disponible, ejecuta el saga en paralelo
    /// para gestionar la asignación, ejecución y completación del job de forma orquestada.
    /// El saga proporciona compensación automática en caso de fallos.
    ///
    /// ## Pasos:
    /// 1. Obtener detalles del worker
    /// 2. (Opcional) Ejecutar ExecutionSaga para asignación y ejecución
    /// 3. Asignar provider al job (fallback/manual)
    /// 4. Persistir estado en BD
    /// 5. Enviar comando RUN_JOB via gRPC (si no usa saga)
    /// 6. Publicar evento JobAssigned
    async fn assign_and_dispatch(&self, job: &mut Job, worker_id: &WorkerId) -> anyhow::Result<()> {
        info!(
            job_id = %job.id,
            worker_id = %worker_id,
            "🔄 JobDispatcher: Starting assign_and_dispatch"
        );

        // EPIC-30 - Use saga-based execution if available
        if let Some(ref dispatcher) = self.execution_saga_dispatcher {
            info!(job_id = %job.id, worker_id = %worker_id, "🚀 JobDispatcher: Starting ExecutionSaga");
            match dispatcher.execute_execution_saga(&job.id, worker_id).await {
                Ok(result) => {
                    info!(job_id = %job.id, saga_status = ?result, "✅ JobDispatcher: ExecutionSaga completed");
                    return Ok(());
                }
                Err(e) => {
                    warn!(job_id = %job.id, error = %e, "⚠️ JobDispatcher: ExecutionSaga failed");
                    return Err(anyhow::anyhow!("ExecutionSaga failed: {}", e));
                }
            }
        }

        // GAP-GO-03: Delegate to JobAssignmentService for non-saga dispatch
        info!(
            job_id = %job.id,
            worker_id = %worker_id,
            "🎯 JobDispatcher: Delegating to JobAssignmentService"
        );

        if let Some(ref assignment_service) = self.job_assignment_service {
            match assignment_service.assign_job(job, worker_id).await {
                Ok(result) => {
                    if result.success {
                        info!(
                            job_id = %job.id,
                            worker_id = %worker_id,
                            duration_ms = result.duration_ms,
                            "✅ JobDispatcher: Job assigned via JobAssignmentService"
                        );
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!(
                            "JobAssignmentService failed: {}",
                            result.error_message.unwrap_or_else(|| "Unknown error".to_string())
                        ))
                    }
                }
                Err(e) => Err(anyhow::anyhow!("JobAssignmentService error: {}", e)),
            }
        } else {
            Err(anyhow::anyhow!("No execution saga dispatcher and no JobAssignmentService available"))
        }
    }

    /// Trigger worker provisioning when no workers are available
    ///
    /// Uses saga-based provisioning when coordinator is available,
    /// otherwise falls back to legacy provisioning.
    #[instrument(skip(self), fields(job_id = %job.id), ret)]
    async fn trigger_provisioning(&self, job: &Job) -> anyhow::Result<()> {
        // Use saga-based provisioning (always enabled when coordinator is present)
        if let Some(ref coordinator) = self.provisioning_saga_coordinator {
            info!(job_id = %job.id, "🚀 JobDispatcher: Using saga-based provisioning");

            // Get enabled providers
            let providers = self.provider_registry.list_enabled_providers().await?;
            if providers.is_empty() {
                warn!("⚠️ JobDispatcher: No providers available for provisioning");
                return Ok(());
            }

            // Select best provider using scheduler with job preferences
            let provider_id = self
                .select_provider_for_provisioning(job, &providers)
                .await?;

            // Get spec from provisioning service
            let provisioning = self.provisioning_service.as_ref().ok_or_else(|| {
                DomainError::InfrastructureError {
                    message: "No provisioning service available".to_string(),
                }
            })?;
            let spec = provisioning
                .default_worker_spec(&provider_id)
                .ok_or_else(|| anyhow::anyhow!("No default spec for provider {}", provider_id))?;

            // Execute saga provisioning
            match coordinator
                .execute_provisioning_saga(&provider_id, &spec, Some(job.id.clone()))
                .await
            {
                Ok((worker_id, result)) => {
                    info!(job_id = %job.id, worker_id = %worker_id, saga_status = ?result, "✅ JobDispatcher: Saga provisioning completed");
                    Ok(())
                }
                Err(e) => Err(anyhow::anyhow!("Provisioning saga failed: {}", e)),
            }
        } else {
            // No coordinator configured - use legacy provisioning
            let provisioning = self.provisioning_service.as_ref().ok_or_else(|| {
                DomainError::InfrastructureError {
                    message: "No provisioning service available".to_string(),
                }
            })?;

            // Get enabled providers
            let providers = self.provider_registry.list_enabled_providers().await?;

            if providers.is_empty() {
                warn!("⚠️ JobDispatcher: No providers available for provisioning");
                return Ok(());
            }

            // Select best provider using scheduler with job preferences
            let provider_id = self
                .select_provider_for_provisioning(job, &providers)
                .await?;

            // Get default spec and provision
            let spec = provisioning
                .default_worker_spec(&provider_id)
                .ok_or_else(|| anyhow::anyhow!("No default spec for provider {}", provider_id))?;

            match provisioning
                .provision_worker(&provider_id, spec, job.id.clone())
                .await
            {
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
    }

    /// Select provider for provisioning using scheduling strategy
    ///
    /// ## GAP-GO-04: Delegado a ResourceAllocator
    /// Los cálculos de métricas están delegados a ResourceAllocator.
    async fn select_provider_for_provisioning(
        &self,
        job: &Job,
        providers: &[hodei_server_domain::providers::ProviderConfig],
    ) -> anyhow::Result<ProviderId> {
        // Delegar cálculos de métricas a ResourceAllocator
        let providers_info: Vec<ProviderInfo> = self
            .resource_allocator
            .configs_to_provider_infos(&providers.iter().cloned().map(Arc::new).collect::<Vec<_>>());

        // Use the actual job with its preferences (not a dummy job)
        self.scheduler
            .select_provider_with_preferences(job, &providers_info)
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

    // ========================================================================
    // EPIC-29: Reactive Event-Driven Methods
    // ========================================================================

    /// Handle a JobQueued event (reactivo)
    ///
    /// Called when a job is queued for processing.
    /// Checks for available workers and either dispatches or requests provisioning.
    ///
    /// ## Flujo
    /// 1. Fetch job from repository
    /// 2. Query healthy workers
    /// 3. If workers available → dispatch to first matching worker
    /// 4. If no workers → request worker provisioning
    pub async fn handle_job_queued(&self, job_id: &hodei_server_domain::shared_kernel::JobId) {
        info!(
            "📦 JobDispatcher: Handling JobQueued event for job {}",
            job_id
        );

        // Step 1: Fetch job from repository
        let job = match self.job_repository.find_by_id(job_id).await {
            Ok(Some(job)) => {
                debug!("JobDispatcher: Found job {} in repository", job_id);
                job
            }
            Ok(None) => {
                warn!("⚠️ JobDispatcher: Job {} not found in repository", job_id);
                return;
            }
            Err(e) => {
                error!("❌ JobDispatcher: Failed to fetch job {}: {}", job_id, e);
                return;
            }
        };

        // Step 2: FIRST check for workers already associated with this job
        // Workers provisioned for this job have current_job_id = job_id
        let associated_worker = match self.worker_registry.get_by_job_id(job_id).await {
            Ok(Some(worker)) => {
                info!(
                    "JobDispatcher: Found worker {} already associated with job {}",
                    worker.id(),
                    job_id
                );
                Some(worker)
            }
            Ok(None) => {
                debug!(
                    "JobDispatcher: No worker found associated with job {}",
                    job_id
                );
                None
            }
            Err(e) => {
                warn!(
                    "JobDispatcher: Failed to find worker for job {}: {}, continuing...",
                    job_id, e
                );
                None
            }
        };

        // Step 3: If we have an associated worker, dispatch directly
        if let Some(worker) = associated_worker {
            info!(
                "🚀 JobDispatcher: Dispatching job {} to provisioned worker {}",
                job_id,
                worker.id()
            );
            let mut job = match self.job_repository.find_by_id(job_id).await {
                Ok(Some(j)) => j,
                _ => {
                    warn!("JobDispatcher: Job {} not found for dispatch", job_id);
                    return;
                }
            };
            if let Err(e) = self.dispatch_job_to_worker(&mut job, worker.id()).await {
                error!("❌ JobDispatcher: Failed to dispatch job {}: {}", job_id, e);
            }
            return;
        }

        // Step 4: No associated worker - query available workers (for jobs without provisioning)
        let workers = match self.query_healthy_workers().await {
            Ok(workers) => {
                debug!("JobDispatcher: Found {} healthy workers", workers.len());
                workers
            }
            Err(e) => {
                error!("❌ JobDispatcher: Failed to query workers: {}", e);
                return;
            }
        };

        // Step 5: Dispatch or request provisioning
        if workers.is_empty() {
            info!(
                "⚠️ JobDispatcher: No workers available for job {}, requesting provisioning",
                job_id
            );
            // Request provisioning - will publish WorkerProvisioningRequested event
            self.request_worker_provisioning(&job).await;
        } else {
            // Dispatch to first available worker (scheduler will handle selection)
            let decision = self.make_scheduling_decision(&job, &workers).await;

            match decision {
                Ok(SchedulingDecision::AssignToWorker { worker_id, .. }) => {
                    info!(
                        "🚀 JobDispatcher: Dispatching job {} to worker {}",
                        job_id, worker_id
                    );
                    let mut job = self
                        .job_repository
                        .find_by_id(job_id)
                        .await
                        .unwrap()
                        .unwrap();
                    if let Err(e) = self.dispatch_job_to_worker(&mut job, &worker_id).await {
                        error!("❌ JobDispatcher: Failed to dispatch job {}: {}", job_id, e);
                    }
                }
                Ok(SchedulingDecision::ProvisionWorker { provider_id, .. }) => {
                    info!(
                        "🛠️ JobDispatcher: Scheduling decision requires provisioning on {}",
                        provider_id
                    );
                    self.request_worker_provisioning(&job).await;
                }
                Ok(_) => {
                    // Fallback to traditional dispatch
                    info!("📤 JobDispatcher: Using legacy dispatch for job {}", job_id);
                    let _job = self
                        .job_repository
                        .find_by_id(job_id)
                        .await
                        .unwrap()
                        .unwrap();
                    if let Err(e) = self.dispatch_once().await {
                        error!("❌ JobDispatcher: Legacy dispatch failed: {}", e);
                    }
                }
                Err(e) => {
                    error!("❌ JobDispatcher: Scheduling decision failed: {}", e);
                }
            }
        }
    }

    /// Dispatch pending job to a ready worker (reactivo)
    ///
    /// Called when a WorkerReadyForJob event is received.
    /// Finds a pending job that matches the worker and dispatches it.
    pub async fn dispatch_pending_job_to_worker(&self, worker_id: &WorkerId) {
        info!(
            "👷 JobDispatcher: Handling WorkerReadyForJob event for worker {}",
            worker_id
        );

        // Get worker details to know its capabilities
        let worker = match self.worker_registry.get(worker_id).await {
            Ok(Some(worker)) => {
                debug!("JobDispatcher: Found worker {}", worker_id);
                worker
            }
            Ok(None) => {
                warn!("⚠️ JobDispatcher: Worker {} not found", worker_id);
                return;
            }
            Err(e) => {
                error!(
                    "❌ JobDispatcher: Failed to fetch worker {}: {}",
                    worker_id, e
                );
                return;
            }
        };

        // Find pending job that matches worker capabilities
        let pending_job = match self.find_pending_job_for_worker(&worker).await {
            Ok(Some(job)) => {
                debug!("JobDispatcher: Found pending job for worker {}", worker_id);
                job
            }
            Ok(None) => {
                info!(
                    "ℹ️ JobDispatcher: No pending jobs match worker {} (will wait for job)",
                    worker_id
                );
                return;
            }
            Err(e) => {
                error!(
                    "❌ JobDispatcher: Failed to find pending job for worker {}: {}",
                    worker_id, e
                );
                return;
            }
        };

        // Dispatch the job
        info!(
            "🚀 JobDispatcher: Dispatching job {} to ready worker {}",
            pending_job.id, worker_id
        );
        let mut job = self
            .job_repository
            .find_by_id(&pending_job.id)
            .await
            .unwrap()
            .unwrap();
        if let Err(e) = self.dispatch_job_to_worker(&mut job, worker_id).await {
            error!(
                "❌ JobDispatcher: Failed to dispatch job {}: {}",
                pending_job.id, e
            );
        }
    }

    /// Find a pending job that matches worker capabilities
    /// Priority: 1) Worker has associated job_id → fetch that specific job
    ///           2) Worker has no job_id → REJECT (no FIFO fallback for legacy mode)
    ///
    /// ## EPIC-28: "Un worker por job" Enforcement
    /// Workers sin current_job_id NO pueden recibir jobs via FIFO.
    /// Esto previene que workers registrados directamente (sin provisioning) reciban múltiples jobs.
    async fn find_pending_job_for_worker(&self, worker: &Worker) -> anyhow::Result<Option<Job>> {
        // REQUISITO: "un worker por job" - workers sin current_job_id NO pueden recibir jobs
        let Some(associated_job_id) = worker.current_job_id() else {
            debug!(
                worker_id = %worker.id(),
                "JobDispatcher: REJECTING - Worker has no current_job_id, cannot receive jobs (ephemeral workers only)"
            );
            return Ok(None);
        };

        debug!(
            worker_id = %worker.id(),
            job_id = %associated_job_id,
            "JobDispatcher: Worker has associated job, fetching directly"
        );

        // Fetch the specific job associated with this worker
        match self.job_repository.find_by_id(&associated_job_id).await {
            Ok(Some(job)) => {
                // Verify job is still in PENDING state
                if *job.state() == JobState::Pending {
                    debug!(
                        job_id = %job.id,
                        "JobDispatcher: Found associated pending job"
                    );
                    Ok(Some(job))
                } else {
                    debug!(
                        job_id = %job.id,
                        state = ?job.state(),
                        "JobDispatcher: Associated job is not pending, rejecting"
                    );
                    Ok(None)
                }
            }
            Ok(None) => {
                debug!(
                    job_id = %associated_job_id,
                    "JobDispatcher: Associated job not found, rejecting"
                );
                Ok(None)
            }
            Err(e) => {
                warn!(
                    error = %e,
                    job_id = %associated_job_id,
                    "JobDispatcher: Failed to fetch associated job, rejecting"
                );
                Err(anyhow::Error::new(e))
            }
        }
    }

    /// Request worker provisioning for a job (publishes events)
    ///
    /// Publishes ProviderSelected event (EPIC-32) followed by WorkerProvisioningRequested.
    /// This provides full traceability of scheduling decisions.
    async fn request_worker_provisioning(&self, job: &Job) {
        info!(
            "🛠️ JobDispatcher: Requesting worker provisioning for job {}",
            job.id
        );

        // Select best provider using scheduler
        let providers = match self.provider_registry.list_enabled_providers().await {
            Ok(providers) => providers,
            Err(e) => {
                error!("❌ JobDispatcher: Failed to list providers: {}", e);
                return;
            }
        };

        if providers.is_empty() {
            warn!("⚠️ JobDispatcher: No enabled providers available");
            return;
        }

        let provider_id = match self.select_provider_for_provisioning(job, &providers).await {
            Ok(id) => id,
            Err(e) => {
                error!("❌ JobDispatcher: Failed to select provider: {}", e);
                return;
            }
        };

        // Get provider config for metrics
        let provider_config = match self.provider_registry.get_provider(&provider_id).await {
            Ok(Some(config)) => config,
            Ok(None) => {
                error!("❌ JobDispatcher: Provider {} not found", provider_id);
                return;
            }
            Err(e) => {
                error!(
                    "❌ JobDispatcher: Failed to get provider {}: {}",
                    provider_id, e
                );
                return;
            }
        };

        // EPIC-32: Publish ProviderSelected event for scheduling traceability
        let selection_start = Instant::now();
        let selection_strategy = "smart_scheduler".to_string();
        // GAP-GO-04: Delegar cálculos a ResourceAllocator
        let effective_cost = self.resource_allocator.calculate_provider_cost(&provider_config);
        let startup_duration = self.resource_allocator.calculate_startup_time(&provider_config);
        let effective_startup_ms = startup_duration.as_millis() as u64;

        // BUG FIX: Persist selected_provider_id in job BEFORE publishing event
        // This ensures the job has provider_id set for subsequent processing
        // FIXED: Now calls assign_to_provider() to properly set selected_provider field
        let mut job = job.clone();
        let execution_context = hodei_server_domain::jobs::ExecutionContext::new(
            job.id.clone(),
            provider_id.clone(),
            format!("exec-{}", Uuid::new_v4()),
        );

        // Use assign_to_provider() to properly set the selected_provider field in aggregate
        if let Err(e) = job.assign_to_provider(provider_id.clone(), execution_context) {
            error!("❌ JobDispatcher: Failed to assign provider to job: {}", e);
            return;
        }

        // Also set preferred_provider for scheduling reference
        job.spec.preferences.preferred_provider = Some(provider_id.to_string());

        // Save job to persist selected_provider_id
        if let Err(e) = self.job_repository.update(&job).await {
            error!(
                "❌ JobDispatcher: Failed to save job with selected_provider: {}",
                e
            );
            return;
        }
        info!(
            "✅ JobDispatcher: Saved job {} with selected_provider_id {}",
            job.id, provider_id
        );

        let provider_selected_event = DomainEvent::ProviderSelected {
            job_id: job.id.clone(),
            provider_id: provider_id.clone(),
            provider_type: provider_config.provider_type.clone(),
            selection_strategy,
            effective_cost,
            effective_startup_ms,
            elapsed_ms: selection_start.elapsed().as_millis() as u64,
            occurred_at: chrono::Utc::now(),
            correlation_id: None,
            actor: Some("job_dispatcher".to_string()),
        };

        if let Err(e) = self.event_bus.publish(&provider_selected_event).await {
            error!(
                "❌ JobDispatcher: Failed to publish ProviderSelected: {}",
                e
            );
        } else {
            info!(
                "✅ JobDispatcher: Published ProviderSelected for job {} -> {} ({}ms)",
                job.id,
                provider_id,
                selection_start.elapsed().as_millis()
            );
        }

        // Publish WorkerProvisioningRequested event (for reactive processing/saga tracking)
        let event = DomainEvent::WorkerProvisioningRequested {
            job_id: job.id.clone(),
            provider_id: provider_id.clone(),
            job_requirements: job.spec.clone(),
            requested_at: chrono::Utc::now(),
            correlation_id: None,
            actor: Some("job_dispatcher".to_string()),
        };

        if let Err(e) = self.event_bus.publish(&event).await {
            error!(
                "❌ JobDispatcher: Failed to publish WorkerProvisioningRequested: {}",
                e
            );
        } else {
            info!(
                "✅ JobDispatcher: Published WorkerProvisioningRequested for job {} on provider {}",
                job.id, provider_id
            );
        }

        // Trigger provisioning directly (since there's no event subscriber yet for WorkerProvisioningRequested)
        // This ensures workers get provisioned even without the reactive infrastructure in place
        if let Err(e) = self.trigger_provisioning(&job).await {
            error!("❌ JobDispatcher: Failed to trigger provisioning: {}", e);
        }
    }
}

// Note: Tests for provider metrics calculations have been moved to resource_allocator.rs
// to maintain Single Responsibility Principle.

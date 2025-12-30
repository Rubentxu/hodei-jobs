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

use crate::providers::ProviderRegistry;
use crate::saga::dispatcher_saga::{DynExecutionSagaDispatcher, ExecutionSagaDispatcher};
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
use hodei_server_domain::shared_kernel::{DomainError, JobId, ProviderId, WorkerId};
use hodei_server_domain::workers::health::WorkerHealthService;
use hodei_server_domain::workers::{Worker, WorkerRegistry};
use sqlx::postgres::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
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
    /// EPIC-28: Track recently provisioned jobs to prevent duplicate provisioning
    /// Key: JobId, Value: Instant when provisioning was triggered
    recently_provisioned: Arc<tokio::sync::Mutex<HashMap<Uuid, Instant>>>,
    /// EPIC-28: Time window to skip re-provisioning for same job
    provisioning_cooldown: Duration,
    /// EPIC-29: Saga orchestrator for execution saga coordination
    execution_saga_dispatcher: Option<Arc<DynExecutionSagaDispatcher>>,
    /// EPIC-30: Saga coordinator for provisioning saga (US-30.2)
    provisioning_saga_coordinator: Option<Arc<DynProvisioningSagaCoordinator>>,
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
        execution_saga_dispatcher: Option<Arc<DynExecutionSagaDispatcher>>,
        provisioning_saga_coordinator: Option<Arc<DynProvisioningSagaCoordinator>>,
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
            recently_provisioned: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            provisioning_cooldown: Duration::from_secs(30), // 30 seconds cooldown
            execution_saga_dispatcher,
            provisioning_saga_coordinator,
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
        execution_saga_dispatcher: Option<Arc<DynExecutionSagaDispatcher>>,
        provisioning_saga_coordinator: Option<Arc<DynProvisioningSagaCoordinator>>,
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
            recently_provisioned: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            provisioning_cooldown: Duration::from_secs(30), // 30 seconds cooldown
            execution_saga_dispatcher,
            provisioning_saga_coordinator,
        }
    }

    /// Execute one dispatch cycle
    /// Returns the number of jobs dispatched
    ///
    /// ## Responsabilidad Core #1: Orquestación
    /// Este método solo orquesta el flujo, delegando los detalles a servicios especializados.
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

        // Convert ProviderConfig to ProviderInfo for scheduler with real metrics
        let providers_info: Vec<ProviderInfo> = available_providers
            .iter()
            .map(|p| {
                let estimated_startup = Self::calculate_startup_time(p);
                let health_score = Self::calculate_health_score(p);
                let cost_per_hour = Self::calculate_provider_cost(p);

                ProviderInfo {
                    provider_id: p.id.clone(),
                    provider_type: p.provider_type.clone(),
                    active_workers: p.active_workers as usize,
                    max_workers: p.max_workers as usize,
                    estimated_startup_time: estimated_startup,
                    health_score,
                    cost_per_hour,
                    // US-27.4: GPU support from capabilities
                    gpu_support: p.capabilities.gpu_support,
                    gpu_types: p.capabilities.gpu_types.clone(),
                    // US-27.6: Region from capabilities
                    regions: p.capabilities.regions.clone(),
                }
            })
            .collect();

        Ok(providers_info)
    }

    /// Calculate estimated startup time from ProviderConfig
    ///
    /// ## Connascence Analysis
    /// Transforma Connascence of Position (hardcoded index) a Connascence of Type
    /// usando ProviderCapabilities.max_execution_time como fuente de verdad.
    fn calculate_startup_time(
        provider: &hodei_server_domain::providers::ProviderConfig,
    ) -> std::time::Duration {
        // Try to get startup time from capabilities
        if let Some(max_exec_time) = provider.capabilities.max_execution_time {
            // Use max_execution_time as a reasonable estimate for startup
            // Typically startup is 10-20% of max execution time for well-tuned systems
            let startup_fraction = 0.15f64;
            let estimated = (max_exec_time.as_secs() as f64 * startup_fraction) as u64;
            std::time::Duration::from_secs(estimated.max(5)) // Minimum 5 seconds
        } else {
            // Fallback to provider-type-based estimates
            Self::default_startup_for_type(&provider.provider_type)
        }
    }

    /// Default startup times by provider type
    fn default_startup_for_type(
        provider_type: &hodei_server_domain::workers::ProviderType,
    ) -> std::time::Duration {
        match provider_type {
            hodei_server_domain::workers::ProviderType::Docker => std::time::Duration::from_secs(3),
            hodei_server_domain::workers::ProviderType::Kubernetes => {
                std::time::Duration::from_secs(15)
            }
            // Firecracker is represented as Custom(String)
            hodei_server_domain::workers::ProviderType::Custom(_) => {
                std::time::Duration::from_secs(8)
            }
            hodei_server_domain::workers::ProviderType::Lambda => std::time::Duration::from_secs(2),
            hodei_server_domain::workers::ProviderType::CloudRun => {
                std::time::Duration::from_secs(10)
            }
            hodei_server_domain::workers::ProviderType::Fargate => {
                std::time::Duration::from_secs(20)
            }
            _ => std::time::Duration::from_secs(30), // Conservative default
        }
    }

    /// Calculate health score from ProviderConfig
    ///
    /// ## US-27.5: Provider Health Monitor Integration
    /// Este método usa indicadores de salud desde ProviderConfig:
    /// - Estado del provider (enabled/active)
    /// - Capacidad disponible (active_workers < max_workers)
    /// - Capacidades reportadas (gpu_support, regions, etc.)
    fn calculate_health_score(provider: &hodei_server_domain::providers::ProviderConfig) -> f64 {
        let mut score: f64 = 0.5; // Base score

        // Provider status contribution (up to 0.2)
        if provider.is_enabled() {
            score += 0.2;
        }

        // Capacity contribution (up to 0.15)
        if provider.has_capacity() {
            score += 0.15;
        }

        // Feature support contribution (up to 0.15)
        if provider.capabilities.gpu_support {
            score += 0.05;
        }
        if !provider.capabilities.regions.is_empty() {
            score += 0.05;
        }
        if provider.capabilities.persistent_storage {
            score += 0.05;
        }

        // Cap at 1.0
        score.min(1.0)
    }

    /// Calculate provider cost per hour from type_config
    ///
    /// ## Connascence Transformation
    /// De Connascence of Position (hardcoded 0.0) a Connascence of Type
    /// usando el tipo de provider para determinar costo base.
    fn calculate_provider_cost(provider: &hodei_server_domain::providers::ProviderConfig) -> f64 {
        use hodei_server_domain::providers::ProviderTypeConfig;

        match &provider.type_config {
            // Container providers - typically pay-per-use or fixed
            ProviderTypeConfig::Docker(_) => 0.0, // Local, no cost
            ProviderTypeConfig::Kubernetes(k8s) => {
                // Estimate based on node selector complexity
                let base_cost = 0.10; // Base cost per hour
                let complexity_bonus = (k8s.node_selector.len() as f64) * 0.02;
                (base_cost + complexity_bonus).min(0.50)
            }
            ProviderTypeConfig::CloudRun(_) => 0.05, // Serverless container
            ProviderTypeConfig::Fargate(_) => 0.15,  // Serverless container (AWS)
            ProviderTypeConfig::ContainerApps(_) => 0.10,

            // Serverless providers - typically pay-per-invocation
            ProviderTypeConfig::Lambda(lambda) => {
                // Memory-based estimate ($0.0000166667 per GB-second approx)
                let memory_factor = (lambda.memory_mb as f64 / 1024.0) * 0.017;
                let timeout_factor = (lambda.timeout_seconds as f64 / 900.0) * 0.01;
                (memory_factor + timeout_factor).min(0.10)
            }
            ProviderTypeConfig::CloudFunctions(_) => 0.05,
            ProviderTypeConfig::AzureFunctions(_) => 0.05,

            // VM providers - typically pay-per-hour
            ProviderTypeConfig::EC2(ec2) => {
                // Instance type based estimate (simplified)
                match ec2.instance_type.as_str() {
                    t if t.contains("t3") || t.contains("t2") => 0.05,
                    t if t.contains("m5") || t.contains("m6") => 0.10,
                    t if t.contains("c5") || t.contains("c6") => 0.12,
                    t if t.contains("r5") || t.contains("r6") => 0.15,
                    t if t.contains("p3") || t.contains("p4") => 3.00, // GPU instances
                    t if t.contains("g4") || t.contains("g5") => 2.50, // GPU instances
                    _ => 0.10,
                }
            }
            ProviderTypeConfig::ComputeEngine(_) => 0.08,
            ProviderTypeConfig::AzureVMs(_) => 0.08,

            // Other providers
            ProviderTypeConfig::BareMetal(_) => 0.20,
            ProviderTypeConfig::Custom(_) => 0.0,
        }
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

        // Step 1: Get worker details
        let worker = self.worker_registry.get(worker_id).await?.ok_or_else(|| {
            DomainError::WorkerNotFound {
                worker_id: worker_id.clone(),
            }
        })?;

        debug!(worker_id = %worker_id, "JobDispatcher: Found worker");

        // Step 2: EPIC-30 - Try saga-based execution if enabled
        let saga_used = if let Some(ref dispatcher) = self.execution_saga_dispatcher {
            if dispatcher.is_saga_enabled() {
                info!(job_id = %job.id, worker_id = %worker_id, "🚀 JobDispatcher: Starting ExecutionSaga");
                match dispatcher.execute_execution_saga(&job.id, worker_id).await {
                    Ok(result) => {
                        info!(job_id = %job.id, saga_status = ?result, "✅ JobDispatcher: ExecutionSaga completed");
                        if !result.is_success() && !dispatcher.is_shadow_mode() {
                            anyhow::bail!("ExecutionSaga failed: {:?}", result);
                        }
                        true
                    }
                    Err(e) => {
                        if dispatcher.is_shadow_mode() {
                            warn!(job_id = %job.id, error = %e, "⚠️ JobDispatcher: ExecutionSaga failed, falling back to legacy dispatch");
                            false
                        } else {
                            anyhow::bail!("ExecutionSaga failed: {}", e);
                        }
                    }
                }
            } else {
                false
            }
        } else {
            false
        };

        // Step 3: If saga didn't handle everything, continue with manual assignment
        if !saga_used {
            // Create execution context and store provider assignment
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
        }

        // Step 4: Update job in repository (BEFORE gRPC to avoid race condition)
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

        // Step 5: Send RUN_JOB command to worker via gRPC
        info!(
            worker_id = %worker_id,
            job_id = %job.id,
            "📡 JobDispatcher: Sending RUN_JOB command to worker"
        );

        // Add timeout to prevent hanging indefinitely
        let dispatch_result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.worker_command_sender.send_run_job(&worker_id, job),
        )
        .await;

        let result = match dispatch_result {
            Ok(inner_result) => inner_result.map_err(|e| anyhow::Error::new(e)),
            Err(_) => Err(anyhow::anyhow!("Timeout waiting for RUN_JOB response")),
        };

        if let Err(e) = result {
            error!(
                error = %e,
                worker_id = %worker_id,
                job_id = %job.id,
                "❌ JobDispatcher: Failed to send RUN_JOB"
            );

            // CRITICAL FIX: Rollback job state to FAILED to prevent it from being stuck in ASSIGNED
            // preventing the queue from processing it again (or requiring manual timeout).
            // Ideally we would set it back to PENDING, but failing is safer to indicate system error.
            if let Err(state_err) =
                job.fail(format!("Failed to dispatch to worker {}: {}", worker_id, e))
            {
                error!(error = %state_err, job_id = %job.id, "Failed to transition job to Failed state");
            } else {
                if let Err(db_err) = self.job_repository.update(job).await {
                    error!(error = %db_err, job_id = %job.id, "Failed to persist Failed state to DB");
                } else {
                    info!(job_id = %job.id, "b↺ Job state rolled back to FAILED due to dispatch error");
                }
            }

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

        // Step 6: Publish JobAssigned event with idempotency key
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
    /// ## EPIC-30: Routing a través de Saga Pattern
    /// 1. Si hay saga coordinator disponible, usar ProvisioningSaga
    /// 2. Fallback al provisioning_service legacy si no hay saga
    async fn trigger_provisioning(&self, job: &Job) -> anyhow::Result<()> {
        // EPIC-30: Try saga-based provisioning first
        if let Some(ref coordinator) = self.provisioning_saga_coordinator {
            if coordinator.is_saga_enabled() {
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
                    .ok_or_else(|| {
                        anyhow::anyhow!("No default spec for provider {}", provider_id)
                    })?;

                // Execute saga
                match coordinator
                    .execute_provisioning_saga(&provider_id, &spec, Some(job.id.clone()))
                    .await
                {
                    Ok((worker_id, result)) => {
                        info!(job_id = %job.id, worker_id = %worker_id, saga_status = ?result, "✅ JobDispatcher: Saga provisioning completed");
                        return Ok(());
                    }
                    Err(e) => {
                        // If saga fails but shadow mode is enabled, try legacy
                        if coordinator.is_shadow_mode() {
                            warn!(job_id = %job.id, error = %e, "⚠️ JobDispatcher: Saga failed, falling back to legacy provisioning");
                        } else {
                            anyhow::bail!("Saga provisioning failed: {}", e);
                        }
                    }
                }
            }
        }

        // Legacy path: use provisioning_service directly
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

        // Select best provider using scheduler with job preferences
        let provider_id = self
            .select_provider_for_provisioning(job, &providers)
            .await?;

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
        job: &Job,
        providers: &[hodei_server_domain::providers::ProviderConfig],
    ) -> anyhow::Result<ProviderId> {
        // Use scheduler to select best provider
        let providers_info: Vec<_> = providers
            .iter()
            .map(|p| {
                let estimated_startup = Self::calculate_startup_time(p);
                let health_score = Self::calculate_health_score(p);
                let cost_per_hour = Self::calculate_provider_cost(p);

                ProviderInfo {
                    provider_id: p.id.clone(),
                    provider_type: p.provider_type.clone(),
                    active_workers: p.active_workers as usize,
                    max_workers: p.max_workers as usize,
                    estimated_startup_time: estimated_startup,
                    health_score,
                    cost_per_hour,
                    // US-27.4: GPU support from capabilities
                    gpu_support: p.capabilities.gpu_support,
                    gpu_types: p.capabilities.gpu_types.clone(),
                    // US-27.6: Region from capabilities
                    regions: p.capabilities.regions.clone(),
                }
            })
            .collect();

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

        // Step 2: Query available workers
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

        // Step 3: Dispatch or request provisioning
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
                    let mut job = self
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
    async fn find_pending_job_for_worker(&self, _worker: &Worker) -> anyhow::Result<Option<Job>> {
        // Try to dequeue a job for this worker
        // If we get a job, it's assigned to this worker (we'll re-enqueue if dispatch fails)
        let dequeued_job = self.job_queue.dequeue().await?;

        if let Some(job) = dequeued_job {
            // Return the job for dispatching
            Ok(Some(job))
        } else {
            Ok(None)
        }
    }

    /// Request worker provisioning for a job (publishes event)
    ///
    /// Publishes WorkerProvisioningRequested event which triggers
    /// the provider to create a new worker.
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
        if let Err(e) = self.trigger_provisioning(job).await {
            error!("❌ JobDispatcher: Failed to trigger provisioning: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::providers::{
        DockerConfig, EC2Config, KubernetesConfig, LambdaConfig, ProviderTypeConfig,
    };
    use hodei_server_domain::workers::ProviderType;

    fn create_test_provider_config(
        provider_type: ProviderType,
        gpu_support: bool,
        max_execution_secs: Option<u64>,
    ) -> hodei_server_domain::providers::ProviderConfig {
        let type_config = match provider_type {
            ProviderType::Docker => ProviderTypeConfig::Docker(DockerConfig::default()),
            ProviderType::Kubernetes => ProviderTypeConfig::Kubernetes(KubernetesConfig::default()),
            _ => ProviderTypeConfig::Docker(DockerConfig::default()),
        };

        let capabilities = hodei_server_domain::workers::ProviderCapabilities {
            gpu_support,
            gpu_types: if gpu_support {
                vec!["nvidia".to_string()]
            } else {
                vec![]
            },
            regions: vec!["local".to_string()],
            max_execution_time: max_execution_secs.map(|s| std::time::Duration::from_secs(s)),
            ..Default::default()
        };

        hodei_server_domain::providers::ProviderConfig::new(
            format!("test-{:?}", provider_type).to_lowercase(),
            provider_type,
            type_config,
        )
        .with_capabilities(capabilities)
    }

    #[test]
    fn test_calculate_startup_time_from_capabilities() {
        // Given: ProviderConfig with max_execution_time
        let provider = create_test_provider_config(
            ProviderType::Kubernetes,
            false,
            Some(600), // 10 minutes max execution
        );

        // When: calculate_startup_time is called
        let startup = JobDispatcher::calculate_startup_time(&provider);

        // Then: Startup time is ~15% of max_execution_time (90 seconds)
        assert_eq!(startup, std::time::Duration::from_secs(90));
    }

    #[test]
    fn test_calculate_startup_time_fallback_to_type_default() {
        // Given: ProviderConfig without max_execution_time
        let provider = create_test_provider_config(ProviderType::Kubernetes, false, None);

        // When: calculate_startup_time is called
        let startup = JobDispatcher::calculate_startup_time(&provider);

        // Then: Uses provider-type default (15 seconds for K8s)
        assert_eq!(startup, std::time::Duration::from_secs(15));
    }

    #[test]
    fn test_calculate_startup_time_docker_faster_than_kubernetes() {
        let docker_provider = create_test_provider_config(ProviderType::Docker, false, None);
        let k8s_provider = create_test_provider_config(ProviderType::Kubernetes, false, None);

        let docker_startup = JobDispatcher::calculate_startup_time(&docker_provider);
        let k8s_startup = JobDispatcher::calculate_startup_time(&k8s_provider);

        // Docker should be faster than Kubernetes
        assert!(docker_startup < k8s_startup);
        assert_eq!(docker_startup, std::time::Duration::from_secs(3));
        assert_eq!(k8s_startup, std::time::Duration::from_secs(15));
    }

    #[test]
    fn test_calculate_health_score_enabled_with_capacity() {
        let provider = create_test_provider_config(ProviderType::Docker, true, None);

        let score = JobDispatcher::calculate_health_score(&provider);

        // Base 0.5 + enabled 0.2 + capacity 0.15 + gpu 0.05 + regions 0.05 = 0.95
        assert!((score - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_calculate_health_score_maximum() {
        let provider = create_test_provider_config(ProviderType::Kubernetes, true, None);

        let score = JobDispatcher::calculate_health_score(&provider);

        // Should be less than or equal to 1.0
        assert!(score <= 1.0);
        assert!(score >= 0.8);
    }

    #[test]
    fn test_calculate_provider_cost_docker_free() {
        let provider = create_test_provider_config(ProviderType::Docker, false, None);

        let cost = JobDispatcher::calculate_provider_cost(&provider);

        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_calculate_provider_cost_kubernetes() {
        use std::collections::HashMap;
        let k8s_config = KubernetesConfig {
            node_selector: HashMap::new(),
            ..Default::default()
        };
        let type_config = ProviderTypeConfig::Kubernetes(k8s_config);

        let provider = hodei_server_domain::providers::ProviderConfig::new(
            "k8s-test".to_string(),
            ProviderType::Kubernetes,
            type_config,
        );

        let cost = JobDispatcher::calculate_provider_cost(&provider);

        // Base cost 0.10 for K8s with empty node_selector
        assert_eq!(cost, 0.10);
    }

    #[test]
    fn test_calculate_provider_cost_lambda() {
        let lambda_config = LambdaConfig {
            memory_mb: 1024,
            timeout_seconds: 300,
            ..Default::default()
        };
        let type_config = ProviderTypeConfig::Lambda(lambda_config);

        let provider = hodei_server_domain::providers::ProviderConfig::new(
            "lambda-test".to_string(),
            ProviderType::Lambda,
            type_config,
        );

        let cost = JobDispatcher::calculate_provider_cost(&provider);

        // Memory factor: 1024/1024 * 0.017 = 0.017
        // Timeout factor: 300/900 * 0.01 = 0.0033
        // Total: ~0.02
        assert!(cost > 0.0 && cost < 0.1);
    }

    #[test]
    fn test_calculate_provider_cost_gpu_ec2_expensive() {
        let ec2_config = EC2Config {
            region: "us-east-1".to_string(),
            ami_id: "ami-12345678".to_string(),
            instance_type: "p3.2xlarge".to_string(),
            key_name: None,
            security_group_ids: vec![],
            subnet_id: None,
            iam_instance_profile: None,
            user_data_template: None,
        };
        let type_config = ProviderTypeConfig::EC2(ec2_config);

        let provider = hodei_server_domain::providers::ProviderConfig::new(
            "ec2-gpu".to_string(),
            ProviderType::EC2,
            type_config,
        );

        let cost = JobDispatcher::calculate_provider_cost(&provider);

        // GPU instances should be expensive
        assert!(cost > 1.0);
    }
}

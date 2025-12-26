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

        // Base 0.5 + enabled 0.2 + capacity 0.15 + gpu 0.05 = 0.9
        assert_eq!(score, 0.9);
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
            instance_type: "p3.2xlarge".to_string(),
            ami_id: None,
            security_group_ids: vec![],
            iam_role: None,
            subnet_id: None,
            tags: std::collections::HashMap::new(),
            disk_size_gb: None,
            ebs_optimized: false,
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

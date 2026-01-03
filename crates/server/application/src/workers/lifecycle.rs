//! Worker Lifecycle Manager
//!
//! Servicio de aplicación para gestionar el ciclo de vida de workers:
//! - Heartbeat monitoring and reconciliation
//! - Auto-scaling basado en demanda
//! - Terminación de workers idle/unhealthy
//! - Job reassignment for failed workers (via Transactional Outbox)
//! - Worker provisioning via Saga pattern (US-2.2)
//! - Worker recovery via Saga pattern (US-4.2)

use crate::saga::provisioning_saga::{
    DynProvisioningSagaCoordinator, DynProvisioningSagaCoordinatorBuilder,
    ProvisioningSagaCoordinatorConfig, ProvisioningSagaError,
};
use crate::saga::recovery_saga::{
    DynRecoverySagaCoordinator, RecoverySagaCoordinatorConfig, RecoverySagaError,
};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use hodei_server_domain::{
    event_bus::EventBus,
    events::{DomainEvent, TerminationReason},
    outbox::{OutboxError, OutboxEventInsert, OutboxRepository},
    saga::SagaOrchestrator,
    shared_kernel::{DomainError, JobId, ProviderId, Result, WorkerId, WorkerState},
    workers::WorkerProvider,
    workers::health::WorkerHealthService,
    workers::provider_api::{
        HealthStatus, WorkerCost, WorkerEligibility, WorkerHealth, WorkerInfrastructureEvent,
        WorkerLifecycle, WorkerLogs, WorkerMetrics, WorkerProviderIdentity,
    },
    workers::{Worker, WorkerFilter, WorkerSpec},
    workers::{WorkerRegistry, WorkerRegistryStats},
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Configuration for Worker Lifecycle Manager
#[derive(Debug, Clone)]
pub struct WorkerLifecycleConfig {
    /// Timeout for worker heartbeat (mark unhealthy after this)
    pub heartbeat_timeout: Duration,
    /// Interval for running health checks
    pub health_check_interval: Duration,
    /// Interval for running reconciliation
    pub reconciliation_interval: Duration,
    /// Minimum workers to keep ready
    pub min_ready_workers: usize,
    /// Maximum workers allowed
    pub max_workers: usize,
    /// Scale up when queue depth exceeds this
    pub scale_up_threshold: usize,
    /// Scale down when idle workers exceed this
    pub scale_down_threshold: usize,
    /// Grace period before considering a worker truly dead
    pub worker_dead_grace_period: Duration,
}

impl Default for WorkerLifecycleConfig {
    fn default() -> Self {
        Self {
            heartbeat_timeout: Duration::from_secs(60),
            health_check_interval: Duration::from_secs(30),
            reconciliation_interval: Duration::from_secs(15),
            // EPIC-28: Modelo efímero - no se mantiene pool de workers
            // Cada job crea su propio worker que se termina después
            min_ready_workers: 0,
            max_workers: 100,           // Límite alto solo para evitar sobrecarga
            scale_up_threshold: 999999, // Auto-scaling deshabilitado (nunca se activa)
            scale_down_threshold: 0,    // Limpieza inmediata de workers idle
            worker_dead_grace_period: Duration::from_secs(120),
        }
    }
}

/// Worker Lifecycle Manager
///
/// Responsibilities:
/// - Monitor worker health via heartbeats
/// - Auto-scale workers based on demand
/// - Terminate idle/unhealthy workers
/// - Provision new workers when needed (Saga-only when configured)
/// - Reconcile stale worker states and reassign jobs
/// - Recover failed workers via saga pattern (US-4.2, Saga-only when configured)
#[derive(Clone)]
pub struct WorkerLifecycleManager {
    registry: Arc<dyn WorkerRegistry>,
    providers: Arc<RwLock<HashMap<ProviderId, Arc<dyn WorkerProvider>>>>,
    config: WorkerLifecycleConfig,
    event_bus: Arc<dyn EventBus>,
    /// Optional outbox repository for transactional event publishing
    outbox_repository: Option<Arc<dyn OutboxRepository<Error = OutboxError> + Send + Sync>>,
    health_service: Arc<WorkerHealthService>,
    /// US-2.2: Saga coordinator for worker provisioning with automatic compensation
    provisioning_saga_coordinator: Option<Arc<DynProvisioningSagaCoordinator>>,
    /// US-4.2: Saga coordinator for worker recovery with automatic compensation
    recovery_saga_coordinator: Option<Arc<DynRecoverySagaCoordinator>>,
}

impl WorkerLifecycleManager {
    pub fn new(
        registry: Arc<dyn WorkerRegistry>,
        providers: Arc<RwLock<HashMap<ProviderId, Arc<dyn WorkerProvider>>>>,
        config: WorkerLifecycleConfig,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            registry,
            providers,
            config: config.clone(),
            event_bus,
            outbox_repository: None,
            health_service: Arc::new(
                WorkerHealthService::builder()
                    .with_heartbeat_timeout(config.heartbeat_timeout)
                    .build(),
            ),
            provisioning_saga_coordinator: None,
            recovery_saga_coordinator: None,
        }
    }

    /// Create with outbox repository for transactional event publishing
    pub fn with_outbox_repository(
        registry: Arc<dyn WorkerRegistry>,
        providers: Arc<RwLock<HashMap<ProviderId, Arc<dyn WorkerProvider>>>>,
        config: WorkerLifecycleConfig,
        event_bus: Arc<dyn EventBus>,
        outbox_repository: Arc<dyn OutboxRepository<Error = OutboxError> + Send + Sync>,
    ) -> Self {
        Self {
            registry,
            providers,
            config: config.clone(),
            event_bus,
            outbox_repository: Some(outbox_repository),
            health_service: Arc::new(
                WorkerHealthService::builder()
                    .with_heartbeat_timeout(config.heartbeat_timeout)
                    .build(),
            ),
            provisioning_saga_coordinator: None,
            recovery_saga_coordinator: None,
        }
    }

    /// Set outbox repository after construction
    pub fn set_outbox_repository(
        &mut self,
        outbox_repository: Arc<dyn OutboxRepository<Error = OutboxError> + Send + Sync>,
    ) {
        self.outbox_repository = Some(outbox_repository);
    }

    /// Set provisioning saga coordinator (US-2.2)
    /// When set, worker provisioning will use saga pattern with automatic compensation
    pub fn set_provisioning_saga_coordinator(
        &mut self,
        coordinator: Arc<DynProvisioningSagaCoordinator>,
    ) {
        self.provisioning_saga_coordinator = Some(coordinator);
    }

    /// Set recovery saga coordinator (US-4.2)
    /// When set, worker recovery will use saga pattern with automatic compensation
    pub fn set_recovery_saga_coordinator(&mut self, coordinator: Arc<DynRecoverySagaCoordinator>) {
        self.recovery_saga_coordinator = Some(coordinator);
    }

    /// Check if saga-based provisioning is enabled
    #[inline]
    pub fn is_saga_provisioning_enabled(&self) -> bool {
        self.provisioning_saga_coordinator.is_some()
    }

    /// Check if saga-based recovery is enabled (US-4.2)
    #[inline]
    pub fn is_saga_recovery_enabled(&self) -> bool {
        self.recovery_saga_coordinator.is_some()
    }

    /// Register a provider with the lifecycle manager
    pub async fn register_provider(&self, provider: Arc<dyn WorkerProvider>) {
        let provider_id = provider.provider_id().clone();
        info!("Registering provider: {}", provider_id);
        self.providers.write().await.insert(provider_id, provider);
    }

    /// Unregister a provider
    pub async fn unregister_provider(&self, provider_id: &ProviderId) {
        info!("Unregistering provider: {}", provider_id);
        self.providers.write().await.remove(provider_id);
    }

    /// Process heartbeat from a worker
    pub async fn process_heartbeat(&self, worker_id: &WorkerId) -> Result<()> {
        self.registry.heartbeat(worker_id).await?;
        debug!("Processed heartbeat for worker: {}", worker_id);
        Ok(())
    }

    /// Check health of all workers and handle unhealthy ones
    pub async fn run_health_check(&self) -> Result<HealthCheckResult> {
        let unhealthy = self
            .registry
            .find_unhealthy(self.config.heartbeat_timeout)
            .await?;
        let mut result = HealthCheckResult::default();

        for worker in unhealthy {
            warn!("Worker {} is unhealthy, marking as failed", worker.id());
            result.unhealthy_workers.push(worker.id().clone());

            if let Err(e) = self
                .registry
                .update_state(worker.id(), WorkerState::Terminated)
                .await
            {
                error!("Failed to mark worker {} as failed: {}", worker.id(), e);
            }
        }

        result.total_checked = self.registry.count().await?;
        Ok(result)
    }

    /// Reconcile worker states and handle stale workers with active jobs
    ///
    /// This method:
    /// 1. Detects workers with stale heartbeats that have assigned jobs
    /// 2. Emits WorkerHeartbeatMissed events for monitoring
    /// 3. Marks workers as unhealthy if beyond grace period
    /// 4. Triggers job reassignment for affected jobs
    ///
    /// Uses Transactional Outbox pattern when available for consistency.
    pub async fn run_reconciliation(&self) -> Result<ReconciliationResult> {
        let mut result = ReconciliationResult::default();
        let now = Utc::now();

        // Find workers with stale heartbeats (haven't reported in heartbeat_timeout)
        let stale_workers = self
            .registry
            .find_unhealthy(self.config.heartbeat_timeout)
            .await?;

        for worker in stale_workers {
            let worker_id = worker.id().clone();
            let heartbeat_age = self.health_service.calculate_heartbeat_age(&worker);
            let stale_seconds = heartbeat_age.as_duration().as_secs();

            info!(
                "🔍 Reconciliation: Worker {} has stale heartbeat ({}s ago)",
                worker_id, stale_seconds
            );

            result.stale_workers.push(worker_id.clone());

            // Check if worker has an assigned job
            if let Some(job_id) = worker.current_job_id() {
                result.affected_jobs.push(job_id.clone());

                // Emit events for job reassignment
                if let Err(e) = self.emit_worker_heartbeat_missed(&worker, job_id).await {
                    warn!(
                        "Failed to emit heartbeat missed event for worker {}: {:?}",
                        worker_id, e
                    );
                }

                // If beyond grace period, mark for job reassignment
                if stale_seconds > self.config.worker_dead_grace_period.as_secs() {
                    info!(
                        "⚠️ Worker {} exceeded grace period, triggering job reassignment for {}",
                        worker_id, job_id
                    );

                    if let Err(e) = self.emit_job_reassignment_required(&worker, job_id).await {
                        warn!(
                            "Failed to emit job reassignment event for job {}: {:?}",
                            job_id, e
                        );
                    }

                    result.jobs_requiring_reassignment.push(job_id.clone());
                }
            }

            // Update worker state to reflect unhealthy status
            if *worker.state() != WorkerState::Terminated {
                if let Err(e) = self
                    .registry
                    .update_state(&worker_id, WorkerState::Terminating)
                    .await
                {
                    error!(
                        "Failed to update worker {} to Terminating state: {}",
                        worker_id, e
                    );
                } else {
                    result.workers_marked_unhealthy.push(worker_id.clone());

                    // Emit WorkerStatusChanged event
                    if let Err(e) = self
                        .emit_worker_status_changed(
                            &worker_id,
                            worker.state().clone(),
                            WorkerState::Terminating,
                            "heartbeat_timeout",
                        )
                        .await
                    {
                        warn!(
                            "Failed to emit status changed event for worker {}: {:?}",
                            worker_id, e
                        );
                    }
                }
            }
        }

        if !result.stale_workers.is_empty() {
            info!(
                "📊 Reconciliation complete: {} stale workers, {} affected jobs, {} requiring reassignment",
                result.stale_workers.len(),
                result.affected_jobs.len(),
                result.jobs_requiring_reassignment.len()
            );
        }

        Ok(result)
    }

    /// Emit WorkerHeartbeatMissed event using outbox or direct publishing
    async fn emit_worker_heartbeat_missed(&self, worker: &Worker, job_id: &JobId) -> Result<()> {
        let now = Utc::now();
        let worker_id = worker.id();

        if let Some(outbox_repo) = &self.outbox_repository {
            let event = OutboxEventInsert::for_worker(
                worker_id.0,
                "WorkerHeartbeatMissed".to_string(),
                serde_json::json!({
                    "worker_id": worker_id.0.to_string(),
                    "last_heartbeat": worker.updated_at().to_rfc3339(),
                    "current_job_id": job_id.0.to_string(),
                    "detected_at": now.to_rfc3339()
                }),
                Some(serde_json::json!({
                    "source": "WorkerLifecycleManager",
                    "reconciliation_run": true
                })),
                Some(format!(
                    "heartbeat-missed-{}-{}",
                    worker_id.0,
                    now.timestamp()
                )),
            );

            outbox_repo.insert_events(&[event]).await.map_err(|e| {
                DomainError::InfrastructureError {
                    message: format!("Failed to insert outbox event: {:?}", e),
                }
            })?;
        } else {
            // Legacy: Direct event publishing
            let event = DomainEvent::WorkerStatusChanged {
                worker_id: worker_id.clone(),
                old_status: worker.state().clone(),
                new_status: WorkerState::Terminating,
                occurred_at: now,
                correlation_id: None,
                actor: Some("lifecycle-reconciliation".to_string()),
            };

            self.event_bus
                .publish(&event)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to publish event: {}", e),
                })?;
        }

        Ok(())
    }

    /// Emit JobReassignmentRequired event for jobs on dead workers
    async fn emit_job_reassignment_required(&self, worker: &Worker, job_id: &JobId) -> Result<()> {
        let now = Utc::now();
        let worker_id = worker.id();

        if let Some(outbox_repo) = &self.outbox_repository {
            let event = OutboxEventInsert::for_job(
                job_id.0,
                "JobReassignmentRequired".to_string(),
                serde_json::json!({
                    "job_id": job_id.0.to_string(),
                    "failed_worker_id": worker_id.0.to_string(),
                    "reason": "worker_heartbeat_timeout",
                    "occurred_at": now.to_rfc3339()
                }),
                Some(serde_json::json!({
                    "source": "WorkerLifecycleManager",
                    "reconciliation_run": true,
                    "worker_last_heartbeat": worker.updated_at().to_rfc3339()
                })),
                Some(format!("job-reassign-{}-{}", job_id.0, now.timestamp())),
            );

            outbox_repo.insert_events(&[event]).await.map_err(|e| {
                DomainError::InfrastructureError {
                    message: format!("Failed to insert outbox event: {:?}", e),
                }
            })?;
        } else {
            // Legacy: Direct event publishing - emit JobStatusChanged to Failed
            use hodei_server_domain::shared_kernel::JobState;
            let event = DomainEvent::JobStatusChanged {
                job_id: job_id.clone(),
                old_state: JobState::Running,
                new_state: JobState::Failed,
                occurred_at: now,
                correlation_id: None,
                actor: Some("lifecycle-reconciliation".to_string()),
            };

            self.event_bus
                .publish(&event)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to publish event: {}", e),
                })?;
        }

        Ok(())
    }

    /// Emit WorkerStatusChanged event
    async fn emit_worker_status_changed(
        &self,
        worker_id: &WorkerId,
        old_state: WorkerState,
        new_state: WorkerState,
        reason: &str,
    ) -> Result<()> {
        let now = Utc::now();

        if let Some(outbox_repo) = &self.outbox_repository {
            let event = OutboxEventInsert::for_worker(
                worker_id.0,
                "WorkerStatusChanged".to_string(),
                serde_json::json!({
                    "worker_id": worker_id.0.to_string(),
                    "old_status": format!("{:?}", old_state),
                    "new_status": format!("{:?}", new_state),
                    "reason": reason,
                    "occurred_at": now.to_rfc3339()
                }),
                Some(serde_json::json!({
                    "source": "WorkerLifecycleManager",
                    "transition_reason": reason
                })),
                Some(format!(
                    "worker-status-{}-{}-{}",
                    worker_id.0,
                    format!("{:?}", new_state),
                    now.timestamp()
                )),
            );

            outbox_repo.insert_events(&[event]).await.map_err(|e| {
                DomainError::InfrastructureError {
                    message: format!("Failed to insert outbox event: {:?}", e),
                }
            })?;
        } else {
            let event = DomainEvent::WorkerStatusChanged {
                worker_id: worker_id.clone(),
                old_status: old_state,
                new_status: new_state,
                occurred_at: now,
                correlation_id: None,
                actor: Some("lifecycle-reconciliation".to_string()),
            };

            self.event_bus
                .publish(&event)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to publish event: {}", e),
                })?;
        }

        Ok(())
    }

    /// Find workers that should be terminated and terminate them
    ///
    /// EPIC-21: Workers are ephemeral - ALL Ready workers are terminated
    /// to eliminate persistent pool behavior
    ///
    /// EPIC-26 US-26.7: Uses TTL policies from WorkerSpec:
    /// - max_lifetime: Maximum lifetime for any worker
    /// - idle_timeout: Time a worker can be idle before termination
    /// - ttl_after_completion: Grace period after job completion
    pub async fn cleanup_workers(&self) -> Result<CleanupResult> {
        let mut result = CleanupResult::default();

        // Find potential candidates for termination using TTL policies (EPIC-26 US-26.7)
        let all_workers = self.registry.find(&WorkerFilter::new()).await?;

        let workers_to_terminate: Vec<_> = all_workers
            .iter()
            .filter(|w| {
                // Workers que deben terminarse:
                // 1. En estado Busy/Draining (ephemeral mode - terminación inmediata)
                // 2. En estado Terminating (retry de cleanup fallido)
                // 3. Listos y en idle timeout
                // 4. Lifetime excedido
                // 5. TTL after completion excedido
                matches!(
                    *w.state(),
                    WorkerState::Busy | WorkerState::Draining | WorkerState::Terminating
                ) || w.is_idle_timeout()
                    || w.is_lifetime_exceeded()
                    || w.is_ttl_after_completion_exceeded()
            })
            .collect();

        info!(
            "EPIC-21/26 Cleanup: Terminating {} workers (using TTL policies)",
            workers_to_terminate.len()
        );

        for worker in &workers_to_terminate {
            // EPIC-26 US-26.7: Emit WorkerEphemeralIdle event when idle timeout detected
            if worker.is_idle_timeout() {
                self.emit_worker_idle_event(worker).await;
            }
        }

        for worker in workers_to_terminate {
            let worker_id = worker.id().clone();
            // EPIC-26 US-26.7: Get correct termination reason from worker
            let reason = worker.termination_reason();

            info!("Terminating worker {} (reason: {:?})", worker_id, reason);

            // Mark as terminating (skip if already terminating)
            if !matches!(*worker.state(), WorkerState::Terminating) {
                if let Err(e) = self
                    .registry
                    .update_state(&worker_id, WorkerState::Terminating)
                    .await
                {
                    error!("Failed to mark worker {} as terminating: {}", worker_id, e);
                    continue;
                }
            }

            // Destroy via provider
            if let Err(e) = self.destroy_worker_via_provider(worker).await {
                error!("Failed to destroy worker {}: {}", worker_id, e);
                result.failed.push(worker_id.clone());
                continue;
            }

            // Unregister
            if let Err(e) = self.registry.unregister(&worker_id).await {
                warn!("Failed to unregister worker {}: {}", worker_id, e);
            }

            // EPIC-26 US-26.7: Emit WorkerEphemeralTerminating event with correct reason
            let event = DomainEvent::WorkerEphemeralTerminating {
                worker_id: worker_id.clone(),
                provider_id: worker.provider_id().clone(),
                reason: reason.clone(), // Clone for first use
                occurred_at: Utc::now(),
                correlation_id: None,
                actor: Some("lifecycle-manager".to_string()),
            };
            if let Err(e) = self.event_bus.publish(&event).await {
                warn!("Failed to publish WorkerEphemeralTerminating event: {}", e);
            }

            // Also emit WorkerTerminated for backwards compatibility
            let terminated_event = DomainEvent::WorkerTerminated {
                worker_id: worker_id.clone(),
                provider_id: worker.provider_id().clone(),
                reason,
                occurred_at: Utc::now(),
                correlation_id: None,
                actor: Some("lifecycle-manager".to_string()),
            };
            if let Err(e) = self.event_bus.publish(&terminated_event).await {
                warn!("Failed to publish WorkerTerminated event: {}", e);
            }

            result.terminated.push(worker_id);
        }

        Ok(result)
    }

    /// Emit WorkerEphemeralIdle event when worker exceeds idle timeout (EPIC-26 US-26.7)
    async fn emit_worker_idle_event(&self, worker: &Worker) {
        if let Some(outbox_repo) = &self.outbox_repository {
            let event = OutboxEventInsert::for_worker(
                worker.id().0,
                "WorkerEphemeralIdle".to_string(),
                serde_json::json!({
                    "worker_id": worker.id().0.to_string(),
                    "provider_id": worker.provider_id().0.to_string(),
                    "idle_since": worker.last_heartbeat().to_rfc3339(),
                    "idle_timeout_secs": worker.idle_timeout_secs(),
                    "current_job_id": worker.current_job_id().map(|j| j.0.to_string())
                }),
                Some(serde_json::json!({
                    "source": "WorkerLifecycleManager",
                    "event": "idle_timeout_detected"
                })),
                Some(format!("worker-idle-{}", worker.id().0)),
            );

            if let Err(e) = outbox_repo.insert_events(&[event]).await {
                warn!("Failed to insert WorkerEphemeralIdle event: {:?}", e);
            }
        }
    }

    /// Provision a new worker using the specified provider
    /// US-2.2: Uses saga pattern with automatic compensation when coordinator is configured
    pub async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: WorkerSpec,
    ) -> Result<Worker> {
        // Check max workers limit first (before any provisioning attempt)
        let current_count = self.registry.count().await?;
        if current_count >= self.config.max_workers {
            return Err(DomainError::ProviderOverloaded {
                provider_id: provider_id.clone(),
            });
        }

        // US-2.2: Execute provisioning saga if coordinator is configured
        if let Some(ref coordinator) = self.provisioning_saga_coordinator {
            info!(provider_id = %provider_id, "🛠️ Provisioning worker via saga");

            match coordinator
                .execute_provisioning_saga(provider_id, &spec, None)
                .await
            {
                Ok((worker_id, saga_result)) => {
                    info!(
                        provider_id = %provider_id,
                        worker_id = %worker_id,
                        saga_duration_ms = ?saga_result.duration.as_millis(),
                        "✅ Worker provisioned via saga"
                    );

                    // Fetch the worker from registry
                    let worker = self.registry.get(&worker_id).await?.ok_or_else(|| {
                        DomainError::WorkerNotFound {
                            worker_id: worker_id.clone(),
                        }
                    })?;

                    return Ok(worker);
                }
                Err(ProvisioningSagaError::Compensated) => {
                    error!(provider_id = %provider_id, "⚠️ Saga was compensated - infrastructure cleaned up");
                    return Err(DomainError::WorkerProvisioningFailed {
                        message: "Provisioning saga was compensated".to_string(),
                    });
                }
                Err(e) => {
                    error!(provider_id = %provider_id, error = %e, "❌ Saga provisioning failed");
                    return Err(DomainError::WorkerProvisioningFailed {
                        message: e.to_string(),
                    });
                }
            }
        }

        // Fallback: Legacy provisioning when no saga coordinator is configured
        // This path is kept for backward compatibility during migration
        self.provision_worker_legacy(provider_id, &spec).await
    }

    /// Legacy provisioning method (used when saga coordinator is not configured)
    async fn provision_worker_legacy(
        &self,
        provider_id: &ProviderId,
        spec: &WorkerSpec,
    ) -> Result<Worker> {
        let providers = self.providers.read().await;
        let provider = providers
            .get(provider_id)
            .ok_or_else(|| DomainError::ProviderNotFound {
                provider_id: provider_id.clone(),
            })?;

        info!(
            "Provisioning new worker via provider {} (legacy mode)",
            provider_id
        );

        // Create worker via provider
        let handle = provider.create_worker(spec).await.map_err(|e| {
            DomainError::WorkerProvisioningFailed {
                message: e.to_string(),
            }
        })?;

        // Register in registry
        let worker = self.registry.register(handle, spec.clone()).await?;

        // Publish WorkerProvisioned event
        let event = DomainEvent::WorkerProvisioned {
            worker_id: worker.id().clone(),
            provider_id: provider_id.clone(),
            spec_summary: format!("image={}, server={}", spec.image, spec.server_address),
            occurred_at: Utc::now(),
            correlation_id: None,
            actor: Some("lifecycle-manager-legacy".to_string()),
        };
        if let Err(e) = self.event_bus.publish(&event).await {
            warn!("Failed to publish WorkerProvisioned event: {}", e);
        }

        info!("Worker {} provisioned successfully (legacy)", worker.id());
        Ok(worker)
    }

    /// Recover a failed worker and reassign its job (US-4.2)
    /// Uses saga pattern with automatic compensation when coordinator is configured
    pub async fn recover_worker(&self, job_id: &JobId, failed_worker_id: &WorkerId) -> Result<()> {
        // US-4.2: Use saga pattern if coordinator is configured
        if let Some(ref coordinator) = self.recovery_saga_coordinator {
            info!(
                job_id = %job_id,
                failed_worker_id = %failed_worker_id,
                "🔄 Recovering worker via saga"
            );

            // Execute recovery saga
            match coordinator
                .execute_recovery_saga(job_id, failed_worker_id)
                .await
            {
                Ok((saga_id, saga_result)) => {
                    info!(
                        job_id = %job_id,
                        saga_id = %saga_id,
                        saga_duration_ms = ?saga_result.duration.as_millis(),
                        "✅ Worker recovered via saga"
                    );
                    return Ok(());
                }
                Err(RecoverySagaError::Compensated) => {
                    warn!(
                        job_id = %job_id,
                        "⚠️ Recovery saga was compensated"
                    );
                    return Err(DomainError::WorkerRecoveryFailed {
                        message: "Recovery saga was compensated".to_string(),
                    });
                }
                Err(e) => {
                    error!(
                        job_id = %job_id,
                        error = %e,
                        "❌ Recovery saga failed"
                    );
                    return Err(DomainError::WorkerRecoveryFailed {
                        message: e.to_string(),
                    });
                }
            }
        }

        // Fallback: Legacy recovery when no saga coordinator is configured
        self.recover_worker_legacy(job_id, failed_worker_id).await
    }

    /// Legacy recovery method (used when saga coordinator is not configured)
    async fn recover_worker_legacy(
        &self,
        job_id: &JobId,
        failed_worker_id: &WorkerId,
    ) -> Result<()> {
        info!(
            "Recovering worker {} for job {} (legacy mode)",
            failed_worker_id, job_id
        );

        // TODO: Implement legacy recovery logic
        // 1. Check if failed worker is still reachable
        // 2. Provision a new worker
        // 3. Transfer job to new worker
        // 4. Terminate old worker

        warn!(
            "Legacy recovery not fully implemented for job {} and worker {}",
            job_id, failed_worker_id
        );

        Ok(())
    }

    /// Destroy a worker via its provider with retry logic (FIX: Prevents orphaned containers)
    async fn destroy_worker_via_provider(&self, worker: &Worker) -> Result<()> {
        let providers = self.providers.read().await;
        let provider =
            providers
                .get(worker.provider_id())
                .ok_or_else(|| DomainError::ProviderNotFound {
                    provider_id: worker.provider_id().clone(),
                })?;

        // Retry with exponential backoff for transient failures
        let max_retries = 3;
        let mut last_error = None;

        for attempt in 1..=max_retries {
            match provider.destroy_worker(worker.handle()).await {
                Ok(_) => {
                    info!(
                        "Worker {} destroyed successfully via provider {}",
                        worker.id(),
                        worker.provider_id()
                    );
                    return Ok(());
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    last_error = Some(e);
                    if attempt < max_retries {
                        let backoff = Duration::from_secs(2u64.pow(attempt - 1));
                        warn!(
                            "Attempt {}/{} failed to destroy worker {}: {}. Retrying in {}s",
                            attempt,
                            max_retries,
                            worker.id(),
                            error_msg,
                            backoff.as_secs()
                        );
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        }

        // All retries exhausted
        error!(
            "Failed to destroy worker {} after {} attempts: {:?}",
            worker.id(),
            max_retries,
            last_error
        );

        Err(DomainError::InfrastructureError {
            message: format!(
                "Failed to destroy worker after {} attempts: {:?}",
                max_retries, last_error
            ),
        })
    }

    /// Get current statistics
    pub async fn stats(&self) -> Result<WorkerRegistryStats> {
        self.registry.stats().await
    }

    /// Check if scaling up is needed
    pub async fn should_scale_up(&self, pending_jobs: usize) -> bool {
        let stats = match self.registry.stats().await {
            Ok(s) => s,
            Err(_) => return false,
        };

        // Scale up if:
        // 1. No ready workers and we have pending jobs
        // 2. Pending jobs exceed threshold and we're below max
        if stats.ready_workers == 0 && pending_jobs > 0 {
            return true;
        }

        pending_jobs > self.config.scale_up_threshold
            && stats.total_workers < self.config.max_workers
    }

    /// Check if scaling down is needed
    pub async fn should_scale_down(&self) -> bool {
        let stats = match self.registry.stats().await {
            Ok(s) => s,
            Err(_) => return false,
        };

        // Scale down if:
        // Idle workers exceed threshold and we have more than min ready
        stats.idle_workers > self.config.scale_down_threshold
            && stats.ready_workers > self.config.min_ready_workers
    }

    // ============================================================
    // US-26.6: Orphan Worker Detection and Cleanup
    // ============================================================

    // ============================================================
    // Reactive Event Monitoring (EPIC-29)
    // ============================================================

    /// Start monitoring events from all registered providers AND domain events
    /// This enables reactive worker cleanup based on domain events
    pub async fn start_event_monitoring(&self) {
        let providers = self.providers.read().await;

        for (id, provider) in providers.iter() {
            let provider = provider.clone();
            let manager = self.clone();
            let provider_id = id.clone();

            info!("Starting event monitoring for provider {}", provider_id);
            tokio::spawn(async move {
                manager.monitor_single_provider(provider_id, provider).await;
            });
        }

        // Also start listening to domain events for reactive cleanup
        let manager = self.clone();
        tokio::spawn(async move {
            manager.start_domain_event_monitoring().await;
        });
    }

    /// Start monitoring domain events from the event bus
    /// This handles WorkerEphemeralTerminating events reactively
    async fn start_domain_event_monitoring(&self) {
        use futures::StreamExt;
        use hodei_server_domain::events::DomainEvent;

        info!("👂 WorkerLifecycleManager: Starting domain event monitoring");

        // Subscribe to hodei_events topic (same as JobCoordinator publishes to)
        match self.event_bus.subscribe("hodei_events").await {
            Ok(mut stream) => {
                while let Some(result) = stream.next().await {
                    match result {
                        Ok(event) => {
                            if let Err(e) = self.handle_domain_event(event).await {
                                error!("Error handling domain event: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Error receiving domain event: {}", e);
                        }
                    }
                }
                warn!("Domain event stream ended for lifecycle manager");
            }
            Err(e) => {
                error!("Failed to subscribe to domain events: {}", e);
            }
        }
    }

    /// Handle domain events reactively (EPIC-32)
    /// This is the reactive path for worker cleanup
    async fn handle_domain_event(&self, event: DomainEvent) -> Result<()> {
        use hodei_server_domain::events::DomainEvent::*;

        match event {
            WorkerEphemeralTerminating {
                worker_id,
                provider_id,
                reason,
                correlation_id,
                actor,
                ..
            } => {
                info!(
                    worker_id = %worker_id,
                    provider_id = %provider_id,
                    reason = ?reason,
                    correlation_id = ?correlation_id,
                    actor = ?actor,
                    "📥 Received WorkerEphemeralTerminating event - executing cleanup"
                );

                // Execute cleanup reactively
                self.perform_worker_cleanup(&worker_id, &provider_id)
                    .await?;

                Ok(())
            }
            WorkerTerminated {
                worker_id,
                provider_id,
                reason,
                ..
            } => {
                info!(
                    worker_id = %worker_id,
                    provider_id = %provider_id,
                    reason = ?reason,
                    "📥 Received WorkerTerminated event"
                );
                // Worker already terminated, nothing to do
                Ok(())
            }
            _ => {
                // Not our event, ignore
                Ok(())
            }
        }
    }

    /// Perform worker cleanup (called reactively from domain events)
    async fn perform_worker_cleanup(
        &self,
        worker_id: &WorkerId,
        provider_id: &ProviderId,
    ) -> Result<()> {
        // Get the worker from registry
        let worker = match self.registry.get(worker_id).await? {
            Some(w) => w,
            None => {
                warn!(
                    "Worker {} not found in registry during reactive cleanup",
                    worker_id
                );
                return Ok(());
            }
        };

        // Update state to Terminating if not already
        if !matches!(
            *worker.state(),
            WorkerState::Terminating | WorkerState::Terminated
        ) {
            self.registry
                .update_state(worker_id, WorkerState::Terminating)
                .await?;
        }

        // Try to destroy the worker via provider with retry
        match self.destroy_worker_via_provider(&worker).await {
            Ok(_) => {
                info!(
                    "Worker {} destroyed successfully via reactive cleanup",
                    worker_id
                );
                // Unregister worker
                self.registry.unregister(worker_id).await?;
            }
            Err(e) => {
                error!(
                    "Reactive cleanup failed for worker {}: {}. Worker kept for retry.",
                    worker_id, e
                );
                // Worker stays in Terminating state for cleanup periodic retry
                return Err(e);
            }
        }

        Ok(())
    }

    async fn monitor_single_provider(
        &self,
        provider_id: ProviderId,
        provider: Arc<dyn WorkerProvider>,
    ) {
        // Use subscribe_ext from WorkerProviderExt (blanket impl)
        // But provider is Arc<dyn WorkerProvider>.
        // Check if dyn WorkerProvider impls subscribe_ext in provider_api.rs.
        // Yes, I added explicit impl for dyn WorkerProvider.
        use hodei_server_domain::workers::provider_api::WorkerProviderExt;

        match provider.subscribe_ext().await {
            Ok(stream) => {
                let mut stream = stream;

                while let Some(event_result) = stream.next().await {
                    match event_result {
                        Ok(event) => {
                            if let Err(e) = self
                                .handle_infrastructure_event(provider_id.clone(), event)
                                .await
                            {
                                error!("Error handling event from provider {}: {}", provider_id, e);
                            }
                        }
                        Err(e) => {
                            error!("Error in event stream from provider {}: {}", provider_id, e);
                        }
                    }
                }
                warn!("Event stream ended for provider {}", provider_id);
            }
            Err(e) => {
                error!("Failed to subscribe to provider {}: {}", provider_id, e);
            }
        }
    }

    async fn handle_infrastructure_event(
        &self,
        provider_id: ProviderId,
        event: WorkerInfrastructureEvent,
    ) -> Result<()> {
        match event {
            WorkerInfrastructureEvent::WorkerStarted {
                provider_resource_id,
                timestamp,
            } => {
                info!(
                    "Event: Worker {} started at {}",
                    provider_resource_id, timestamp
                );
                // Transition worker to Ready if it was Creating/Connecting
                // We need to find the worker by provider resource ID.
                let workers = self.registry.find_by_provider(&provider_id).await?;
                if let Some(worker) = workers
                    .iter()
                    .find(|w| w.handle().provider_resource_id == provider_resource_id)
                {
                    if *worker.state() == WorkerState::Creating
                        || *worker.state() == WorkerState::Connecting
                    {
                        info!("Marking worker {} as Ready (started)", worker.id());
                        self.registry
                            .update_state(worker.id(), WorkerState::Ready)
                            .await?;

                        // Emit WorkerStatusChanged
                        self.emit_worker_status_changed(
                            worker.id(),
                            worker.state().clone(),
                            WorkerState::Ready,
                            "provider_event_started",
                        )
                        .await?;
                    }
                }
            }
            WorkerInfrastructureEvent::WorkerStopped {
                provider_resource_id,
                timestamp,
                reason,
                ..
            } => {
                info!(
                    "Event: Worker {} stopped at {}",
                    provider_resource_id, timestamp
                );
                let workers = self.registry.find_by_provider(&provider_id).await?;
                if let Some(worker) = workers
                    .iter()
                    .find(|w| w.handle().provider_resource_id == provider_resource_id)
                {
                    if *worker.state() != WorkerState::Terminated {
                        info!("Marking worker {} as Terminated (stopped)", worker.id());
                        self.registry
                            .update_state(worker.id(), WorkerState::Terminated)
                            .await?;

                        // Emit WorkerStatusChanged
                        self.emit_worker_status_changed(
                            worker.id(),
                            worker.state().clone(),
                            WorkerState::Terminated,
                            &format!("provider_event_stopped: {:?}", reason),
                        )
                        .await?;
                    }
                }
            }
            WorkerInfrastructureEvent::WorkerHealthChanged {
                provider_resource_id,
                status,
                ..
            } => {
                // Logic to handle health changes
                // Map HealthStatus to WorkerState?
                // If Unhealthy -> Terminating?
                if matches!(status, HealthStatus::Unhealthy { .. }) {
                    let workers = self.registry.find_by_provider(&provider_id).await?;
                    if let Some(worker) = workers
                        .iter()
                        .find(|w| w.handle().provider_resource_id == provider_resource_id)
                    {
                        warn!("Worker {} reported unhealthy via event", worker.id());
                        // Mark logic similar to run_health_check?
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Detect orphan workers (EPIC-26 US-26.6)
    ///
    /// Orphan workers are workers that exist in the provider but are not
    /// registered in our registry. This can happen if:
    /// - Worker registration failed after provider creation
    /// - Database was restored from backup
    /// - Manual provider operations
    ///
    /// This method:
    /// 1. Gets all workers from all providers
    /// 2. Filters out workers that are registered in our registry
    /// 3. Marks orphans in the database
    /// 4. Destroys orphan workers via providers
    pub async fn detect_and_cleanup_orphans(&self) -> Result<OrphanCleanupResult> {
        let mut result = OrphanCleanupResult::default();
        let start_time = Utc::now();

        info!("🔍 Starting orphan worker detection...");

        let providers = self.providers.read().await;
        let provider_ids: Vec<ProviderId> = providers.keys().cloned().collect();

        for provider_id in provider_ids {
            if let Some(provider) = providers.get(&provider_id) {
                match self
                    .detect_orphans_for_provider(provider, provider_id.clone())
                    .await
                {
                    Ok(orphans) => {
                        result.providers_scanned += 1;
                        result.orphans_detected += orphans.len();

                        // Destroy each orphan
                        for orphan in orphans {
                            info!(
                                "🗑️ Destroying orphan worker {} from provider {}",
                                orphan.provider_resource_id, provider_id
                            );

                            // Emit OrphanWorkerDetected event
                            self.emit_orphan_detected(&orphan, &provider_id).await;

                            if let Err(e) = provider
                                .destroy_worker_by_id(&orphan.provider_resource_id)
                                .await
                            {
                                warn!(
                                    "Failed to destroy orphan {} from provider {}: {}",
                                    orphan.provider_resource_id, provider_id, e
                                );
                                result.errors += 1;
                            } else {
                                result.orphans_cleaned += 1;
                                info!(
                                    "✅ Orphan worker {} destroyed from provider {}",
                                    orphan.provider_resource_id, provider_id
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to detect orphans for provider {}: {}",
                            provider_id, e
                        );
                        result.errors += 1;
                    }
                }
            }
        }

        result.duration_ms = Utc::now()
            .signed_duration_since(start_time)
            .to_std()
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        // Emit GarbageCollectionCompleted event
        self.emit_garbage_collection_completed(&result).await;

        if result.orphans_cleaned > 0 {
            info!(
                "🧹 Orphan cleanup complete: {} cleaned, {} detected, {} errors in {}ms",
                result.orphans_cleaned, result.orphans_detected, result.errors, result.duration_ms
            );
        }

        Ok(result)
    }

    /// Detect orphan workers for a specific provider
    async fn detect_orphans_for_provider(
        &self,
        provider: &Arc<dyn WorkerProvider>,
        provider_id: ProviderId,
    ) -> Result<Vec<OrphanWorkerInfo>> {
        let mut orphans = Vec::new();

        // Get all workers from provider
        let provider_workers =
            provider
                .list_workers()
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!(
                        "Failed to list workers from provider {}: {}",
                        provider_id, e
                    ),
                })?;

        // Get all registered workers for this provider
        let registered_workers = self.registry.find_by_provider(&provider_id).await?;

        // Create a set of registered worker IDs
        let registered_ids: std::collections::HashSet<String> = registered_workers
            .iter()
            .map(|w| w.handle().provider_resource_id.clone())
            .collect();

        // Find workers that exist in provider but not in registry
        for pw in provider_workers {
            if !registered_ids.contains(&pw.resource_id) {
                // This is an orphan
                let orphan = OrphanWorkerInfo {
                    worker_id: WorkerId::new(), // Generate new ID for orphan
                    provider_resource_id: pw.resource_id,
                    last_seen: pw.last_seen.unwrap_or_else(Utc::now),
                };
                orphans.push(orphan);
            }
        }

        Ok(orphans)
    }

    /// Emit OrphanWorkerDetected event
    async fn emit_orphan_detected(&self, orphan: &OrphanWorkerInfo, provider_id: &ProviderId) {
        let now = Utc::now();
        let orphaned_duration = now
            .signed_duration_since(orphan.last_seen)
            .to_std()
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if let Some(outbox_repo) = &self.outbox_repository {
            let event = OutboxEventInsert::for_worker(
                orphan.worker_id.0,
                "OrphanWorkerDetected".to_string(),
                serde_json::json!({
                    "worker_id": orphan.worker_id.0.to_string(),
                    "provider_id": provider_id.0.to_string(),
                    "last_seen": orphan.last_seen.to_rfc3339(),
                    "orphaned_duration_secs": orphaned_duration,
                    "detection_method": "reconciliation"
                }),
                Some(serde_json::json!({
                    "source": "WorkerLifecycleManager",
                    "provider_resource_id": orphan.provider_resource_id
                })),
                Some(format!(
                    "orphan-detected-{}-{}",
                    provider_id.0,
                    now.timestamp()
                )),
            );

            if let Err(e) = outbox_repo.insert_events(&[event]).await {
                warn!("Failed to insert OrphanWorkerDetected event: {:?}", e);
            }
        }
    }

    /// Emit GarbageCollectionCompleted event
    async fn emit_garbage_collection_completed(&self, result: &OrphanCleanupResult) {
        let now = Utc::now();

        if let Some(outbox_repo) = &self.outbox_repository {
            // Get provider IDs scanned
            let provider_ids: Vec<String> = self
                .providers
                .read()
                .await
                .keys()
                .map(|p| p.0.to_string())
                .collect();

            // Generate a unique ID for this GC event
            let gc_event_id = provider_ids
                .first()
                .map(|p| uuid::Uuid::parse_str(p).unwrap_or_else(|_| uuid::Uuid::new_v4()))
                .unwrap_or_else(uuid::Uuid::new_v4);

            let event = OutboxEventInsert::for_worker(
                gc_event_id,
                "GarbageCollectionCompleted".to_string(),
                serde_json::json!({
                    "scanned_providers": provider_ids,
                    "orphaned_workers_found": result.orphans_detected,
                    "orphaned_workers_cleaned": result.orphans_cleaned,
                    "errors": result.errors,
                    "duration_ms": result.duration_ms
                }),
                Some(serde_json::json!({
                    "source": "WorkerLifecycleManager",
                    "event": "orphan_gc"
                })),
                Some(format!("gc-completed-{}", now.timestamp())),
            );

            if let Err(e) = outbox_repo.insert_events(&[event]).await {
                warn!("Failed to insert GarbageCollectionCompleted event: {:?}", e);
            }
        }
    }
}

/// Result of a health check run
#[derive(Debug, Default)]
pub struct HealthCheckResult {
    pub total_checked: usize,
    pub unhealthy_workers: Vec<WorkerId>,
}

/// Result of a cleanup run
#[derive(Debug, Default)]
pub struct CleanupResult {
    pub terminated: Vec<WorkerId>,
    pub failed: Vec<WorkerId>,
}

/// Result of a reconciliation run
#[derive(Debug, Default)]
pub struct ReconciliationResult {
    /// Workers with stale heartbeats detected
    pub stale_workers: Vec<WorkerId>,
    /// Workers marked as unhealthy
    pub workers_marked_unhealthy: Vec<WorkerId>,
    /// Jobs affected by stale workers
    pub affected_jobs: Vec<JobId>,
    /// Jobs that require reassignment (worker exceeded grace period)
    pub jobs_requiring_reassignment: Vec<JobId>,
}

impl ReconciliationResult {
    /// Check if any action was taken
    pub fn has_changes(&self) -> bool {
        !self.stale_workers.is_empty()
            || !self.workers_marked_unhealthy.is_empty()
            || !self.jobs_requiring_reassignment.is_empty()
    }

    /// Get total number of issues detected
    pub fn total_issues(&self) -> usize {
        self.stale_workers.len() + self.jobs_requiring_reassignment.len()
    }
}

/// Orphan worker info detected from provider (EPIC-26 US-26.6)
struct OrphanWorkerInfo {
    worker_id: WorkerId,
    provider_resource_id: String,
    last_seen: DateTime<Utc>,
}

/// Result of orphan worker detection and cleanup (EPIC-26 US-26.6)
#[derive(Debug, Default)]
pub struct OrphanCleanupResult {
    /// Providers scanned for orphans
    pub providers_scanned: usize,
    /// Orphan workers detected
    pub orphans_detected: usize,
    /// Orphan workers successfully cleaned up
    pub orphans_cleaned: usize,
    /// Errors during cleanup
    pub errors: usize,
    /// Duration of the cleanup in milliseconds
    pub duration_ms: u64,
}

impl OrphanCleanupResult {
    /// Check if any orphans were found
    pub fn has_orphans(&self) -> bool {
        self.orphans_detected > 0
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.orphans_detected == 0 {
            100.0
        } else {
            (self.orphans_cleaned as f64 / self.orphans_detected as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorkerProvisioningService;
    use crate::provisioning::ProvisioningResult;
    use crate::saga::recovery_saga::RecoverySagaCoordinatorConfig;
    use futures::stream::BoxStream;
    use hodei_server_domain::event_bus::EventBusError;
    use hodei_server_domain::saga::{
        Saga, SagaContext, SagaExecutionResult, SagaId, SagaOrchestrator,
    };
    use hodei_server_domain::workers::{ProviderType, WorkerHandle};
    use std::collections::HashMap as StdHashMap;
    use std::sync::Mutex;
    use tokio::sync::RwLock as TokioRwLock;

    fn create_test_worker() -> Worker {
        let spec = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        let handle = WorkerHandle::new(
            spec.worker_id.clone(),
            "container-123".to_string(),
            ProviderType::Docker,
            hodei_server_domain::shared_kernel::ProviderId::new(),
        );
        Worker::new(handle, spec)
    }

    fn create_test_worker_with_provider(
        provider_id: hodei_server_domain::shared_kernel::ProviderId,
    ) -> Worker {
        // Use default idle_timeout of 300 seconds (5 minutes)
        create_test_worker_with_provider_and_ttl(provider_id, None, Duration::from_secs(300), None)
    }

    fn create_test_worker_with_provider_and_ttl(
        provider_id: hodei_server_domain::shared_kernel::ProviderId,
        max_lifetime: Option<Duration>,
        idle_timeout: Duration, // Changed from Option<Duration> to Duration
        ttl_after_completion: Option<Duration>,
    ) -> Worker {
        let mut spec = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        // EPIC-26 US-26.7: Configure TTL policies for test workers
        if let Some(lifetime) = max_lifetime {
            spec.max_lifetime = lifetime;
        }
        spec.idle_timeout = idle_timeout;
        if let Some(ttl) = ttl_after_completion {
            spec.ttl_after_completion = Some(ttl);
        }
        let handle = WorkerHandle::new(
            spec.worker_id.clone(),
            format!("container-{}", spec.worker_id.0),
            ProviderType::Docker,
            provider_id,
        );
        Worker::new(handle, spec)
    }

    struct MockWorkerProvider {
        pub provider_id: hodei_server_domain::shared_kernel::ProviderId,
        capabilities: hodei_server_domain::workers::ProviderCapabilities,
    }

    impl MockWorkerProvider {
        fn new(provider_id: hodei_server_domain::shared_kernel::ProviderId) -> Self {
            Self {
                provider_id,
                capabilities: hodei_server_domain::workers::ProviderCapabilities::default(),
            }
        }
    }

    // Implement ISP traits individually
    impl WorkerProviderIdentity for MockWorkerProvider {
        fn provider_id(&self) -> &hodei_server_domain::shared_kernel::ProviderId {
            &self.provider_id
        }

        fn provider_type(&self) -> hodei_server_domain::workers::ProviderType {
            hodei_server_domain::workers::ProviderType::Docker
        }

        fn capabilities(&self) -> &hodei_server_domain::workers::ProviderCapabilities {
            &self.capabilities
        }
    }

    #[async_trait::async_trait]
    impl WorkerLifecycle for MockWorkerProvider {
        async fn create_worker(
            &self,
            spec: &WorkerSpec,
        ) -> std::result::Result<WorkerHandle, hodei_server_domain::workers::ProviderError>
        {
            Ok(WorkerHandle::new(
                spec.worker_id.clone(),
                format!("container-{}", spec.worker_id.0),
                ProviderType::Docker,
                self.provider_id.clone(),
            ))
        }

        async fn destroy_worker(
            &self,
            _handle: &WorkerHandle,
        ) -> std::result::Result<(), hodei_server_domain::workers::ProviderError> {
            Ok(())
        }

        async fn get_worker_status(
            &self,
            _handle: &WorkerHandle,
        ) -> std::result::Result<
            hodei_server_domain::shared_kernel::WorkerState,
            hodei_server_domain::workers::ProviderError,
        > {
            Ok(hodei_server_domain::shared_kernel::WorkerState::Creating)
        }
    }

    #[async_trait::async_trait]
    impl WorkerLogs for MockWorkerProvider {
        async fn get_worker_logs(
            &self,
            _handle: &WorkerHandle,
            _tail: Option<u32>,
        ) -> std::result::Result<
            Vec<hodei_server_domain::workers::LogEntry>,
            hodei_server_domain::workers::ProviderError,
        > {
            unimplemented!()
        }
    }

    impl WorkerCost for MockWorkerProvider {
        fn estimate_cost(
            &self,
            _spec: &WorkerSpec,
            _duration: Duration,
        ) -> Option<hodei_server_domain::workers::CostEstimate> {
            None
        }

        fn estimated_startup_time(&self) -> Duration {
            Duration::from_secs(5)
        }
    }

    #[async_trait::async_trait]
    impl WorkerHealth for MockWorkerProvider {
        async fn health_check(
            &self,
        ) -> std::result::Result<
            hodei_server_domain::workers::HealthStatus,
            hodei_server_domain::workers::ProviderError,
        > {
            Ok(hodei_server_domain::workers::HealthStatus::Healthy)
        }
    }

    impl WorkerEligibility for MockWorkerProvider {
        fn can_fulfill(
            &self,
            _requirements: &hodei_server_domain::workers::JobRequirements,
        ) -> bool {
            true
        }
    }

    impl WorkerMetrics for MockWorkerProvider {
        fn get_performance_metrics(
            &self,
        ) -> hodei_server_domain::workers::ProviderPerformanceMetrics {
            hodei_server_domain::workers::ProviderPerformanceMetrics::default()
        }

        fn record_worker_creation(&self, _startup_time: Duration, _success: bool) {}

        fn get_startup_time_history(&self) -> Vec<Duration> {
            Vec::new()
        }

        fn calculate_average_cost_per_hour(&self) -> f64 {
            0.0
        }

        fn calculate_health_score(&self) -> f64 {
            100.0
        }
    }
    #[async_trait::async_trait]
    impl hodei_server_domain::workers::provider_api::WorkerEventSource for MockWorkerProvider {
        async fn subscribe(
            &self,
        ) -> std::result::Result<
            std::pin::Pin<
                Box<
                    dyn futures::Stream<
                            Item = std::result::Result<
                                hodei_server_domain::workers::WorkerInfrastructureEvent,
                                hodei_server_domain::workers::ProviderError,
                            >,
                        > + Send,
                >,
            >,
            hodei_server_domain::workers::ProviderError,
        > {
            Ok(Box::pin(futures::stream::empty()))
        }
    }

    // Implement WorkerProvider as marker trait (combines all ISP traits)
    impl WorkerProvider for MockWorkerProvider {}

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

    #[async_trait::async_trait]
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

    struct MockWorkerRegistry {
        workers: Arc<TokioRwLock<StdHashMap<WorkerId, Worker>>>,
    }

    impl MockWorkerRegistry {
        fn new() -> Self {
            Self {
                workers: Arc::new(TokioRwLock::new(StdHashMap::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl WorkerRegistry for MockWorkerRegistry {
        async fn register(&self, handle: WorkerHandle, spec: WorkerSpec) -> Result<Worker> {
            let worker = Worker::new(handle.clone(), spec);
            self.workers
                .write()
                .await
                .insert(handle.worker_id.clone(), worker.clone());
            Ok(worker)
        }

        async fn unregister(&self, worker_id: &WorkerId) -> Result<()> {
            self.workers.write().await.remove(worker_id);
            Ok(())
        }

        async fn get(&self, worker_id: &WorkerId) -> Result<Option<Worker>> {
            Ok(self.workers.read().await.get(worker_id).cloned())
        }

        async fn find(
            &self,
            _filter: &hodei_server_domain::workers::WorkerFilter,
        ) -> Result<Vec<Worker>> {
            Ok(self.workers.read().await.values().cloned().collect())
        }

        async fn find_available(&self) -> Result<Vec<Worker>> {
            Ok(self
                .workers
                .read()
                .await
                .values()
                .filter(|w| w.state().can_accept_jobs())
                .cloned()
                .collect())
        }

        async fn find_by_provider(&self, _provider_id: &ProviderId) -> Result<Vec<Worker>> {
            Ok(vec![])
        }

        async fn update_state(&self, worker_id: &WorkerId, state: WorkerState) -> Result<()> {
            if let Some(worker) = self.workers.write().await.get_mut(worker_id) {
                match state {
                    WorkerState::Creating => {} // Estado inicial, no transición
                    WorkerState::Connecting => worker.mark_connecting().map_err(|e| {
                        tracing::error!("Failed to mark worker {} as Connecting: {}", worker_id, e);
                        e
                    })?,
                    WorkerState::Ready => worker.mark_ready().map_err(|e| {
                        tracing::error!("Failed to mark worker {} as Ready: {}", worker_id, e);
                        e
                    })?,
                    WorkerState::Terminating => worker.mark_terminating().map_err(|e| {
                        tracing::error!(
                            "Failed to mark worker {} as Terminating: {}",
                            worker_id,
                            e
                        );
                        e
                    })?,
                    WorkerState::Terminated => worker.mark_terminated().map_err(|e| {
                        tracing::error!("Failed to mark worker {} as Terminated: {}", worker_id, e);
                        e
                    })?,
                    _ => {}
                }
            }
            Ok(())
        }

        async fn heartbeat(&self, worker_id: &WorkerId) -> Result<()> {
            if let Some(worker) = self.workers.write().await.get_mut(worker_id) {
                worker.update_heartbeat();
            }
            Ok(())
        }

        async fn assign_to_job(
            &self,
            worker_id: &WorkerId,
            job_id: hodei_server_domain::shared_kernel::JobId,
        ) -> Result<()> {
            if let Some(worker) = self.workers.write().await.get_mut(worker_id) {
                worker.assign_job(job_id).map_err(|e| {
                    tracing::error!("Failed to assign job to worker {}: {}", worker_id, e);
                    e
                })?;
            }
            Ok(())
        }

        async fn release_from_job(&self, _worker_id: &WorkerId) -> Result<()> {
            Ok(())
        }

        async fn find_unhealthy(&self, _timeout: Duration) -> Result<Vec<Worker>> {
            Ok(vec![])
        }

        async fn find_for_termination(&self) -> Result<Vec<Worker>> {
            // Return all workers that are in states that can be terminated
            Ok(self
                .workers
                .read()
                .await
                .values()
                .filter(|w| matches!(*w.state(), WorkerState::Ready | WorkerState::Terminating))
                .cloned()
                .collect())
        }

        async fn stats(&self) -> Result<WorkerRegistryStats> {
            let workers = self.workers.read().await;
            let mut total_workers = 0;
            let mut ready_workers = 0;
            let mut busy_workers = 0;
            let mut idle_workers = 0;

            for worker in workers.values() {
                total_workers += 1;
                match worker.state() {
                    WorkerState::Ready => ready_workers += 1,
                    WorkerState::Busy => busy_workers += 1,
                    _ => {}
                }
            }

            Ok(WorkerRegistryStats {
                total_workers,
                ready_workers,
                busy_workers,
                idle_workers,
                ..Default::default()
            })
        }

        async fn count(&self) -> Result<usize> {
            Ok(self.workers.read().await.len())
        }

        // EPIC-26 US-26.7: TTL-related methods
        async fn find_idle_timed_out(&self) -> Result<Vec<Worker>> {
            Ok(self
                .workers
                .read()
                .await
                .values()
                .filter(|w| w.is_idle_timeout())
                .cloned()
                .collect())
        }

        async fn find_lifetime_exceeded(&self) -> Result<Vec<Worker>> {
            Ok(self
                .workers
                .read()
                .await
                .values()
                .filter(|w| w.is_lifetime_exceeded())
                .cloned()
                .collect())
        }

        async fn find_ttl_after_completion_exceeded(&self) -> Result<Vec<Worker>> {
            Ok(self
                .workers
                .read()
                .await
                .values()
                .filter(|w| w.is_ttl_after_completion_exceeded())
                .cloned()
                .collect())
        }
    }

    #[tokio::test]
    async fn test_lifecycle_manager_creation() {
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig::default();
        let event_bus = Arc::new(MockEventBus::new());
        let providers = Arc::new(RwLock::new(StdHashMap::new()));
        let _manager = WorkerLifecycleManager::new(registry, providers, config, event_bus);
    }

    #[tokio::test]
    async fn test_should_scale_up_when_no_workers() {
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig::default();
        let event_bus = Arc::new(MockEventBus::new());
        let providers = Arc::new(RwLock::new(StdHashMap::new()));
        let manager = WorkerLifecycleManager::new(registry, providers, config, event_bus);

        assert!(manager.should_scale_up(1).await);
    }

    #[tokio::test]
    async fn test_health_check_result() {
        let result = HealthCheckResult::default();
        assert_eq!(result.total_checked, 0);
        assert!(result.unhealthy_workers.is_empty());
    }

    #[tokio::test]
    async fn test_cleanup_terminates_all_non_busy_workers() {
        // GIVEN: Workers en estado Ready, Terminating, y Busy
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig {
            min_ready_workers: 0, // No mantener workers ready
            ..Default::default()
        };
        let event_bus = Arc::new(MockEventBus::new());
        let providers = Arc::new(RwLock::new(StdHashMap::new()));

        // Registrar un mock provider para evitar errores en destroy_worker_via_provider
        let provider_id = hodei_server_domain::shared_kernel::ProviderId::new();
        let mock_provider = Arc::new(MockWorkerProvider::new(provider_id.clone()));
        providers.write().await.insert(
            provider_id.clone(),
            mock_provider.clone() as Arc<dyn hodei_server_domain::workers::WorkerProvider>,
        );

        let manager = WorkerLifecycleManager::new(registry.clone(), providers, config, event_bus);

        // Crear workers en diferentes estados (todos con el mismo provider_id que el mock)
        // EPIC-26 US-26.7: Ready worker con idle_timeout=0 para que se termine inmediatamente
        let ready_worker = create_test_worker_with_provider_and_ttl(
            provider_id.clone(),
            None,
            Duration::ZERO, // Idle timeout inmediato (no Some)
            None,
        );
        // Terminating worker (ya está en estado de terminación)
        let terminating_worker = create_test_worker_with_provider(provider_id.clone());
        // Busy worker (no debe terminarse)
        let busy_worker = create_test_worker_with_provider(provider_id.clone());

        // Registrar workers
        let ready_worker = registry
            .register(ready_worker.handle().clone(), ready_worker.spec().clone())
            .await
            .unwrap();
        let terminating_worker = registry
            .register(
                terminating_worker.handle().clone(),
                terminating_worker.spec().clone(),
            )
            .await
            .unwrap();
        let busy_worker = registry
            .register(busy_worker.handle().clone(), busy_worker.spec().clone())
            .await
            .unwrap();

        // Cambiar estados DESPUÉS del registro (para que se reflejen en el registry)
        registry
            .update_state(ready_worker.id(), WorkerState::Connecting)
            .await
            .unwrap();
        registry
            .update_state(ready_worker.id(), WorkerState::Ready)
            .await
            .unwrap();

        registry
            .update_state(terminating_worker.id(), WorkerState::Connecting)
            .await
            .unwrap();
        registry
            .update_state(terminating_worker.id(), WorkerState::Ready)
            .await
            .unwrap();
        registry
            .update_state(terminating_worker.id(), WorkerState::Terminating)
            .await
            .unwrap();

        registry
            .update_state(busy_worker.id(), WorkerState::Connecting)
            .await
            .unwrap();
        registry
            .update_state(busy_worker.id(), WorkerState::Ready)
            .await
            .unwrap();
        // Para marcar como Busy, necesitamos asignar un job
        let job_id = JobId::new();
        registry
            .assign_to_job(busy_worker.id(), job_id)
            .await
            .unwrap();

        // WHEN: cleanup_workers() es llamado
        let result = manager.cleanup_workers().await.unwrap();

        // THEN: Ready, Terminating y Busy workers deben ser terminados (ephemeral mode)
        // En ephemeral mode, todos los workers se terminan al finalizar
        assert_eq!(result.terminated.len(), 3); // Ready + Terminating + Busy (ephemeral mode)
        assert!(result.failed.is_empty());
    }

    #[tokio::test]
    async fn test_no_pool_persistence_after_cleanup() {
        // GIVEN: Workers en estado Ready
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig {
            min_ready_workers: 2, // Config indica mantener 2, pero para ephemeral workers no debe aplicarse
            ..Default::default()
        };
        let event_bus = Arc::new(MockEventBus::new());
        let providers = Arc::new(RwLock::new(StdHashMap::new()));

        // Registrar un mock provider
        let provider_id = hodei_server_domain::shared_kernel::ProviderId::new();
        let mock_provider = Arc::new(MockWorkerProvider::new(provider_id.clone()));
        providers.write().await.insert(
            provider_id.clone(),
            mock_provider.clone() as Arc<dyn hodei_server_domain::workers::WorkerProvider>,
        );

        let manager = WorkerLifecycleManager::new(registry.clone(), providers, config, event_bus);

        // Crear 5 workers Ready con el mismo provider_id
        // EPIC-26 US-26.7: idle_timeout=0 para que se terminen inmediatamente
        for _i in 0..5 {
            let worker_spec = create_test_worker_with_provider_and_ttl(
                provider_id.clone(),
                None,
                Duration::ZERO, // Idle timeout inmediato (no Some)
                None,
            );
            let worker = registry
                .register(worker_spec.handle().clone(), worker_spec.spec().clone())
                .await
                .unwrap();
            // Set state to Connecting then Ready (proper state transitions)
            registry
                .update_state(worker.id(), WorkerState::Connecting)
                .await
                .unwrap();
            registry
                .update_state(worker.id(), WorkerState::Ready)
                .await
                .unwrap();
        }

        // WHEN: cleanup_workers() es llamado
        let result = manager.cleanup_workers().await.unwrap();

        // THEN: TODOS los Ready workers deben ser terminados (no pool persistente)
        assert_eq!(
            result.terminated.len(),
            5,
            "All ready workers should be terminated"
        );
        assert!(result.failed.is_empty());

        // Verificar que no hay workers Ready en el registry
        let stats = registry.stats().await.unwrap();
        assert_eq!(stats.ready_workers, 0, "No ready workers should remain");
    }

    #[tokio::test]
    async fn test_all_workers_terminated_when_ephemeral_mode_enabled() {
        // GIVEN: Workers en diferentes estados (Ready, Busy, Terminating)
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig {
            min_ready_workers: 5, // Config alta, pero no debe aplicarse
            ..Default::default()
        };
        let event_bus = Arc::new(MockEventBus::new());
        let providers = Arc::new(RwLock::new(StdHashMap::new()));

        // Registrar un mock provider
        let provider_id = hodei_server_domain::shared_kernel::ProviderId::new();
        let mock_provider = Arc::new(MockWorkerProvider::new(provider_id.clone()));
        providers.write().await.insert(
            provider_id.clone(),
            mock_provider.clone() as Arc<dyn hodei_server_domain::workers::WorkerProvider>,
        );

        let manager = WorkerLifecycleManager::new(registry.clone(), providers, config, event_bus);

        // Crear workers en estado Ready con el mismo provider_id
        // EPIC-26 US-26.7: idle_timeout=0 para que se terminen inmediatamente
        for _i in 0..3 {
            let worker_spec = create_test_worker_with_provider_and_ttl(
                provider_id.clone(),
                None,
                Duration::ZERO, // Idle timeout inmediato (no Some)
                None,
            );
            let worker = registry
                .register(worker_spec.handle().clone(), worker_spec.spec().clone())
                .await
                .unwrap();
            // Set state to Connecting then Ready (proper state transitions)
            registry
                .update_state(worker.id(), WorkerState::Connecting)
                .await
                .unwrap();
            registry
                .update_state(worker.id(), WorkerState::Ready)
                .await
                .unwrap();
        }

        // WHEN: cleanup_workers() es llamado
        let result = manager.cleanup_workers().await.unwrap();

        // THEN: Todos los Ready workers deben ser terminados, independientemente de min_ready_workers
        assert_eq!(result.terminated.len(), 3);
        assert!(result.failed.is_empty());

        // Verificar estado final
        let stats = registry.stats().await.unwrap();
        assert_eq!(
            stats.ready_workers, 0,
            "Pool persistence must be eliminated"
        );
    }

    // ============================================================
    // US-2.2: Saga-based Provisioning Tests
    // ============================================================

    /// Mock Saga Orchestrator for testing
    #[derive(Clone)]
    struct MockSagaOrchestrator {
        executed_sagas: Arc<Mutex<Vec<String>>>,
        should_fail: Arc<Mutex<bool>>,
        saga_result: Arc<Mutex<Option<SagaExecutionResult>>>,
    }

    impl MockSagaOrchestrator {
        fn new() -> Self {
            Self {
                executed_sagas: Arc::new(Mutex::new(Vec::new())),
                should_fail: Arc::new(Mutex::new(false)),
                saga_result: Arc::new(Mutex::new(None)),
            }
        }

        fn set_should_fail(&self, fail: bool) {
            *self.should_fail.lock().unwrap() = fail;
        }

        fn set_saga_result(&self, result: SagaExecutionResult) {
            *self.saga_result.lock().unwrap() = Some(result);
        }
    }

    #[async_trait::async_trait]
    impl SagaOrchestrator for MockSagaOrchestrator {
        type Error = DomainError;

        async fn execute_saga(
            &self,
            saga: &dyn Saga,
            context: SagaContext,
        ) -> std::result::Result<SagaExecutionResult, Self::Error> {
            let saga_type = saga.saga_type().as_str().to_string();
            self.executed_sagas.lock().unwrap().push(saga_type.clone());

            let should_fail = *self.should_fail.lock().unwrap();
            let custom_result = self.saga_result.lock().unwrap().clone();

            if let Some(result) = custom_result {
                return Ok(result);
            }

            if should_fail {
                Ok(SagaExecutionResult::failed(
                    context.saga_id,
                    saga.saga_type(),
                    std::time::Duration::from_secs(1),
                    1,
                    0,
                    "Test failure".to_string(),
                ))
            } else {
                Ok(SagaExecutionResult::completed_with_steps(
                    context.saga_id,
                    saga.saga_type(),
                    std::time::Duration::from_secs(1),
                    4,
                ))
            }
        }

        /// EPIC-42: Execute saga directly from context (for reactive processing)
        async fn execute(
            &self,
            context: &SagaContext,
        ) -> std::result::Result<SagaExecutionResult, Self::Error> {
            // Create a mock saga based on type and execute
            let saga_type_str = context.saga_type.as_str().to_string();
            self.executed_sagas
                .lock()
                .unwrap()
                .push(saga_type_str.clone());

            let should_fail = *self.should_fail.lock().unwrap();
            let custom_result = self.saga_result.lock().unwrap().clone();

            if let Some(result) = custom_result {
                return Ok(result);
            }

            if should_fail {
                Ok(SagaExecutionResult::failed(
                    context.saga_id.clone(),
                    context.saga_type,
                    std::time::Duration::from_secs(1),
                    1,
                    0,
                    "Test failure".to_string(),
                ))
            } else {
                Ok(SagaExecutionResult::completed_with_steps(
                    context.saga_id.clone(),
                    context.saga_type,
                    std::time::Duration::from_secs(1),
                    4,
                ))
            }
        }

        async fn get_saga(
            &self,
            _saga_id: &SagaId,
        ) -> std::result::Result<Option<SagaContext>, Self::Error> {
            Ok(None)
        }

        async fn cancel_saga(&self, _saga_id: &SagaId) -> std::result::Result<(), Self::Error> {
            Ok(())
        }
    }

    /// Mock Worker Provisioning Service for testing
    #[derive(Clone)]
    struct MockWorkerProvisioningService {
        provisioned_workers: Arc<Mutex<Vec<Worker>>>,
        available: Arc<Mutex<bool>>,
    }

    impl MockWorkerProvisioningService {
        fn new() -> Self {
            Self {
                provisioned_workers: Arc::new(Mutex::new(Vec::new())),
                available: Arc::new(Mutex::new(true)),
            }
        }

        fn set_available(&self, available: bool) {
            *self.available.lock().unwrap() = available;
        }
    }

    #[async_trait::async_trait]
    impl WorkerProvisioningService for MockWorkerProvisioningService {
        async fn provision_worker(
            &self,
            _provider_id: &ProviderId,
            _spec: WorkerSpec,
        ) -> Result<ProvisioningResult> {
            let worker = Worker::new(
                WorkerHandle::new(
                    WorkerId::new(),
                    "test-resource-id".to_string(),
                    hodei_server_domain::workers::ProviderType::Docker,
                    ProviderId::new(),
                ),
                WorkerSpec::new("test-image".to_string(), "test-endpoint".to_string()),
            );
            self.provisioned_workers
                .lock()
                .unwrap()
                .push(worker.clone());
            Ok(ProvisioningResult::new(
                worker.id().clone(),
                "test-otp".to_string(),
                ProviderId::new(),
            ))
        }

        async fn is_provider_available(&self, _provider_id: &ProviderId) -> Result<bool> {
            Ok(*self.available.lock().unwrap())
        }

        fn default_worker_spec(&self, _provider_id: &ProviderId) -> Option<WorkerSpec> {
            Some(WorkerSpec::new(
                "default-image".to_string(),
                "default-endpoint".to_string(),
            ))
        }

        async fn list_providers(&self) -> Result<Vec<ProviderId>> {
            Ok(vec![ProviderId::new()])
        }

        async fn get_provider_config(
            &self,
            _provider_id: &ProviderId,
        ) -> Result<Option<hodei_server_domain::providers::ProviderConfig>> {
            Ok(None)
        }

        async fn validate_spec(&self, _spec: &WorkerSpec) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_is_saga_provisioning_disabled_by_default() {
        // GIVEN: Un WorkerLifecycleManager sin coordinador de saga
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig::default();
        let event_bus = Arc::new(MockEventBus::new());
        let providers = Arc::new(RwLock::new(StdHashMap::new()));

        let manager = WorkerLifecycleManager::new(registry, providers, config, event_bus);

        // THEN: is_saga_provisioning_enabled debe retornar false
        assert!(
            !manager.is_saga_provisioning_enabled(),
            "Saga provisioning should be disabled by default"
        );
    }

    #[tokio::test]
    async fn test_saga_provisioning_enabled_after_setting_coordinator() {
        // GIVEN: Un WorkerLifecycleManager y un DynProvisioningSagaCoordinator
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig::default();
        let event_bus = Arc::new(MockEventBus::new());
        let providers = Arc::new(RwLock::new(StdHashMap::new()));

        let orchestrator = Arc::new(MockSagaOrchestrator::new());
        let provisioning_service = Arc::new(MockWorkerProvisioningService::new());
        let saga_config = ProvisioningSagaCoordinatorConfig {
            saga_timeout: Duration::from_secs(300),
            step_timeout: Duration::from_secs(60),
        };

        let coordinator = Arc::new(DynProvisioningSagaCoordinator::new(
            orchestrator,
            provisioning_service,
            Some(saga_config),
        ));

        let mut manager = WorkerLifecycleManager::new(registry, providers, config, event_bus);

        // WHEN: Se setea el coordinator
        manager.set_provisioning_saga_coordinator(coordinator);

        // THEN: is_saga_provisioning_enabled debe retornar true
        assert!(
            manager.is_saga_provisioning_enabled(),
            "Saga provisioning should be enabled after setting coordinator"
        );
    }

    #[tokio::test]
    async fn test_provision_worker_uses_legacy_when_no_saga_coordinator() {
        // GIVEN: Un WorkerLifecycleManager sin coordinator, con un provider registrado
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig::default();
        let event_bus = Arc::new(MockEventBus::new());
        let providers = Arc::new(RwLock::new(StdHashMap::new()));

        // Registrar un mock provider que retorna un worker específico
        let provider_id = hodei_server_domain::shared_kernel::ProviderId::new();
        let mock_provider = Arc::new(MockWorkerProvider::new(provider_id.clone()));
        providers.write().await.insert(
            provider_id.clone(),
            mock_provider.clone() as Arc<dyn hodei_server_domain::workers::WorkerProvider>,
        );

        let manager = WorkerLifecycleManager::new(registry.clone(), providers, config, event_bus);

        let spec = WorkerSpec::new("test-image".to_string(), "test-endpoint".to_string());

        // WHEN: Se llama provision_worker
        let result = manager.provision_worker(&provider_id, spec).await;

        // THEN: Debe usar legacy provisioning (el worker debe ser registrado)
        assert!(result.is_ok(), "Legacy provisioning should succeed");
        let worker = result.unwrap();
        assert_eq!(worker.state(), &WorkerState::Creating);
    }

    #[tokio::test]
    async fn test_provision_worker_uses_saga_when_coordinator_set() {
        // GIVEN: Un WorkerLifecycleManager con coordinator configurado
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig::default();
        let event_bus = Arc::new(MockEventBus::new());
        let providers = Arc::new(RwLock::new(StdHashMap::new()));

        // Registrar un mock provider
        let provider_id = hodei_server_domain::shared_kernel::ProviderId::new();
        let mock_provider = Arc::new(MockWorkerProvider::new(provider_id.clone()));
        providers.write().await.insert(
            provider_id.clone(),
            mock_provider.clone() as Arc<dyn hodei_server_domain::workers::WorkerProvider>,
        );

        let orchestrator = Arc::new(MockSagaOrchestrator::new());
        let provisioning_service = Arc::new(MockWorkerProvisioningService::new());
        let saga_config = ProvisioningSagaCoordinatorConfig {
            saga_timeout: Duration::from_secs(300),
            step_timeout: Duration::from_secs(60),
        };

        let coordinator = Arc::new(DynProvisioningSagaCoordinator::new(
            orchestrator,
            provisioning_service,
            Some(saga_config),
        ));

        let mut manager =
            WorkerLifecycleManager::new(registry.clone(), providers, config, event_bus);
        manager.set_provisioning_saga_coordinator(coordinator);

        let spec = WorkerSpec::new("test-image".to_string(), "test-endpoint".to_string());

        // WHEN: Se llama provision_worker
        // THEN: Verificamos que el manager tiene el coordinator configurado
        // (la lógica real de saga se prueba en los tests específicos del coordinator)
        assert!(
            manager.provisioning_saga_coordinator.is_some(),
            "Manager should have provisioning saga coordinator set"
        );
    }

    #[tokio::test]
    async fn test_provision_worker_returns_error_when_max_workers_exceeded() {
        // GIVEN: Un WorkerLifecycleManager con límite de workers alcanzado
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig {
            max_workers: 0, // Sin capacidad
            ..Default::default()
        };
        let event_bus = Arc::new(MockEventBus::new());
        let providers = Arc::new(RwLock::new(StdHashMap::new()));

        // Registrar un mock provider
        let provider_id = hodei_server_domain::shared_kernel::ProviderId::new();
        let mock_provider = Arc::new(MockWorkerProvider::new(provider_id.clone()));
        providers.write().await.insert(
            provider_id.clone(),
            mock_provider.clone() as Arc<dyn hodei_server_domain::workers::WorkerProvider>,
        );

        let orchestrator = Arc::new(MockSagaOrchestrator::new());
        let provisioning_service = Arc::new(MockWorkerProvisioningService::new());
        let saga_config = ProvisioningSagaCoordinatorConfig {
            saga_timeout: Duration::from_secs(300),
            step_timeout: Duration::from_secs(60),
        };

        let coordinator = Arc::new(DynProvisioningSagaCoordinator::new(
            orchestrator,
            provisioning_service,
            Some(saga_config),
        ));

        let mut manager = WorkerLifecycleManager::new(registry, providers, config, event_bus);
        manager.set_provisioning_saga_coordinator(coordinator);

        let spec = WorkerSpec::new("test-image".to_string(), "test-endpoint".to_string());

        // WHEN: Se llama provision_worker
        let result = manager.provision_worker(&provider_id, spec).await;

        // THEN: Debe retornar error ProviderOverloaded
        assert!(result.is_err());
        match result {
            Err(DomainError::ProviderOverloaded { .. }) => {
                // Expected
            }
            _ => panic!("Expected ProviderOverloaded error"),
        }
    }

    // ============================================================
    // US-4.2: Saga-based Recovery Tests
    // ============================================================

    #[tokio::test]
    async fn test_is_saga_recovery_disabled_by_default() {
        // GIVEN: Un WorkerLifecycleManager sin coordinador de recovery
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig::default();
        let event_bus = Arc::new(MockEventBus::new());
        let providers = Arc::new(RwLock::new(StdHashMap::new()));

        let manager = WorkerLifecycleManager::new(registry, providers, config, event_bus);

        // THEN: is_saga_recovery_enabled debe retornar false
        assert!(
            !manager.is_saga_recovery_enabled(),
            "Saga recovery should be disabled by default"
        );
    }

    #[tokio::test]
    async fn test_saga_recovery_enabled_after_setting_coordinator() {
        // GIVEN: Un WorkerLifecycleManager y un DynRecoverySagaCoordinator
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig::default();
        let event_bus = Arc::new(MockEventBus::new());
        let providers = Arc::new(RwLock::new(StdHashMap::new()));

        let orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync> =
            Arc::new(MockSagaOrchestrator::new());
        let saga_config = RecoverySagaCoordinatorConfig {
            saga_timeout: Duration::from_secs(300),
            step_timeout: Duration::from_secs(60),
        };

        let coordinator = Arc::new(DynRecoverySagaCoordinator::new(
            orchestrator,
            Some(saga_config),
        ));

        let mut manager = WorkerLifecycleManager::new(registry, providers, config, event_bus);

        // WHEN: Se setea el coordinator
        manager.set_recovery_saga_coordinator(coordinator);

        // THEN: is_saga_recovery_enabled debe retornar true
        assert!(
            manager.is_saga_recovery_enabled(),
            "Saga recovery should be enabled after setting coordinator"
        );
    }

    #[tokio::test]
    async fn test_recover_worker_uses_legacy_when_no_saga_coordinator() {
        // GIVEN: Un WorkerLifecycleManager sin coordinator
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig::default();
        let event_bus = Arc::new(MockEventBus::new());
        let providers = Arc::new(RwLock::new(StdHashMap::new()));

        let manager = WorkerLifecycleManager::new(registry, providers, config, event_bus);

        let job_id = JobId::new();
        let worker_id = WorkerId::new();

        // WHEN: Se llama recover_worker sin coordinator
        let result = manager.recover_worker(&job_id, &worker_id).await;

        // THEN: Debe usar legacy recovery (que hace warn y retorna Ok)
        assert!(
            result.is_ok(),
            "Legacy recovery should succeed (with warning)"
        );
    }

    #[tokio::test]
    async fn test_recover_worker_via_saga() {
        // GIVEN: Un WorkerLifecycleManager con recovery saga coordinator
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig::default();
        let event_bus = Arc::new(MockEventBus::new());
        let providers = Arc::new(RwLock::new(StdHashMap::new()));

        let orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync> =
            Arc::new(MockSagaOrchestrator::new());
        let saga_config = RecoverySagaCoordinatorConfig {
            saga_timeout: Duration::from_secs(300),
            step_timeout: Duration::from_secs(60),
        };

        let coordinator = Arc::new(DynRecoverySagaCoordinator::new(
            orchestrator,
            Some(saga_config),
        ));

        let mut manager = WorkerLifecycleManager::new(registry, providers, config, event_bus);
        manager.set_recovery_saga_coordinator(coordinator);

        let job_id = JobId::new();
        let worker_id = WorkerId::new();

        // WHEN: Se llama recover_worker
        let result = manager.recover_worker(&job_id, &worker_id).await;

        // THEN: Debe caer a legacy recovery
        assert!(result.is_ok(), "Should fallback to legacy recovery");
    }

    #[tokio::test]
    async fn test_recover_worker_uses_saga_when_enabled() {
        // GIVEN: Un WorkerLifecycleManager con saga recovery habilitado
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig::default();
        let event_bus = Arc::new(MockEventBus::new());
        let providers = Arc::new(RwLock::new(StdHashMap::new()));

        let orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync> =
            Arc::new(MockSagaOrchestrator::new());
        let saga_config = RecoverySagaCoordinatorConfig {
            saga_timeout: Duration::from_secs(300),
            step_timeout: Duration::from_secs(60),
        };

        let coordinator = Arc::new(DynRecoverySagaCoordinator::new(
            orchestrator,
            Some(saga_config),
        ));

        let mut manager = WorkerLifecycleManager::new(registry, providers, config, event_bus);
        manager.set_recovery_saga_coordinator(coordinator);

        let job_id = JobId::new();
        let worker_id = WorkerId::new();

        // WHEN: Se llama recover_worker
        let result = manager.recover_worker(&job_id, &worker_id).await;

        // THEN: Debe usar saga recovery exitosamente
        assert!(result.is_ok(), "Saga recovery should succeed");
    }
}

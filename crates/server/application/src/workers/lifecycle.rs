//! Worker Lifecycle Manager
//!
//! Servicio de aplicación para gestionar el ciclo de vida de workers:
//! - Heartbeat monitoring
//! - Auto-scaling basado en demanda
//! - Terminación de workers idle/unhealthy

use chrono::Utc;
use hodei_server_domain::{
    event_bus::EventBus,
    events::{DomainEvent, TerminationReason},
    shared_kernel::{DomainError, ProviderId, Result, WorkerId, WorkerState},
    workers::WorkerProvider,
    workers::{Worker, WorkerSpec},
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
    /// Minimum workers to keep ready
    pub min_ready_workers: usize,
    /// Maximum workers allowed
    pub max_workers: usize,
    /// Scale up when queue depth exceeds this
    pub scale_up_threshold: usize,
    /// Scale down when idle workers exceed this
    pub scale_down_threshold: usize,
}

impl Default for WorkerLifecycleConfig {
    fn default() -> Self {
        Self {
            heartbeat_timeout: Duration::from_secs(60),
            health_check_interval: Duration::from_secs(30),
            min_ready_workers: 1,
            max_workers: 10,
            scale_up_threshold: 5,
            scale_down_threshold: 3,
        }
    }
}

/// Worker Lifecycle Manager
///
/// Responsibilities:
/// - Monitor worker health via heartbeats
/// - Auto-scale workers based on demand
/// - Terminate idle/unhealthy workers
/// - Provision new workers when needed
pub struct WorkerLifecycleManager {
    registry: Arc<dyn WorkerRegistry>,
    providers: Arc<RwLock<HashMap<ProviderId, Arc<dyn WorkerProvider>>>>,
    config: WorkerLifecycleConfig,
    event_bus: Arc<dyn EventBus>,
}

impl WorkerLifecycleManager {
    pub fn new(
        registry: Arc<dyn WorkerRegistry>,
        config: WorkerLifecycleConfig,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            registry,
            providers: Arc::new(RwLock::new(HashMap::new())),
            config,
            event_bus,
        }
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

    /// Find workers that should be terminated and terminate them
    pub async fn cleanup_workers(&self) -> Result<CleanupResult> {
        let mut result = CleanupResult::default();

        // Find workers to terminate (idle timeout or lifetime exceeded)
        let workers_to_terminate = self.registry.find_for_termination().await?;

        for worker in workers_to_terminate {
            info!(
                "Terminating worker {} (idle/lifetime exceeded)",
                worker.id()
            );

            // Mark as terminating
            if let Err(e) = self
                .registry
                .update_state(worker.id(), WorkerState::Terminating)
                .await
            {
                error!(
                    "Failed to mark worker {} as terminating: {}",
                    worker.id(),
                    e
                );
                continue;
            }

            // Destroy via provider
            if let Err(e) = self.destroy_worker_via_provider(&worker).await {
                error!("Failed to destroy worker {}: {}", worker.id(), e);
                result.failed.push(worker.id().clone());
                continue;
            }

            // Unregister
            if let Err(e) = self.registry.unregister(worker.id()).await {
                warn!("Failed to unregister worker {}: {}", worker.id(), e);
            }

            // Publish WorkerTerminated event
            let event = DomainEvent::WorkerTerminated {
                worker_id: worker.id().clone(),
                provider_id: worker.provider_id().clone(),
                reason: TerminationReason::IdleTimeout,
                occurred_at: Utc::now(),
                correlation_id: None,
                actor: Some("lifecycle-manager".to_string()),
            };
            if let Err(e) = self.event_bus.publish(&event).await {
                warn!("Failed to publish WorkerTerminated event: {}", e);
            }

            result.terminated.push(worker.id().clone());
        }

        Ok(result)
    }

    /// Provision a new worker using the specified provider
    pub async fn provision_worker(
        &self,
        provider_id: &ProviderId,
        spec: WorkerSpec,
    ) -> Result<Worker> {
        let providers = self.providers.read().await;
        let provider = providers
            .get(provider_id)
            .ok_or_else(|| DomainError::ProviderNotFound {
                provider_id: provider_id.clone(),
            })?;

        // Check max workers limit
        let current_count = self.registry.count().await?;
        if current_count >= self.config.max_workers {
            return Err(DomainError::ProviderOverloaded {
                provider_id: provider_id.clone(),
            });
        }

        info!("Provisioning new worker via provider {}", provider_id);

        // Create worker via provider
        let handle = provider.create_worker(&spec).await.map_err(|e| {
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
            actor: Some("lifecycle-manager".to_string()),
        };
        if let Err(e) = self.event_bus.publish(&event).await {
            warn!("Failed to publish WorkerProvisioned event: {}", e);
        }

        info!("Worker {} provisioned successfully", worker.id());
        Ok(worker)
    }

    /// Destroy a worker via its provider
    async fn destroy_worker_via_provider(&self, worker: &Worker) -> Result<()> {
        let providers = self.providers.read().await;
        let provider =
            providers
                .get(worker.provider_id())
                .ok_or_else(|| DomainError::ProviderNotFound {
                    provider_id: worker.provider_id().clone(),
                })?;

        provider
            .destroy_worker(worker.handle())
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: e.to_string(),
            })?;

        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::BoxStream;
    use hodei_server_domain::event_bus::EventBusError;
    use hodei_server_domain::workers::WorkerHandle;
    use std::collections::HashMap as StdHashMap;
    use std::sync::Mutex;
    use tokio::sync::RwLock as TokioRwLock;

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
                    WorkerState::Connecting => worker.mark_connecting()?,
                    WorkerState::Ready => worker.mark_ready()?,
                    WorkerState::Terminating => worker.mark_terminating()?,
                    WorkerState::Terminated => worker.mark_terminated()?,
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
            _worker_id: &WorkerId,
            _job_id: hodei_server_domain::shared_kernel::JobId,
        ) -> Result<()> {
            Ok(())
        }

        async fn release_from_job(&self, _worker_id: &WorkerId) -> Result<()> {
            Ok(())
        }

        async fn find_unhealthy(&self, _timeout: Duration) -> Result<Vec<Worker>> {
            Ok(vec![])
        }

        async fn find_for_termination(&self) -> Result<Vec<Worker>> {
            Ok(vec![])
        }

        async fn stats(&self) -> Result<WorkerRegistryStats> {
            let workers = self.workers.read().await;
            Ok(WorkerRegistryStats {
                total_workers: workers.len(),
                ..Default::default()
            })
        }

        async fn count(&self) -> Result<usize> {
            Ok(self.workers.read().await.len())
        }
    }

    #[tokio::test]
    async fn test_lifecycle_manager_creation() {
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig::default();
        let event_bus = Arc::new(MockEventBus::new());
        let _manager = WorkerLifecycleManager::new(registry, config, event_bus);
    }

    #[tokio::test]
    async fn test_should_scale_up_when_no_workers() {
        let registry = Arc::new(MockWorkerRegistry::new());
        let config = WorkerLifecycleConfig::default();
        let event_bus = Arc::new(MockEventBus::new());
        let manager = WorkerLifecycleManager::new(registry, config, event_bus);

        assert!(manager.should_scale_up(1).await);
    }

    #[tokio::test]
    async fn test_health_check_result() {
        let result = HealthCheckResult::default();
        assert_eq!(result.total_checked, 0);
        assert!(result.unhealthy_workers.is_empty());
    }
}

//! Smart Scheduler - Servicio de scheduling inteligente
//!
//! Implementa estrategias configurables para asignar jobs a workers
//! y decidir cuándo provisionar nuevos workers.

use hodei_jobs_domain::{
    job_execution::Job,
    job_scheduler::{
        FastestStartupProviderSelector, FirstAvailableWorkerSelector, HealthiestProviderSelector,
        JobScheduler, LeastLoadedWorkerSelector, LowestCostProviderSelector,
        MostCapacityProviderSelector, ProviderInfo, ProviderSelector,
        ProviderSelectionStrategy, SchedulingContext, SchedulingDecision, WorkerSelector,
        WorkerSelectionStrategy,
    },
    shared_kernel::{ProviderId, Result, WorkerId},
    worker::Worker,
};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::{debug, info};

/// Configuration for SmartScheduler
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Strategy for selecting workers
    pub worker_strategy: WorkerSelectionStrategy,
    /// Strategy for selecting providers
    pub provider_strategy: ProviderSelectionStrategy,
    /// Max queue depth before rejecting
    pub max_queue_depth: usize,
    /// System load threshold for scaling up
    pub scale_up_load_threshold: f64,
    /// Prefer existing workers over provisioning
    pub prefer_existing_workers: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            worker_strategy: WorkerSelectionStrategy::LeastLoaded,
            provider_strategy: ProviderSelectionStrategy::FastestStartup,
            max_queue_depth: 100,
            scale_up_load_threshold: 0.8,
            prefer_existing_workers: true,
        }
    }
}

/// Smart Scheduler implementation
pub struct SmartScheduler {
    config: SchedulerConfig,
    round_robin_worker_index: AtomicUsize,
    round_robin_provider_index: AtomicUsize,
}

impl SmartScheduler {
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            config,
            round_robin_worker_index: AtomicUsize::new(0),
            round_robin_provider_index: AtomicUsize::new(0),
        }
    }

    /// Select a worker using the configured strategy
    fn select_worker(&self, job: &Job, workers: &[Worker]) -> Option<WorkerId> {
        if workers.is_empty() {
            return None;
        }

        match self.config.worker_strategy {
            WorkerSelectionStrategy::FirstAvailable => {
                FirstAvailableWorkerSelector.select_worker(job, workers)
            }
            WorkerSelectionStrategy::LeastLoaded => {
                LeastLoadedWorkerSelector.select_worker(job, workers)
            }
            WorkerSelectionStrategy::RoundRobin => {
                let available: Vec<_> = workers
                    .iter()
                    .filter(|w| w.state().can_accept_jobs())
                    .collect();

                if available.is_empty() {
                    return None;
                }

                let index = self.round_robin_worker_index.fetch_add(1, Ordering::SeqCst);
                let worker = available[index % available.len()];
                Some(worker.id().clone())
            }
            WorkerSelectionStrategy::MostCapacity => {
                // For now, same as LeastLoaded (could be enhanced with resource metrics)
                LeastLoadedWorkerSelector.select_worker(job, workers)
            }
            WorkerSelectionStrategy::Affinity => {
                // Check for job type affinity in worker labels
                // Falls back to first available if no affinity match
                workers
                    .iter()
                    .filter(|w| w.state().can_accept_jobs())
                    .find(|w| {
                        // Check if worker has matching capabilities for job image
                        w.spec()
                            .labels
                            .get("image_type")
                            .map(|t| Some(t.as_str()) == job.spec.image.as_deref())
                            .unwrap_or(false)
                    })
                    .or_else(|| workers.iter().find(|w| w.state().can_accept_jobs()))
                    .map(|w| w.id().clone())
            }
        }
    }

    /// Select a provider using the configured strategy
    fn select_provider(&self, job: &Job, providers: &[ProviderInfo]) -> Option<ProviderId> {
        if providers.is_empty() {
            return None;
        }

        match self.config.provider_strategy {
            ProviderSelectionStrategy::FirstAvailable => providers
                .iter()
                .find(|p| p.can_accept_workers())
                .map(|p| p.provider_id.clone()),
            ProviderSelectionStrategy::LowestCost => {
                LowestCostProviderSelector.select_provider(job, providers)
            }
            ProviderSelectionStrategy::FastestStartup => {
                FastestStartupProviderSelector.select_provider(job, providers)
            }
            ProviderSelectionStrategy::MostCapacity => {
                MostCapacityProviderSelector.select_provider(job, providers)
            }
            ProviderSelectionStrategy::RoundRobin => {
                let available: Vec<_> = providers
                    .iter()
                    .filter(|p| p.can_accept_workers())
                    .collect();

                if available.is_empty() {
                    return None;
                }

                let index = self.round_robin_provider_index.fetch_add(1, Ordering::SeqCst);
                Some(available[index % available.len()].provider_id.clone())
            }
            ProviderSelectionStrategy::Healthiest => {
                HealthiestProviderSelector.select_provider(job, providers)
            }
        }
    }
}

#[async_trait]
impl JobScheduler for SmartScheduler {
    async fn schedule(&self, context: SchedulingContext) -> Result<SchedulingDecision> {
        let job_id = context.job.id.clone();

        debug!(
            "Scheduling job {} with {} available workers and {} providers",
            job_id,
            context.available_workers.len(),
            context.available_providers.len()
        );

        // Check queue depth limit
        if context.pending_jobs_count > self.config.max_queue_depth {
            return Ok(SchedulingDecision::Reject {
                job_id,
                reason: format!(
                    "Queue depth {} exceeds max {}",
                    context.pending_jobs_count, self.config.max_queue_depth
                ),
            });
        }

        // Try to assign to existing worker first if preferred
        if self.config.prefer_existing_workers {
            if let Some(worker_id) = self.select_worker(&context.job, &context.available_workers) {
                info!("Assigning job {} to existing worker {}", job_id, worker_id);
                return Ok(SchedulingDecision::AssignToWorker { job_id, worker_id });
            }
        }

        // Try to provision new worker
        if let Some(provider_id) = self.select_provider(&context.job, &context.available_providers) {
            info!("Provisioning new worker for job {} via provider {}", job_id, provider_id);
            return Ok(SchedulingDecision::ProvisionWorker { job_id, provider_id });
        }

        // Check if we should scale up
        if context.system_load > self.config.scale_up_load_threshold {
            return Ok(SchedulingDecision::Enqueue {
                job_id,
                reason: "System under high load, enqueueing for later".to_string(),
            });
        }

        // Last resort: enqueue
        Ok(SchedulingDecision::Enqueue {
            job_id,
            reason: "No workers or providers available".to_string(),
        })
    }

    fn strategy_name(&self) -> &str {
        "smart_scheduler"
    }
}

/// Scheduling Service - Coordina scheduling con registry y providers
pub struct SchedulingService {
    scheduler: SmartScheduler,
}

impl SchedulingService {
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            scheduler: SmartScheduler::new(config),
        }
    }

    pub fn with_default_config() -> Self {
        Self::new(SchedulerConfig::default())
    }

    /// Get the underlying scheduler
    pub fn scheduler(&self) -> &SmartScheduler {
        &self.scheduler
    }

    /// Make a scheduling decision
    pub async fn make_decision(&self, context: SchedulingContext) -> Result<SchedulingDecision> {
        self.scheduler.schedule(context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_jobs_domain::{
        job_execution::JobSpec,
        shared_kernel::{JobId, ProviderId},
        worker::{ProviderType, Worker, WorkerHandle, WorkerSpec},
    };

    fn create_test_job() -> Job {
        Job::new(JobId::new(), JobSpec::new(vec!["echo".to_string(), "test".to_string()]))
    }

    fn create_test_worker(ready: bool) -> Worker {
        let spec = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        let handle = WorkerHandle::new(
            spec.worker_id.clone(),
            "container-123".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut worker = Worker::new(handle, spec);
        if ready {
            worker.mark_starting().unwrap();
            worker.mark_ready().unwrap();
        }
        worker
    }

    fn create_test_provider() -> ProviderInfo {
        ProviderInfo {
            provider_id: ProviderId::new(),
            provider_type: ProviderType::Docker,
            active_workers: 2,
            max_workers: 10,
            estimated_startup_time: std::time::Duration::from_secs(5),
            health_score: 0.9,
            cost_per_hour: 0.0,
        }
    }

    #[tokio::test]
    async fn test_schedule_to_existing_worker() {
        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let context = SchedulingContext {
            job: create_test_job(),
            available_workers: vec![create_test_worker(true)],
            available_providers: vec![create_test_provider()],
            pending_jobs_count: 0,
            system_load: 0.5,
        };

        let decision = scheduler.schedule(context).await.unwrap();
        assert!(matches!(decision, SchedulingDecision::AssignToWorker { .. }));
    }

    #[tokio::test]
    async fn test_schedule_provision_when_no_workers() {
        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let context = SchedulingContext {
            job: create_test_job(),
            available_workers: vec![],
            available_providers: vec![create_test_provider()],
            pending_jobs_count: 0,
            system_load: 0.5,
        };

        let decision = scheduler.schedule(context).await.unwrap();
        assert!(matches!(decision, SchedulingDecision::ProvisionWorker { .. }));
    }

    #[tokio::test]
    async fn test_schedule_reject_when_queue_full() {
        let config = SchedulerConfig {
            max_queue_depth: 10,
            ..Default::default()
        };
        let scheduler = SmartScheduler::new(config);
        let context = SchedulingContext {
            job: create_test_job(),
            available_workers: vec![],
            available_providers: vec![],
            pending_jobs_count: 100,
            system_load: 0.9,
        };

        let decision = scheduler.schedule(context).await.unwrap();
        assert!(matches!(decision, SchedulingDecision::Reject { .. }));
    }

    #[tokio::test]
    async fn test_schedule_enqueue_when_no_resources() {
        let scheduler = SmartScheduler::new(SchedulerConfig::default());
        let context = SchedulingContext {
            job: create_test_job(),
            available_workers: vec![],
            available_providers: vec![],
            pending_jobs_count: 5,
            system_load: 0.5,
        };

        let decision = scheduler.schedule(context).await.unwrap();
        assert!(matches!(decision, SchedulingDecision::Enqueue { .. }));
    }

    #[tokio::test]
    async fn test_round_robin_worker_selection() {
        let config = SchedulerConfig {
            worker_strategy: WorkerSelectionStrategy::RoundRobin,
            ..Default::default()
        };
        let scheduler = SmartScheduler::new(config);

        let workers = vec![create_test_worker(true), create_test_worker(true)];

        let first = scheduler.select_worker(&create_test_job(), &workers);
        let second = scheduler.select_worker(&create_test_job(), &workers);

        assert!(first.is_some());
        assert!(second.is_some());
        assert_ne!(first, second);
    }

    #[test]
    fn test_scheduling_service_creation() {
        let service = SchedulingService::with_default_config();
        assert_eq!(service.scheduler().strategy_name(), "smart_scheduler");
    }
}

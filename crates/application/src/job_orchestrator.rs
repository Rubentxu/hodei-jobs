//! Job Orchestrator - Orquestación de ejecución de jobs
//!
//! Integra el scheduler, lifecycle manager y registros para
//! la ejecución completa de jobs.

use crate::{
    smart_scheduler::{SchedulerConfig, SchedulingService},
    worker_lifecycle::{WorkerLifecycleConfig, WorkerLifecycleManager},
};
use hodei_jobs_domain::{
    job_execution::{Job, JobQueue, JobRepository},
    job_scheduler::{ProviderInfo, SchedulingContext, SchedulingDecision},
    shared_kernel::{DomainError, JobId, JobState, ProviderId, Result, WorkerId},
    worker::WorkerSpec,
    worker_provider::WorkerProvider,
    worker_registry::{WorkerRegistry, WorkerRegistryStats},
};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Job Orchestrator - Coordina la ejecución de jobs
pub struct JobOrchestrator {
    scheduler: SchedulingService,
    lifecycle_manager: WorkerLifecycleManager,
    registry: Arc<dyn WorkerRegistry>,
    job_repository: Arc<dyn JobRepository>,
    job_queue: Arc<dyn JobQueue>,
    providers: Arc<RwLock<HashMap<ProviderId, Arc<dyn WorkerProvider>>>>,
    default_worker_spec: WorkerSpec,
}

impl JobOrchestrator {
    pub fn new(
        registry: Arc<dyn WorkerRegistry>,
        job_repository: Arc<dyn JobRepository>,
        job_queue: Arc<dyn JobQueue>,
        scheduler_config: SchedulerConfig,
        lifecycle_config: WorkerLifecycleConfig,
    ) -> Self {
        let default_worker_spec = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );

        Self {
            scheduler: SchedulingService::new(scheduler_config),
            lifecycle_manager: WorkerLifecycleManager::new(registry.clone(), lifecycle_config),
            registry,
            job_repository,
            job_queue,
            providers: Arc::new(RwLock::new(HashMap::new())),
            default_worker_spec,
        }
    }

    /// Register a provider with the orchestrator
    pub async fn register_provider(&self, provider: Arc<dyn WorkerProvider>) {
        let provider_id = provider.provider_id().clone();
        info!("Registering provider: {}", provider_id);

        self.lifecycle_manager.register_provider(provider.clone()).await;
        self.providers.write().await.insert(provider_id, provider);
    }

    /// Submit a job for execution
    pub async fn submit_job(&self, job: Job) -> Result<JobId> {
        let job_id = job.id.clone();
        info!("Submitting job: {}", job_id);

        // Save job
        self.job_repository.save(&job).await?;

        // Try to schedule immediately
        match self.try_schedule_job(job).await {
            Ok(decision) => {
                self.handle_scheduling_decision(decision).await?;
            }
            Err(e) => {
                warn!("Failed to schedule job immediately: {}", e);
                // Enqueue for later processing
                let job = self.job_repository.find_by_id(&job_id).await?.ok_or_else(|| {
                    DomainError::JobNotFound { job_id: job_id.clone() }
                })?;
                self.job_queue.enqueue(job).await?;
            }
        }

        Ok(job_id)
    }

    /// Try to schedule a job
    async fn try_schedule_job(&self, job: Job) -> Result<SchedulingDecision> {
        let context = self.build_scheduling_context(job).await?;
        self.scheduler.make_decision(context).await
    }

    /// Build scheduling context
    async fn build_scheduling_context(&self, job: Job) -> Result<SchedulingContext> {
        let available_workers = self.registry.find_available().await?;
        let pending_jobs_count = self.job_queue.len().await?;
        let stats = self.registry.stats().await?;

        let providers = self.providers.read().await;
        let mut available_providers = Vec::new();

        for (provider_id, provider) in providers.iter() {
            let health = provider.health_check().await;
            let health_score = match health {
                Ok(hodei_jobs_domain::worker_provider::HealthStatus::Healthy) => 1.0,
                Ok(hodei_jobs_domain::worker_provider::HealthStatus::Degraded { .. }) => 0.5,
                _ => 0.0,
            };

            let workers_for_provider = self.registry.find_by_provider(provider_id).await?;
            let caps = provider.capabilities();

            available_providers.push(ProviderInfo {
                provider_id: provider_id.clone(),
                provider_type: provider.provider_type(),
                active_workers: workers_for_provider.len(),
                max_workers: 10, // Could be from config
                estimated_startup_time: caps.max_execution_time.unwrap_or(Duration::from_secs(30)),
                health_score,
                cost_per_hour: 0.0, // Could be from provider
            });
        }

        let system_load = if stats.total_workers > 0 {
            stats.busy_workers as f64 / stats.total_workers as f64
        } else {
            0.0
        };

        Ok(SchedulingContext {
            job,
            available_workers,
            available_providers,
            pending_jobs_count,
            system_load,
        })
    }

    /// Handle a scheduling decision
    async fn handle_scheduling_decision(&self, decision: SchedulingDecision) -> Result<()> {
        match decision {
            SchedulingDecision::AssignToWorker { job_id, worker_id } => {
                self.assign_job_to_worker(&job_id, &worker_id).await?;
            }
            SchedulingDecision::ProvisionWorker { job_id, provider_id } => {
                self.provision_and_assign(&job_id, &provider_id).await?;
            }
            SchedulingDecision::Enqueue { job_id, reason } => {
                debug!("Job {} enqueued: {}", job_id, reason);
                let job = self.job_repository.find_by_id(&job_id).await?.ok_or_else(|| {
                    DomainError::JobNotFound { job_id: job_id.clone() }
                })?;
                self.job_queue.enqueue(job).await?;
            }
            SchedulingDecision::Reject { job_id, reason } => {
                error!("Job {} rejected: {}", job_id, reason);
                // Update job state to failed
                if let Some(mut job) = self.job_repository.find_by_id(&job_id).await? {
                    job.state = JobState::Failed;
                    self.job_repository.update(&job).await?;
                }
            }
        }
        Ok(())
    }

    /// Assign a job to an existing worker
    async fn assign_job_to_worker(&self, job_id: &JobId, worker_id: &WorkerId) -> Result<()> {
        info!("Assigning job {} to worker {}", job_id, worker_id);

        // Update worker state
        self.registry.assign_to_job(worker_id, job_id.clone()).await?;

        // Update job state
        if let Some(mut job) = self.job_repository.find_by_id(job_id).await? {
            job.state = JobState::Running;
            self.job_repository.update(&job).await?;
        }

        Ok(())
    }

    /// Provision a new worker and assign job
    async fn provision_and_assign(&self, job_id: &JobId, provider_id: &ProviderId) -> Result<()> {
        info!("Provisioning worker for job {} via {}", job_id, provider_id);

        // Provision worker
        let spec = self.default_worker_spec.clone();
        let worker = self.lifecycle_manager.provision_worker(provider_id, spec).await?;

        // Wait for worker to be ready (simplified - in practice would use events)
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Assign job
        self.assign_job_to_worker(job_id, worker.id()).await?;

        Ok(())
    }

    /// Process pending jobs in the queue
    pub async fn process_queue(&self) -> Result<usize> {
        let mut processed = 0;

        while let Some(job) = self.job_queue.dequeue().await? {
            match self.try_schedule_job(job.clone()).await {
                Ok(decision) => {
                    if let Err(e) = self.handle_scheduling_decision(decision).await {
                        warn!("Failed to handle scheduling decision: {}", e);
                        // Re-enqueue
                        self.job_queue.enqueue(job).await?;
                    } else {
                        processed += 1;
                    }
                }
                Err(e) => {
                    warn!("Failed to schedule job: {}", e);
                    self.job_queue.enqueue(job).await?;
                    break; // Stop processing if we can't schedule
                }
            }
        }

        Ok(processed)
    }

    /// Run maintenance tasks (health checks, cleanup)
    pub async fn run_maintenance(&self) -> Result<MaintenanceResult> {
        let mut result = MaintenanceResult::default();

        // Health check
        let health_result = self.lifecycle_manager.run_health_check().await?;
        result.unhealthy_workers = health_result.unhealthy_workers.len();

        // Cleanup idle workers
        let cleanup_result = self.lifecycle_manager.cleanup_workers().await?;
        result.terminated_workers = cleanup_result.terminated.len();

        // Process queue if we have capacity
        if self.lifecycle_manager.should_scale_up(self.job_queue.len().await?).await {
            result.jobs_processed = self.process_queue().await?;
        }

        Ok(result)
    }

    /// Get orchestrator statistics
    pub async fn stats(&self) -> Result<OrchestratorStats> {
        let registry_stats = self.registry.stats().await?;
        let queue_depth = self.job_queue.len().await?;
        let provider_count = self.providers.read().await.len();

        Ok(OrchestratorStats {
            registry_stats,
            queue_depth,
            provider_count,
        })
    }
}

/// Result of maintenance run
#[derive(Debug, Default)]
pub struct MaintenanceResult {
    pub unhealthy_workers: usize,
    pub terminated_workers: usize,
    pub jobs_processed: usize,
}

/// Orchestrator statistics
#[derive(Debug)]
pub struct OrchestratorStats {
    pub registry_stats: WorkerRegistryStats,
    pub queue_depth: usize,
    pub provider_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_jobs_domain::job_execution::JobSpec;
    use std::collections::HashMap as StdHashMap;
    use tokio::sync::RwLock as TokioRwLock;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    // Simple mock implementations for testing
    struct MockJobRepository {
        jobs: Arc<TokioRwLock<StdHashMap<JobId, Job>>>,
    }

    impl MockJobRepository {
        fn new() -> Self {
            Self {
                jobs: Arc::new(TokioRwLock::new(StdHashMap::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl JobRepository for MockJobRepository {
        async fn save(&self, job: &Job) -> Result<()> {
            self.jobs.write().await.insert(job.id.clone(), job.clone());
            Ok(())
        }

        async fn find_by_id(&self, job_id: &JobId) -> Result<Option<Job>> {
            Ok(self.jobs.read().await.get(job_id).cloned())
        }

        async fn find_by_state(&self, _state: &JobState) -> Result<Vec<Job>> {
            Ok(vec![])
        }

        async fn find_pending(&self) -> Result<Vec<Job>> {
            Ok(vec![])
        }

        async fn delete(&self, _job_id: &JobId) -> Result<()> {
            Ok(())
        }

        async fn update(&self, job: &Job) -> Result<()> {
            self.jobs.write().await.insert(job.id.clone(), job.clone());
            Ok(())
        }
    }

    struct MockJobQueue {
        queue: Arc<Mutex<VecDeque<Job>>>,
    }

    impl MockJobQueue {
        fn new() -> Self {
            Self {
                queue: Arc::new(Mutex::new(VecDeque::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl JobQueue for MockJobQueue {
        async fn enqueue(&self, job: Job) -> Result<()> {
            self.queue.lock().unwrap().push_back(job);
            Ok(())
        }

        async fn dequeue(&self) -> Result<Option<Job>> {
            Ok(self.queue.lock().unwrap().pop_front())
        }

        async fn peek(&self) -> Result<Option<Job>> {
            Ok(self.queue.lock().unwrap().front().cloned())
        }

        async fn len(&self) -> Result<usize> {
            Ok(self.queue.lock().unwrap().len())
        }

        async fn is_empty(&self) -> Result<bool> {
            Ok(self.queue.lock().unwrap().is_empty())
        }

        async fn clear(&self) -> Result<()> {
            self.queue.lock().unwrap().clear();
            Ok(())
        }
    }

    struct MockWorkerRegistry;

    #[async_trait::async_trait]
    impl WorkerRegistry for MockWorkerRegistry {
        async fn register(&self, _handle: hodei_jobs_domain::worker::WorkerHandle, _spec: WorkerSpec) -> Result<hodei_jobs_domain::worker::Worker> {
            unimplemented!()
        }
        async fn unregister(&self, _worker_id: &WorkerId) -> Result<()> {
            Ok(())
        }
        async fn get(&self, _worker_id: &WorkerId) -> Result<Option<hodei_jobs_domain::worker::Worker>> {
            Ok(None)
        }
        async fn find(&self, _filter: &hodei_jobs_domain::worker_registry::WorkerFilter) -> Result<Vec<hodei_jobs_domain::worker::Worker>> {
            Ok(vec![])
        }
        async fn find_available(&self) -> Result<Vec<hodei_jobs_domain::worker::Worker>> {
            Ok(vec![])
        }
        async fn find_by_provider(&self, _provider_id: &ProviderId) -> Result<Vec<hodei_jobs_domain::worker::Worker>> {
            Ok(vec![])
        }
        async fn update_state(&self, _worker_id: &WorkerId, _state: hodei_jobs_domain::shared_kernel::WorkerState) -> Result<()> {
            Ok(())
        }
        async fn heartbeat(&self, _worker_id: &WorkerId) -> Result<()> {
            Ok(())
        }
        async fn assign_to_job(&self, _worker_id: &WorkerId, _job_id: JobId) -> Result<()> {
            Ok(())
        }
        async fn release_from_job(&self, _worker_id: &WorkerId) -> Result<()> {
            Ok(())
        }
        async fn find_unhealthy(&self, _timeout: Duration) -> Result<Vec<hodei_jobs_domain::worker::Worker>> {
            Ok(vec![])
        }
        async fn find_for_termination(&self) -> Result<Vec<hodei_jobs_domain::worker::Worker>> {
            Ok(vec![])
        }
        async fn stats(&self) -> Result<WorkerRegistryStats> {
            Ok(WorkerRegistryStats::default())
        }
        async fn count(&self) -> Result<usize> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let registry = Arc::new(MockWorkerRegistry);
        let job_repo = Arc::new(MockJobRepository::new());
        let job_queue = Arc::new(MockJobQueue::new());

        let _orchestrator = JobOrchestrator::new(
            registry,
            job_repo,
            job_queue,
            SchedulerConfig::default(),
            WorkerLifecycleConfig::default(),
        );
    }

    #[tokio::test]
    async fn test_submit_job_enqueues_when_no_resources() {
        let registry = Arc::new(MockWorkerRegistry);
        let job_repo = Arc::new(MockJobRepository::new());
        let job_queue = Arc::new(MockJobQueue::new());

        let orchestrator = JobOrchestrator::new(
            registry,
            job_repo.clone(),
            job_queue.clone(),
            SchedulerConfig::default(),
            WorkerLifecycleConfig::default(),
        );

        let job = Job::new(JobId::new(), JobSpec::new(vec!["echo".to_string()]));
        let job_id = orchestrator.submit_job(job).await.unwrap();

        // Job should be saved
        let saved = job_repo.find_by_id(&job_id).await.unwrap();
        assert!(saved.is_some());
    }

    #[tokio::test]
    async fn test_orchestrator_stats() {
        let registry = Arc::new(MockWorkerRegistry);
        let job_repo = Arc::new(MockJobRepository::new());
        let job_queue = Arc::new(MockJobQueue::new());

        let orchestrator = JobOrchestrator::new(
            registry,
            job_repo,
            job_queue,
            SchedulerConfig::default(),
            WorkerLifecycleConfig::default(),
        );

        let stats = orchestrator.stats().await.unwrap();
        assert_eq!(stats.queue_depth, 0);
        assert_eq!(stats.provider_count, 0);
    }
}

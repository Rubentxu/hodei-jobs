//! Job Orchestrator - Orquestación de ejecución de jobs
//!
//! Integra el scheduler, lifecycle manager y registros para
//! la ejecución completa de jobs.

use crate::saga::recovery_saga::DynRecoverySagaCoordinator;
use crate::{
    scheduling::smart_scheduler::{SchedulerConfig, SchedulingService},
    workers::lifecycle::{WorkerLifecycleConfig, WorkerLifecycleManager},
};
use dashmap::DashMap;
use hodei_server_domain::{
    event_bus::EventBus,
    jobs::{Job, JobQueue, JobRepository},
    outbox::OutboxRepository,
    scheduling::{ProviderInfo, SchedulingContext, SchedulingDecision},
    shared_kernel::{DomainError, JobId, ProviderId, WorkerId},
    workers::WorkerProvider,
    workers::WorkerSpec,
    workers::{WorkerRegistry, WorkerRegistryStats},
};
use std::{sync::Arc, time::Duration};
use tracing::{debug, error, info, warn};

/// Job Orchestrator - Coordina la ejecución de jobs
pub struct JobOrchestrator {
    scheduler: SchedulingService,
    lifecycle_manager: WorkerLifecycleManager,
    registry: Arc<dyn WorkerRegistry>,
    job_repository: Arc<dyn JobRepository>,
    job_queue: Arc<dyn JobQueue>,
    providers: Arc<DashMap<ProviderId, Arc<dyn WorkerProvider>>>,
    default_worker_spec: WorkerSpec,
}

impl JobOrchestrator {
    pub fn new(
        registry: Arc<dyn WorkerRegistry>,
        job_repository: Arc<dyn JobRepository>,
        job_queue: Arc<dyn JobQueue>,
        scheduler_config: SchedulerConfig,
        lifecycle_config: WorkerLifecycleConfig,
        event_bus: Arc<dyn EventBus>,
        outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
        recovery_saga_coordinator: Arc<DynRecoverySagaCoordinator>,
    ) -> Self {
        let default_worker_spec = WorkerSpec::new(
            "hodei-jobs-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );

        let providers = Arc::new(DashMap::new());

        Self {
            scheduler: SchedulingService::new(scheduler_config),
            lifecycle_manager: WorkerLifecycleManager::with_recovery_saga_coordinator(
                registry.clone(),
                providers.clone(),
                lifecycle_config,
                event_bus,
                outbox_repository,
                recovery_saga_coordinator,
            ),
            registry,
            job_repository,
            job_queue,
            providers,
            default_worker_spec,
        }
    }

    /// Register a provider with the orchestrator
    pub async fn register_provider(&self, provider: Arc<dyn WorkerProvider>) {
        let provider_id = provider.provider_id().clone();
        info!("Registering provider: {}", provider_id);

        self.lifecycle_manager
            .register_provider(provider.clone())
            .await;
        self.providers.insert(provider_id, provider);
    }

    /// Submit a job for execution
    pub async fn submit_job(&self, job: Job) -> anyhow::Result<JobId> {
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
                let job = self
                    .job_repository
                    .find_by_id(&job_id)
                    .await?
                    .ok_or_else(|| DomainError::JobNotFound {
                        job_id: job_id.clone(),
                    })?;
                self.job_queue.enqueue(job).await?;
            }
        }

        Ok(job_id)
    }

    /// Try to schedule a job
    async fn try_schedule_job(&self, job: Job) -> anyhow::Result<SchedulingDecision> {
        let context = self.build_scheduling_context(job).await?;
        self.scheduler.make_decision(context).await
    }

    /// Build scheduling context
    async fn build_scheduling_context(&self, job: Job) -> anyhow::Result<SchedulingContext> {
        let available_workers = self.registry.find_available().await?;
        let pending_jobs_count = self.job_queue.len().await?;
        let stats = self.registry.stats().await?;

        let mut available_providers = Vec::new();

        for entry in self.providers.iter() {
            let provider_id = entry.key();
            let provider = entry.value();
            let health = provider.health_check().await;
            let _health_score = match health {
                Ok(hodei_server_domain::workers::HealthStatus::Healthy) => 1.0,
                Ok(hodei_server_domain::workers::HealthStatus::Degraded { .. }) => 0.5,
                _ => 0.0,
            };

            let workers_for_provider = self.registry.find_by_provider(provider_id).await?;
            let caps = provider.capabilities();

            // Calculate real metrics from provider
            let real_health_score = provider.calculate_health_score();
            let real_cost_per_hour = provider.calculate_average_cost_per_hour();

            available_providers.push(ProviderInfo {
                provider_id: provider_id.clone(),
                provider_type: provider.provider_type(),
                active_workers: workers_for_provider.len(),
                max_workers: 10, // Could be from config
                estimated_startup_time: caps.max_execution_time.unwrap_or(Duration::from_secs(30)),
                health_score: real_health_score,
                cost_per_hour: real_cost_per_hour,
                // US-27.4: GPU support from capabilities
                gpu_support: caps.gpu_support,
                gpu_types: caps.gpu_types.clone(),
                // US-27.6: Region from capabilities
                regions: caps.regions.clone(),
            });
        }

        let system_load = if stats.total_workers > 0 {
            stats.busy_workers as f64 / stats.total_workers as f64
        } else {
            0.0
        };

        Ok(SchedulingContext {
            job: job.clone(),
            job_preferences: job.spec.preferences.clone(),
            available_workers,
            available_providers,
            pending_jobs_count,
            system_load,
        })
    }

    /// Handle a scheduling decision
    async fn handle_scheduling_decision(&self, decision: SchedulingDecision) -> anyhow::Result<()> {
        match decision {
            SchedulingDecision::AssignToWorker { job_id, worker_id } => {
                self.assign_job_to_worker(&job_id, &worker_id).await?;
            }
            SchedulingDecision::ProvisionWorker {
                job_id,
                provider_id,
            } => {
                self.provision_and_assign(&job_id, &provider_id).await?;
            }
            SchedulingDecision::Enqueue { job_id, reason } => {
                debug!("Job {} enqueued: {}", job_id, reason);
                let job = self
                    .job_repository
                    .find_by_id(&job_id)
                    .await?
                    .ok_or_else(|| DomainError::JobNotFound {
                        job_id: job_id.clone(),
                    })?;
                self.job_queue.enqueue(job).await?;
            }
            SchedulingDecision::Reject { job_id, reason } => {
                error!("Job {} rejected: {}", job_id, reason);
                // Update job state to failed
                if let Some(mut job) = self.job_repository.find_by_id(&job_id).await? {
                    job.fail(reason)?;
                    self.job_repository.update(&job).await?;
                }
            }
        }
        Ok(())
    }

    /// Assign a job to an existing worker
    async fn assign_job_to_worker(
        &self,
        job_id: &JobId,
        worker_id: &WorkerId,
    ) -> anyhow::Result<()> {
        info!("Assigning job {} to worker {}", job_id, worker_id);

        // Update worker state
        self.registry
            .assign_to_job(worker_id, job_id.clone())
            .await?;

        // Update job state
        if let Some(mut job) = self.job_repository.find_by_id(job_id).await? {
            job.mark_running()?;
            self.job_repository.update(&job).await?;
        }

        Ok(())
    }

    /// Provision a new worker and assign job
    async fn provision_and_assign(
        &self,
        job_id: &JobId,
        provider_id: &ProviderId,
    ) -> anyhow::Result<()> {
        info!("Provisioning worker for job {} via {}", job_id, provider_id);

        // Provision worker
        let spec = self.default_worker_spec.clone();
        let worker = self
            .lifecycle_manager
            .provision_worker(provider_id, spec, job_id.clone())
            .await?;

        // Wait for worker to be ready (simplified - in practice would use events)
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Assign job
        self.assign_job_to_worker(job_id, worker.id()).await?;

        Ok(())
    }

    /// Process pending jobs in the queue
    pub async fn process_queue(&self) -> anyhow::Result<usize> {
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
    pub async fn run_maintenance(&self) -> anyhow::Result<MaintenanceResult> {
        let mut result = MaintenanceResult::default();

        // Health check
        let health_result = self.lifecycle_manager.run_health_check().await?;
        result.unhealthy_workers = health_result.unhealthy_workers.len();

        // Cleanup idle workers
        let cleanup_result = self.lifecycle_manager.cleanup_workers().await?;
        result.terminated_workers = cleanup_result.terminated.len();

        // Process queue if we have capacity
        if self
            .lifecycle_manager
            .should_scale_up(self.job_queue.len().await?)
            .await
        {
            result.jobs_processed = self.process_queue().await?;
        }

        Ok(result)
    }

    /// Get orchestrator statistics
    pub async fn stats(&self) -> anyhow::Result<OrchestratorStats> {
        let registry_stats = self.registry.stats().await?;
        let queue_depth = self.job_queue.len().await?;
        let provider_count = self.providers.len();

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
    use futures::stream::BoxStream;
    use hodei_server_domain::event_bus::EventBusError;
    use hodei_server_domain::events::DomainEvent;
    use hodei_server_domain::jobs::{JobSpec, JobsFilter};
    use hodei_server_domain::shared_kernel::{JobState, Result};
    use std::collections::HashMap as StdHashMap;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use tokio::sync::RwLock as TokioRwLock;

    use hodei_server_domain::outbox::{OutboxError, OutboxEventInsert};

    struct MockEventBus;

    #[async_trait::async_trait]
    impl EventBus for MockEventBus {
        async fn publish(&self, _event: &DomainEvent) -> std::result::Result<(), EventBusError> {
            Ok(())
        }
        async fn subscribe(
            &self,
            _topic: &str,
        ) -> std::result::Result<
            BoxStream<'static, std::result::Result<DomainEvent, EventBusError>>,
            EventBusError,
        > {
            Err(EventBusError::SubscribeError("Mock".to_string()))
        }
    }

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

        async fn find(&self, _filter: JobsFilter) -> Result<Vec<Job>> {
            Ok(vec![])
        }

        async fn count_by_state(&self, _state: &JobState) -> Result<u64> {
            Ok(0)
        }

        async fn find_by_state(&self, _state: &JobState) -> Result<Vec<Job>> {
            Ok(vec![])
        }

        async fn find_pending(&self) -> Result<Vec<Job>> {
            Ok(vec![])
        }

        async fn find_all(&self, _limit: usize, _offset: usize) -> Result<(Vec<Job>, usize)> {
            Ok((vec![], 0))
        }

        async fn find_by_execution_id(&self, _execution_id: &str) -> Result<Option<Job>> {
            Ok(None)
        }

        async fn delete(&self, _job_id: &JobId) -> Result<()> {
            Ok(())
        }

        async fn update(&self, job: &Job) -> Result<()> {
            self.jobs.write().await.insert(job.id.clone(), job.clone());
            Ok(())
        }

        async fn update_state(&self, _job_id: &JobId, _new_state: JobState) -> Result<()> {
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
        async fn register(
            &self,
            _handle: hodei_server_domain::workers::WorkerHandle,
            _spec: WorkerSpec,
            _job_id: hodei_server_domain::shared_kernel::JobId,
        ) -> Result<hodei_server_domain::workers::Worker> {
            unimplemented!()
        }

        async fn save(&self, _worker: &hodei_server_domain::workers::Worker) -> Result<()> {
            Ok(())
        }

        async fn unregister(&self, _worker_id: &WorkerId) -> Result<()> {
            Ok(())
        }

        async fn find_by_id(
            &self,
            _worker_id: &WorkerId,
        ) -> Result<Option<hodei_server_domain::workers::Worker>> {
            Ok(None)
        }

        async fn get(
            &self,
            _worker_id: &WorkerId,
        ) -> Result<Option<hodei_server_domain::workers::Worker>> {
            Ok(None)
        }

        async fn get_by_job_id(
            &self,
            _job_id: &JobId,
        ) -> Result<Option<hodei_server_domain::workers::Worker>> {
            Ok(None)
        }

        async fn find(
            &self,
            _filter: &hodei_server_domain::workers::WorkerFilter,
        ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }

        async fn find_ready_worker(
            &self,
            _filter: Option<&hodei_server_domain::workers::WorkerFilter>,
        ) -> Result<Option<hodei_server_domain::workers::Worker>> {
            Ok(None)
        }

        async fn find_available(&self) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }

        async fn find_by_provider(
            &self,
            _provider_id: &ProviderId,
        ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }

        async fn update_state(
            &self,
            _worker_id: &WorkerId,
            _state: hodei_server_domain::shared_kernel::WorkerState,
        ) -> Result<()> {
            Ok(())
        }

        async fn update_heartbeat(&self, _worker_id: &WorkerId) -> Result<()> {
            Ok(())
        }

        async fn heartbeat(&self, _worker_id: &WorkerId) -> Result<()> {
            Ok(())
        }

        async fn mark_busy(&self, _worker_id: &WorkerId, _job_id: Option<JobId>) -> Result<()> {
            Ok(())
        }

        async fn assign_to_job(&self, _worker_id: &WorkerId, _job_id: JobId) -> Result<()> {
            Ok(())
        }

        async fn release_from_job(&self, _worker_id: &WorkerId) -> Result<()> {
            Ok(())
        }
        async fn find_unhealthy(
            &self,
            _timeout: Duration,
        ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }
        async fn find_for_termination(&self) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }
        // EPIC-26 US-26.7: TTL-related methods
        async fn find_idle_timed_out(&self) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }
        async fn find_lifetime_exceeded(
            &self,
        ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }
        async fn find_ttl_after_completion_exceeded(
            &self,
        ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            Ok(vec![])
        }
        async fn stats(&self) -> Result<WorkerRegistryStats> {
            Ok(WorkerRegistryStats::default())
        }
        async fn count(&self) -> Result<usize> {
            Ok(0)
        }
    }

    struct MockOutboxRepository;

    use hodei_server_domain::saga::{
        SagaContext, SagaExecutionResult, SagaId, SagaState, SagaType,
    };

    /// Mock SagaOrchestrator for testing
    struct MockSagaOrchestrator;

    impl MockSagaOrchestrator {
        fn new() -> Self {
            Self
        }
    }

    #[async_trait::async_trait]
    impl SagaOrchestrator for MockSagaOrchestrator {
        type Error = DomainError;

        async fn execute_saga(
            &self,
            saga: &dyn hodei_server_domain::saga::Saga,
            context: SagaContext,
        ) -> std::result::Result<SagaExecutionResult, Self::Error> {
            Ok(SagaExecutionResult {
                saga_id: SagaId::new(),
                saga_type: saga.saga_type(),
                state: SagaState::Completed,
                steps_executed: 1,
                compensations_executed: 0,
                duration: Duration::from_millis(10),
                error_message: None,
            })
        }

        async fn execute(
            &self,
            context: &SagaContext,
        ) -> std::result::Result<SagaExecutionResult, Self::Error> {
            Ok(SagaExecutionResult {
                saga_id: context.saga_id.clone(),
                saga_type: SagaType::Recovery,
                state: SagaState::Completed,
                steps_executed: 0,
                compensations_executed: 0,
                duration: Duration::from_millis(5),
                error_message: None,
            })
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

    /// Create a mock DynRecoverySagaCoordinator for tests
    fn create_mock_recovery_coordinator(
        registry: Arc<dyn WorkerRegistry + Send + Sync>,
        event_bus: Arc<dyn EventBus + Send + Sync>,
    ) -> Arc<DynRecoverySagaCoordinator> {
        use hodei_server_domain::command::InMemoryErasedCommandBus;

        let orchestrator: Arc<dyn SagaOrchestrator<Error = DomainError> + Send + Sync> =
            Arc::new(MockSagaOrchestrator);
        let saga_config = RecoverySagaCoordinatorConfig::default();
        let command_bus: DynCommandBus = Arc::new(InMemoryErasedCommandBus::new());

        Arc::new(DynRecoverySagaCoordinator::new(
            orchestrator,
            command_bus,
            registry,
            event_bus,
            None,
            None,
            Some(saga_config),
            None,
        ))
    }

    #[async_trait::async_trait]
    impl OutboxRepository for MockOutboxRepository {
        async fn insert_events(
            &self,
            _events: &[OutboxEventInsert],
        ) -> std::result::Result<(), OutboxError> {
            Ok(())
        }

        async fn get_pending_events(
            &self,
            _limit: usize,
            _max_retries: i32,
        ) -> std::result::Result<Vec<hodei_server_domain::outbox::OutboxEventView>, OutboxError>
        {
            Ok(vec![])
        }

        async fn mark_published(
            &self,
            _ids: &[uuid::Uuid],
        ) -> std::result::Result<(), OutboxError> {
            Ok(())
        }

        async fn mark_failed(
            &self,
            _event_id: &uuid::Uuid,
            _error: &str,
        ) -> std::result::Result<(), OutboxError> {
            Ok(())
        }

        async fn exists_by_idempotency_key(
            &self,
            _key: &str,
        ) -> std::result::Result<bool, OutboxError> {
            Ok(false)
        }

        async fn count_pending(&self) -> std::result::Result<u64, OutboxError> {
            Ok(0)
        }

        async fn get_stats(
            &self,
        ) -> std::result::Result<hodei_server_domain::outbox::OutboxStats, OutboxError> {
            Ok(hodei_server_domain::outbox::OutboxStats {
                pending_count: 0,
                published_count: 0,
                failed_count: 0,
                oldest_pending_age_seconds: None,
            })
        }

        async fn cleanup_published_events(
            &self,
            _older_than: std::time::Duration,
        ) -> std::result::Result<u64, OutboxError> {
            Ok(0)
        }

        async fn cleanup_failed_events(
            &self,
            _max_retries: i32,
            _older_than: std::time::Duration,
        ) -> std::result::Result<u64, OutboxError> {
            Ok(0)
        }

        async fn find_by_id(
            &self,
            _id: uuid::Uuid,
        ) -> std::result::Result<Option<hodei_server_domain::outbox::OutboxEventView>, OutboxError>
        {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let registry: Arc<dyn WorkerRegistry> = Arc::new(MockWorkerRegistry);
        let job_repo = Arc::new(MockJobRepository::new());
        let job_queue = Arc::new(MockJobQueue::new());
        let event_bus: Arc<dyn EventBus> = Arc::new(MockEventBus);
        let outbox = Arc::new(MockOutboxRepository);
        let recovery_coordinator =
            create_mock_recovery_coordinator(registry.clone(), event_bus.clone());

        let _orchestrator = JobOrchestrator::new(
            registry,
            job_repo,
            job_queue,
            SchedulerConfig::default(),
            WorkerLifecycleConfig::default(),
            event_bus,
            outbox,
            recovery_coordinator,
        );
    }

    #[tokio::test]
    async fn test_submit_job_enqueues_when_no_resources() {
        let registry: Arc<dyn WorkerRegistry> = Arc::new(MockWorkerRegistry);
        let job_repo = Arc::new(MockJobRepository::new());
        let job_queue = Arc::new(MockJobQueue::new());
        let event_bus: Arc<dyn EventBus> = Arc::new(MockEventBus);
        let recovery_coordinator =
            create_mock_recovery_coordinator(registry.clone(), event_bus.clone());

        let orchestrator = JobOrchestrator::new(
            registry,
            job_repo.clone(),
            job_queue.clone(),
            SchedulerConfig::default(),
            WorkerLifecycleConfig::default(),
            event_bus,
            Arc::new(MockOutboxRepository),
            recovery_coordinator,
        );

        let job = Job::new(
            JobId::new(),
            "test-job".to_string(),
            JobSpec::new(vec!["echo".to_string()]),
        );
        let job_id = orchestrator.submit_job(job).await.unwrap();

        // Job should be saved
        let saved = job_repo.find_by_id(&job_id).await.unwrap();
        assert!(saved.is_some());
    }

    #[tokio::test]
    async fn test_orchestrator_stats() {
        let registry: Arc<dyn WorkerRegistry> = Arc::new(MockWorkerRegistry);
        let job_repo = Arc::new(MockJobRepository::new());
        let job_queue = Arc::new(MockJobQueue::new());
        let event_bus: Arc<dyn EventBus> = Arc::new(MockEventBus);
        let recovery_coordinator =
            create_mock_recovery_coordinator(registry.clone(), event_bus.clone());

        let orchestrator = JobOrchestrator::new(
            registry,
            job_repo,
            job_queue,
            SchedulerConfig::default(),
            WorkerLifecycleConfig::default(),
            event_bus,
            Arc::new(MockOutboxRepository),
            recovery_coordinator,
        );

        let stats = orchestrator.stats().await.unwrap();
        assert_eq!(stats.queue_depth, 0);
        assert_eq!(stats.provider_count, 0);
    }
}

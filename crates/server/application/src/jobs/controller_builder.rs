//! JobControllerBuilder
//!
//! Builder pattern to reduce Feature Envy in JobController construction.
//! Hides the details of component creation and improves testability.
//!
//! ## Connascence Reduction
//! - **Before**: Connascence of Algorithm - JobController::new() knew cómo build all components
//! - **After**: Connascence of Type - Builder encapsulates construction logic
//!
//! ## Usage
//! See unit tests for complete usage examples.

use crate::providers::ProviderRegistry;
use crate::saga::provisioning_workflow_coordinator::ProvisioningWorkflowCoordinator;
use crate::scheduling::smart_scheduler::SchedulerConfig;
use crate::workers::commands::WorkerCommandSender;
use crate::workers::provisioning::WorkerProvisioningService;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::jobs::{JobQueue, JobRepository};
use hodei_server_domain::outbox::OutboxRepository;
use hodei_server_domain::workers::WorkerRegistry;
use sqlx::PgPool;
use std::sync::Arc;

/// Builder for JobController
///
/// Reduces Feature Envy by encapsulating the construction logic.
/// The caller only needs to provide the required dependencies,
/// and the builder handles the creation of specialized components.
#[derive(Default)]
pub struct JobControllerBuilder {
    // Required dependencies
    job_queue: Option<Arc<dyn JobQueue>>,
    job_repository: Option<Arc<dyn JobRepository>>,
    worker_registry: Option<Arc<dyn WorkerRegistry>>,
    provider_registry: Option<Arc<ProviderRegistry>>,
    scheduler_config: Option<SchedulerConfig>,
    worker_command_sender: Option<Arc<dyn WorkerCommandSender>>,
    event_bus: Option<Arc<dyn EventBus>>,
    outbox_repository: Option<Arc<dyn OutboxRepository + Send + Sync>>,

    // Optional dependencies
    provisioning_service: Option<Arc<dyn WorkerProvisioningService>>,

    // EPIC-32: Reactive system dependencies
    pool: Option<PgPool>,

    // EPIC-94-C: v4.0 Provisioning workflow coordinator
    provisioning_workflow_coordinator: Option<Arc<dyn ProvisioningWorkflowCoordinator>>,
}

impl std::fmt::Debug for JobControllerBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobControllerBuilder")
            .field("job_queue", &self.job_queue.is_some())
            .field("job_repository", &self.job_repository.is_some())
            .field("worker_registry", &self.worker_registry.is_some())
            .field("provider_registry", &self.provider_registry.is_some())
            .field("scheduler_config", &self.scheduler_config.is_some())
            .field(
                "worker_command_sender",
                &self.worker_command_sender.is_some(),
            )
            .field("event_bus", &self.event_bus.is_some())
            .field("provisioning_service", &self.provisioning_service.is_some())
            .finish()
    }
}

impl JobControllerBuilder {
    /// Create a new builder with default (empty) configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the job queue (REQUIRED)
    pub fn with_job_queue(mut self, job_queue: Arc<dyn JobQueue>) -> Self {
        self.job_queue = Some(job_queue);
        self
    }

    /// Set the job repository (REQUIRED)
    pub fn with_job_repository(mut self, job_repository: Arc<dyn JobRepository>) -> Self {
        self.job_repository = Some(job_repository);
        self
    }

    /// Set the worker registry (REQUIRED)
    pub fn with_worker_registry(mut self, worker_registry: Arc<dyn WorkerRegistry>) -> Self {
        self.worker_registry = Some(worker_registry);
        self
    }

    /// Set the provider registry (REQUIRED)
    pub fn with_provider_registry(mut self, provider_registry: Arc<ProviderRegistry>) -> Self {
        self.provider_registry = Some(provider_registry);
        self
    }

    /// Set the scheduler configuration (REQUIRED)
    pub fn with_scheduler_config(mut self, scheduler_config: SchedulerConfig) -> Self {
        self.scheduler_config = Some(scheduler_config);
        self
    }

    /// Set the worker command sender (REQUIRED)
    pub fn with_worker_command_sender(
        mut self,
        worker_command_sender: Arc<dyn WorkerCommandSender>,
    ) -> Self {
        self.worker_command_sender = Some(worker_command_sender);
        self
    }

    /// Set the event bus (REQUIRED)
    pub fn with_event_bus(mut self, event_bus: Arc<dyn EventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Set the outbox repository (REQUIRED)
    pub fn with_outbox_repository(
        mut self,
        outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
    ) -> Self {
        self.outbox_repository = Some(outbox_repository);
        self
    }

    /// Set the provisioning service (OPTIONAL)
    pub fn with_provisioning_service(
        mut self,
        provisioning_service: Arc<dyn WorkerProvisioningService>,
    ) -> Self {
        self.provisioning_service = Some(provisioning_service);
        self
    }

    /// Set the database pool for reactive subscriptions (EPIC-32)
    pub fn with_pool(mut self, pool: PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    /// Set the provisioning workflow coordinator (EPIC-94-C v4.0)
    pub fn with_provisioning_workflow_coordinator(
        mut self,
        provisioning_workflow_coordinator: Arc<dyn ProvisioningWorkflowCoordinator>,
    ) -> Self {
        self.provisioning_workflow_coordinator = Some(provisioning_workflow_coordinator);
        self
    }

    /// Build the JobController
    ///
    /// # Errors
    /// Returns an error if any required dependency is missing.
    ///
    /// # Returns
    /// A configured JobController ready to use.
    pub fn build(self) -> anyhow::Result<super::JobController> {
        // Validate required fields
        let job_queue = self.job_queue.ok_or_else(|| {
            anyhow::anyhow!("JobControllerBuilder: job_queue is required. Use with_job_queue().")
        })?;
        let job_repository = self.job_repository.ok_or_else(|| {
            anyhow::anyhow!(
                "JobControllerBuilder: job_repository is required. Use with_job_repository()."
            )
        })?;
        let worker_registry = self.worker_registry.ok_or_else(|| {
            anyhow::anyhow!(
                "JobControllerBuilder: worker_registry is required. Use with_worker_registry()."
            )
        })?;
        let provider_registry = self.provider_registry.ok_or_else(|| {
            anyhow::anyhow!(
                "JobControllerBuilder: provider_registry is required. Use with_provider_registry()."
            )
        })?;
        let scheduler_config = self.scheduler_config.ok_or_else(|| {
            anyhow::anyhow!(
                "JobControllerBuilder: scheduler_config is required. Use with_scheduler_config()."
            )
        })?;
        let worker_command_sender = self.worker_command_sender.ok_or_else(|| {
            anyhow::anyhow!("JobControllerBuilder: worker_command_sender is required. Use with_worker_command_sender().")
        })?;
        let event_bus = self.event_bus.ok_or_else(|| {
            anyhow::anyhow!("JobControllerBuilder: event_bus is required. Use with_event_bus().")
        })?;
        let outbox_repository = self.outbox_repository.ok_or_else(|| {
            anyhow::anyhow!(
                "JobControllerBuilder: outbox_repository is required. Use with_outbox_repository()."
            )
        })?;

        // Note: JobDispatcher and WorkerMonitor are created internally by JobController::new()
        // This is intentional - we use the public API to maintain encapsulation

        // Create the JobController using its public constructor
        let controller = super::JobController::new(
            job_queue,
            job_repository,
            worker_registry,
            provider_registry,
            scheduler_config,
            worker_command_sender,
            event_bus,
            outbox_repository,
            self.provisioning_service,
            None, // execution_saga_dispatcher - can be set via builder
            self.provisioning_workflow_coordinator, // EPIC-94-C: v4.0 workflow coordinator
            self.pool.clone().unwrap_or_else(||
                // For tests, create a lazy pool that won't actually connect
                PgPool::connect_lazy("postgresql://localhost/hodei_test").expect("Failed to create test pool")
            ),
        );

        Ok(controller)
    }
}

/// Error indicating a missing required field in JobControllerBuilder
#[derive(Debug, thiserror::Error)]
#[error("Missing required field: {field}")]
pub struct MissingFieldError {
    field: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::registry::ProviderRegistry;
    use async_trait::async_trait;
    use hodei_server_domain::event_bus::{EventBus, EventBusError};
    use hodei_server_domain::events::DomainEvent;
    use hodei_server_domain::jobs::{Job, JobQueue, JobRepository, JobsFilter};
    use hodei_server_domain::outbox::{OutboxError, OutboxEventInsert, OutboxRepository};
    use hodei_server_domain::shared_kernel::{
        DomainError, JobId, JobState, ProviderId, WorkerId, WorkerState,
    };
    use hodei_server_domain::workers::{
        Worker, WorkerFilter, WorkerHandle, WorkerRegistry, WorkerRegistryStats, WorkerSpec,
    };
    use std::collections::HashMap;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::sync::RwLock;

    use futures::stream::BoxStream;

    type TestResult<T = ()> = std::result::Result<T, DomainError>;

    // Local mock implementations
    struct MockEventBus;

    #[async_trait]
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

    struct MockOutboxRepository;

    #[async_trait]
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

    #[async_trait]
    impl JobQueue for MockJobQueue {
        async fn enqueue(&self, job: Job) -> TestResult {
            self.queue.lock().unwrap().push_back(job);
            Ok(())
        }
        async fn dequeue(&self) -> TestResult<Option<Job>> {
            Ok(self.queue.lock().unwrap().pop_front())
        }
        async fn peek(&self) -> TestResult<Option<Job>> {
            Ok(self.queue.lock().unwrap().front().cloned())
        }
        async fn len(&self) -> TestResult<usize> {
            Ok(self.queue.lock().unwrap().len())
        }
        async fn is_empty(&self) -> TestResult<bool> {
            Ok(self.queue.lock().unwrap().is_empty())
        }
        async fn clear(&self) -> TestResult {
            self.queue.lock().unwrap().clear();
            Ok(())
        }
    }

    struct MockJobRepository {
        jobs: Arc<RwLock<HashMap<JobId, Job>>>,
    }

    impl MockJobRepository {
        fn new() -> Self {
            Self {
                jobs: Arc::new(RwLock::new(HashMap::new())),
            }
        }
    }

    #[async_trait]
    impl JobRepository for MockJobRepository {
        async fn save(&self, job: &Job) -> TestResult {
            self.jobs.write().await.insert(job.id.clone(), job.clone());
            Ok(())
        }
        async fn find_by_id(&self, job_id: &JobId) -> TestResult<Option<Job>> {
            Ok(self.jobs.read().await.get(job_id).cloned())
        }
        async fn find_by_state(&self, _state: &JobState) -> TestResult<Vec<Job>> {
            Ok(vec![])
        }
        async fn find_pending(&self) -> TestResult<Vec<Job>> {
            Ok(vec![])
        }
        async fn find_all(&self, _limit: usize, _offset: usize) -> TestResult<(Vec<Job>, usize)> {
            Ok((vec![], 0))
        }
        async fn find_by_execution_id(&self, _execution_id: &str) -> TestResult<Option<Job>> {
            Ok(None)
        }
        async fn delete(&self, _job_id: &JobId) -> TestResult {
            Ok(())
        }
        async fn update(&self, job: &Job) -> TestResult {
            self.jobs.write().await.insert(job.id.clone(), job.clone());
            Ok(())
        }

        async fn find(&self, _filter: JobsFilter) -> TestResult<Vec<Job>> {
            Ok(vec![])
        }

        async fn count_by_state(&self, _state: &JobState) -> TestResult<u64> {
            Ok(0)
        }

        async fn update_state(&self, _job_id: &JobId, _new_state: JobState) -> TestResult {
            Ok(())
        }
    }

    struct MockWorkerRegistry;

    impl MockWorkerRegistry {
        fn new() -> Self {
            Self
        }
    }

    #[async_trait]
    impl WorkerRegistry for MockWorkerRegistry {
        async fn register(
            &self,
            _handle: WorkerHandle,
            _spec: WorkerSpec,
            _job_id: JobId,
        ) -> TestResult<Worker> {
            unimplemented!()
        }

        async fn save(&self, _worker: &Worker) -> TestResult {
            Ok(())
        }

        async fn unregister(&self, _worker_id: &WorkerId) -> TestResult {
            Ok(())
        }

        async fn find_by_id(&self, _worker_id: &WorkerId) -> TestResult<Option<Worker>> {
            Ok(None)
        }

        async fn get(&self, _worker_id: &WorkerId) -> TestResult<Option<Worker>> {
            Ok(None)
        }

        async fn get_by_job_id(&self, _job_id: &JobId) -> TestResult<Option<Worker>> {
            Ok(None)
        }

        async fn find(&self, _filter: &WorkerFilter) -> TestResult<Vec<Worker>> {
            Ok(vec![])
        }

        async fn find_ready_worker(
            &self,
            _filter: Option<&WorkerFilter>,
        ) -> TestResult<Option<Worker>> {
            Ok(None)
        }

        async fn find_available(&self) -> TestResult<Vec<Worker>> {
            Ok(vec![])
        }

        async fn find_by_provider(&self, _provider_id: &ProviderId) -> TestResult<Vec<Worker>> {
            Ok(vec![])
        }

        async fn update_state(&self, _worker_id: &WorkerId, _state: WorkerState) -> TestResult {
            Ok(())
        }

        async fn update_heartbeat(&self, _worker_id: &WorkerId) -> TestResult {
            Ok(())
        }

        async fn heartbeat(&self, _worker_id: &WorkerId) -> TestResult {
            Ok(())
        }

        async fn mark_busy(&self, _worker_id: &WorkerId, _job_id: Option<JobId>) -> TestResult {
            Ok(())
        }

        async fn assign_to_job(&self, _worker_id: &WorkerId, _job_id: JobId) -> TestResult {
            Ok(())
        }

        async fn release_from_job(&self, _worker_id: &WorkerId) -> TestResult {
            Ok(())
        }
        async fn find_unhealthy(&self, _timeout: Duration) -> TestResult<Vec<Worker>> {
            Ok(vec![])
        }
        async fn find_for_termination(&self) -> TestResult<Vec<Worker>> {
            Ok(vec![])
        }
        // EPIC-26 US-26.7: TTL-related methods
        async fn find_idle_timed_out(&self) -> TestResult<Vec<Worker>> {
            Ok(vec![])
        }
        async fn find_lifetime_exceeded(&self) -> TestResult<Vec<Worker>> {
            Ok(vec![])
        }
        async fn find_ttl_after_completion_exceeded(&self) -> TestResult<Vec<Worker>> {
            Ok(vec![])
        }
        async fn stats(&self) -> TestResult<WorkerRegistryStats> {
            Ok(WorkerRegistryStats::default())
        }
        async fn count(&self) -> TestResult<usize> {
            Ok(0)
        }
    }

    struct MockWorkerCommandSender;

    impl MockWorkerCommandSender {
        fn new() -> Self {
            Self
        }
    }

    #[async_trait]
    impl crate::workers::commands::WorkerCommandSender for MockWorkerCommandSender {
        async fn send_run_job(&self, _worker_id: &WorkerId, _job: &Job) -> TestResult {
            Ok(())
        }
    }

    fn create_provider_registry() -> Arc<ProviderRegistry> {
        // Create a minimal ProviderRegistry with no providers
        // This is enough for the builder to construct the controller
        struct DummyProviderConfigRepo;

        #[async_trait]
        impl hodei_server_domain::providers::ProviderConfigRepository for DummyProviderConfigRepo {
            async fn save(
                &self,
                _config: &hodei_server_domain::providers::ProviderConfig,
            ) -> TestResult {
                Ok(())
            }
            async fn find_by_id(
                &self,
                _id: &ProviderId,
            ) -> TestResult<Option<hodei_server_domain::providers::ProviderConfig>> {
                Ok(None)
            }
            async fn find_all(
                &self,
            ) -> TestResult<Vec<hodei_server_domain::providers::ProviderConfig>> {
                Ok(vec![])
            }
            async fn delete(&self, _id: &ProviderId) -> TestResult {
                Ok(())
            }
            async fn find_by_type(
                &self,
                _provider_type: &hodei_server_domain::ProviderType,
            ) -> TestResult<Vec<hodei_server_domain::providers::ProviderConfig>> {
                Ok(vec![])
            }
            async fn find_enabled(
                &self,
            ) -> TestResult<Vec<hodei_server_domain::providers::ProviderConfig>> {
                Ok(vec![])
            }
            async fn find_by_name(
                &self,
                _name: &str,
            ) -> TestResult<Option<hodei_server_domain::providers::ProviderConfig>> {
                Ok(None)
            }
            async fn find_with_capacity(
                &self,
            ) -> TestResult<Vec<hodei_server_domain::providers::ProviderConfig>> {
                Ok(vec![])
            }
            async fn update(
                &self,
                _config: &hodei_server_domain::providers::ProviderConfig,
            ) -> TestResult {
                Ok(())
            }
            async fn exists_by_name(&self, _name: &str) -> TestResult<bool> {
                Ok(false)
            }
        }

        let config_repo: Arc<dyn hodei_server_domain::providers::ProviderConfigRepository> =
            Arc::new(DummyProviderConfigRepo);
        Arc::new(ProviderRegistry::with_event_bus(
            config_repo,
            Arc::new(MockEventBus),
        ))
    }

    fn create_test_components() -> (
        Arc<dyn JobQueue>,
        Arc<dyn JobRepository>,
        Arc<dyn WorkerRegistry>,
        Arc<ProviderRegistry>,
        Arc<dyn WorkerCommandSender>,
        Arc<dyn EventBus>,
        Arc<dyn OutboxRepository + Send + Sync>,
    ) {
        let job_queue: Arc<dyn JobQueue> = Arc::new(MockJobQueue::new());
        let job_repository: Arc<dyn JobRepository> = Arc::new(MockJobRepository::new());
        let worker_registry: Arc<dyn WorkerRegistry> = Arc::new(MockWorkerRegistry::new());
        let provider_registry = create_provider_registry();
        let worker_command_sender: Arc<dyn WorkerCommandSender> =
            Arc::new(MockWorkerCommandSender::new());
        let event_bus: Arc<dyn EventBus> = Arc::new(MockEventBus);
        let outbox_repository: Arc<dyn OutboxRepository + Send + Sync> =
            Arc::new(MockOutboxRepository);

        (
            job_queue,
            job_repository,
            worker_registry,
            provider_registry,
            worker_command_sender,
            event_bus,
            outbox_repository,
        )
    }

    #[tokio::test]
    async fn test_builder_all_fields() {
        let (
            job_queue,
            job_repository,
            worker_registry,
            provider_registry,
            worker_command_sender,
            event_bus,
            outbox_repository,
        ) = create_test_components();

        let controller = JobControllerBuilder::new()
            .with_job_queue(job_queue)
            .with_job_repository(job_repository)
            .with_worker_registry(worker_registry)
            .with_provider_registry(provider_registry)
            .with_scheduler_config(SchedulerConfig::default())
            .with_worker_command_sender(worker_command_sender)
            .with_event_bus(event_bus)
            .with_outbox_repository(outbox_repository)
            // Skip pool for unit tests since it requires async runtime
            .build();

        assert!(controller.is_ok());
    }

    #[tokio::test]
    async fn test_builder_missing_job_queue() {
        let (
            _,
            job_repository,
            worker_registry,
            provider_registry,
            worker_command_sender,
            event_bus,
            outbox_repository,
        ) = create_test_components();

        let result = JobControllerBuilder::new()
            .with_job_repository(job_repository)
            .with_worker_registry(worker_registry)
            .with_provider_registry(provider_registry)
            .with_scheduler_config(SchedulerConfig::default())
            .with_worker_command_sender(worker_command_sender)
            .with_event_bus(event_bus)
            .with_outbox_repository(outbox_repository)
            // Skip pool for unit tests
            .build();

        // Verify that building fails when job_queue is missing
        match result {
            Ok(_) => panic!("Expected error when job_queue is missing"),
            Err(_) => {} // Test passes
        }
    }

    #[tokio::test]
    async fn test_builder_missing_job_repository() {
        let (
            job_queue,
            _,
            worker_registry,
            provider_registry,
            worker_command_sender,
            event_bus,
            outbox_repository,
        ) = create_test_components();

        let result = JobControllerBuilder::new()
            .with_job_queue(job_queue)
            .with_worker_registry(worker_registry)
            .with_provider_registry(provider_registry)
            .with_scheduler_config(SchedulerConfig::default())
            .with_worker_command_sender(worker_command_sender)
            .with_event_bus(event_bus)
            .with_outbox_repository(outbox_repository)
            // Skip pool for unit tests
            .build();

        // Verify that building fails when job_repository is missing
        match result {
            Ok(_) => panic!("Expected error when job_repository is missing"),
            Err(_) => {} // Test passes
        }
    }

    #[tokio::test]
    async fn test_builder_all_required_fields_present() {
        let (
            job_queue,
            job_repository,
            worker_registry,
            provider_registry,
            worker_command_sender,
            event_bus,
            outbox_repository,
        ) = create_test_components();

        let result = JobControllerBuilder::new()
            .with_job_queue(job_queue)
            .with_job_repository(job_repository)
            .with_worker_registry(worker_registry)
            .with_provider_registry(provider_registry)
            .with_scheduler_config(SchedulerConfig::default())
            .with_worker_command_sender(worker_command_sender)
            .with_event_bus(event_bus)
            .with_outbox_repository(outbox_repository)
            // Skip pool for unit tests
            .build();

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_builder_fluent_interface() {
        let (
            job_queue,
            job_repository,
            worker_registry,
            provider_registry,
            worker_command_sender,
            event_bus,
            outbox_repository,
        ) = create_test_components();

        // Test that all setters can be chained
        let controller = JobControllerBuilder::new()
            .with_job_queue(job_queue)
            .with_job_repository(job_repository)
            .with_worker_registry(worker_registry)
            .with_provider_registry(provider_registry)
            .with_scheduler_config(SchedulerConfig::default())
            .with_worker_command_sender(worker_command_sender)
            .with_event_bus(event_bus)
            .with_outbox_repository(outbox_repository)
            // Skip pool for unit tests
            .build();

        assert!(controller.is_ok());
    }

    #[test]
    fn test_builder_default() {
        // Default builder should have no fields set
        let builder = JobControllerBuilder::new();
        assert!(builder.job_queue.is_none());
        assert!(builder.job_repository.is_none());
        assert!(builder.worker_registry.is_none());
        assert!(builder.provider_registry.is_none());
        assert!(builder.scheduler_config.is_none());
        assert!(builder.worker_command_sender.is_none());
        assert!(builder.event_bus.is_none());
    }
}

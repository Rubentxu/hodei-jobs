//! In-Memory Repositories - TEST ONLY
//!
//! These implementations are for testing purposes only.
//! Do NOT use in production code.
//!
//! They provide fast, isolated test data without requiring a database.

pub mod test_in_memory {
    use async_trait::async_trait;
    use hodei_server_domain::jobs::{Job, JobQueue, JobRepository};
    use hodei_server_domain::shared_kernel::{DomainError, JobId, Result};
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Arc, Mutex};
    use tokio::sync::RwLock;

    /// In-memory Job Repository for tests
    #[derive(Clone)]
    pub struct InMemoryJobRepository {
        jobs: Arc<RwLock<HashMap<JobId, Job>>>,
    }

    impl InMemoryJobRepository {
        pub fn new() -> Self {
            Self {
                jobs: Arc::new(RwLock::new(HashMap::new())),
            }
        }
    }

    impl Default for InMemoryJobRepository {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait::async_trait]
    impl JobRepository for InMemoryJobRepository {
        async fn save(&self, job: &Job) -> Result<()> {
            let mut jobs = self.jobs.write().await;
            jobs.insert(job.id.clone(), job.clone());
            Ok(())
        }

        async fn find_by_id(&self, job_id: &JobId) -> Result<Option<Job>> {
            let jobs = self.jobs.read().await;
            Ok(jobs.get(job_id).cloned())
        }

        async fn find_by_state(
            &self,
            state: &hodei_server_domain::shared_kernel::JobState,
        ) -> Result<Vec<Job>> {
            let jobs = self.jobs.read().await;
            Ok(jobs
                .values()
                .filter(|job| job.state() == state)
                .cloned()
                .collect())
        }

        async fn find_pending(&self) -> Result<Vec<Job>> {
            let jobs = self.jobs.read().await;
            Ok(jobs
                .values()
                .filter(|job| {
                    matches!(
                        job.state(),
                        hodei_server_domain::shared_kernel::JobState::Pending
                    )
                })
                .cloned()
                .collect())
        }

        async fn find_all(&self, limit: usize, offset: usize) -> Result<(Vec<Job>, usize)> {
            let jobs = self.jobs.read().await;
            let total = jobs.len();
            let mut sorted_jobs: Vec<Job> = jobs.values().cloned().collect();
            sorted_jobs.sort_by(|a, b| b.created_at().cmp(a.created_at()));

            let page = sorted_jobs.into_iter().skip(offset).take(limit).collect();
            Ok((page, total))
        }

        async fn find_by_execution_id(&self, execution_id: &str) -> Result<Option<Job>> {
            let jobs = self.jobs.read().await;
            Ok(jobs
                .values()
                .find(|job| {
                    job.execution_context()
                        .map(|ctx| ctx.provider_execution_id == execution_id)
                        .unwrap_or(false)
                })
                .cloned())
        }

        async fn delete(&self, job_id: &JobId) -> Result<()> {
            let mut jobs = self.jobs.write().await;
            jobs.remove(job_id);
            Ok(())
        }

        async fn update(&self, job: &Job) -> Result<()> {
            let mut jobs = self.jobs.write().await;
            jobs.insert(job.id.clone(), job.clone());
            Ok(())
        }

        async fn update_state(
            &self,
            job_id: &JobId,
            new_state: hodei_shared::states::JobState,
        ) -> Result<()> {
            let mut jobs = self.jobs.write().await;
            if let Some(job) = jobs.get_mut(job_id) {
                job.set_state(new_state)?;
            }
            Ok(())
        }
    }

    /// In-memory Job Queue for tests
    #[derive(Clone)]
    pub struct InMemoryJobQueue {
        queue: Arc<Mutex<VecDeque<Job>>>,
    }

    impl InMemoryJobQueue {
        pub fn new() -> Self {
            Self {
                queue: Arc::new(Mutex::new(VecDeque::new())),
            }
        }
    }

    impl Default for InMemoryJobQueue {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait::async_trait]
    impl JobQueue for InMemoryJobQueue {
        async fn enqueue(&self, job: Job) -> Result<()> {
            let mut queue = self.queue.lock().unwrap();
            queue.push_back(job);
            Ok(())
        }

        async fn dequeue(&self) -> Result<Option<Job>> {
            let mut queue = self.queue.lock().unwrap();
            Ok(queue.pop_front())
        }

        async fn peek(&self) -> Result<Option<Job>> {
            let queue = self.queue.lock().unwrap();
            Ok(queue.front().cloned())
        }

        async fn len(&self) -> Result<usize> {
            let queue = self.queue.lock().unwrap();
            Ok(queue.len())
        }

        async fn is_empty(&self) -> Result<bool> {
            let queue = self.queue.lock().unwrap();
            Ok(queue.is_empty())
        }

        async fn clear(&self) -> Result<()> {
            let mut queue = self.queue.lock().unwrap();
            queue.clear();
            Ok(())
        }
    }

    /// In-memory Worker Registry for tests
    #[derive(Debug, Clone)]
    pub struct InMemoryWorkerRegistry {
        workers: Arc<
            RwLock<
                HashMap<
                    hodei_server_domain::shared_kernel::WorkerId,
                    hodei_server_domain::workers::Worker,
                >,
            >,
        >,
    }

    impl InMemoryWorkerRegistry {
        pub fn new() -> Self {
            Self {
                workers: Arc::new(RwLock::new(HashMap::new())),
            }
        }
    }

    impl Default for InMemoryWorkerRegistry {
        fn default() -> Self {
            Self::new()
        }
    }
    #[async_trait]
    impl hodei_server_domain::workers::registry::WorkerRegistry for InMemoryWorkerRegistry {
        async fn register(
            &self,
            handle: hodei_server_domain::workers::WorkerHandle,
            spec: hodei_server_domain::workers::WorkerSpec,
            _job_id: hodei_server_domain::shared_kernel::JobId,
        ) -> Result<hodei_server_domain::workers::Worker> {
            let mut workers = self.workers.write().await;
            if workers.contains_key(&handle.worker_id) {
                return Err(DomainError::InfrastructureError {
                    message: format!("Worker already registered: {}", handle.worker_id),
                });
            }
            let worker = hodei_server_domain::workers::Worker::new(handle.clone(), spec);
            workers.insert(handle.worker_id.clone(), worker.clone());
            Ok(worker)
        }

        async fn unregister(
            &self,
            worker_id: &hodei_server_domain::shared_kernel::WorkerId,
        ) -> Result<()> {
            let mut workers = self.workers.write().await;
            workers.remove(worker_id);
            Ok(())
        }

        async fn get(
            &self,
            worker_id: &hodei_server_domain::shared_kernel::WorkerId,
        ) -> Result<Option<hodei_server_domain::workers::Worker>> {
            let workers = self.workers.read().await;
            Ok(workers.get(worker_id).cloned())
        }

        async fn find(
            &self,
            filter: &hodei_server_domain::workers::registry::WorkerFilter,
        ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            let workers = self.workers.read().await;
            Ok(workers
                .values()
                .filter(|w| {
                    if let Some(states) = &filter.states {
                        if !states.contains(w.state()) {
                            return false;
                        }
                    }
                    if let Some(pid) = &filter.provider_id {
                        if w.provider_id() != pid {
                            return false;
                        }
                    }
                    if let Some(pt) = &filter.provider_type {
                        if w.provider_type() != pt {
                            return false;
                        }
                    }
                    true
                })
                .cloned()
                .collect())
        }

        async fn find_available(&self) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            let workers = self.workers.read().await;
            Ok(workers
                .values()
                .filter(|w| *w.state() == hodei_server_domain::shared_kernel::WorkerState::Ready)
                .filter(|w| w.current_job_id().is_none())
                .cloned()
                .collect())
        }

        async fn find_by_provider(
            &self,
            provider_id: &hodei_server_domain::shared_kernel::ProviderId,
        ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            let workers = self.workers.read().await;
            Ok(workers
                .values()
                .filter(|w| w.provider_id() == provider_id)
                .cloned()
                .collect())
        }

        async fn get_by_job_id(
            &self,
            job_id: &hodei_server_domain::shared_kernel::JobId,
        ) -> Result<Option<hodei_server_domain::workers::Worker>> {
            let workers = self.workers.read().await;
            Ok(workers
                .values()
                .find(|w| w.current_job_id() == Some(job_id))
                .cloned())
        }

        async fn update_state(
            &self,
            worker_id: &hodei_server_domain::shared_kernel::WorkerId,
            state: hodei_server_domain::shared_kernel::WorkerState,
        ) -> Result<()> {
            let mut workers = self.workers.write().await;
            if let Some(worker) = workers.get_mut(worker_id) {
                // Simplified state update for tests (Crash-Only Design: 4 states)
                match state {
                    hodei_server_domain::shared_kernel::WorkerState::Creating => {
                        // Start in Creating, no transition needed
                    }
                    hodei_server_domain::shared_kernel::WorkerState::Ready => {
                        let _ = worker.mark_ready();
                    }
                    hodei_server_domain::shared_kernel::WorkerState::Busy => {
                        // Simulate job assignment to create Busy state
                        let _ = worker.assign_job(hodei_server_domain::shared_kernel::JobId::new());
                    }
                    hodei_server_domain::shared_kernel::WorkerState::Terminated => {
                        let _ = worker.mark_terminating();
                    }
                };
            }
            Ok(())
        }

        async fn heartbeat(
            &self,
            worker_id: &hodei_server_domain::shared_kernel::WorkerId,
        ) -> Result<()> {
            let mut workers = self.workers.write().await;
            if let Some(worker) = workers.get_mut(worker_id) {
                worker.update_heartbeat();
            }
            Ok(())
        }

        async fn assign_to_job(
            &self,
            worker_id: &hodei_server_domain::shared_kernel::WorkerId,
            job_id: hodei_server_domain::shared_kernel::JobId,
        ) -> Result<()> {
            let mut workers = self.workers.write().await;
            if let Some(worker) = workers.get_mut(worker_id) {
                let _ = worker.assign_job(job_id);
            }
            Ok(())
        }

        async fn release_from_job(
            &self,
            worker_id: &hodei_server_domain::shared_kernel::WorkerId,
        ) -> Result<()> {
            let mut workers = self.workers.write().await;
            if let Some(worker) = workers.get_mut(worker_id) {
                let _ = worker.complete_job(); // Back to ready and clear job_id
            }
            Ok(())
        }

        async fn find_unhealthy(
            &self,
            _timeout: std::time::Duration,
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

        async fn stats(
            &self,
        ) -> Result<hodei_server_domain::workers::registry::WorkerRegistryStats> {
            let workers = self.workers.read().await;
            Ok(
                hodei_server_domain::workers::registry::WorkerRegistryStats {
                    total_workers: workers.len(),
                    ..Default::default()
                },
            )
        }

        async fn count(&self) -> Result<usize> {
            let workers = self.workers.read().await;
            Ok(workers.len())
        }
    }

    /// In-memory Provider Config Repository
    #[derive(Clone)]
    pub struct InMemoryProviderConfigRepository {
        configs: Arc<
            RwLock<
                HashMap<
                    hodei_server_domain::shared_kernel::ProviderId,
                    hodei_server_domain::providers::ProviderConfig,
                >,
            >,
        >,
    }

    impl InMemoryProviderConfigRepository {
        pub fn new() -> Self {
            Self {
                configs: Arc::new(RwLock::new(HashMap::new())),
            }
        }
    }

    impl Default for InMemoryProviderConfigRepository {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl hodei_server_domain::providers::ProviderConfigRepository for InMemoryProviderConfigRepository {
        async fn save(
            &self,
            config: &hodei_server_domain::providers::ProviderConfig,
        ) -> Result<()> {
            let mut configs = self.configs.write().await;
            configs.insert(config.id.clone(), config.clone());
            Ok(())
        }

        async fn find_by_id(
            &self,
            id: &hodei_server_domain::shared_kernel::ProviderId,
        ) -> Result<Option<hodei_server_domain::providers::ProviderConfig>> {
            let configs = self.configs.read().await;
            Ok(configs.get(id).cloned())
        }

        async fn find_by_name(
            &self,
            name: &str,
        ) -> Result<Option<hodei_server_domain::providers::ProviderConfig>> {
            let configs = self.configs.read().await;
            Ok(configs.values().find(|c| c.name == name).cloned())
        }

        async fn find_by_type(
            &self,
            provider_type: &hodei_server_domain::workers::ProviderType,
        ) -> Result<Vec<hodei_server_domain::providers::ProviderConfig>> {
            let configs = self.configs.read().await;
            Ok(configs
                .values()
                .filter(|c| &c.provider_type == provider_type)
                .cloned()
                .collect())
        }

        async fn find_enabled(
            &self,
        ) -> Result<Vec<hodei_server_domain::providers::ProviderConfig>> {
            let configs = self.configs.read().await;
            Ok(configs
                .values()
                .filter(|c| c.is_enabled())
                .cloned()
                .collect())
        }

        async fn find_with_capacity(
            &self,
        ) -> Result<Vec<hodei_server_domain::providers::ProviderConfig>> {
            let configs = self.configs.read().await;
            Ok(configs
                .values()
                .filter(|c| c.has_capacity())
                .cloned()
                .collect())
        }

        async fn find_all(&self) -> Result<Vec<hodei_server_domain::providers::ProviderConfig>> {
            let configs = self.configs.read().await;
            Ok(configs.values().cloned().collect())
        }

        async fn update(
            &self,
            config: &hodei_server_domain::providers::ProviderConfig,
        ) -> Result<()> {
            let mut configs = self.configs.write().await;
            configs.insert(config.id.clone(), config.clone());
            Ok(())
        }

        async fn delete(&self, id: &hodei_server_domain::shared_kernel::ProviderId) -> Result<()> {
            let mut configs = self.configs.write().await;
            configs.remove(id);
            Ok(())
        }

        async fn exists_by_name(&self, name: &str) -> Result<bool> {
            let configs = self.configs.read().await;
            Ok(configs.values().any(|c| c.name == name))
        }
    }
}

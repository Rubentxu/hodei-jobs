// Hodei Job Platform - Infrastructure Layer
// Implementaciones concretas reorganizadas por tecnología y dominio

// Por tecnología
pub mod messaging;
pub mod persistence;

// Por dominio (implementaciones de providers)
pub mod providers;

// Security
pub mod credentials;

// Legacy
pub mod provisioning;

#[cfg(test)]
mod tests;

// Re-exports
// pub use messaging::*;
// pub use persistence::*;
// pub use providers::*;

pub mod repositories {
    pub use crate::persistence::postgres::in_memory::test_in_memory as in_memory;
    pub use crate::persistence::postgres::in_memory::test_in_memory::*;
}

#[cfg(test)]
pub mod test_infrastructure {
    use hodei_server_domain::jobs::{Job, JobQueue, JobRepository};
    use hodei_server_domain::providers::config::{ProviderConfig, ProviderConfigRepository};
    use hodei_server_domain::shared_kernel::{JobId, ProviderId, Result};
    use std::collections::HashMap;
    use std::sync::Mutex;

    pub struct InMemoryJobRepository {
        jobs: Mutex<HashMap<JobId, Job>>,
    }

    impl InMemoryJobRepository {
        pub fn new() -> Self {
            Self {
                jobs: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl JobRepository for InMemoryJobRepository {
        async fn save(&self, job: &Job) -> Result<()> {
            let mut jobs = self.jobs.lock().unwrap();
            jobs.insert(job.id.clone(), job.clone());
            Ok(())
        }

        async fn find_by_id(&self, job_id: &JobId) -> Result<Option<Job>> {
            let jobs = self.jobs.lock().unwrap();
            Ok(jobs.get(job_id).cloned())
        }

        async fn find_by_state(
            &self,
            _state: &hodei_server_domain::shared_kernel::JobState,
        ) -> Result<Vec<Job>> {
            let jobs = self.jobs.lock().unwrap();
            Ok(jobs.values().cloned().collect())
        }

        async fn find_pending(&self) -> Result<Vec<Job>> {
            let jobs = self.jobs.lock().unwrap();
            Ok(jobs.values().cloned().collect())
        }

        async fn find_all(&self, _limit: usize, _offset: usize) -> Result<(Vec<Job>, usize)> {
            let jobs = self.jobs.lock().unwrap();
            Ok((jobs.values().cloned().collect(), jobs.len()))
        }

        async fn find_by_execution_id(&self, _execution_id: &str) -> Result<Option<Job>> {
            Ok(None)
        }

        async fn delete(&self, job_id: &JobId) -> Result<()> {
            let mut jobs = self.jobs.lock().unwrap();
            jobs.remove(job_id);
            Ok(())
        }

        async fn update(&self, job: &Job) -> Result<()> {
            let mut jobs = self.jobs.lock().unwrap();
            jobs.insert(job.id.clone(), job.clone());
            Ok(())
        }
    }

    pub struct InMemoryJobQueue {
        queue: Mutex<Vec<Job>>,
    }

    impl InMemoryJobQueue {
        pub fn new() -> Self {
            Self {
                queue: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl JobQueue for InMemoryJobQueue {
        async fn enqueue(&self, job: Job) -> Result<()> {
            let mut queue = self.queue.lock().unwrap();
            if !queue.iter().any(|j| j.id == job.id) {
                queue.push(job);
            }
            Ok(())
        }

        async fn dequeue(&self) -> Result<Option<Job>> {
            let mut queue = self.queue.lock().unwrap();
            Ok(queue.pop())
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

    pub struct InMemoryProviderConfigRepository {
        configs: Mutex<HashMap<ProviderId, ProviderConfig>>,
    }

    impl InMemoryProviderConfigRepository {
        pub fn new() -> Self {
            Self {
                configs: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl ProviderConfigRepository for InMemoryProviderConfigRepository {
        async fn save(&self, config: &ProviderConfig) -> Result<()> {
            let mut configs = self.configs.lock().unwrap();
            configs.insert(config.id.clone(), config.clone());
            Ok(())
        }

        async fn find_by_id(&self, id: &ProviderId) -> Result<Option<ProviderConfig>> {
            let configs = self.configs.lock().unwrap();
            Ok(configs.get(id).cloned())
        }

        async fn find_by_name(&self, _name: &str) -> Result<Option<ProviderConfig>> {
            Ok(None)
        }

        async fn find_by_type(
            &self,
            _provider_type: &hodei_server_domain::workers::ProviderType,
        ) -> Result<Vec<ProviderConfig>> {
            let configs = self.configs.lock().unwrap();
            Ok(configs.values().cloned().collect())
        }

        async fn find_enabled(&self) -> Result<Vec<ProviderConfig>> {
            let configs = self.configs.lock().unwrap();
            Ok(configs.values().cloned().collect())
        }

        async fn find_with_capacity(&self) -> Result<Vec<ProviderConfig>> {
            let configs = self.configs.lock().unwrap();
            Ok(configs.values().cloned().collect())
        }

        async fn find_all(&self) -> Result<Vec<ProviderConfig>> {
            let configs = self.configs.lock().unwrap();
            Ok(configs.values().cloned().collect())
        }

        async fn update(&self, config: &ProviderConfig) -> Result<()> {
            let mut configs = self.configs.lock().unwrap();
            configs.insert(config.id.clone(), config.clone());
            Ok(())
        }

        async fn delete(&self, id: &ProviderId) -> Result<()> {
            let mut configs = self.configs.lock().unwrap();
            configs.remove(id);
            Ok(())
        }

        async fn exists_by_name(&self, _name: &str) -> Result<bool> {
            Ok(false)
        }
    }

    pub struct InMemoryWorkerRegistry {
        workers: Mutex<
            HashMap<
                hodei_server_domain::shared_kernel::WorkerId,
                hodei_server_domain::workers::Worker,
            >,
        >,
    }

    impl InMemoryWorkerRegistry {
        pub fn new() -> Self {
            Self {
                workers: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl hodei_server_domain::workers::registry::WorkerRegistry for InMemoryWorkerRegistry {
        async fn register(
            &self,
            handle: hodei_server_domain::workers::WorkerHandle,
            spec: hodei_server_domain::workers::WorkerSpec,
        ) -> Result<hodei_server_domain::workers::Worker> {
            let mut workers = self.workers.lock().unwrap();
            let worker = hodei_server_domain::workers::Worker::new(handle, spec);
            workers.insert(worker.handle().worker_id.clone(), worker.clone());
            Ok(worker)
        }

        async fn unregister(
            &self,
            worker_id: &hodei_server_domain::shared_kernel::WorkerId,
        ) -> Result<()> {
            let mut workers = self.workers.lock().unwrap();
            workers.remove(worker_id);
            Ok(())
        }

        async fn get(
            &self,
            worker_id: &hodei_server_domain::shared_kernel::WorkerId,
        ) -> Result<Option<hodei_server_domain::workers::Worker>> {
            let workers = self.workers.lock().unwrap();
            Ok(workers.get(worker_id).cloned())
        }

        async fn find(
            &self,
            _filter: &hodei_server_domain::workers::registry::WorkerFilter,
        ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            let workers = self.workers.lock().unwrap();
            Ok(workers.values().cloned().collect())
        }

        async fn find_available(&self) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            let workers = self.workers.lock().unwrap();
            Ok(workers.values().cloned().collect())
        }

        async fn find_by_provider(
            &self,
            _provider_id: &hodei_server_domain::shared_kernel::ProviderId,
        ) -> Result<Vec<hodei_server_domain::workers::Worker>> {
            let workers = self.workers.lock().unwrap();
            Ok(workers.values().cloned().collect())
        }

        async fn update_state(
            &self,
            _worker_id: &hodei_server_domain::shared_kernel::WorkerId,
            _state: hodei_server_domain::shared_kernel::WorkerState,
        ) -> Result<()> {
            Ok(())
        }

        async fn heartbeat(
            &self,
            _worker_id: &hodei_server_domain::shared_kernel::WorkerId,
        ) -> Result<()> {
            Ok(())
        }

        async fn assign_to_job(
            &self,
            _worker_id: &hodei_server_domain::shared_kernel::WorkerId,
            _job_id: hodei_server_domain::shared_kernel::JobId,
        ) -> Result<()> {
            Ok(())
        }

        async fn release_from_job(
            &self,
            _worker_id: &hodei_server_domain::shared_kernel::WorkerId,
        ) -> Result<()> {
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
            let workers = self.workers.lock().unwrap();
            Ok(
                hodei_server_domain::workers::registry::WorkerRegistryStats {
                    total_workers: workers.len(),
                    ready_workers: 0,
                    busy_workers: 0,
                    idle_workers: 0,
                    terminating_workers: 0,
                    workers_by_provider: std::collections::HashMap::new(),
                    workers_by_type: std::collections::HashMap::new(),
                },
            )
        }

        async fn count(&self) -> Result<usize> {
            let workers = self.workers.lock().unwrap();
            Ok(workers.len())
        }
    }
}

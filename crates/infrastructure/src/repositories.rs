// In-memory Repositories
// Implementaciones en memoria para MVP

use hodei_jobs_domain::job_execution::{Job, JobRepository, JobQueue};
use hodei_jobs_domain::shared_kernel::{JobId, Result};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

/// Repositorio en memoria para Jobs
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

    async fn find_by_state(&self, state: &hodei_jobs_domain::shared_kernel::JobState) -> Result<Vec<Job>> {
        let jobs = self.jobs.read().await;
        Ok(jobs.values()
            .filter(|job| &job.state == state)
            .cloned()
            .collect())
    }

    async fn find_pending(&self) -> Result<Vec<Job>> {
        let jobs = self.jobs.read().await;
        Ok(jobs.values()
            .filter(|job| matches!(job.state, hodei_jobs_domain::shared_kernel::JobState::Pending))
            .cloned()
            .collect())
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
}

/// Cola en memoria para Jobs (FIFO)
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

// ============================================================================
// Worker Registry - registro de workers efímeros
// ============================================================================

use chrono::Utc;
use hodei_jobs_domain::{
    shared_kernel::{DomainError, ProviderId, WorkerId, WorkerState},
    worker::{Worker, WorkerHandle, WorkerSpec},
    worker_registry::{WorkerFilter, WorkerRegistry, WorkerRegistryStats},
};
use std::time::Duration;
use tracing::{debug, info};

/// In-memory implementation of WorkerRegistry
#[derive(Debug, Clone)]
pub struct InMemoryWorkerRegistry {
    workers: Arc<RwLock<HashMap<WorkerId, Worker>>>,
    heartbeat_timeout: Duration,
}

impl InMemoryWorkerRegistry {
    pub fn new() -> Self {
        Self {
            workers: Arc::new(RwLock::new(HashMap::new())),
            heartbeat_timeout: Duration::from_secs(60),
        }
    }

    pub fn with_heartbeat_timeout(mut self, timeout: Duration) -> Self {
        self.heartbeat_timeout = timeout;
        self
    }

    fn matches_filter(worker: &Worker, filter: &WorkerFilter) -> bool {
        if let Some(ref states) = filter.states {
            if !states.contains(worker.state()) {
                return false;
            }
        }

        if let Some(ref provider_id) = filter.provider_id {
            if worker.provider_id() != provider_id {
                return false;
            }
        }

        if let Some(ref provider_type) = filter.provider_type {
            if worker.provider_type() != provider_type {
                return false;
            }
        }

        if let Some(can_accept) = filter.can_accept_jobs {
            if worker.state().can_accept_jobs() != can_accept {
                return false;
            }
        }

        if let Some(idle_duration) = filter.idle_for {
            if !matches!(worker.state(), WorkerState::Ready) {
                return false;
            }
            let since_heartbeat = Utc::now()
                .signed_duration_since(worker.last_heartbeat())
                .to_std()
                .unwrap_or(Duration::ZERO);
            if since_heartbeat < idle_duration {
                return false;
            }
        }

        if let Some(unhealthy_timeout) = filter.unhealthy_timeout {
            if worker.is_healthy(unhealthy_timeout) {
                return false;
            }
        }

        true
    }
}

impl Default for InMemoryWorkerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WorkerRegistry for InMemoryWorkerRegistry {
    async fn register(&self, handle: WorkerHandle, spec: WorkerSpec) -> Result<Worker> {
        let worker_id = handle.worker_id.clone();
        let worker = Worker::new(handle, spec);

        let mut workers = self.workers.write().await;
        if workers.contains_key(&worker_id) {
            return Err(DomainError::WorkerAlreadyExists { worker_id });
        }

        info!("Registering worker: {}", worker_id);
        workers.insert(worker_id, worker.clone());
        Ok(worker)
    }

    async fn unregister(&self, worker_id: &WorkerId) -> Result<()> {
        let mut workers = self.workers.write().await;
        if workers.remove(worker_id).is_none() {
            return Err(DomainError::WorkerNotFound {
                worker_id: worker_id.clone(),
            });
        }
        info!("Unregistered worker: {}", worker_id);
        Ok(())
    }

    async fn get(&self, worker_id: &WorkerId) -> Result<Option<Worker>> {
        let workers = self.workers.read().await;
        Ok(workers.get(worker_id).cloned())
    }

    async fn find(&self, filter: &WorkerFilter) -> Result<Vec<Worker>> {
        let workers = self.workers.read().await;
        Ok(workers
            .values()
            .filter(|w| Self::matches_filter(w, filter))
            .cloned()
            .collect())
    }

    async fn find_available(&self) -> Result<Vec<Worker>> {
        let filter = WorkerFilter::new().accepting_jobs();
        self.find(&filter).await
    }

    async fn find_by_provider(&self, provider_id: &ProviderId) -> Result<Vec<Worker>> {
        let filter = WorkerFilter::new().with_provider_id(provider_id.clone());
        self.find(&filter).await
    }

    async fn update_state(&self, worker_id: &WorkerId, state: WorkerState) -> Result<()> {
        let mut workers = self.workers.write().await;
        let worker = workers.get_mut(worker_id).ok_or_else(|| DomainError::WorkerNotFound {
            worker_id: worker_id.clone(),
        })?;

        debug!("Updating worker {} state to {:?}", worker_id, state);

        match state {
            WorkerState::Creating => {},  // Estado inicial
            WorkerState::Connecting => worker.mark_connecting()?,
            WorkerState::Ready => worker.mark_ready()?,
            WorkerState::Draining => worker.mark_draining()?,
            WorkerState::Terminating => worker.mark_terminating()?,
            WorkerState::Terminated => worker.mark_terminated()?,
            WorkerState::Busy => {
                return Err(DomainError::InvalidWorkerStateTransition {
                    current: format!("{:?}", worker.state()),
                    requested: format!("{:?}", state),
                });
            }
        }

        Ok(())
    }

    async fn heartbeat(&self, worker_id: &WorkerId) -> Result<()> {
        let mut workers = self.workers.write().await;
        let worker = workers.get_mut(worker_id).ok_or_else(|| DomainError::WorkerNotFound {
            worker_id: worker_id.clone(),
        })?;

        worker.update_heartbeat();
        debug!("Heartbeat received for worker: {}", worker_id);
        Ok(())
    }

    async fn assign_to_job(&self, worker_id: &WorkerId, job_id: JobId) -> Result<()> {
        let mut workers = self.workers.write().await;
        let worker = workers.get_mut(worker_id).ok_or_else(|| DomainError::WorkerNotFound {
            worker_id: worker_id.clone(),
        })?;

        worker.assign_job(job_id.clone())?;
        info!("Assigned worker {} to job {}", worker_id, job_id);
        Ok(())
    }

    async fn release_from_job(&self, worker_id: &WorkerId) -> Result<()> {
        let mut workers = self.workers.write().await;
        let worker = workers.get_mut(worker_id).ok_or_else(|| DomainError::WorkerNotFound {
            worker_id: worker_id.clone(),
        })?;

        worker.complete_job()?;
        info!("Released worker {} from job", worker_id);
        Ok(())
    }

    async fn find_unhealthy(&self, timeout: Duration) -> Result<Vec<Worker>> {
        let workers = self.workers.read().await;
        Ok(workers
            .values()
            .filter(|w| !w.is_healthy(timeout) && !w.state().is_terminated())
            .cloned()
            .collect())
    }

    async fn find_for_termination(&self) -> Result<Vec<Worker>> {
        let workers = self.workers.read().await;
        Ok(workers
            .values()
            .filter(|w| {
                !w.state().is_terminated() && (w.is_idle_timeout() || w.is_lifetime_exceeded())
            })
            .cloned()
            .collect())
    }

    async fn stats(&self) -> Result<WorkerRegistryStats> {
        let workers = self.workers.read().await;

        let mut stats = WorkerRegistryStats {
            total_workers: workers.len(),
            ..Default::default()
        };

        for worker in workers.values() {
            match worker.state() {
                WorkerState::Ready => stats.ready_workers += 1,
                WorkerState::Busy => stats.busy_workers += 1,
                WorkerState::Draining => stats.idle_workers += 1,  // Draining cuenta como idle
                WorkerState::Terminating => stats.terminating_workers += 1,
                _ => {}
            }

            *stats
                .workers_by_provider
                .entry(worker.provider_id().clone())
                .or_insert(0) += 1;

            *stats
                .workers_by_type
                .entry(worker.provider_type().clone())
                .or_insert(0) += 1;
        }

        Ok(stats)
    }

    async fn count(&self) -> Result<usize> {
        Ok(self.workers.read().await.len())
    }
}

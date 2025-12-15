use crate::smart_scheduler::{SchedulerConfig, SchedulingService};
use crate::worker_command_sender::WorkerCommandSender;
use hodei_jobs_domain::job_execution::{ExecutionContext, JobQueue, JobRepository};
use hodei_jobs_domain::job_scheduler::SchedulingContext;
use hodei_jobs_domain::shared_kernel::{DomainError, JobState, Result, WorkerId};
use hodei_jobs_domain::worker_registry::WorkerRegistry;
use std::sync::Arc;
use uuid::Uuid;

pub struct JobController {
    job_queue: Arc<dyn JobQueue>,
    job_repository: Arc<dyn JobRepository>,
    worker_registry: Arc<dyn WorkerRegistry>,
    scheduler: SchedulingService,
    worker_command_sender: Arc<dyn WorkerCommandSender>,
}

impl JobController {
    pub fn new(
        job_queue: Arc<dyn JobQueue>,
        job_repository: Arc<dyn JobRepository>,
        worker_registry: Arc<dyn WorkerRegistry>,
        scheduler_config: SchedulerConfig,
        worker_command_sender: Arc<dyn WorkerCommandSender>,
    ) -> Self {
        Self {
            job_queue,
            job_repository,
            worker_registry,
            scheduler: SchedulingService::new(scheduler_config),
            worker_command_sender,
        }
    }

    pub async fn run_once(&self) -> Result<usize> {
        let Some(mut job) = self.job_queue.dequeue().await? else {
            return Ok(0);
        };

        let available_workers = self.worker_registry.find_available().await?;
        let pending_jobs_count = self.job_queue.len().await?;
        let stats = self.worker_registry.stats().await?;

        let system_load = if stats.total_workers > 0 {
            stats.busy_workers as f64 / stats.total_workers as f64
        } else {
            0.0
        };

        let ctx = SchedulingContext {
            job: job.clone(),
            available_workers,
            available_providers: Vec::new(),
            pending_jobs_count,
            system_load,
        };

        let decision = self.scheduler.make_decision(ctx).await?;

        match decision {
            hodei_jobs_domain::job_scheduler::SchedulingDecision::AssignToWorker {
                worker_id,
                ..
            } => {
                self.assign_and_dispatch(&mut job, &worker_id).await?;
                self.job_repository.update(&job).await?;
                Ok(1)
            }
            hodei_jobs_domain::job_scheduler::SchedulingDecision::Enqueue { .. }
            | hodei_jobs_domain::job_scheduler::SchedulingDecision::ProvisionWorker { .. } => {
                self.job_queue.enqueue(job).await?;
                Ok(0)
            }
            hodei_jobs_domain::job_scheduler::SchedulingDecision::Reject { reason, .. } => {
                job.state = JobState::Failed;
                job.error_message = Some(reason);
                self.job_repository.update(&job).await?;
                Ok(0)
            }
        }
    }

    async fn assign_and_dispatch(
        &self,
        job: &mut hodei_jobs_domain::job_execution::Job,
        worker_id: &WorkerId,
    ) -> Result<()> {
        let worker = self.worker_registry.get(worker_id).await?.ok_or_else(|| {
            DomainError::WorkerNotFound {
                worker_id: worker_id.clone(),
            }
        })?;

        let provider_id = worker.handle().provider_id.clone();
        let provider_execution_id = Uuid::new_v4().to_string();

        let exec_ctx = ExecutionContext::new(job.id.clone(), provider_id, provider_execution_id);

        job.submit_to_provider(worker.handle().provider_id.clone(), exec_ctx)?;
        job.mark_running()?;

        self.worker_registry
            .assign_to_job(worker_id, job.id.clone())
            .await?;

        if let Err(e) = self
            .worker_command_sender
            .send_run_job(worker_id, job)
            .await
        {
            let _ = self.worker_registry.release_from_job(worker_id).await;
            job.state = JobState::Failed;
            job.error_message = Some(format!("dispatch_failed: {}", e));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use hodei_jobs_domain::job_execution::{Job, JobSpec};
    use hodei_jobs_domain::shared_kernel::{JobId, ProviderId, WorkerState};
    use hodei_jobs_domain::worker::{ProviderType, WorkerHandle, WorkerSpec as DomainWorkerSpec};
    use hodei_jobs_domain::worker_registry::{WorkerFilter, WorkerRegistryStats};
    use std::collections::{HashMap, VecDeque};
    use std::sync::Mutex;
    use tokio::sync::RwLock;

    #[derive(Default)]
    struct MockJobRepository {
        jobs: Arc<RwLock<HashMap<JobId, Job>>>,
    }

    #[async_trait]
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

    #[derive(Default)]
    struct MockWorkerRegistry {
        workers: Arc<RwLock<HashMap<WorkerId, hodei_jobs_domain::worker::Worker>>>,
    }

    #[async_trait]
    impl WorkerRegistry for MockWorkerRegistry {
        async fn register(
            &self,
            handle: WorkerHandle,
            spec: DomainWorkerSpec,
        ) -> Result<hodei_jobs_domain::worker::Worker> {
            let worker = hodei_jobs_domain::worker::Worker::new(handle.clone(), spec);
            self.workers
                .write()
                .await
                .insert(handle.worker_id.clone(), worker.clone());
            Ok(worker)
        }

        async fn unregister(&self, _worker_id: &WorkerId) -> Result<()> {
            Ok(())
        }

        async fn get(
            &self,
            worker_id: &WorkerId,
        ) -> Result<Option<hodei_jobs_domain::worker::Worker>> {
            Ok(self.workers.read().await.get(worker_id).cloned())
        }

        async fn find(
            &self,
            _filter: &WorkerFilter,
        ) -> Result<Vec<hodei_jobs_domain::worker::Worker>> {
            Ok(self.workers.read().await.values().cloned().collect())
        }

        async fn find_available(&self) -> Result<Vec<hodei_jobs_domain::worker::Worker>> {
            Ok(self
                .workers
                .read()
                .await
                .values()
                .filter(|w| w.state().can_accept_jobs())
                .cloned()
                .collect())
        }

        async fn find_by_provider(
            &self,
            _provider_id: &ProviderId,
        ) -> Result<Vec<hodei_jobs_domain::worker::Worker>> {
            Ok(vec![])
        }

        async fn update_state(&self, worker_id: &WorkerId, state: WorkerState) -> Result<()> {
            let mut workers = self.workers.write().await;
            let worker = workers
                .get_mut(worker_id)
                .ok_or_else(|| DomainError::WorkerNotFound {
                    worker_id: worker_id.clone(),
                })?;

            match state {
                WorkerState::Creating => {}
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

        async fn heartbeat(&self, _worker_id: &WorkerId) -> Result<()> {
            Ok(())
        }

        async fn assign_to_job(&self, worker_id: &WorkerId, job_id: JobId) -> Result<()> {
            let mut workers = self.workers.write().await;
            let worker = workers
                .get_mut(worker_id)
                .ok_or_else(|| DomainError::WorkerNotFound {
                    worker_id: worker_id.clone(),
                })?;
            worker.assign_job(job_id)?;
            Ok(())
        }

        async fn release_from_job(&self, worker_id: &WorkerId) -> Result<()> {
            let mut workers = self.workers.write().await;
            let worker = workers
                .get_mut(worker_id)
                .ok_or_else(|| DomainError::WorkerNotFound {
                    worker_id: worker_id.clone(),
                })?;
            worker.complete_job()?;
            Ok(())
        }

        async fn find_unhealthy(
            &self,
            _timeout: std::time::Duration,
        ) -> Result<Vec<hodei_jobs_domain::worker::Worker>> {
            Ok(vec![])
        }

        async fn find_for_termination(&self) -> Result<Vec<hodei_jobs_domain::worker::Worker>> {
            Ok(vec![])
        }

        async fn stats(&self) -> Result<WorkerRegistryStats> {
            let workers = self.workers.read().await;
            let mut stats = WorkerRegistryStats {
                total_workers: workers.len(),
                ..Default::default()
            };

            for w in workers.values() {
                match w.state() {
                    WorkerState::Ready => stats.ready_workers += 1,
                    WorkerState::Busy => stats.busy_workers += 1,
                    _ => {}
                }
            }

            Ok(stats)
        }

        async fn count(&self) -> Result<usize> {
            Ok(self.workers.read().await.len())
        }
    }

    #[derive(Default)]
    struct MockWorkerCommandSender {
        sent: Arc<RwLock<Vec<(WorkerId, JobId)>>>,
    }

    #[async_trait]
    impl WorkerCommandSender for MockWorkerCommandSender {
        async fn send_run_job(&self, worker_id: &WorkerId, job: &Job) -> Result<()> {
            self.sent
                .write()
                .await
                .push((worker_id.clone(), job.id.clone()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn run_once_assigns_and_dispatches() {
        let job_repo = Arc::new(MockJobRepository::default());
        let queue = Arc::new(MockJobQueue::new());
        let registry = Arc::new(MockWorkerRegistry::default());
        let sender = Arc::new(MockWorkerCommandSender::default());

        let worker_id = WorkerId::new();
        let handle = WorkerHandle::new(
            worker_id.clone(),
            "resource".to_string(),
            ProviderType::Docker,
            ProviderId::new(),
        );
        let mut spec = DomainWorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        spec.worker_id = worker_id.clone();

        registry.register(handle, spec).await.unwrap();
        registry
            .update_state(&worker_id, WorkerState::Connecting)
            .await
            .unwrap();
        registry
            .update_state(&worker_id, WorkerState::Ready)
            .await
            .unwrap();

        let job_id = JobId::new();
        let job = Job::new(
            job_id.clone(),
            JobSpec::new(vec!["echo".to_string(), "hi".to_string()]),
        );
        job_repo.save(&job).await.unwrap();
        queue.enqueue(job).await.unwrap();

        let controller = JobController::new(
            queue,
            job_repo.clone(),
            registry.clone(),
            SchedulerConfig::default(),
            sender.clone(),
        );

        let processed = controller.run_once().await.unwrap();
        assert_eq!(processed, 1);

        let updated = job_repo.find_by_id(&job_id).await.unwrap().unwrap();
        assert!(matches!(
            updated.state,
            JobState::Running | JobState::Failed
        ));

        let sent = sender.sent.read().await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, worker_id);
        assert_eq!(sent[0].1, job_id);

        let worker = registry.get(&worker_id).await.unwrap().unwrap();
        assert!(matches!(
            worker.state(),
            WorkerState::Busy | WorkerState::Ready
        ));
    }
}

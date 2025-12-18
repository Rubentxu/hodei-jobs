use crate::scheduling::smart_scheduler::{SchedulerConfig, SchedulingService};
use crate::workers::commands::WorkerCommandSender;
use crate::workers::provisioning::WorkerProvisioningService;
use chrono::Utc;
use hodei_jobs_domain::event_bus::EventBus;
use hodei_jobs_domain::events::DomainEvent;
use hodei_jobs_domain::jobs::{ExecutionContext, JobQueue, JobRepository};

use hodei_jobs_domain::scheduling::{ProviderInfo, SchedulingContext};
use hodei_jobs_domain::shared_kernel::{DomainError, JobState, Result, WorkerId};
use hodei_jobs_domain::workers::WorkerRegistry;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

pub struct JobController {
    job_queue: Arc<dyn JobQueue>,
    job_repository: Arc<dyn JobRepository>,
    worker_registry: Arc<dyn WorkerRegistry>,
    scheduler: SchedulingService,
    worker_command_sender: Arc<dyn WorkerCommandSender>,
    event_bus: Arc<dyn EventBus>,
    provisioning_service: Option<Arc<dyn WorkerProvisioningService>>,
}

impl JobController {
    pub fn new(
        job_queue: Arc<dyn JobQueue>,
        job_repository: Arc<dyn JobRepository>,
        worker_registry: Arc<dyn WorkerRegistry>,
        scheduler_config: SchedulerConfig,
        worker_command_sender: Arc<dyn WorkerCommandSender>,
        event_bus: Arc<dyn EventBus>,
        provisioning_service: Option<Arc<dyn WorkerProvisioningService>>,
    ) -> Self {
        Self {
            job_queue,
            job_repository,
            worker_registry,
            scheduler: SchedulingService::new(scheduler_config),
            worker_command_sender,
            event_bus,
            provisioning_service,
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

        let mut available_providers = Vec::new();
        if let Some(provisioning) = &self.provisioning_service {
            if let Ok(providers) = provisioning.list_providers().await {
                for provider_id in providers {
                    // Check if provider is available
                    if provisioning
                        .is_provider_available(&provider_id)
                        .await
                        .unwrap_or(false)
                    {
                        // Get stats if possible, otherwise defaults
                        let workers_count = self
                            .worker_registry
                            .find_by_provider(&provider_id)
                            .await
                            .unwrap_or_default()
                            .len();

                        available_providers.push(ProviderInfo {
                            provider_id,
                            provider_type: hodei_jobs_domain::workers::ProviderType::Docker, // Hardcoded for simplified verification, ideally get from provider
                            active_workers: workers_count,
                            max_workers: 10, // Default limit
                            estimated_startup_time: std::time::Duration::from_secs(5),
                            health_score: 1.0,
                            cost_per_hour: 0.0,
                        });
                    }
                }
            }
        }

        let ctx = SchedulingContext {
            job: job.clone(),
            available_workers,
            available_providers,
            pending_jobs_count,
            system_load,
        };

        let decision = self.scheduler.make_decision(ctx).await?;

        match decision {
            hodei_jobs_domain::scheduling::SchedulingDecision::AssignToWorker {
                worker_id, ..
            } => {
                debug!("Assigning job {} to worker {}", job.id, worker_id);
                self.assign_and_dispatch(&mut job, &worker_id).await?;
                self.job_repository.update(&job).await?;
                Ok(1)
            }
            hodei_jobs_domain::scheduling::SchedulingDecision::ProvisionWorker {
                provider_id,
                ..
            } => {
                debug!("Provisioning new worker from provider {}", provider_id);
                if let Some(provisioning) = &self.provisioning_service {
                    if let Some(spec) = provisioning.default_worker_spec(&provider_id) {
                        match provisioning.provision_worker(&provider_id, spec).await {
                            Ok(result) => {
                                info!("Provisioned worker {} for job {}", result.worker_id, job.id);
                                self.job_queue.enqueue(job).await?;
                                Ok(0)
                            }
                            Err(e) => {
                                error!("Failed to provision worker: {}", e);
                                self.job_queue.enqueue(job).await?;
                                Ok(0)
                            }
                        }
                    } else {
                        warn!("No default spec for provider {}", provider_id);
                        self.job_queue.enqueue(job).await?;
                        Ok(0)
                    }
                } else {
                    warn!("Provisioning requested but service not available");
                    self.job_queue.enqueue(job).await?;
                    Ok(0)
                }
            }
            hodei_jobs_domain::scheduling::SchedulingDecision::Enqueue { .. } => {
                self.job_queue.enqueue(job).await?;
                Ok(0)
            }
            hodei_jobs_domain::scheduling::SchedulingDecision::Reject { reason, .. } => {
                job.fail(reason)?;
                self.job_repository.update(&job).await?;
                Ok(0)
            }
        }
    }

    async fn assign_and_dispatch(
        &self,
        job: &mut hodei_jobs_domain::jobs::Job,
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
        let old_state = job.state().clone();
        job.mark_running()?;

        // Use job_id as correlation_id for traceability
        let correlation_id = Some(job.id.to_string());
        let actor = Some("job-controller".to_string());

        // Publish JobAssigned event
        let assigned_event = DomainEvent::JobAssigned {
            job_id: job.id.clone(),
            worker_id: worker_id.clone(),
            occurred_at: Utc::now(),
            correlation_id: correlation_id.clone(),
            actor: actor.clone(),
        };
        if let Err(e) = self.event_bus.publish(&assigned_event).await {
            tracing::error!("Failed to publish JobAssigned event: {}", e);
        }

        // Publish JobStatusChanged event
        let event = DomainEvent::JobStatusChanged {
            job_id: job.id.clone(),
            old_state,
            new_state: JobState::Running,
            occurred_at: Utc::now(),
            correlation_id,
            actor,
        };

        if let Err(e) = self.event_bus.publish(&event).await {
            tracing::error!("Failed to publish JobStatusChanged (Running) event: {}", e);
        }

        self.worker_registry
            .assign_to_job(worker_id, job.id.clone())
            .await?;

        // Persist RUNNING state before dispatching to avoid race condition
        // where worker completes before DB is updated
        self.job_repository.update(job).await?;

        if let Err(e) = self
            .worker_command_sender
            .send_run_job(worker_id, job)
            .await
        {
            let _ = self.worker_registry.release_from_job(worker_id).await;
            job.fail(format!("dispatch_failed: {}", e))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use hodei_jobs_domain::jobs::{Job, JobSpec};
    use hodei_jobs_domain::shared_kernel::{JobId, ProviderId, WorkerState};
    use hodei_jobs_domain::workers::{ProviderType, WorkerHandle, WorkerSpec as DomainWorkerSpec};
    use hodei_jobs_domain::workers::{WorkerFilter, WorkerRegistryStats};
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
        workers: Arc<RwLock<HashMap<WorkerId, hodei_jobs_domain::workers::Worker>>>,
    }

    #[async_trait]
    impl WorkerRegistry for MockWorkerRegistry {
        async fn register(
            &self,
            handle: WorkerHandle,
            spec: DomainWorkerSpec,
        ) -> Result<hodei_jobs_domain::workers::Worker> {
            let worker = hodei_jobs_domain::workers::Worker::new(handle.clone(), spec);
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
        ) -> Result<Option<hodei_jobs_domain::workers::Worker>> {
            Ok(self.workers.read().await.get(worker_id).cloned())
        }

        async fn find(
            &self,
            _filter: &WorkerFilter,
        ) -> Result<Vec<hodei_jobs_domain::workers::Worker>> {
            Ok(self.workers.read().await.values().cloned().collect())
        }

        async fn find_available(&self) -> Result<Vec<hodei_jobs_domain::workers::Worker>> {
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
        ) -> Result<Vec<hodei_jobs_domain::workers::Worker>> {
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
        ) -> Result<Vec<hodei_jobs_domain::workers::Worker>> {
            Ok(vec![])
        }

        async fn find_for_termination(&self) -> Result<Vec<hodei_jobs_domain::workers::Worker>> {
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

    use futures::stream::BoxStream;
    use hodei_jobs_domain::DomainEvent;
    use hodei_jobs_domain::event_bus::{EventBus, EventBusError};
    struct MockEventBus {
        published: Arc<std::sync::Mutex<Vec<DomainEvent>>>,
    }
    impl MockEventBus {
        fn new() -> Self {
            Self {
                published: Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }
    }
    #[async_trait]
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
            "hodei-jobs-worker:latest".to_string(),
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

        let bus = Arc::new(MockEventBus::new());
        let controller = JobController::new(
            queue,
            job_repo.clone(),
            registry.clone(),
            SchedulerConfig::default(),
            sender.clone(),
            bus,
            None,
        );

        let processed = controller.run_once().await.unwrap();
        assert_eq!(processed, 1);

        let updated = job_repo.find_by_id(&job_id).await.unwrap().unwrap();
        assert!(matches!(
            updated.state(),
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

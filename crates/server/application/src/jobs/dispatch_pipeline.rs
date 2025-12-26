//! Dispatch Pipeline
//!
//! Provides explicit step naming to transform Connascence of Position into Connascence of Name.

use crate::scheduling::{SchedulerConfig, SchedulingService};
use hodei_server_domain::jobs::{Job, JobQueue, JobRepository};
use hodei_server_domain::scheduling::{SchedulingContext, SchedulingDecision};
use hodei_server_domain::shared_kernel::{JobId, WorkerId};
use hodei_server_domain::workers::health::WorkerHealthService;
use hodei_server_domain::workers::{Worker, WorkerRegistry};
use std::sync::Arc;

/// Pipeline step enumeration for explicit naming
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchStep {
    /// Query healthy workers from registry
    QueryWorkers,
    /// Dequeue next job from queue
    DequeueJob,
    /// Make scheduling decision
    MakeDecision,
    /// Dispatch job to worker
    DispatchToWorker,
    /// Re-enqueue job for later processing
    ReEnqueue,
}

impl std::fmt::Display for DispatchStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DispatchStep::QueryWorkers => write!(f, "QueryWorkers"),
            DispatchStep::DequeueJob => write!(f, "DequeueJob"),
            DispatchStep::MakeDecision => write!(f, "MakeDecision"),
            DispatchStep::DispatchToWorker => write!(f, "DispatchToWorker"),
            DispatchStep::ReEnqueue => write!(f, "ReEnqueue"),
        }
    }
}

/// Pipeline result type
#[derive(Debug)]
pub enum DispatchResult {
    /// Job was dispatched to a worker
    Dispatched { job_id: JobId, worker_id: WorkerId },
    /// No jobs available
    NoJobs,
    /// No workers available
    NoWorkers,
    /// Job was re-enqueued
    ReEnqueued { job_id: JobId },
    /// An error occurred
    Error(String),
}

/// Components required for dispatch pipeline
#[async_trait::async_trait]
pub trait DispatchPipelineComponents: Send + Sync {
    async fn query_healthy_workers(&self) -> Result<Vec<Worker>, String>;
    async fn dequeue_job(&self) -> Result<Option<Job>, String>;
    async fn make_decision(
        &self,
        job: &Job,
        workers: &[Worker],
    ) -> Result<SchedulingDecision, String>;
    async fn dispatch_to_worker(&self, job: &mut Job, worker_id: &WorkerId) -> Result<(), String>;
    async fn re_enqueue(&self, job: Job) -> Result<(), String>;
}

/// Dispatch Pipeline with explicit steps
pub struct DispatchPipeline<C: DispatchPipelineComponents> {
    components: C,
    steps: Vec<DispatchStep>,
}

impl<C: DispatchPipelineComponents> DispatchPipeline<C> {
    pub fn new(components: C) -> Self {
        Self {
            components,
            steps: vec![
                DispatchStep::QueryWorkers,
                DispatchStep::DequeueJob,
                DispatchStep::MakeDecision,
                DispatchStep::DispatchToWorker,
                DispatchStep::ReEnqueue,
            ],
        }
    }

    pub fn with_steps(components: C, steps: Vec<DispatchStep>) -> Self {
        Self { components, steps }
    }

    pub async fn execute(&self) -> DispatchResult {
        // Step 1: Query Workers (named explicitly)
        let workers = match self.components.query_healthy_workers().await {
            Ok(w) => w,
            Err(e) => return DispatchResult::Error(format!("QueryWorkers failed: {}", e)),
        };

        if workers.is_empty() {
            return DispatchResult::NoWorkers;
        }

        // Step 2: Dequeue Job (named explicitly)
        let job = match self.components.dequeue_job().await {
            Ok(Some(j)) => j,
            Ok(None) => return DispatchResult::NoJobs,
            Err(e) => return DispatchResult::Error(format!("DequeueJob failed: {}", e)),
        };

        // Step 3: Make Decision (named explicitly)
        let decision = match self.components.make_decision(&job, &workers).await {
            Ok(d) => d,
            Err(e) => return DispatchResult::Error(format!("MakeDecision failed: {}", e)),
        };

        // Step 4: Dispatch or ReEnqueue (named explicitly)
        let job_id = job.id.clone();
        match decision {
            SchedulingDecision::AssignToWorker { worker_id, .. } => {
                // Need to clone job for dispatch since we need to pass as mutable
                let mut job_for_dispatch = job;
                match self
                    .components
                    .dispatch_to_worker(&mut job_for_dispatch, &worker_id)
                    .await
                {
                    Ok(()) => DispatchResult::Dispatched { job_id, worker_id },
                    Err(e) => DispatchResult::Error(format!("DispatchToWorker failed: {}", e)),
                }
            }
            _ => match self.components.re_enqueue(job).await {
                Ok(()) => DispatchResult::ReEnqueued { job_id },
                Err(e) => DispatchResult::Error(format!("ReEnqueue failed: {}", e)),
            },
        }
    }

    pub fn steps(&self) -> &[DispatchStep] {
        &self.steps
    }
}

/// Implementation of DispatchPipelineComponents
pub struct DispatchPipelineComponentsImpl {
    job_queue: Arc<dyn JobQueue>,
    job_repository: Arc<dyn JobRepository>,
    worker_registry: Arc<dyn WorkerRegistry>,
    scheduler: SchedulingService,
    worker_health_service: Arc<WorkerHealthService>,
}

impl DispatchPipelineComponentsImpl {
    pub fn new(
        job_queue: Arc<dyn JobQueue>,
        job_repository: Arc<dyn JobRepository>,
        worker_registry: Arc<dyn WorkerRegistry>,
        scheduler_config: SchedulerConfig,
    ) -> Self {
        Self {
            job_queue,
            job_repository,
            worker_registry,
            scheduler: SchedulingService::new(scheduler_config),
            worker_health_service: Arc::new(WorkerHealthService::builder().build()),
        }
    }
}

#[async_trait::async_trait]
impl DispatchPipelineComponents for DispatchPipelineComponentsImpl {
    async fn query_healthy_workers(&self) -> Result<Vec<Worker>, String> {
        let all_workers = self
            .worker_registry
            .find_available()
            .await
            .map_err(|e| e.to_string())?;

        let healthy_workers: Vec<_> = all_workers
            .into_iter()
            .filter(|worker| self.worker_health_service.is_healthy(worker))
            .collect();

        Ok(healthy_workers)
    }

    async fn dequeue_job(&self) -> Result<Option<Job>, String> {
        self.job_queue.dequeue().await.map_err(|e| e.to_string())
    }

    async fn make_decision(
        &self,
        job: &Job,
        workers: &[Worker],
    ) -> Result<SchedulingDecision, String> {
        let ctx = SchedulingContext {
            job: job.clone(),
            job_preferences: job.spec.preferences.clone(),
            available_workers: workers.to_vec(),
            available_providers: vec![],
            pending_jobs_count: 0,
            system_load: 0.0,
        };

        self.scheduler
            .make_decision(ctx)
            .await
            .map_err(|e| e.to_string())
    }

    async fn dispatch_to_worker(
        &self,
        _job: &mut Job,
        _worker_id: &WorkerId,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn re_enqueue(&self, job: Job) -> Result<(), String> {
        self.job_queue.enqueue(job).await.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::jobs::JobSpec;
    use hodei_server_domain::workers::{ProviderType, WorkerHandle, WorkerSpec};

    struct MockComponents {
        workers: Vec<Worker>,
        jobs: std::sync::Mutex<Vec<Job>>,
    }

    impl MockComponents {
        fn new() -> Self {
            Self {
                workers: vec![],
                jobs: std::sync::Mutex::new(vec![]),
            }
        }

        fn with_worker(worker: Worker) -> Self {
            Self {
                workers: vec![worker],
                jobs: std::sync::Mutex::new(vec![]),
            }
        }

        fn with_job_and_worker(job: Job, worker: Worker) -> Self {
            Self {
                workers: vec![worker],
                jobs: std::sync::Mutex::new(vec![job]),
            }
        }
    }

    #[async_trait::async_trait]
    impl DispatchPipelineComponents for MockComponents {
        async fn query_healthy_workers(&self) -> Result<Vec<Worker>, String> {
            Ok(self.workers.clone())
        }

        async fn dequeue_job(&self) -> Result<Option<Job>, String> {
            let mut jobs = self.jobs.lock().unwrap();
            Ok(jobs.pop())
        }

        async fn make_decision(
            &self,
            _job: &Job,
            workers: &[Worker],
        ) -> Result<SchedulingDecision, String> {
            if workers.is_empty() {
                Ok(SchedulingDecision::Enqueue {
                    job_id: JobId::new(),
                    reason: "No workers".to_string(),
                })
            } else {
                let worker_id = workers[0].id().clone();
                Ok(SchedulingDecision::AssignToWorker {
                    job_id: JobId::new(),
                    worker_id,
                })
            }
        }

        async fn dispatch_to_worker(
            &self,
            _job: &mut Job,
            _worker_id: &WorkerId,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn re_enqueue(&self, job: Job) -> Result<(), String> {
            let mut jobs = self.jobs.lock().unwrap();
            jobs.push(job);
            Ok(())
        }
    }

    fn create_test_worker() -> Worker {
        let spec = WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "http://localhost:50051".to_string(),
        );
        let handle = WorkerHandle::new(
            spec.worker_id.clone(),
            "container-1".to_string(),
            ProviderType::Docker,
            hodei_server_domain::shared_kernel::ProviderId::new(),
        );
        Worker::new(handle, spec)
    }

    fn create_test_job() -> Job {
        let spec = JobSpec::new(vec!["echo".to_string()]);
        Job::new(JobId::new(), spec)
    }

    #[tokio::test]
    async fn test_pipeline_no_workers_returns_no_workers() {
        let components = MockComponents::new();
        let pipeline = DispatchPipeline::new(components);
        let result = pipeline.execute().await;
        assert!(matches!(result, DispatchResult::NoWorkers));
    }

    #[tokio::test]
    async fn test_pipeline_no_jobs_returns_no_jobs() {
        let worker = create_test_worker();
        let components = MockComponents::with_worker(worker);
        let pipeline = DispatchPipeline::new(components);
        let result = pipeline.execute().await;
        assert!(matches!(result, DispatchResult::NoJobs));
    }

    #[tokio::test]
    async fn test_pipeline_dispatches_when_both_available() {
        let worker = create_test_worker();
        let job = create_test_job();
        let components = MockComponents::with_job_and_worker(job, worker);
        let pipeline = DispatchPipeline::new(components);
        let result = pipeline.execute().await;
        assert!(matches!(result, DispatchResult::Dispatched { .. }));
    }

    #[test]
    fn test_dispatch_step_display() {
        assert_eq!(format!("{}", DispatchStep::QueryWorkers), "QueryWorkers");
        assert_eq!(format!("{}", DispatchStep::DequeueJob), "DequeueJob");
        assert_eq!(format!("{}", DispatchStep::MakeDecision), "MakeDecision");
        assert_eq!(
            format!("{}", DispatchStep::DispatchToWorker),
            "DispatchToWorker"
        );
        assert_eq!(format!("{}", DispatchStep::ReEnqueue), "ReEnqueue");
    }
}

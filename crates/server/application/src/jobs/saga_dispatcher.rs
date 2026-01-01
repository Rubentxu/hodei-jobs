//! Job Dispatcher - Saga-Only Implementation
use crate::providers::ProviderRegistry;
use crate::saga::dispatcher_saga::DynExecutionSagaDispatcher;
use crate::saga::provisioning_saga::DynProvisioningSagaCoordinator;
use crate::scheduling::smart_scheduler::SchedulingService;
use crate::workers::commands::WorkerCommandSender;
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::DomainEvent;
use hodei_server_domain::jobs::{Job, JobQueue, JobRepository};
use hodei_server_domain::outbox::{OutboxError, OutboxRepository};
use hodei_server_domain::scheduling::SchedulerConfig;
use hodei_server_domain::shared_kernel::{DomainError, JobId, WorkerId};
use hodei_server_domain::workers::WorkerRegistry;
use hodei_server_domain::workers::health::WorkerHealthService;
use std::sync::Arc;

pub struct SagaOnlyJobDispatcher {
    job_queue: Arc<dyn JobQueue>,
    job_repository: Arc<dyn JobRepository>,
    worker_registry: Arc<dyn WorkerRegistry>,
    provider_registry: Arc<ProviderRegistry>,
    scheduler: SchedulingService,
    worker_command_sender: Arc<dyn WorkerCommandSender>,
    event_bus: Arc<dyn EventBus>,
    outbox_repository: Option<Arc<dyn OutboxRepository<Error = OutboxError> + Send + Sync>>,
    execution_saga_dispatcher: Arc<DynExecutionSagaDispatcher>,
    provisioning_saga_coordinator: Option<Arc<DynProvisioningSagaCoordinator>>,
}

impl SagaOnlyJobDispatcher {
    pub fn new(
        job_queue: Arc<dyn JobQueue>,
        job_repository: Arc<dyn JobRepository>,
        worker_registry: Arc<dyn WorkerRegistry>,
        provider_registry: Arc<ProviderRegistry>,
        scheduler_config: SchedulerConfig,
        worker_command_sender: Arc<dyn WorkerCommandSender>,
        event_bus: Arc<dyn EventBus>,
        outbox_repository: Option<Arc<dyn OutboxRepository<Error = OutboxError> + Send + Sync>>,
        execution_saga_dispatcher: Arc<DynExecutionSagaDispatcher>,
        provisioning_saga_coordinator: Option<Arc<DynProvisioningSagaCoordinator>>,
    ) -> Self {
        Self {
            job_queue,
            job_repository,
            worker_registry,
            provider_registry,
            scheduler: SchedulingService::new(scheduler_config),
            worker_command_sender,
            event_bus,
            outbox_repository,
            execution_saga_dispatcher,
            provisioning_saga_coordinator,
        }
    }

    pub async fn dispatch_job(
        &self,
        job_id: &JobId,
        worker_id: &WorkerId,
    ) -> Result<(), DispatchError> {
        let job = self
            .job_queue
            .dequeue()
            .await
            .map_err(|e| DispatchError::from(e))?
            .ok_or_else(|| DispatchError::NoJobsAvailable)?;

        if &job.id != job_id {
            return Err(DispatchError::JobMismatch);
        }

        self.execution_saga_dispatcher
            .execute_execution_saga(job_id, worker_id)
            .await
            .map_err(|e| DispatchError::SagaFailed(e.to_string()))?;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("No jobs available in queue")]
    NoJobsAvailable,
    #[error("Job mismatch")]
    JobMismatch,
    #[error("Saga execution failed: {0}")]
    SagaFailed(String),
}

impl From<DomainError> for DispatchError {
    fn from(e: DomainError) -> Self {
        DispatchError::SagaFailed(e.to_string())
    }
}

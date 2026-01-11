//! Execution Saga Command Handlers
//!
//! Handlers for execution-related commands: ValidateJobCommand, AssignWorkerCommand,
//! ExecuteJobCommand, and CompleteJobCommand.
//! These handlers implement the business logic for job execution operations.

use async_trait::async_trait;
use std::fmt::Debug;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use hodei_server_domain::command::{Command, CommandHandler};
use hodei_server_domain::jobs::JobRepository;
use hodei_server_domain::saga::commands::execution::{
    AssignWorkerCommand, AssignWorkerError, CompleteJobCommand, CompleteJobError,
    ExecuteJobCommand, ExecuteJobError, JobCompletionResult,
    JobExecutionResult, JobValidationResult, ValidateJobCommand, ValidateJobError,
    WorkerAssignmentResult,
};
use hodei_server_domain::shared_kernel::{JobId, JobState, WorkerId, WorkerState};
use hodei_server_domain::workers::{WorkerFilter, WorkerRegistry};
use crate::workers::WorkerCommandSender;

// =============================================================================
// ValidateJobHandler
// =============================================================================

/// Handler for ValidateJobCommand.
///
/// This handler validates that a job exists and is in a valid state
/// to proceed with execution.
///
/// # Architecture
/// - Domain layer: Defines `ValidateJobCommand` and `ValidateJobError`
/// - Application layer: Provides this handler implementation
#[derive(Debug)]
pub struct ValidateJobHandler<J>
where
    J: JobRepository + Debug,
{
    job_repository: Arc<J>,
}

impl<J> ValidateJobHandler<J>
where
    J: JobRepository + Debug,
{
    /// Creates a new handler with the given job repository.
    #[inline]
    pub fn new(job_repository: Arc<J>) -> Self {
        Self { job_repository }
    }
}

#[async_trait]
impl<J> CommandHandler<ValidateJobCommand> for ValidateJobHandler<J>
where
    J: JobRepository + Debug + Send + Sync + 'static,
{
    type Error = ValidateJobError;

    async fn handle(&self, command: ValidateJobCommand) -> Result<JobValidationResult, Self::Error> {
        debug!(
            job_id = %command.job_id,
            saga_id = %command.saga_id,
            "Validating job for execution"
        );

        // Find the job in repository
        let job_opt = self
            .job_repository
            .find_by_id(&command.job_id)
            .await
            .ok()  // Convert DomainError to None
            .flatten();  // Flatten Option<Option<T>> to Option<T>

        let job = job_opt.ok_or_else(|| {
            error!(job_id = %command.job_id, "Job not found in repository");
            ValidateJobError::JobNotFound {
                job_id: command.job_id.clone(),
            }
        })?;

        // Check if job is cancelled
        if *job.state() == JobState::Cancelled {
            warn!(job_id = %command.job_id, "Job is cancelled");
            return Err(ValidateJobError::JobCancelled {
                job_id: command.job_id.clone(),
            });
        }

        // Check if job is in valid state for execution
        if !job.state().is_scheduled() && *job.state() != JobState::Pending {
            error!(
                job_id = %command.job_id,
                state = ?job.state(),
                "Job is in invalid state for execution"
            );
            return Err(ValidateJobError::InvalidState {
                job_id: command.job_id.clone(),
                state: job.state().clone(),
            });
        }

        info!(
            job_id = %command.job_id,
            state = ?job.state(),
            "Job validation successful"
        );

        Ok(JobValidationResult::valid(job.state().clone()))
    }
}

// =============================================================================
// AssignWorkerHandler
// =============================================================================

/// Handler for AssignWorkerCommand.
///
/// This handler finds an available worker and assigns it to the job.
#[derive(Debug)]
pub struct AssignWorkerHandler<J, W>
where
    J: JobRepository + Debug,
    W: WorkerRegistry + Debug,
{
    job_repository: Arc<J>,
    worker_registry: Arc<W>,
}

impl<J, W> AssignWorkerHandler<J, W>
where
    J: JobRepository + Debug,
    W: WorkerRegistry + Debug,
{
    /// Creates a new handler with the given repositories.
    #[inline]
    pub fn new(job_repository: Arc<J>, worker_registry: Arc<W>) -> Self {
        Self {
            job_repository,
            worker_registry,
        }
    }
}

#[async_trait]
impl<J, W> CommandHandler<AssignWorkerCommand> for AssignWorkerHandler<J, W>
where
    J: JobRepository + Debug + Send + Sync + 'static,
    W: WorkerRegistry + Debug + Send + Sync + 'static,
{
    type Error = AssignWorkerError;

    async fn handle(&self, command: AssignWorkerCommand) -> Result<WorkerAssignmentResult, Self::Error> {
        debug!(
            job_id = %command.job_id,
            saga_id = %command.saga_id,
            "Assigning worker to job"
        );

        // Find available workers
        let filter = WorkerFilter::new()
            .with_state(WorkerState::Ready)
            .accepting_jobs();

        let available_workers = self
            .worker_registry
            .find(&filter)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to find available workers");
                AssignWorkerError::AssignmentFailed {
                    job_id: command.job_id.clone(),
                    source: e,
                }
            })?;

        if available_workers.is_empty() {
            warn!(job_id = %command.job_id, "No available workers found");
            return Ok(WorkerAssignmentResult::failure("No available workers"));
        }

        // Assign first available worker
        let worker = &available_workers[0];

        // Get job for update
        let job_opt = self
            .job_repository
            .find_by_id(&command.job_id)
            .await
            .ok()  // Convert DomainError to None
            .flatten();  // Flatten Option<Option<T>> to Option<T>

        let mut job = job_opt.ok_or_else(|| AssignWorkerError::NoAvailableWorkers {
            job_id: command.job_id.clone(),
        })?;

        // Mark job as assigned
        job.set_state(JobState::Assigned).map_err(|e| {
            error!(error = %e, "Failed to set job state");
            AssignWorkerError::AssignmentFailed {
                job_id: command.job_id.clone(),
                source: e,
            }
        })?;

        self.job_repository.update(&job)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to update job");
                AssignWorkerError::AssignmentFailed {
                    job_id: command.job_id.clone(),
                    source: e,
                }
            })?;

        // Update worker state to Busy
        self.worker_registry
            .update_state(worker.id(), WorkerState::Busy)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to update worker state");
                AssignWorkerError::AssignmentFailed {
                    job_id: command.job_id.clone(),
                    source: e,
                }
            })?;

        info!(
            job_id = %command.job_id,
            worker_id = %worker.id(),
            "Worker assigned successfully"
        );

        Ok(WorkerAssignmentResult::success(worker.id().clone(), worker.state().clone()))
    }
}

// =============================================================================
// ExecuteJobHandler
// =============================================================================

/// Handler for ExecuteJobCommand.
///
/// This handler dispatches the job to the worker via gRPC and marks it as running.
/// Results come via JobStatusChanged events when the worker sends JobResultMessage.
///
/// Architecture:
/// - ExecuteJobHandler: Dispatch job (send RunJobCommand)
/// - WorkerStream: Receives JobResultMessage from worker
/// - on_job_result: Updates job state and publishes JobStatusChanged event
/// - Event-driven flow continues independently
pub struct ExecuteJobHandler<J, W, S>
where
    J: JobRepository + Debug,
    W: WorkerRegistry + Debug,
    S: WorkerCommandSender,
{
    job_repository: Arc<J>,
    worker_registry: Arc<W>,
    command_sender: Arc<S>,
}

impl<J, W, S> std::fmt::Debug for ExecuteJobHandler<J, W, S>
where
    J: JobRepository + Debug,
    W: WorkerRegistry + Debug,
    S: WorkerCommandSender,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecuteJobHandler")
            .field("job_repository", &"Arc<JobRepository>")
            .field("worker_registry", &"Arc<WorkerRegistry>")
            .field("command_sender", &"Arc<WorkerCommandSender>")
            .finish()
    }
}

impl<J, W, S> ExecuteJobHandler<J, W, S>
where
    J: JobRepository + Debug,
    W: WorkerRegistry + Debug,
    S: WorkerCommandSender,
{
    #[inline]
    pub fn new(
        job_repository: Arc<J>,
        worker_registry: Arc<W>,
        command_sender: Arc<S>,
    ) -> Self {
        Self {
            job_repository,
            worker_registry,
            command_sender,
        }
    }
}

#[async_trait]
impl<J, W, S> CommandHandler<ExecuteJobCommand> for ExecuteJobHandler<J, W, S>
where
    J: JobRepository + Debug + Send + Sync + 'static,
    W: WorkerRegistry + Debug + Send + Sync + 'static,
    S: WorkerCommandSender + Send + Sync + 'static,
{
    type Error = ExecuteJobError;

    async fn handle(&self, command: ExecuteJobCommand) -> Result<JobExecutionResult, Self::Error> {
        debug!(
            job_id = %command.job_id,
            worker_id = %command.worker_id,
            "Executing job"
        );

        // 1. Verify job exists and get full job data
        let job = self
            .job_repository
            .find_by_id(&command.job_id)
            .await
            .ok()  // Convert DomainError to None
            .flatten()  // Flatten Option<Option<T>> to Option<T>
            .ok_or_else(|| ExecuteJobError::JobNotFound {
                job_id: command.job_id.clone(),
            })?;

        // 2. Verify worker exists
        let _worker = self
            .worker_registry
            .find_by_id(&command.worker_id)
            .await
            .ok()  // Convert DomainError to None
            .flatten()  // Flatten Option<Option<T>> to Option<T>
            .ok_or_else(|| ExecuteJobError::WorkerNotFound {
                worker_id: command.worker_id.clone(),
            })?;

        // 3. Update job state to Running
        self.job_repository
            .update_state(&command.job_id, JobState::Running)
            .await
            .map_err(|e| ExecuteJobError::ExecutionFailed {
                job_id: command.job_id.clone(),
                source: e,
            })?;

        // 4. Send RunJobCommand to worker via gRPC
        info!(
            job_id = %command.job_id,
            worker_id = %command.worker_id,
            "📤 Dispatching job to worker"
        );

        self.command_sender
            .send_run_job(&command.worker_id, &job)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to send RunJobCommand to worker");
                ExecuteJobError::ExecutionFailed {
                    job_id: command.job_id.clone(),
                    source: e,
                }
            })?;

        info!(
            job_id = %command.job_id,
            worker_id = %command.worker_id,
            "✅ Job dispatched. Result will come via JobStatusChanged event."
        );

        // Job is dispatched - result comes via events from on_job_result
        Ok(JobExecutionResult::initiated(JobState::Running))
    }
}

// =============================================================================
// CompleteJobHandler
// =============================================================================

/// Handler for CompleteJobCommand.
///
/// This handler marks the job as completed and releases the worker.
#[derive(Debug)]
pub struct CompleteJobHandler<J, W>
where
    J: JobRepository + Debug,
    W: WorkerRegistry + Debug,
{
    job_repository: Arc<J>,
    worker_registry: Arc<W>,
}

impl<J, W> CompleteJobHandler<J, W>
where
    J: JobRepository + Debug,
    W: WorkerRegistry + Debug,
{
    #[inline]
    pub fn new(job_repository: Arc<J>, worker_registry: Arc<W>) -> Self {
        Self {
            job_repository,
            worker_registry,
        }
    }
}

#[async_trait]
impl<J, W> CommandHandler<CompleteJobCommand> for CompleteJobHandler<J, W>
where
    J: JobRepository + Debug + Send + Sync + 'static,
    W: WorkerRegistry + Debug + Send + Sync + 'static,
{
    type Error = CompleteJobError;

    async fn handle(&self, command: CompleteJobCommand) -> std::result::Result<JobCompletionResult, Self::Error> {
        debug!(
            job_id = %command.job_id,
            final_state = ?command.final_state,
            "Completing job"
        );

        // Get current job state
        let job_opt = self
            .job_repository
            .find_by_id(&command.job_id)
            .await
            .ok()  // Convert DomainError to None
            .flatten();  // Flatten Option<Option<T>> to Option<T>

        let job = job_opt.ok_or_else(|| CompleteJobError::JobNotFound {
            job_id: command.job_id.clone(),
        })?;

        // Validate state transition
        if !job.state().can_transition_to(&command.final_state) {
            return Err(CompleteJobError::InvalidTransition {
                job_id: command.job_id.clone(),
                current: job.state().clone(),
                desired: command.final_state.clone(),
            });
        }

        // Update job state
        self.job_repository
            .update_state(&command.job_id, command.final_state.clone())
            .await
            .map_err(|e| CompleteJobError::CompletionFailed {
                job_id: command.job_id.clone(),
                source: e,
            })?;

        // If job is completed, release the worker
        if command.final_state.is_terminal() {
            if let Ok(Some(worker)) = self.worker_registry.get_by_job_id(&command.job_id).await {
                self.worker_registry
                    .update_state(worker.id(), WorkerState::Terminated)
                    .await
                    .map_err(|e| CompleteJobError::CompletionFailed {
                        job_id: command.job_id.clone(),
                        source: e,
                    })?;
            }
        }

        info!(
            job_id = %command.job_id,
            final_state = ?command.final_state,
            "Job completed successfully"
        );

        Ok(JobCompletionResult::new(command.final_state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use hodei_server_domain::jobs::{Job, JobSpec};
    use hodei_server_domain::shared_kernel::ProviderId;
    use std::sync::Arc;

    // Test implementations
    #[derive(Debug)]
    struct MockJobRepository;

    #[async_trait]
    impl JobRepository for MockJobRepository {
        async fn find_by_id(&self, _id: &JobId) -> Result<Option<Job>> {
            Ok(Some(Job::new(JobId::new(), JobSpec::default())))
        }
        async fn update(&self, _job: &Job) -> Result<()> { Ok(()) }
        async fn update_state(&self, _id: &JobId, _state: JobState) -> Result<()> { Ok(()) }
        async fn find(&self, _filter: crate::jobs::JobFilter) -> Result<Vec<Job>> { Ok(vec![]) }
        async fn delete(&self, _id: &JobId) -> Result<()> { Ok(()) }
    }

    #[derive(Debug)]
    struct MockWorkerRegistry;

    #[async_trait]
    impl WorkerRegistry for MockWorkerRegistry {
        async fn find_by_id(&self, _id: &WorkerId) -> Result<Option<hodei_server_domain::workers::Worker>> { Ok(None) }
        async fn find(&self, _filter: WorkerFilter) -> Result<Vec<hodei_server_domain::workers::Worker>> { Ok(vec![]) }
        async fn update_state(&self, _id: &WorkerId, _state: WorkerState) -> Result<()> { Ok(()) }
        async fn get_by_job_id(&self, _job_id: &JobId) -> Result<Option<hodei_server_domain::workers::Worker>> { Ok(None) }
    }

    #[tokio::test]
    async fn validate_job_handler_success() {
        let repo = Arc::new(MockJobRepository);
        let handler = ValidateJobHandler::new(repo);
        let command = ValidateJobCommand::new(JobId::new(), "test-saga".to_string());

        let result = handler.handle(command).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_valid);
    }

    #[tokio::test]
    async fn validate_job_handler_not_found() {
        #[derive(Debug)]
        struct NotFoundRepo;
        #[async_trait]
        impl JobRepository for NotFoundRepo {
            async fn find_by_id(&self, _id: &JobId) -> Result<Option<Job>> { Ok(None) }
            async fn update(&self, _job: &Job) -> Result<()> { Ok(()) }
            async fn update_state(&self, _id: &JobId, _state: JobState) -> Result<()> { Ok(()) }
            async fn find(&self, _filter: crate::jobs::JobFilter) -> Result<Vec<Job>> { Ok(vec![]) }
            async fn delete(&self, _id: &JobId) -> Result<()> { Ok(()) }
        }

        let repo = Arc::new(NotFoundRepo);
        let handler = ValidateJobHandler::new(repo);
        let command = ValidateJobCommand::new(JobId::new(), "test-saga".to_string());

        let result = handler.handle(command).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ValidateJobError::JobNotFound { .. }));
    }
}

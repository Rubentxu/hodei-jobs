// Execution Saga Commands
//
// Commands used by the ExecutionSaga for job execution lifecycle management.
// These commands encapsulate the intent to execute, track, and complete jobs.

use crate::command::{Command, CommandHandler, CommandMetadataDefault, CommandTargetType};
use crate::jobs::JobRepository;
use crate::shared_kernel::{JobId, JobState, WorkerId, WorkerState};
use crate::workers::{WorkerFilter, WorkerRegistry};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;

/// Command to validate a job before execution.
///
/// This command validates that a job exists and is in a valid state
/// to proceed with execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateJobCommand {
    /// The job ID to validate
    pub job_id: JobId,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Optional metadata for tracing
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl ValidateJobCommand {
    /// Creates a new ValidateJobCommand.
    #[inline]
    pub fn new(job_id: JobId, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            saga_id,
            metadata,
        }
    }
}

impl Command for ValidateJobCommand {
    type Output = JobValidationResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{}-validate-job-{}", self.saga_id, self.job_id))
    }

    fn target_type(&self) -> CommandTargetType {
        CommandTargetType::Job
    }

    fn command_name(&self) -> Cow<'_, str> {
        Cow::Borrowed("ValidateJob")
    }
}

/// Result of job validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobValidationResult {
    /// Whether validation passed
    pub is_valid: bool,
    /// The current state of the job
    pub current_state: JobState,
    /// Reason for validation failure (if any)
    pub failure_reason: Option<String>,
}

impl JobValidationResult {
    /// Creates a successful validation result.
    #[inline]
    pub fn valid(current_state: JobState) -> Self {
        Self {
            is_valid: true,
            current_state,
            failure_reason: None,
        }
    }

    /// Creates a failed validation result.
    #[inline]
    pub fn invalid(current_state: JobState, reason: impl Into<String>) -> Self {
        Self {
            is_valid: false,
            current_state,
            failure_reason: Some(reason.into()),
        }
    }
}

/// Error types for ValidateJobHandler.
#[derive(Debug, thiserror::Error)]
pub enum ValidateJobError {
    #[error("Job {job_id} not found")]
    JobNotFound { job_id: JobId },

    #[error("Job {job_id} is in invalid state: {state}")]
    InvalidState { job_id: JobId, state: JobState },

    #[error("Job {job_id} is cancelled")]
    JobCancelled { job_id: JobId },
}

/// Handler for ValidateJobCommand.
pub struct ValidateJobHandler {
    job_repository: Arc<dyn JobRepository + Send + Sync>,
}

impl ValidateJobHandler {
    /// Creates a new handler with the given job repository.
    #[inline]
    pub fn new(job_repository: Arc<dyn JobRepository + Send + Sync>) -> Self {
        Self { job_repository }
    }
}

#[async_trait]
impl CommandHandler<ValidateJobCommand> for ValidateJobHandler {
    type Error = ValidateJobError;

    async fn handle(
        &self,
        command: ValidateJobCommand,
    ) -> Result<JobValidationResult, Self::Error> {
        // Clone job_id since we need it in closures
        let job_id = command.job_id.clone();

        let job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(|_| ValidateJobError::JobNotFound {
                job_id: job_id.clone(),
            })?
            .ok_or_else(|| ValidateJobError::JobNotFound {
                job_id: job_id.clone(),
            })?;

        // Check if job is in a valid state for execution
        if *job.state() == JobState::Cancelled {
            return Err(ValidateJobError::JobCancelled {
                job_id: job_id.clone(),
            });
        }

        if !job.state().is_scheduled() && *job.state() != JobState::Pending {
            return Err(ValidateJobError::InvalidState {
                job_id: job_id.clone(),
                state: job.state().clone(),
            });
        }

        Ok(JobValidationResult::valid(job.state().clone()))
    }
}

/// Command to assign a worker to a job.
///
/// This command finds an available worker or uses a pre-assigned one,
/// and assigns it to the job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignWorkerCommand {
    /// The job ID to assign a worker to
    pub job_id: JobId,
    /// Optional pre-assigned worker ID
    pub worker_id: Option<WorkerId>,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Optional metadata for tracing
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl AssignWorkerCommand {
    /// Creates a new AssignWorkerCommand with automatic worker selection.
    #[inline]
    pub fn new(job_id: JobId, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            worker_id: None,
            saga_id,
            metadata,
        }
    }

    /// Creates a new AssignWorkerCommand with a pre-assigned worker.
    #[inline]
    pub fn with_worker(job_id: JobId, worker_id: WorkerId, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            worker_id: Some(worker_id),
            saga_id,
            metadata,
        }
    }
}

impl Command for AssignWorkerCommand {
    type Output = WorkerAssignmentResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        if let Some(ref worker_id) = self.worker_id {
            Cow::Owned(format!(
                "{}-assign-worker-{}-{}",
                self.saga_id, self.job_id, worker_id
            ))
        } else {
            Cow::Owned(format!("{}-assign-worker-{}", self.saga_id, self.job_id))
        }
    }

    fn target_type(&self) -> CommandTargetType {
        CommandTargetType::Worker
    }

    fn command_name(&self) -> Cow<'_, str> {
        Cow::Borrowed("AssignWorker")
    }
}

/// Result of worker assignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerAssignmentResult {
    /// Whether assignment was successful
    pub success: bool,
    /// The assigned worker ID (if successful)
    pub worker_id: Option<WorkerId>,
    /// Current state of the worker
    pub worker_state: Option<WorkerState>,
    /// Reason for assignment failure (if any)
    pub failure_reason: Option<String>,
}

impl WorkerAssignmentResult {
    /// Creates a successful assignment result.
    #[inline]
    pub fn success(worker_id: WorkerId, state: WorkerState) -> Self {
        Self {
            success: true,
            worker_id: Some(worker_id),
            worker_state: Some(state),
            failure_reason: None,
        }
    }

    /// Creates a failed assignment result.
    #[inline]
    pub fn failure(reason: impl Into<String>) -> Self {
        Self {
            success: false,
            worker_id: None,
            worker_state: None,
            failure_reason: Some(reason.into()),
        }
    }
}

/// Error types for AssignWorkerHandler.
#[derive(Debug, thiserror::Error)]
pub enum AssignWorkerError {
    #[error("No available workers found for job {job_id}")]
    NoAvailableWorkers { job_id: JobId },

    #[error("Failed to assign worker to job {job_id}: {source}")]
    AssignmentFailed {
        job_id: JobId,
        source: crate::shared_kernel::DomainError,
    },

    #[error("Worker {worker_id} not found")]
    WorkerNotFound { worker_id: WorkerId },

    #[error("Worker {worker_id} is not available (state: {state:?})")]
    WorkerNotAvailable {
        worker_id: WorkerId,
        state: WorkerState,
    },
}

/// Handler for AssignWorkerCommand.
pub struct AssignWorkerHandler {
    job_repository: Arc<dyn JobRepository + Send + Sync>,
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl AssignWorkerHandler {
    /// Creates a new handler with the given repositories.
    #[inline]
    pub fn new(
        job_repository: Arc<dyn JobRepository + Send + Sync>,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    ) -> Self {
        Self {
            job_repository,
            worker_registry,
        }
    }
}

#[async_trait]
impl CommandHandler<AssignWorkerCommand> for AssignWorkerHandler {
    type Error = AssignWorkerError;

    async fn handle(
        &self,
        command: AssignWorkerCommand,
    ) -> Result<WorkerAssignmentResult, Self::Error> {
        // Clone job_id since we need it in closures
        let job_id = command.job_id.clone();

        // Step 1: Find or Validate Worker
        let worker_id = if let Some(pre_assigned_id) = command.worker_id {
            // Validate pre-assigned worker
            let worker = self
                .worker_registry
                .find_by_id(&pre_assigned_id)
                .await
                .map_err(|e| AssignWorkerError::AssignmentFailed {
                    job_id: job_id.clone(),
                    source: e,
                })?
                .ok_or_else(|| AssignWorkerError::WorkerNotFound {
                    worker_id: pre_assigned_id.clone(),
                })?;

            // Ensure worker is available (Ready or Busy if it was already assigned to this job)
            if *worker.state() != WorkerState::Ready && *worker.state() != WorkerState::Busy {
                return Err(AssignWorkerError::WorkerNotAvailable {
                    worker_id: pre_assigned_id,
                    state: worker.state().clone(),
                });
            }
            pre_assigned_id
        } else {
            // Find an available worker using find with WorkerFilter
            let filter = WorkerFilter::new()
                .with_state(WorkerState::Ready)
                .accepting_jobs();
            let available_workers = self.worker_registry.find(&filter).await.map_err(|e| {
                AssignWorkerError::AssignmentFailed {
                    job_id: job_id.clone(),
                    source: e,
                }
            })?;

            if available_workers.is_empty() {
                return Ok(WorkerAssignmentResult::failure("No available workers"));
            }

            // Assign the first available worker
            available_workers[0].id().clone()
        };

        // Step 2: Update job with worker assignment
        let mut job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(|e| AssignWorkerError::AssignmentFailed {
                job_id: job_id.clone(),
                source: e,
            })?
            .ok_or_else(|| AssignWorkerError::NoAvailableWorkers {
                job_id: job_id.clone(),
            })?;

        // Mark job as assigned
        job.set_state(JobState::Assigned)
            .map_err(|e| AssignWorkerError::AssignmentFailed {
                job_id: job_id.clone(),
                source: e,
            })?;

        self.job_repository.update(&job).await.map_err(|e| {
            AssignWorkerError::AssignmentFailed {
                job_id: job_id.clone(),
                source: e,
            }
        })?;

        // Step 3: Update worker state to Busy
        self.worker_registry
            .update_state(&worker_id, WorkerState::Busy)
            .await
            .map_err(|e| AssignWorkerError::AssignmentFailed {
                job_id: job_id.clone(),
                source: e,
            })?;

        // Fetch final worker state for result
        let final_worker = self
            .worker_registry
            .find_by_id(&worker_id)
            .await
            .map_err(|e| AssignWorkerError::AssignmentFailed {
                job_id: job_id.clone(),
                source: e,
            })?
            .ok_or_else(|| AssignWorkerError::WorkerNotFound {
                worker_id: worker_id.clone(),
            })?;

        Ok(WorkerAssignmentResult::success(
            worker_id,
            final_worker.state().clone(),
        ))
    }
}

/// Command to execute a job.
///
/// This command marks the job as running and dispatches it to the worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteJobCommand {
    /// The job ID to execute
    pub job_id: JobId,
    /// The worker ID that will execute the job
    pub worker_id: WorkerId,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Optional metadata for tracing
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl ExecuteJobCommand {
    /// Creates a new ExecuteJobCommand.
    #[inline]
    pub fn new(job_id: JobId, worker_id: WorkerId, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            worker_id,
            saga_id,
            metadata,
        }
    }
}

impl Command for ExecuteJobCommand {
    type Output = JobExecutionResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{}-execute-job-{}", self.saga_id, self.job_id))
    }

    fn target_type(&self) -> CommandTargetType {
        CommandTargetType::Worker
    }

    fn command_name(&self) -> Cow<'_, str> {
        Cow::Borrowed("ExecuteJob")
    }
}

/// Result of job execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobExecutionResult {
    /// Whether execution was initiated successfully
    pub initiated: bool,
    /// The job's new state
    pub new_state: JobState,
    /// Reason for failure (if any)
    pub failure_reason: Option<String>,
}

impl JobExecutionResult {
    /// Creates a successful execution result.
    #[inline]
    pub fn initiated(state: JobState) -> Self {
        Self {
            initiated: true,
            new_state: state,
            failure_reason: None,
        }
    }

    /// Creates a failed execution result.
    #[inline]
    pub fn failed(state: JobState, reason: impl Into<String>) -> Self {
        Self {
            initiated: false,
            new_state: state,
            failure_reason: Some(reason.into()),
        }
    }
}

/// Error types for ExecuteJobHandler.
#[derive(Debug, thiserror::Error)]
pub enum ExecuteJobError {
    #[error("Job {job_id} not found")]
    JobNotFound { job_id: JobId },

    #[error("Worker {worker_id} not found")]
    WorkerNotFound { worker_id: WorkerId },

    #[error("Failed to start job execution: {source}")]
    ExecutionFailed {
        job_id: JobId,
        source: crate::shared_kernel::DomainError,
    },
}

/// Handler for ExecuteJobCommand.
///
/// ## SRP: Responsabilidad Única
/// Este handler delega el envío de RUN_JOB a un servicio especializado.
/// El patrón es: Saga → Comando → Handler → Servicio (acción gRPC)
pub struct ExecuteJobHandler {
    job_repository: Arc<dyn JobRepository + Send + Sync>,
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    /// Servicio especializado para enviar RUN_JOB al worker via gRPC
    /// El handler delega la acción a este servicio
    job_executor: Option<Arc<dyn JobExecutor + Send + Sync>>,
}

/// Trait para servicio especializado de ejecución de jobs
/// Sigue el patrón: comando → servicio (acción)
#[async_trait::async_trait]
pub trait JobExecutor {
    /// Envía el comando RUN_JOB al worker y actualiza estado a RUNNING
    async fn execute_job(
        &self,
        job_id: &JobId,
        worker_id: &WorkerId,
    ) -> Result<(), ExecuteJobError>;
}

impl ExecuteJobHandler {
    /// Creates a new handler with the given repositories.
    #[inline]
    pub fn new(
        job_repository: Arc<dyn JobRepository + Send + Sync>,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
        job_executor: Option<Arc<dyn JobExecutor + Send + Sync>>,
    ) -> Self {
        Self {
            job_repository,
            worker_registry,
            job_executor,
        }
    }
}

#[async_trait]
impl CommandHandler<ExecuteJobCommand> for ExecuteJobHandler {
    type Error = ExecuteJobError;

    async fn handle(&self, command: ExecuteJobCommand) -> Result<JobExecutionResult, Self::Error> {
        // Clone IDs since we need them in closures
        let job_id = command.job_id.clone();
        let worker_id = command.worker_id.clone();

        // Verify job exists
        let _job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(|_| ExecuteJobError::JobNotFound {
                job_id: job_id.clone(),
            })?
            .ok_or_else(|| ExecuteJobError::JobNotFound {
                job_id: job_id.clone(),
            })?;

        // Verify worker exists
        let _worker = self
            .worker_registry
            .find_by_id(&worker_id)
            .await
            .map_err(|_| ExecuteJobError::WorkerNotFound {
                worker_id: worker_id.clone(),
            })?
            .ok_or_else(|| ExecuteJobError::WorkerNotFound {
                worker_id: worker_id.clone(),
            })?;

        // Delegate to specialized service for RUN_JOB action
        // This follows the pattern: Saga → Command → Handler → Service (action)
        if let Some(ref executor) = self.job_executor {
            executor.execute_job(&job_id, &worker_id).await?;
            tracing::info!(
                "ExecuteJobHandler: Job {} dispatched to worker {} via JobExecutor",
                job_id,
                worker_id
            );
        } else {
            // Fallback: just mark as running (no RUN_JOB sent)
            // This is for testing or when no executor is configured
            self.job_repository
                .update_state(&job_id, JobState::Running)
                .await
                .map_err(|e| ExecuteJobError::ExecutionFailed {
                    job_id: job_id.clone(),
                    source: e,
                })?;
            tracing::warn!(
                "ExecuteJobHandler: No JobExecutor configured, only marked job {} as RUNNING",
                job_id
            );
        }

        Ok(JobExecutionResult::initiated(JobState::Running))
    }
}

/// Command to complete a job.
///
/// This command marks the job as completed with the final result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteJobCommand {
    /// The job ID to complete
    pub job_id: JobId,
    /// The final state of the job
    pub final_state: JobState,
    /// Optional result message or error details
    pub result_message: Option<String>,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Optional metadata for tracing
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl CompleteJobCommand {
    /// Creates a new CompleteJobCommand for success.
    #[inline]
    pub fn success(job_id: JobId, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            final_state: JobState::Succeeded,
            result_message: None,
            saga_id,
            metadata,
        }
    }

    /// Creates a new CompleteJobCommand for failure.
    #[inline]
    pub fn failed(job_id: JobId, message: impl Into<String>, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            final_state: JobState::Failed,
            result_message: Some(message.into()),
            saga_id,
            metadata,
        }
    }

    /// Creates a new CompleteJobCommand with custom final state.
    #[inline]
    pub fn with_state(
        job_id: JobId,
        state: JobState,
        message: Option<String>,
        saga_id: String,
    ) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            final_state: state,
            result_message: message,
            saga_id,
            metadata,
        }
    }
}

impl Command for CompleteJobCommand {
    type Output = JobCompletionResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{}-complete-job-{}", self.saga_id, self.job_id))
    }

    fn target_type(&self) -> CommandTargetType {
        CommandTargetType::Job
    }

    fn command_name(&self) -> Cow<'_, str> {
        Cow::Borrowed("CompleteJob")
    }
}

/// Result of job completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCompletionResult {
    /// Whether completion was successful
    pub success: bool,
    /// The final state of the job
    pub final_state: JobState,
}

impl JobCompletionResult {
    /// Creates a successful completion result.
    #[inline]
    pub fn new(state: JobState) -> Self {
        Self {
            success: state.is_terminal(),
            final_state: state,
        }
    }
}

/// Error types for CompleteJobHandler.
#[derive(Debug, thiserror::Error)]
pub enum CompleteJobError {
    #[error("Job {job_id} not found")]
    JobNotFound { job_id: JobId },

    #[error("Cannot transition job {job_id} from {current} to {desired}")]
    InvalidTransition {
        job_id: JobId,
        current: JobState,
        desired: JobState,
    },

    #[error("Failed to complete job: {source}")]
    CompletionFailed {
        job_id: JobId,
        source: crate::shared_kernel::DomainError,
    },
}

/// Handler for CompleteJobCommand.
pub struct CompleteJobHandler {
    job_repository: Arc<dyn JobRepository + Send + Sync>,
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl CompleteJobHandler {
    /// Creates a new handler with the given repositories.
    #[inline]
    pub fn new(
        job_repository: Arc<dyn JobRepository + Send + Sync>,
        worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
    ) -> Self {
        Self {
            job_repository,
            worker_registry,
        }
    }
}

#[async_trait]
impl CommandHandler<CompleteJobCommand> for CompleteJobHandler {
    type Error = CompleteJobError;

    async fn handle(
        &self,
        command: CompleteJobCommand,
    ) -> Result<JobCompletionResult, Self::Error> {
        // Clone job_id since we need it in closures
        let job_id = command.job_id.clone();
        let final_state = command.final_state.clone();

        // Get current job state
        let job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(|_| CompleteJobError::JobNotFound {
                job_id: job_id.clone(),
            })?
            .ok_or_else(|| CompleteJobError::JobNotFound {
                job_id: job_id.clone(),
            })?;

        // Validate state transition
        if !job.state().can_transition_to(&final_state) {
            return Err(CompleteJobError::InvalidTransition {
                job_id: job_id.clone(),
                current: job.state().clone(),
                desired: final_state.clone(),
            });
        }

        // Update job state
        self.job_repository
            .update_state(&job_id, final_state.clone())
            .await
            .map_err(|e| CompleteJobError::CompletionFailed {
                job_id: job_id.clone(),
                source: e,
            })?;

        // If job is completed, release the worker
        if final_state.is_terminal()
            && let Ok(Some(worker)) = self.worker_registry.get_by_job_id(&job_id).await
        {
            self.worker_registry
                .update_state(worker.id(), WorkerState::Terminated)
                .await
                .map_err(|e| CompleteJobError::CompletionFailed {
                    job_id: job_id.clone(),
                    source: e,
                })?;
        }

        Ok(JobCompletionResult::new(command.final_state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_kernel::{JobId, JobState, ProviderId, WorkerId, WorkerState};

    #[tokio::test]
    async fn validate_job_command_idempotency() {
        let job_id = JobId::new();
        let saga_id = "saga-123".to_string();
        let cmd = ValidateJobCommand::new(job_id.clone(), saga_id.clone());

        let key = cmd.idempotency_key();
        assert!(key.contains(&saga_id));
        assert!(key.contains(&job_id.to_string()));
    }

    #[tokio::test]
    async fn assign_worker_command_idempotency() {
        let job_id = JobId::new();
        let saga_id = "saga-456".to_string();
        let cmd = AssignWorkerCommand::new(job_id.clone(), saga_id.clone());

        let key = cmd.idempotency_key();
        assert!(key.contains(&saga_id));
        assert!(key.contains("assign-worker"));
    }

    #[tokio::test]
    async fn execute_job_command_idempotency() {
        let job_id = JobId::new();
        let worker_id = WorkerId::new();
        let saga_id = "saga-789".to_string();
        let cmd = ExecuteJobCommand::new(job_id.clone(), worker_id.clone(), saga_id.clone());

        let key = cmd.idempotency_key();
        assert!(key.contains(&saga_id));
        assert!(key.contains("execute-job"));
    }

    #[tokio::test]
    async fn complete_job_command_success() {
        let job_id = JobId::new();
        let saga_id = "saga-complete".to_string();
        let cmd = CompleteJobCommand::success(job_id.clone(), saga_id);

        assert_eq!(cmd.final_state, JobState::Succeeded);
    }

    #[tokio::test]
    async fn complete_job_command_failure() {
        let job_id = JobId::new();
        let saga_id = "saga-complete".to_string();
        let cmd = CompleteJobCommand::failed(job_id.clone(), "Execution failed", saga_id);

        assert_eq!(cmd.final_state, JobState::Failed);
        assert_eq!(cmd.result_message, Some("Execution failed".to_string()));
    }

    #[tokio::test]
    async fn job_validation_result_valid() {
        let result = JobValidationResult::valid(JobState::Assigned);
        assert!(result.is_valid);
        assert_eq!(result.current_state, JobState::Assigned);
        assert!(result.failure_reason.is_none());
    }

    #[tokio::test]
    async fn job_validation_result_invalid() {
        let result = JobValidationResult::invalid(JobState::Cancelled, "Job was cancelled");
        assert!(!result.is_valid);
        assert_eq!(result.current_state, JobState::Cancelled);
        assert_eq!(result.failure_reason, Some("Job was cancelled".to_string()));
    }

    #[tokio::test]
    async fn worker_assignment_result_success() {
        let worker_id = WorkerId::new();
        let result = WorkerAssignmentResult::success(worker_id.clone(), WorkerState::Busy);
        assert!(result.success);
        assert_eq!(result.worker_id, Some(worker_id.clone()));
        assert_eq!(result.worker_state, Some(WorkerState::Busy));
    }

    #[tokio::test]
    async fn worker_assignment_result_failure() {
        let result = WorkerAssignmentResult::failure("No workers available");
        assert!(!result.success);
        assert!(result.worker_id.is_none());
        assert_eq!(
            result.failure_reason,
            Some("No workers available".to_string())
        );
    }

    #[tokio::test]
    async fn job_execution_result_initiated() {
        let result = JobExecutionResult::initiated(JobState::Running);
        assert!(result.initiated);
        assert_eq!(result.new_state, JobState::Running);
    }

    #[tokio::test]
    async fn job_execution_result_failed() {
        let result = JobExecutionResult::failed(JobState::Assigned, "Worker not available");
        assert!(!result.initiated);
        assert_eq!(result.new_state, JobState::Assigned);
    }

    #[tokio::test]
    async fn job_completion_result_success() {
        let result = JobCompletionResult::new(JobState::Succeeded);
        assert!(result.success);
        assert_eq!(result.final_state, JobState::Succeeded);
    }

    #[tokio::test]
    async fn job_completion_result_pending() {
        let result = JobCompletionResult::new(JobState::Pending);
        assert!(!result.success);
        assert_eq!(result.final_state, JobState::Pending);
    }
}

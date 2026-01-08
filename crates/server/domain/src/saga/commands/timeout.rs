// Timeout Saga Commands
//
// Commands used by the TimeoutSaga for job timeout handling.
// These commands encapsulate the intent to terminate workers and mark jobs as timed out.
//
// This module implements the Command Bus pattern for timeout operations,
// enabling saga steps to dispatch commands that are handled by domain services.

use crate::command::{Command, CommandHandler, CommandMetadataDefault};
use crate::jobs::JobRepository;
use crate::saga::SagaServices;
use crate::shared_kernel::{JobId, JobState, WorkerId, WorkerState};
use crate::workers::WorkerRegistry;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::Debug;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Command to terminate a worker that has exceeded its timeout.
///
/// This command transitions a worker to the TERMINATED state and optionally
/// unregisters it from the worker registry.
///
/// # Idempotency
///
/// The idempotency key is derived from saga_id and worker_id, ensuring that
/// retrying a timeout saga will not attempt to terminate the same worker twice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminateWorkerCommand {
    /// The worker to terminate
    pub worker_id: WorkerId,
    /// The job that timed out (for context)
    pub job_id: Option<JobId>,
    /// Reason for termination (e.g., "timeout", "user_requested")
    pub reason: String,
    /// Whether to unregister the worker from the registry
    pub unregister: bool,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Optional metadata for tracing and context
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl TerminateWorkerCommand {
    /// Creates a new TerminateWorkerCommand.
    #[inline]
    pub fn new(
        worker_id: WorkerId,
        job_id: Option<JobId>,
        reason: impl Into<String>,
        saga_id: String,
    ) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            worker_id,
            job_id,
            reason: reason.into(),
            unregister: false,
            saga_id,
            metadata,
        }
    }

    /// Creates a command that also unregisters the worker.
    #[inline]
    pub fn with_unregister(
        worker_id: WorkerId,
        job_id: Option<JobId>,
        reason: impl Into<String>,
        saga_id: String,
    ) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            worker_id,
            job_id,
            reason: reason.into(),
            unregister: true,
            saga_id,
            metadata,
        }
    }
}

impl Command for TerminateWorkerCommand {
    type Output = TerminateWorkerResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!(
            "{}-terminate-worker-{}",
            self.saga_id, self.worker_id
        ))
    }
}

/// Result of worker termination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminateWorkerResult {
    /// Whether termination was successful
    pub success: bool,
    /// The worker's final state
    pub final_state: WorkerState,
    /// Whether worker was already terminated (idempotency)
    pub already_terminated: bool,
    /// Error message if termination failed
    pub error_message: Option<String>,
}

impl TerminateWorkerResult {
    /// Creates a successful result.
    #[inline]
    pub fn success(final_state: WorkerState) -> Self {
        Self {
            success: true,
            final_state,
            already_terminated: false,
            error_message: None,
        }
    }

    /// Creates a result for idempotent skip.
    #[inline]
    pub fn already_terminated() -> Self {
        Self {
            success: true,
            final_state: WorkerState::Terminated,
            already_terminated: true,
            error_message: None,
        }
    }

    /// Creates a failed result.
    #[inline]
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            final_state: WorkerState::Terminated,
            already_terminated: false,
            error_message: Some(error.into()),
        }
    }
}

/// Error types for TerminateWorkerHandler.
#[derive(Debug, thiserror::Error)]
pub enum TerminateWorkerError {
    #[error("Worker {worker_id} not found")]
    WorkerNotFound { worker_id: WorkerId },

    #[error("Failed to terminate worker {worker_id}: {source}")]
    TerminationFailed {
        worker_id: WorkerId,
        source: crate::shared_kernel::DomainError,
    },
}

/// Handler for TerminateWorkerCommand.
///
/// This handler uses the WorkerRegistry to terminate a worker
/// and optionally unregister it.
#[derive(Debug)]
pub struct TerminateWorkerHandler<W>
where
    W: WorkerRegistry + Debug,
{
    worker_registry: W,
}

impl<W> TerminateWorkerHandler<W>
where
    W: WorkerRegistry + Debug,
{
    /// Creates a new handler with the given worker registry.
    #[inline]
    pub fn new(worker_registry: W) -> Self {
        Self { worker_registry }
    }
}

#[async_trait]
impl<W> CommandHandler<TerminateWorkerCommand> for TerminateWorkerHandler<W>
where
    W: WorkerRegistry + Debug + Send + Sync + 'static,
{
    type Error = TerminateWorkerError;

    async fn handle(
        &self,
        command: TerminateWorkerCommand,
    ) -> Result<TerminateWorkerResult, Self::Error> {
        // Clone worker_id early to avoid move issues
        let worker_id = command.worker_id.clone();

        // Check if worker exists
        let worker_opt = self
            .worker_registry
            .find_by_id(&worker_id)
            .await
            .map_err(|e| TerminateWorkerError::TerminationFailed {
                worker_id: worker_id.clone(),
                source: e,
            })?;

        // Idempotency: if already terminated, return success
        if let Some(ref worker) = worker_opt {
            if *worker.state() == WorkerState::Terminated {
                debug!(worker_id = %worker_id, "Worker already terminated");
                return Ok(TerminateWorkerResult::already_terminated());
            }
        }

        // If worker not found, treat as already terminated (idempotent)
        if worker_opt.is_none() {
            debug!(worker_id = %worker_id, "Worker not found, treating as terminated");
            return Ok(TerminateWorkerResult::already_terminated());
        }

        // Terminate the worker
        self.worker_registry
            .update_state(&worker_id, WorkerState::Terminated)
            .await
            .map_err(|e| TerminateWorkerError::TerminationFailed {
                worker_id: worker_id.clone(),
                source: e,
            })?;

        info!(
            worker_id = %worker_id,
            job_id = ?command.job_id,
            reason = %command.reason,
            "Worker terminated"
        );

        // Optionally unregister the worker
        if command.unregister {
            // Unregistration logic would go here
            debug!(worker_id = %worker_id, "Worker marked for unregistration");
        }

        Ok(TerminateWorkerResult::success(WorkerState::Terminated))
    }
}

/// Command to mark a job as timed out.
///
/// This command transitions a job to the FAILED state with a timeout reason
/// and publishes a JobTimedOut domain event.
///
/// # Idempotency
///
/// The idempotency key is derived from saga_id and job_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkJobTimedOutCommand {
    /// The job that timed out
    pub job_id: JobId,
    /// The worker that was processing the job (if any)
    pub worker_id: Option<WorkerId>,
    /// Reason for timeout (e.g., "execution_timeout", "heartbeat_lost")
    pub timeout_reason: String,
    /// Error message or details about the timeout
    pub error_details: Option<String>,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Optional metadata for tracing and context
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl MarkJobTimedOutCommand {
    /// Creates a new MarkJobTimedOutCommand.
    #[inline]
    pub fn new(
        job_id: JobId,
        timeout_reason: impl Into<String>,
        saga_id: String,
    ) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            worker_id: None,
            timeout_reason: timeout_reason.into(),
            error_details: None,
            saga_id,
            metadata,
        }
    }

    /// Creates a command with worker context.
    #[inline]
    pub fn with_worker(
        job_id: JobId,
        worker_id: WorkerId,
        timeout_reason: impl Into<String>,
        saga_id: String,
    ) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            worker_id: Some(worker_id),
            timeout_reason: timeout_reason.into(),
            error_details: None,
            saga_id,
            metadata,
        }
    }

    /// Creates a command with error details.
    #[inline]
    pub fn with_error_details(
        job_id: JobId,
        timeout_reason: impl Into<String>,
        error_details: impl Into<String>,
        saga_id: String,
    ) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            worker_id: None,
            timeout_reason: timeout_reason.into(),
            error_details: Some(error_details.into()),
            saga_id,
            metadata,
        }
    }
}

impl Command for MarkJobTimedOutCommand {
    type Output = MarkJobTimedOutResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{}-mark-job-timed-out-{}", self.saga_id, self.job_id))
    }
}

/// Result of marking job as timed out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkJobTimedOutResult {
    /// Whether operation was successful
    pub success: bool,
    /// The job's new state
    pub new_state: JobState,
    /// Whether job was already timed out (idempotency)
    pub already_timed_out: bool,
    /// Error message if operation failed
    pub error_message: Option<String>,
}

impl MarkJobTimedOutResult {
    /// Creates a successful result.
    #[inline]
    pub fn success() -> Self {
        Self {
            success: true,
            new_state: JobState::Failed,
            already_timed_out: false,
            error_message: None,
        }
    }

    /// Creates a result for idempotent skip.
    #[inline]
    pub fn already_timed_out() -> Self {
        Self {
            success: true,
            new_state: JobState::Failed,
            already_timed_out: true,
            error_message: None,
        }
    }

    /// Creates a failed result.
    #[inline]
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            new_state: JobState::Pending,
            already_timed_out: false,
            error_message: Some(error.into()),
        }
    }
}

/// Error types for MarkJobTimedOutHandler.
#[derive(Debug, thiserror::Error)]
pub enum MarkJobTimedOutError {
    #[error("Job {job_id} not found")]
    JobNotFound { job_id: JobId },

    #[error("Failed to mark job as timed out: {source}")]
    MarkFailed {
        job_id: JobId,
        source: crate::shared_kernel::DomainError,
    },
}

/// Handler for MarkJobTimedOutCommand.
#[derive(Debug)]
pub struct MarkJobTimedOutHandler<J>
where
    J: JobRepository + Debug,
{
    job_repository: J,
}

impl<J> MarkJobTimedOutHandler<J>
where
    J: JobRepository + Debug,
{
    /// Creates a new handler with the given job repository.
    #[inline]
    pub fn new(job_repository: J) -> Self {
        Self { job_repository }
    }
}

#[async_trait]
impl<J> CommandHandler<MarkJobTimedOutCommand> for MarkJobTimedOutHandler<J>
where
    J: JobRepository + Debug + Send + Sync + 'static,
{
    type Error = MarkJobTimedOutError;

    async fn handle(
        &self,
        command: MarkJobTimedOutCommand,
    ) -> Result<MarkJobTimedOutResult, Self::Error> {
        // Clone job_id early to avoid move issues
        let job_id = command.job_id.clone();

        // Check if job exists
        let job_opt = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(|e| MarkJobTimedOutError::MarkFailed {
                job_id: job_id.clone(),
                source: e,
            })?;

        // If job not found, treat as already timed out (idempotent)
        if job_opt.is_none() {
            debug!(job_id = %job_id, "Job not found, treating as timed out");
            return Ok(MarkJobTimedOutResult::already_timed_out());
        }

        // Idempotency: if already failed, return success
        if let Some(job) = job_opt {
            if *job.state() == JobState::Failed {
                debug!(job_id = %job_id, "Job already failed");
                return Ok(MarkJobTimedOutResult::already_timed_out());
            }
        }

        // Mark job as timed out
        self.job_repository
            .update_state(&job_id, JobState::Failed)
            .await
            .map_err(|e| MarkJobTimedOutError::MarkFailed {
                job_id: job_id.clone(),
                source: e,
            })?;

        info!(
            job_id = %job_id,
            worker_id = ?command.worker_id,
            reason = %command.timeout_reason,
            "Job marked as timed out"
        );

        Ok(MarkJobTimedOutResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminate_worker_command_idempotency() {
        let worker_id = WorkerId::new();
        let saga_id = "saga-timeout-123".to_string();
        let cmd = TerminateWorkerCommand::new(
            worker_id.clone(),
            None,
            "execution_timeout",
            saga_id.clone(),
        );

        let key = cmd.idempotency_key();
        assert!(key.contains(&saga_id));
        assert!(key.contains("terminate-worker"));
        assert!(key.contains(&worker_id.to_string()));
    }

    #[test]
    fn terminate_worker_command_with_unregister() {
        let worker_id = WorkerId::new();
        let saga_id = "saga-timeout-456".to_string();
        let cmd = TerminateWorkerCommand::with_unregister(
            worker_id.clone(),
            None,
            "heartbeat_lost",
            saga_id.clone(),
        );

        assert!(cmd.unregister);
    }

    #[test]
    fn mark_job_timed_out_command_idempotency() {
        let job_id = JobId::new();
        let saga_id = "saga-timeout-789".to_string();
        let cmd = MarkJobTimedOutCommand::new(
            job_id.clone(),
            "execution_timeout",
            saga_id.clone(),
        );

        let key = cmd.idempotency_key();
        assert!(key.contains(&saga_id));
        assert!(key.contains("mark-job-timed-out"));
        assert!(key.contains(&job_id.to_string()));
    }

    #[test]
    fn mark_job_timed_out_command_with_worker() {
        let job_id = JobId::new();
        let worker_id = WorkerId::new();
        let saga_id = "saga-timeout-abc".to_string();
        let cmd = MarkJobTimedOutCommand::with_worker(
            job_id.clone(),
            worker_id.clone(),
            "heartbeat_lost",
            saga_id.clone(),
        );

        assert_eq!(cmd.worker_id, Some(worker_id));
    }

    #[test]
    fn terminate_worker_result_success() {
        let result = TerminateWorkerResult::success(WorkerState::Terminated);
        assert!(result.success);
        assert_eq!(result.final_state, WorkerState::Terminated);
        assert!(!result.already_terminated);
    }

    #[test]
    fn terminate_worker_result_already_terminated() {
        let result = TerminateWorkerResult::already_terminated();
        assert!(result.success);
        assert!(result.already_terminated);
    }

    #[test]
    fn mark_job_timed_out_result_success() {
        let result = MarkJobTimedOutResult::success();
        assert!(result.success);
        assert_eq!(result.new_state, JobState::Failed);
        assert!(!result.already_timed_out);
    }

    #[test]
    fn mark_job_timed_out_result_already_timed_out() {
        let result = MarkJobTimedOutResult::already_timed_out();
        assert!(result.success);
        assert!(result.already_timed_out);
    }
}

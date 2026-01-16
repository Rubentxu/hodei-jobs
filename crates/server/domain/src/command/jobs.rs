// Job Commands Module
//
// Command types for job lifecycle management via Command Bus pattern.

use crate::command::{Command, CommandHandler, CommandMetadataDefault};
use crate::jobs::JobRepository;
use crate::shared_kernel::{JobId, JobState};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::Debug;

/// Command to mark a job as failed.
///
/// This command is used when a job needs to be marked as failed
/// with a specific reason. It validates the state transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkJobFailedCommand {
    /// The job ID to mark as failed
    pub job_id: JobId,
    /// The reason for failure
    pub reason: String,
    /// Optional metadata for tracing
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl MarkJobFailedCommand {
    /// Creates a new MarkJobFailedCommand.
    #[inline]
    pub fn new(job_id: JobId, reason: impl Into<String>) -> Self {
        let metadata = CommandMetadataDefault::new();
        Self {
            job_id,
            reason: reason.into(),
            metadata,
        }
    }
}

impl Command for MarkJobFailedCommand {
    type Output = JobFailureResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!("mark-job-failed-{}", self.job_id))
    }
}

/// Result of marking a job as failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobFailureResult {
    /// Whether the operation was successful
    pub success: bool,
    /// The job's previous state
    pub previous_state: JobState,
    /// The job's new state (should be Failed)
    pub new_state: JobState,
    /// Error message if failed
    pub error_message: Option<String>,
}

impl JobFailureResult {
    /// Creates a successful result.
    #[inline]
    pub fn success(previous: JobState) -> Self {
        Self {
            success: true,
            previous_state: previous,
            new_state: JobState::Failed,
            error_message: None,
        }
    }

    /// Creates a failed result.
    #[inline]
    pub fn failure(previous: JobState, message: impl Into<String>) -> Self {
        Self {
            success: false,
            previous_state: previous.clone(),
            new_state: previous,
            error_message: Some(message.into()),
        }
    }
}

/// Error types for MarkJobFailedHandler.
#[derive(Debug, thiserror::Error)]
pub enum MarkJobFailedError {
    #[error("Job {job_id} not found")]
    JobNotFound { job_id: JobId },

    #[error("Cannot transition job {job_id} from {current} to Failed")]
    InvalidTransition { job_id: JobId, current: JobState },

    #[error("Job {job_id} is already in terminal state: {state}")]
    TerminalState { job_id: JobId, state: JobState },
}

/// Handler for MarkJobFailedCommand.
#[derive(Debug)]
pub struct MarkJobFailedHandler<J>
where
    J: JobRepository + Debug,
{
    job_repository: J,
}

impl<J> MarkJobFailedHandler<J>
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
impl<J> CommandHandler<MarkJobFailedCommand> for MarkJobFailedHandler<J>
where
    J: JobRepository + Debug + Send + Sync + 'static,
{
    type Error = MarkJobFailedError;

    async fn handle(
        &self,
        command: MarkJobFailedCommand,
    ) -> Result<JobFailureResult, Self::Error> {
        // Clone job_id since we need it in multiple closures
        let job_id = command.job_id.clone();

        // Get the job
        let job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(|_| MarkJobFailedError::JobNotFound {
                job_id: job_id.clone(),
            })?
            .ok_or_else(|| MarkJobFailedError::JobNotFound {
                job_id: job_id.clone(),
            })?;

        let previous_state = job.state().clone();

        // Check if already in terminal state
        if previous_state.is_terminal() {
            return Err(MarkJobFailedError::TerminalState {
                job_id: job_id.clone(),
                state: previous_state.clone(),
            });
        }

        // Validate transition to Failed
        if !previous_state.can_transition_to(&JobState::Failed) {
            return Err(MarkJobFailedError::InvalidTransition {
                job_id: job_id.clone(),
                current: previous_state.clone(),
            });
        }

        // Update job state
        self.job_repository
            .update_state(&job_id, JobState::Failed)
            .await
            .map_err(|_e| MarkJobFailedError::InvalidTransition {
                job_id: job_id.clone(),
                current: previous_state.clone(),
            })?;

        Ok(JobFailureResult::success(previous_state))
    }
}

/// Command to resume a job from ManualInterventionRequired state.
///
/// This command allows an admin to resume a job that was
/// stuck in ManualInterventionRequired state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeFromManualInterventionCommand {
    /// The job ID to resume
    pub job_id: JobId,
    /// Optional reason for the resume (for audit)
    pub resume_reason: Option<String>,
    /// Optional metadata for tracing
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl ResumeFromManualInterventionCommand {
    /// Creates a new ResumeFromManualInterventionCommand.
    #[inline]
    pub fn new(job_id: JobId) -> Self {
        let metadata = CommandMetadataDefault::new();
        Self {
            job_id,
            resume_reason: None,
            metadata,
        }
    }

    /// Creates a command with a reason.
    #[inline]
    pub fn with_reason(job_id: JobId, reason: impl Into<String>) -> Self {
        let metadata = CommandMetadataDefault::new();
        Self {
            job_id,
            resume_reason: Some(reason.into()),
            metadata,
        }
    }
}

impl Command for ResumeFromManualInterventionCommand {
    type Output = ResumeFromManualInterventionResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!("resume-from-manual-intervention-{}", self.job_id))
    }
}

/// Result of resuming from manual intervention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeFromManualInterventionResult {
    /// Whether the operation was successful
    pub success: bool,
    /// The job's previous state
    pub previous_state: JobState,
    /// The job's new state (should be Pending)
    pub new_state: JobState,
    /// Error message if failed
    pub error_message: Option<String>,
}

impl ResumeFromManualInterventionResult {
    /// Creates a successful result.
    #[inline]
    pub fn success(previous: JobState) -> Self {
        Self {
            success: true,
            previous_state: previous,
            new_state: JobState::Pending,
            error_message: None,
        }
    }

    /// Creates a failed result.
    #[inline]
    pub fn failure(previous: JobState, message: impl Into<String>) -> Self {
        Self {
            success: false,
            previous_state: previous.clone(),
            new_state: previous,
            error_message: Some(message.into()),
        }
    }
}

/// Error types for ResumeFromManualInterventionHandler.
#[derive(Debug, thiserror::Error)]
pub enum ResumeFromManualInterventionError {
    #[error("Job {job_id} not found")]
    JobNotFound { job_id: JobId },

    #[error("Job {job_id} is not in ManualInterventionRequired state (current: {current})")]
    NotInManualInterventionState { job_id: JobId, current: JobState },
}

/// Handler for ResumeFromManualInterventionCommand.
#[derive(Debug)]
pub struct ResumeFromManualInterventionHandler<J>
where
    J: JobRepository + Debug,
{
    job_repository: J,
}

impl<J> ResumeFromManualInterventionHandler<J>
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
impl<J> CommandHandler<ResumeFromManualInterventionCommand>
    for ResumeFromManualInterventionHandler<J>
where
    J: JobRepository + Debug + Send + Sync + 'static,
{
    type Error = ResumeFromManualInterventionError;

    async fn handle(
        &self,
        command: ResumeFromManualInterventionCommand,
    ) -> Result<ResumeFromManualInterventionResult, Self::Error> {
        // Clone job_id since we need it in multiple closures
        let job_id = command.job_id.clone();

        // Get the job
        let job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(|_| ResumeFromManualInterventionError::JobNotFound {
                job_id: job_id.clone(),
            })?
            .ok_or_else(|| ResumeFromManualInterventionError::JobNotFound {
                job_id: job_id.clone(),
            })?;

        let previous_state = job.state().clone();

        // Check if in ManualInterventionRequired state
        if !previous_state.requires_manual_intervention() {
            return Err(ResumeFromManualInterventionError::NotInManualInterventionState {
                job_id: job_id.clone(),
                current: previous_state.clone(),
            });
        }

        // Transition back to Pending
        self.job_repository
            .update_state(&job_id, JobState::Pending)
            .await
            .map_err(|_e| ResumeFromManualInterventionError::NotInManualInterventionState {
                job_id: job_id.clone(),
                current: previous_state.clone(),
            })?;

        Ok(ResumeFromManualInterventionResult::success(previous_state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_kernel::JobId;

    #[tokio::test]
    async fn mark_job_failed_command_idempotency() {
        let job_id = JobId::new();
        let cmd = MarkJobFailedCommand::new(job_id.clone(), "Test failure");

        let key = cmd.idempotency_key();
        assert!(key.contains(&job_id.to_string()));
    }

    #[tokio::test]
    async fn resume_from_manual_intervention_command_idempotency() {
        let job_id = JobId::new();
        let cmd = ResumeFromManualInterventionCommand::new(job_id.clone());

        let key = cmd.idempotency_key();
        assert!(key.contains(&job_id.to_string()));
    }

    #[tokio::test]
    async fn resume_with_reason() {
        let job_id = JobId::new();
        let cmd = ResumeFromManualInterventionCommand::with_reason(
            job_id.clone(),
            "Admin manually reviewed and approved",
        );

        assert_eq!(cmd.resume_reason, Some("Admin manually reviewed and approved".to_string()));
    }

    #[tokio::test]
    async fn job_failure_result_success() {
        let result = JobFailureResult::success(JobState::Running);
        assert!(result.success);
        assert_eq!(result.previous_state, JobState::Running);
        assert_eq!(result.new_state, JobState::Failed);
    }

    #[tokio::test]
    async fn job_failure_result_failure() {
        let result = JobFailureResult::failure(JobState::Timeout, "Invalid transition");
        assert!(!result.success);
        assert_eq!(result.previous_state, JobState::Timeout);
        assert!(result.error_message.is_some());
    }

    #[tokio::test]
    async fn job_failure_result_invalid_transition() {
        let result = JobFailureResult::failure(JobState::Succeeded, "Cannot transition from terminal");
        assert!(!result.success);
        assert_eq!(result.previous_state, JobState::Succeeded);
    }

    #[tokio::test]
    async fn resume_result_success() {
        let result = ResumeFromManualInterventionResult::success(JobState::ManualInterventionRequired);
        assert!(result.success);
        assert_eq!(result.previous_state, JobState::ManualInterventionRequired);
        assert_eq!(result.new_state, JobState::Pending);
    }

    #[tokio::test]
    async fn resume_result_failure() {
        let result = ResumeFromManualInterventionResult::failure(JobState::Running, "Not in manual intervention");
        assert!(!result.success);
        assert_eq!(result.previous_state, JobState::Running);
        assert!(result.error_message.is_some());
    }
}

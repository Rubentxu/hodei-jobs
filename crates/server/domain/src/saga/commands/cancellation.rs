// Cancellation Saga Commands
//
// Commands used by the CancellationSaga for job cancellation operations.
// These commands encapsulate the intent to notify workers, update job states,
// and release workers during cancellation workflows.
//
// This module implements the Command Bus pattern for cancellation operations,
// enabling saga steps to dispatch commands that are handled by domain services.

use crate::command::{Command, CommandHandler, CommandMetadataDefault};
use crate::jobs::JobRepository;
use crate::shared_kernel::{JobId, JobState, WorkerId, WorkerState};
use crate::workers::WorkerRegistry;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::Debug;
use std::sync::Arc;
use tracing::{debug, info};

/// Command to notify a worker that it should stop execution.
///
/// This command sends a cancellation signal to the worker processing a job.
/// The actual mechanism (gRPC, NATS, etc.) is delegated to the handler.
///
/// # Idempotency
///
/// The idempotency key is derived from saga_id and job_id, ensuring that
/// retrying a cancellation saga will not send duplicate notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyWorkerCommand {
    /// The job being cancelled
    pub job_id: JobId,
    /// The worker to notify (optional, will be looked up if not provided)
    pub worker_id: Option<WorkerId>,
    /// Reason for cancellation
    pub reason: String,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Optional metadata for tracing and context
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl NotifyWorkerCommand {
    /// Creates a new NotifyWorkerCommand.
    #[inline]
    pub fn new(
        job_id: JobId,
        worker_id: Option<WorkerId>,
        reason: impl Into<String>,
        saga_id: String,
    ) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            worker_id,
            reason: reason.into(),
            saga_id,
            metadata,
        }
    }

    /// Creates a command with custom metadata.
    #[inline]
    pub fn with_metadata(
        job_id: JobId,
        worker_id: Option<WorkerId>,
        reason: impl Into<String>,
        saga_id: String,
        metadata: CommandMetadataDefault,
    ) -> Self {
        Self {
            job_id,
            worker_id,
            reason: reason.into(),
            saga_id,
            metadata,
        }
    }
}

impl Command for NotifyWorkerCommand {
    type Output = NotifyWorkerResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        match &self.worker_id {
            Some(wid) => Cow::Owned(format!(
                "{}-notify-worker-{}-{}",
                self.saga_id, self.job_id, wid
            )),
            None => Cow::Owned(format!("{}-notify-worker-{}", self.saga_id, self.job_id)),
        }
    }
}

/// Result of worker notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyWorkerResult {
    /// Whether notification was successful
    pub success: bool,
    /// The worker ID that was notified (resolved if not provided)
    pub worker_id: Option<WorkerId>,
    /// Whether the worker was already notified (idempotency)
    pub already_notified: bool,
    /// Error message if notification failed
    pub error_message: Option<String>,
}

impl NotifyWorkerResult {
    /// Creates a successful result.
    #[inline]
    pub fn success(worker_id: Option<WorkerId>) -> Self {
        Self {
            success: true,
            worker_id,
            already_notified: false,
            error_message: None,
        }
    }

    /// Creates a result indicating idempotent skip.
    #[inline]
    pub fn already_notified(worker_id: WorkerId) -> Self {
        Self {
            success: true,
            worker_id: Some(worker_id),
            already_notified: true,
            error_message: None,
        }
    }

    /// Creates a failed result.
    #[inline]
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            worker_id: None,
            already_notified: false,
            error_message: Some(error.into()),
        }
    }
}

/// Error types for NotifyWorkerHandler.
#[derive(Debug, thiserror::Error)]
pub enum NotifyWorkerError {
    #[error("No worker assigned to job {job_id}")]
    NoWorkerAssigned { job_id: JobId },

    #[error("Worker {worker_id} not found")]
    WorkerNotFound { worker_id: WorkerId },

    #[error("Failed to notify worker {worker_id}: {source}")]
    NotificationFailed {
        worker_id: WorkerId,
        source: crate::shared_kernel::DomainError,
    },
}

/// Handler for NotifyWorkerCommand.
///
/// This handler uses the WorkerRegistry to find the worker for a job
/// and dispatches the cancellation notification.
#[derive(Debug)]
pub struct NotifyWorkerHandler<W>
where
    W: WorkerRegistry + Debug,
{
    worker_registry: W,
}

impl<W> NotifyWorkerHandler<W>
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
impl<W> CommandHandler<NotifyWorkerCommand> for NotifyWorkerHandler<W>
where
    W: WorkerRegistry + Debug + Send + Sync + 'static,
{
    type Error = NotifyWorkerError;

    async fn handle(
        &self,
        command: NotifyWorkerCommand,
    ) -> Result<NotifyWorkerResult, Self::Error> {
        // Resolve worker ID if not provided - clone values before move
        let job_id = command.job_id.clone();
        let worker_id_opt = command.worker_id.clone();

        let worker_id = match worker_id_opt {
            Some(id) => id,
            None => {
                let worker = self
                    .worker_registry
                    .get_by_job_id(&job_id)
                    .await
                    .map_err(|e| NotifyWorkerError::NotificationFailed {
                        worker_id: WorkerId::new(),
                        source: e,
                    })?
                    .ok_or_else(|| NotifyWorkerError::NoWorkerAssigned {
                        job_id: job_id.clone(),
                    })?;
                worker.id().clone()
            }
        };

        // In a real implementation, this would send an actual signal to the worker
        // via gRPC stream termination or NATS message. For now, we log the intent.
        info!(
            job_id = %job_id,
            worker_id = %worker_id,
            reason = %command.reason,
            "Worker notified of cancellation"
        );

        Ok(NotifyWorkerResult::success(Some(worker_id)))
    }
}

/// Concrete handler for NotifyWorkerCommand using Arc<dyn WorkerRegistry>.
pub struct GenericNotifyWorkerHandler {
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl GenericNotifyWorkerHandler {
    /// Creates a new handler with the given worker registry.
    #[inline]
    pub fn new(worker_registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { worker_registry }
    }
}

impl std::fmt::Debug for GenericNotifyWorkerHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GenericNotifyWorkerHandler")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl CommandHandler<NotifyWorkerCommand> for GenericNotifyWorkerHandler {
    type Error = NotifyWorkerError;

    async fn handle(
        &self,
        command: NotifyWorkerCommand,
    ) -> Result<NotifyWorkerResult, Self::Error> {
        let job_id = command.job_id.clone();
        let worker_id_opt = command.worker_id.clone();

        let worker_id = match worker_id_opt {
            Some(id) => id,
            None => {
                let worker = self
                    .worker_registry
                    .get_by_job_id(&job_id)
                    .await
                    .map_err(|e| NotifyWorkerError::NotificationFailed {
                        worker_id: WorkerId::new(),
                        source: e,
                    })?
                    .ok_or_else(|| NotifyWorkerError::NoWorkerAssigned {
                        job_id: job_id.clone(),
                    })?;
                worker.id().clone()
            }
        };

        info!(
            job_id = %job_id,
            worker_id = %worker_id,
            reason = %command.reason,
            "Worker notified of cancellation"
        );

        Ok(NotifyWorkerResult::success(Some(worker_id)))
    }
}

/// Command to update a job's state.
///
/// This command encapsulates the intent to transition a job to a new state,
/// typically to `Cancelled` during a cancellation saga.
///
/// # Idempotency
///
/// The idempotency key is derived from saga_id and job_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateJobStateCommand {
    /// The job to update
    pub job_id: JobId,
    /// The target state
    pub target_state: JobState,
    /// Optional original state for compensation
    pub original_state: Option<JobState>,
    /// Reason for state change (for event)
    pub reason: Option<String>,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Whether to publish a domain event
    pub publish_event: bool,
    /// Optional metadata for tracing and context
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl UpdateJobStateCommand {
    /// Creates a command to cancel a job.
    #[inline]
    pub fn cancel(job_id: JobId, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            target_state: JobState::Cancelled,
            original_state: None,
            reason: Some("user_requested".to_string()),
            saga_id,
            publish_event: true,
            metadata,
        }
    }

    /// Creates a new UpdateJobStateCommand.
    #[inline]
    pub fn new(job_id: JobId, target_state: JobState, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            target_state,
            original_state: None,
            reason: None,
            saga_id,
            publish_event: false,
            metadata,
        }
    }

    /// Creates a command with original state for compensation.
    #[inline]
    pub fn with_original_state(
        job_id: JobId,
        target_state: JobState,
        original_state: JobState,
        saga_id: String,
    ) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            target_state,
            original_state: Some(original_state),
            reason: None,
            saga_id,
            publish_event: false,
            metadata,
        }
    }
}

impl Command for UpdateJobStateCommand {
    type Output = UpdateJobStateResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!(
            "{}-update-job-state-{}-{:?}",
            self.saga_id, self.job_id, self.target_state
        ))
    }
}

/// Result of job state update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateJobStateResult {
    /// Whether update was successful
    pub success: bool,
    /// The new state of the job
    pub new_state: JobState,
    /// Whether update was idempotent skip
    pub already_in_state: bool,
    /// Error message if update failed
    pub error_message: Option<String>,
}

impl UpdateJobStateResult {
    /// Creates a successful result.
    #[inline]
    pub fn success(state: JobState) -> Self {
        Self {
            success: true,
            new_state: state,
            already_in_state: false,
            error_message: None,
        }
    }

    /// Creates a result for idempotent skip.
    #[inline]
    pub fn already_in_state(state: JobState) -> Self {
        Self {
            success: true,
            new_state: state,
            already_in_state: true,
            error_message: None,
        }
    }

    /// Creates a failed result.
    #[inline]
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            new_state: JobState::Pending,
            already_in_state: false,
            error_message: Some(error.into()),
        }
    }
}

/// Error types for UpdateJobStateHandler.
#[derive(Debug, thiserror::Error)]
pub enum UpdateJobStateError {
    #[error("Job {job_id} not found")]
    JobNotFound { job_id: JobId },

    #[error("Failed to update job state: {source}")]
    UpdateFailed {
        job_id: JobId,
        source: crate::shared_kernel::DomainError,
    },
}

/// Handler for UpdateJobStateCommand.
pub struct UpdateJobStateHandler {
    job_repository: Arc<dyn JobRepository + Send + Sync>,
}

impl UpdateJobStateHandler {
    /// Creates a new handler with the given job repository.
    #[inline]
    pub fn new(job_repository: Arc<dyn JobRepository + Send + Sync>) -> Self {
        Self { job_repository }
    }
}

impl std::fmt::Debug for UpdateJobStateHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateJobStateHandler")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl CommandHandler<UpdateJobStateCommand> for UpdateJobStateHandler {
    type Error = UpdateJobStateError;

    async fn handle(
        &self,
        command: UpdateJobStateCommand,
    ) -> Result<UpdateJobStateResult, Self::Error> {
        // Check if already in target state (idempotency)
        // Clone job_id early to avoid move issues
        let job_id = command.job_id.clone();
        let target_state = command.target_state.clone();

        if let Some(job) = self.job_repository.find_by_id(&job_id).await.map_err(|e| {
            UpdateJobStateError::UpdateFailed {
                job_id: job_id.clone(),
                source: e,
            }
        })? && *job.state() == target_state
        {
            return Ok(UpdateJobStateResult::already_in_state(target_state));
        }

        // Update the job state
        self.job_repository
            .update_state(&job_id, target_state.clone())
            .await
            .map_err(|e| UpdateJobStateError::UpdateFailed {
                job_id: job_id.clone(),
                source: e,
            })?;

        Ok(UpdateJobStateResult::success(target_state))
    }
}

/// Command to release a worker back to the pool.
///
/// This command transitions a worker from its current state (typically Busy)
/// back to Ready, making it available for new jobs.
///
/// # Idempotency
///
/// Release commands are inherently idempotent - releasing an already-ready
/// worker should succeed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseWorkerCommand {
    /// The worker to release
    pub worker_id: WorkerId,
    /// The job that was being processed (for logging/tracing)
    pub job_id: Option<JobId>,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Optional metadata for tracing and context
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl ReleaseWorkerCommand {
    /// Creates a new ReleaseWorkerCommand.
    #[inline]
    pub fn new(worker_id: WorkerId, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            worker_id,
            job_id: None,
            saga_id,
            metadata,
        }
    }

    /// Creates a command with job context.
    #[inline]
    pub fn with_job(worker_id: WorkerId, job_id: JobId, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            worker_id,
            job_id: Some(job_id),
            saga_id,
            metadata,
        }
    }
}

impl Command for ReleaseWorkerCommand {
    type Output = ReleaseWorkerResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!(
            "{}-release-worker-{}",
            self.saga_id, self.worker_id
        ))
    }
}

/// Result of worker release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseWorkerResult {
    /// Whether release was successful
    pub success: bool,
    /// The worker's new state
    pub new_state: WorkerState,
    /// Error message if release failed
    pub error_message: Option<String>,
}

impl ReleaseWorkerResult {
    /// Creates a successful result.
    #[inline]
    pub fn success() -> Self {
        Self {
            success: true,
            new_state: WorkerState::Ready,
            error_message: None,
        }
    }

    /// Creates a failed result.
    #[inline]
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            new_state: WorkerState::Terminated,
            error_message: Some(error.into()),
        }
    }
}

/// Error types for ReleaseWorkerHandler.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseWorkerError {
    #[error("Worker {worker_id} not found")]
    WorkerNotFound { worker_id: WorkerId },

    #[error("Failed to release worker {worker_id}: {source}")]
    ReleaseFailed {
        worker_id: WorkerId,
        source: crate::shared_kernel::DomainError,
    },
}

/// Handler for ReleaseWorkerCommand.
pub struct ReleaseWorkerHandler {
    worker_registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl ReleaseWorkerHandler {
    /// Creates a new handler with the given worker registry.
    #[inline]
    pub fn new(worker_registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { worker_registry }
    }
}

impl std::fmt::Debug for ReleaseWorkerHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReleaseWorkerHandler")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl CommandHandler<ReleaseWorkerCommand> for ReleaseWorkerHandler {
    type Error = ReleaseWorkerError;

    async fn handle(
        &self,
        command: ReleaseWorkerCommand,
    ) -> Result<ReleaseWorkerResult, Self::Error> {
        // Clone worker_id early to avoid move issues
        let worker_id = command.worker_id.clone();

        // Verify worker exists
        self.worker_registry
            .find_by_id(&worker_id)
            .await
            .map_err(|e| ReleaseWorkerError::ReleaseFailed {
                worker_id: worker_id.clone(),
                source: e,
            })?
            .ok_or_else(|| ReleaseWorkerError::WorkerNotFound {
                worker_id: worker_id.clone(),
            })?;

        // Release worker to ready state
        self.worker_registry
            .update_state(&worker_id, WorkerState::Ready)
            .await
            .map_err(|e| ReleaseWorkerError::ReleaseFailed {
                worker_id: worker_id.clone(),
                source: e,
            })?;

        debug!(worker_id = %worker_id, "Worker released to Ready state");

        Ok(ReleaseWorkerResult::success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_kernel::{JobId, WorkerId};

    #[test]
    fn notify_worker_command_idempotency_with_worker_id() {
        let job_id = JobId::new();
        let worker_id = WorkerId::new();
        let saga_id = "saga-123".to_string();
        let cmd = NotifyWorkerCommand::new(
            job_id.clone(),
            Some(worker_id.clone()),
            "user_requested",
            saga_id.clone(),
        );

        let key = cmd.idempotency_key();
        assert!(key.contains(&saga_id));
        assert!(key.contains("notify-worker"));
        assert!(key.contains(&job_id.to_string()));
        assert!(key.contains(&worker_id.to_string()));
    }

    #[test]
    fn notify_worker_command_idempotency_without_worker_id() {
        let job_id = JobId::new();
        let saga_id = "saga-456".to_string();
        let cmd = NotifyWorkerCommand::new(job_id.clone(), None, "timeout", saga_id.clone());

        let key = cmd.idempotency_key();
        assert!(key.contains(&saga_id));
        assert!(key.contains("notify-worker"));
        assert!(key.contains(&job_id.to_string()));
    }

    #[test]
    fn update_job_state_command_cancel() {
        let job_id = JobId::new();
        let saga_id = "saga-789".to_string();
        let cmd = UpdateJobStateCommand::cancel(job_id.clone(), saga_id.clone());

        assert_eq!(cmd.target_state, JobState::Cancelled);
        assert!(cmd.publish_event);
        assert_eq!(cmd.reason, Some("user_requested".to_string()));
    }

    #[test]
    fn update_job_state_command_idempotency() {
        let job_id = JobId::new();
        let saga_id = "saga-abc".to_string();
        let cmd = UpdateJobStateCommand::new(job_id.clone(), JobState::Cancelled, saga_id.clone());

        let key = cmd.idempotency_key();
        assert!(key.contains(&saga_id));
        assert!(key.contains("update-job-state"));
        assert!(key.contains(&job_id.to_string()));
        assert!(key.contains("Cancelled"));
    }

    #[test]
    fn release_worker_command_idempotency() {
        let worker_id = WorkerId::new();
        let saga_id = "saga-release".to_string();
        let cmd = ReleaseWorkerCommand::new(worker_id.clone(), saga_id.clone());

        let key = cmd.idempotency_key();
        assert!(key.contains(&saga_id));
        assert!(key.contains("release-worker"));
        assert!(key.contains(&worker_id.to_string()));
    }

    #[test]
    fn release_worker_command_with_job() {
        let worker_id = WorkerId::new();
        let job_id = JobId::new();
        let saga_id = "saga-release-with-job".to_string();
        let cmd =
            ReleaseWorkerCommand::with_job(worker_id.clone(), job_id.clone(), saga_id.clone());

        assert_eq!(cmd.job_id, Some(job_id));
    }

    #[test]
    fn notify_worker_result_success() {
        let worker_id = WorkerId::new();
        let result = NotifyWorkerResult::success(Some(worker_id.clone()));
        assert!(result.success);
        assert_eq!(result.worker_id, Some(worker_id));
        assert!(!result.already_notified);
    }

    #[test]
    fn notify_worker_result_already_notified() {
        let worker_id = WorkerId::new();
        let result = NotifyWorkerResult::already_notified(worker_id.clone());
        assert!(result.success);
        assert_eq!(result.worker_id, Some(worker_id));
        assert!(result.already_notified);
    }

    #[test]
    fn notify_worker_result_failure() {
        let result = NotifyWorkerResult::failure("Connection refused");
        assert!(!result.success);
        assert_eq!(result.error_message, Some("Connection refused".to_string()));
    }

    #[test]
    fn update_job_state_result_success() {
        let result = UpdateJobStateResult::success(JobState::Cancelled);
        assert!(result.success);
        assert_eq!(result.new_state, JobState::Cancelled);
        assert!(!result.already_in_state);
    }

    #[test]
    fn update_job_state_result_already_in_state() {
        let result = UpdateJobStateResult::already_in_state(JobState::Cancelled);
        assert!(result.success);
        assert_eq!(result.new_state, JobState::Cancelled);
        assert!(result.already_in_state);
    }

    #[test]
    fn release_worker_result_success() {
        let result = ReleaseWorkerResult::success();
        assert!(result.success);
        assert_eq!(result.new_state, WorkerState::Ready);
    }

    #[test]
    fn release_worker_result_failure() {
        let result = ReleaseWorkerResult::failure("Worker not found");
        assert!(!result.success);
        assert_eq!(result.new_state, WorkerState::Terminated);
        assert_eq!(result.error_message, Some("Worker not found".to_string()));
    }
}

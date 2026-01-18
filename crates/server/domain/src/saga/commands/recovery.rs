// Recovery Saga Commands
//
// Commands used by the RecoverySaga for worker/job recovery operations.
// These commands encapsulate the intent to check connectivity, provision
// new workers, transfer jobs, and clean up old workers.

use crate::command::{Command, CommandHandler, CommandMetadataDefault};
use crate::jobs::JobRepository;
use crate::shared_kernel::{JobId, JobState, ProviderId, WorkerId, WorkerState};
use crate::workers::{WorkerProvisioning, WorkerRegistry};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::Debug;

/// Command to check connectivity to a worker.
///
/// This command verifies if a worker is still reachable and responsive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckConnectivityCommand {
    /// The worker ID to check
    pub worker_id: WorkerId,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Optional metadata for tracing
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl CheckConnectivityCommand {
    /// Creates a new CheckConnectivityCommand.
    #[inline]
    pub fn new(worker_id: WorkerId, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            worker_id,
            saga_id,
            metadata,
        }
    }
}

impl Command for CheckConnectivityCommand {
    type Output = CheckConnectivityResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!(
            "{}-check-connectivity-{}",
            self.saga_id, self.worker_id
        ))
    }
}

/// Result of connectivity check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckConnectivityResult {
    /// Whether the worker is reachable
    pub is_reachable: bool,
    /// Current state of the worker (if known)
    pub worker_state: Option<WorkerState>,
    /// Latency in milliseconds (if reachable)
    pub latency_ms: Option<u64>,
    /// Error message (if not reachable)
    pub error_message: Option<String>,
}

impl CheckConnectivityResult {
    /// Creates a successful connectivity result.
    #[inline]
    pub fn reachable(state: WorkerState, latency_ms: u64) -> Self {
        Self {
            is_reachable: true,
            worker_state: Some(state),
            latency_ms: Some(latency_ms),
            error_message: None,
        }
    }

    /// Creates an unreachable result.
    #[inline]
    pub fn unreachable(error: impl Into<String>) -> Self {
        Self {
            is_reachable: false,
            worker_state: None,
            latency_ms: None,
            error_message: Some(error.into()),
        }
    }
}

/// Error types for CheckConnectivityHandler.
#[derive(Debug, thiserror::Error)]
pub enum CheckConnectivityError {
    #[error("Worker {worker_id} not found")]
    WorkerNotFound { worker_id: WorkerId },

    #[error("Failed to check connectivity: {source}")]
    CheckFailed {
        worker_id: WorkerId,
        source: crate::shared_kernel::DomainError,
    },
}

/// Handler for CheckConnectivityCommand.
#[derive(Debug)]
pub struct CheckConnectivityHandler<W>
where
    W: WorkerRegistry + Debug,
{
    worker_repository: W,
}

impl<W> CheckConnectivityHandler<W>
where
    W: WorkerRegistry + Debug,
{
    /// Creates a new handler with the given worker repository.
    #[inline]
    pub fn new(worker_repository: W) -> Self {
        Self { worker_repository }
    }
}

#[async_trait]
impl<W> CommandHandler<CheckConnectivityCommand> for CheckConnectivityHandler<W>
where
    W: WorkerRegistry + Debug + Send + Sync + 'static,
{
    type Error = CheckConnectivityError;

    async fn handle(
        &self,
        command: CheckConnectivityCommand,
    ) -> Result<CheckConnectivityResult, Self::Error> {
        // Clone worker_id since we need it in closures
        let worker_id = command.worker_id.clone();

        // Get worker info
        let worker = self
            .worker_repository
            .find_by_id(&worker_id)
            .await
            .map_err(|e| CheckConnectivityError::CheckFailed {
                worker_id: worker_id.clone(),
                source: e,
            })?
            .ok_or_else(|| CheckConnectivityError::WorkerNotFound {
                worker_id: worker_id.clone(),
            })?;

        // For now, we just report the current state
        // In a real implementation, this would ping the worker
        Ok(CheckConnectivityResult::reachable(
            worker.state().clone(),
            0,
        ))
    }
}

/// Command to mark a job for recovery.
///
/// This command updates the job state to indicate it needs recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkJobForRecoveryCommand {
    /// The job ID to mark
    pub job_id: JobId,
    /// The old worker ID (no longer reachable)
    pub old_worker_id: WorkerId,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Reason for recovery
    #[serde(default)]
    pub recovery_reason: Option<String>,
    /// Optional metadata for tracing
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl MarkJobForRecoveryCommand {
    /// Creates a new MarkJobForRecoveryCommand.
    #[inline]
    pub fn new(job_id: JobId, old_worker_id: WorkerId, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            old_worker_id,
            saga_id,
            recovery_reason: None,
            metadata,
        }
    }

    /// Creates a command with a recovery reason.
    #[inline]
    pub fn with_reason(
        job_id: JobId,
        old_worker_id: WorkerId,
        saga_id: String,
        reason: impl Into<String>,
    ) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            old_worker_id,
            saga_id,
            recovery_reason: Some(reason.into()),
            metadata,
        }
    }
}

impl Command for MarkJobForRecoveryCommand {
    type Output = JobRecoveryMarkResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!(
            "{}-mark-recovery-{}-{}",
            self.saga_id, self.job_id, self.old_worker_id
        ))
    }
}

/// Result of marking job for recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRecoveryMarkResult {
    /// Whether marking was successful
    pub success: bool,
    /// The new state of the job
    pub new_state: JobState,
    /// Reason for failure (if any)
    pub failure_reason: Option<String>,
}

impl JobRecoveryMarkResult {
    /// Creates a successful result.
    #[inline]
    pub fn success(state: JobState) -> Self {
        Self {
            success: true,
            new_state: state,
            failure_reason: None,
        }
    }

    /// Creates a failed result.
    #[inline]
    pub fn failure(reason: impl Into<String>) -> Self {
        Self {
            success: false,
            new_state: JobState::Pending,
            failure_reason: Some(reason.into()),
        }
    }
}

/// Error types for MarkJobForRecoveryHandler.
#[derive(Debug, thiserror::Error)]
pub enum MarkJobForRecoveryError {
    #[error("Job {job_id} not found")]
    JobNotFound { job_id: JobId },

    #[error("Failed to mark job for recovery: {source}")]
    MarkFailed {
        job_id: JobId,
        source: crate::shared_kernel::DomainError,
    },
}

/// Handler for MarkJobForRecoveryCommand.
#[derive(Debug)]
pub struct MarkJobForRecoveryHandler<J>
where
    J: JobRepository + Debug,
{
    job_repository: J,
}

impl<J> MarkJobForRecoveryHandler<J>
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
impl<J> CommandHandler<MarkJobForRecoveryCommand> for MarkJobForRecoveryHandler<J>
where
    J: JobRepository + Debug + Send + Sync + 'static,
{
    type Error = MarkJobForRecoveryError;

    async fn handle(
        &self,
        command: MarkJobForRecoveryCommand,
    ) -> Result<JobRecoveryMarkResult, Self::Error> {
        // Clone job_id since we need it in closures
        let job_id = command.job_id.clone();

        // Get current job state
        let _job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(|e| MarkJobForRecoveryError::MarkFailed {
                job_id: job_id.clone(),
                source: e,
            })?
            .ok_or_else(|| MarkJobForRecoveryError::JobNotFound {
                job_id: job_id.clone(),
            })?;

        // Transition job back to Pending for recovery
        // This allows the scheduler to pick it up again
        self.job_repository
            .update_state(&job_id, JobState::Pending)
            .await
            .map_err(|e| MarkJobForRecoveryError::MarkFailed {
                job_id: job_id.clone(),
                source: e,
            })?;

        Ok(JobRecoveryMarkResult::success(JobState::Pending))
    }
}

/// Command to provision a new worker for recovery.
///
/// This command creates a new worker to replace the unreachable one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionNewWorkerCommand {
    /// The job this worker will serve
    pub job_id: JobId,
    /// The provider to use
    pub provider_id: ProviderId,
    /// The old worker ID (for reference)
    pub old_worker_id: WorkerId,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Optional metadata for tracing
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl ProvisionNewWorkerCommand {
    /// Creates a new ProvisionNewWorkerCommand.
    #[inline]
    pub fn new(
        job_id: JobId,
        provider_id: ProviderId,
        old_worker_id: WorkerId,
        saga_id: String,
    ) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            provider_id,
            old_worker_id,
            saga_id,
            metadata,
        }
    }
}

impl Command for ProvisionNewWorkerCommand {
    type Output = WorkerProvisioningResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!(
            "{}-provision-new-worker-{}-{}",
            self.saga_id, self.job_id, self.old_worker_id
        ))
    }
}

/// Result of provisioning a new worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerProvisioningResult {
    /// Whether provisioning was successful
    pub success: bool,
    /// The new worker ID (if successful)
    pub new_worker_id: Option<WorkerId>,
    /// Provider ID used
    pub provider_id: Option<ProviderId>,
    /// Error message (if failed)
    pub error_message: Option<String>,
}

impl WorkerProvisioningResult {
    /// Creates a successful result.
    #[inline]
    pub fn success(worker_id: WorkerId, provider_id: ProviderId) -> Self {
        Self {
            success: true,
            new_worker_id: Some(worker_id),
            provider_id: Some(provider_id),
            error_message: None,
        }
    }

    /// Creates a failed result.
    #[inline]
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            new_worker_id: None,
            provider_id: None,
            error_message: Some(error.into()),
        }
    }
}

/// Error types for ProvisionNewWorkerHandler.
#[derive(Debug, thiserror::Error)]
pub enum ProvisionNewWorkerError {
    #[error("Provider {provider_id} not available")]
    ProviderNotAvailable { provider_id: ProviderId },

    #[error("Failed to provision worker: {source}")]
    ProvisioningFailed {
        provider_id: ProviderId,
        source: crate::shared_kernel::DomainError,
    },
}

/// Handler for ProvisionNewWorkerCommand.
#[derive(Debug)]
pub struct ProvisionNewWorkerHandler<P>
where
    P: WorkerProvisioning + Debug,
{
    provisioning: P,
}

impl<P> ProvisionNewWorkerHandler<P>
where
    P: WorkerProvisioning + Debug,
{
    /// Creates a new handler with the given provisioning service.
    #[inline]
    pub fn new(provisioning: P) -> Self {
        Self { provisioning }
    }
}

#[async_trait]
impl<P> CommandHandler<ProvisionNewWorkerCommand> for ProvisionNewWorkerHandler<P>
where
    P: WorkerProvisioning + Debug + Send + Sync + 'static,
{
    type Error = ProvisionNewWorkerError;

    async fn handle(
        &self,
        command: ProvisionNewWorkerCommand,
    ) -> Result<WorkerProvisioningResult, Self::Error> {
        // Get the original job to reuse its worker spec
        // In a real implementation, we'd have access to the job repository here
        // For now, we create a basic recovery spec
        let spec = crate::workers::WorkerSpec::new(
            "hodei-worker:latest".to_string(),
            "localhost:50051".to_string(),
        )
        .with_label("recovery", "true")
        .with_label("original_worker_id", command.old_worker_id.to_string());

        let result = self
            .provisioning
            .provision_worker(&command.provider_id, spec, command.job_id)
            .await
            .map_err(|e| ProvisionNewWorkerError::ProvisioningFailed {
                provider_id: command.provider_id,
                source: e,
            })?;

        Ok(WorkerProvisioningResult::success(
            result.worker_id,
            result.provider_id,
        ))
    }
}

/// Command to transfer a job to a new worker.
///
/// This command updates the job to use the new worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferJobCommand {
    /// The job ID to transfer
    pub job_id: JobId,
    /// The new worker ID
    pub new_worker_id: WorkerId,
    /// The old worker ID (being replaced)
    pub old_worker_id: WorkerId,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Optional metadata for tracing
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl TransferJobCommand {
    /// Creates a new TransferJobCommand.
    #[inline]
    pub fn new(
        job_id: JobId,
        new_worker_id: WorkerId,
        old_worker_id: WorkerId,
        saga_id: String,
    ) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            job_id,
            new_worker_id,
            old_worker_id,
            saga_id,
            metadata,
        }
    }
}

impl Command for TransferJobCommand {
    type Output = JobTransferResult;

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!(
            "{}-transfer-job-{}-{}-{}",
            self.saga_id, self.job_id, self.old_worker_id, self.new_worker_id
        ))
    }
}

/// Result of job transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobTransferResult {
    /// Whether transfer was successful
    pub success: bool,
    /// The new state of the job
    pub new_state: JobState,
    /// Error message (if failed)
    pub error_message: Option<String>,
}

impl JobTransferResult {
    /// Creates a successful result.
    #[inline]
    pub fn success(state: JobState) -> Self {
        Self {
            success: true,
            new_state: state,
            error_message: None,
        }
    }

    /// Creates a failed result.
    #[inline]
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            new_state: JobState::Pending,
            error_message: Some(error.into()),
        }
    }
}

/// Error types for TransferJobHandler.
#[derive(Debug, thiserror::Error)]
pub enum TransferJobError {
    #[error("Job {job_id} not found")]
    JobNotFound { job_id: JobId },

    #[error("Failed to transfer job: {source}")]
    TransferFailed {
        job_id: JobId,
        source: crate::shared_kernel::DomainError,
    },
}

/// Handler for TransferJobCommand.
#[derive(Debug)]
pub struct TransferJobHandler<J, W>
where
    J: JobRepository + Debug,
    W: WorkerRegistry + Debug,
{
    job_repository: J,
    worker_repository: W,
}

impl<J, W> TransferJobHandler<J, W>
where
    J: JobRepository + Debug,
    W: WorkerRegistry + Debug,
{
    /// Creates a new handler with the given repositories.
    #[inline]
    pub fn new(job_repository: J, worker_repository: W) -> Self {
        Self {
            job_repository,
            worker_repository,
        }
    }
}

#[async_trait]
impl<J, W> CommandHandler<TransferJobCommand> for TransferJobHandler<J, W>
where
    J: JobRepository + Debug + Send + Sync + 'static,
    W: WorkerRegistry + Debug + Send + Sync + 'static,
{
    type Error = TransferJobError;

    async fn handle(&self, command: TransferJobCommand) -> Result<JobTransferResult, Self::Error> {
        // Clone job_id since we need it in closures
        let job_id = command.job_id.clone();
        let new_worker_id = command.new_worker_id.clone();

        // Get job and update it with new worker assignment
        let _job = self
            .job_repository
            .find_by_id(&job_id)
            .await
            .map_err(|e| TransferJobError::TransferFailed {
                job_id: job_id.clone(),
                source: e,
            })?
            .ok_or_else(|| TransferJobError::JobNotFound {
                job_id: job_id.clone(),
            })?;

        // Update job's execution context with new worker
        // This is done through the Job's methods rather than a direct assign_worker call
        // The worker registry already has the job association via current_job_id

        // Mark new worker as busy
        self.worker_repository
            .update_state(&new_worker_id, WorkerState::Busy)
            .await
            .map_err(|e| TransferJobError::TransferFailed {
                job_id: job_id.clone(),
                source: e,
            })?;

        // Transition job back to Running (it was marked Pending for recovery)
        self.job_repository
            .update_state(&job_id, JobState::Running)
            .await
            .map_err(|e| TransferJobError::TransferFailed {
                job_id: job_id.clone(),
                source: e,
            })?;

        Ok(JobTransferResult::success(JobState::Running))
    }
}

/// Command to destroy an old (unreachable) worker.
///
/// This command cleans up the worker that is no longer reachable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DestroyOldWorkerCommand {
    /// The worker ID to destroy
    pub worker_id: WorkerId,
    /// The provider that owns this worker
    pub provider_id: ProviderId,
    /// The saga that initiated this command
    pub saga_id: String,
    /// Reason for destruction
    #[serde(default)]
    pub reason: Option<String>,
    /// Optional metadata for tracing
    #[serde(default)]
    pub metadata: CommandMetadataDefault,
}

impl DestroyOldWorkerCommand {
    /// Creates a new DestroyOldWorkerCommand.
    #[inline]
    pub fn new(worker_id: WorkerId, provider_id: ProviderId, saga_id: String) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            worker_id,
            provider_id,
            saga_id,
            reason: None,
            metadata,
        }
    }

    /// Creates a command with a reason.
    #[inline]
    pub fn with_reason(
        worker_id: WorkerId,
        provider_id: ProviderId,
        saga_id: String,
        reason: impl Into<String>,
    ) -> Self {
        let metadata = CommandMetadataDefault::new().with_saga_id(&saga_id);
        Self {
            worker_id,
            provider_id,
            saga_id,
            reason: Some(reason.into()),
            metadata,
        }
    }
}

impl Command for DestroyOldWorkerCommand {
    type Output = ();

    #[inline]
    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!(
            "{}-destroy-old-worker-{}",
            self.saga_id, self.worker_id
        ))
    }
}

/// Error types for DestroyOldWorkerHandler.
#[derive(Debug, thiserror::Error)]
pub enum DestroyOldWorkerError {
    #[error("Worker {worker_id} not found")]
    WorkerNotFound { worker_id: WorkerId },

    #[error("Failed to destroy worker {worker_id}: {source}")]
    DestructionFailed {
        worker_id: WorkerId,
        source: crate::shared_kernel::DomainError,
    },
}

/// Handler for DestroyOldWorkerCommand.
#[derive(Debug)]
pub struct DestroyOldWorkerHandler<P>
where
    P: WorkerProvisioning + Debug,
{
    provisioning: P,
}

impl<P> DestroyOldWorkerHandler<P>
where
    P: WorkerProvisioning + Debug,
{
    /// Creates a new handler with the given provisioning service.
    #[inline]
    pub fn new(provisioning: P) -> Self {
        Self { provisioning }
    }
}

#[async_trait]
impl<P> CommandHandler<DestroyOldWorkerCommand> for DestroyOldWorkerHandler<P>
where
    P: WorkerProvisioning + Debug + Send + Sync + 'static,
{
    type Error = DestroyOldWorkerError;

    async fn handle(&self, command: DestroyOldWorkerCommand) -> Result<(), Self::Error> {
        // Attempt to destroy the worker
        // This is best-effort - the worker might already be gone
        let result = self.provisioning.destroy_worker(&command.worker_id).await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(DestroyOldWorkerError::DestructionFailed {
                worker_id: command.worker_id,
                source: e,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_kernel::{JobId, JobState, ProviderId, WorkerId, WorkerState};

    #[tokio::test]
    async fn check_connectivity_command_idempotency() {
        let worker_id = WorkerId::new();
        let saga_id = "saga-123".to_string();
        let cmd = CheckConnectivityCommand::new(worker_id.clone(), saga_id.clone());

        let key = cmd.idempotency_key();
        assert!(key.contains(&saga_id));
        assert!(key.contains("check-connectivity"));
    }

    #[tokio::test]
    async fn mark_job_for_recovery_command_idempotency() {
        let job_id = JobId::new();
        let worker_id = WorkerId::new();
        let saga_id = "saga-456".to_string();
        let cmd =
            MarkJobForRecoveryCommand::new(job_id.clone(), worker_id.clone(), saga_id.clone());

        let key = cmd.idempotency_key();
        assert!(key.contains(&saga_id));
        assert!(key.contains("mark-recovery"));
    }

    #[tokio::test]
    async fn provision_new_worker_command_idempotency() {
        let job_id = JobId::new();
        let provider_id = ProviderId::new();
        let worker_id = WorkerId::new();
        let saga_id = "saga-789".to_string();
        let cmd = ProvisionNewWorkerCommand::new(
            job_id.clone(),
            provider_id.clone(),
            worker_id.clone(),
            saga_id.clone(),
        );

        let key = cmd.idempotency_key();
        assert!(key.contains(&saga_id));
        assert!(key.contains("provision-new-worker"));
    }

    #[tokio::test]
    async fn transfer_job_command_idempotency() {
        let job_id = JobId::new();
        let new_worker_id = WorkerId::new();
        let old_worker_id = WorkerId::new();
        let saga_id = "saga-transfer".to_string();
        let cmd = TransferJobCommand::new(
            job_id.clone(),
            new_worker_id.clone(),
            old_worker_id.clone(),
            saga_id.clone(),
        );

        let key = cmd.idempotency_key();
        assert!(key.contains(&saga_id));
        assert!(key.contains("transfer-job"));
    }

    #[tokio::test]
    async fn destroy_old_worker_command_idempotency() {
        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();
        let saga_id = "saga-destroy".to_string();
        let cmd =
            DestroyOldWorkerCommand::new(worker_id.clone(), provider_id.clone(), saga_id.clone());

        let key = cmd.idempotency_key();
        assert!(key.contains(&saga_id));
        assert!(key.contains("destroy-old-worker"));
    }

    #[tokio::test]
    async fn check_connectivity_result_reachable() {
        let result = CheckConnectivityResult::reachable(WorkerState::Ready, 50);
        assert!(result.is_reachable);
        assert_eq!(result.worker_state, Some(WorkerState::Ready));
        assert_eq!(result.latency_ms, Some(50));
    }

    #[tokio::test]
    async fn check_connectivity_result_unreachable() {
        let result = CheckConnectivityResult::unreachable("Connection timeout");
        assert!(!result.is_reachable);
        assert_eq!(result.error_message, Some("Connection timeout".to_string()));
    }

    #[tokio::test]
    async fn mark_job_for_recovery_result() {
        let result = JobRecoveryMarkResult::success(JobState::Pending);
        assert!(result.success);
        assert_eq!(result.new_state, JobState::Pending);
    }

    #[tokio::test]
    async fn worker_provisioning_result_success() {
        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();
        let result = WorkerProvisioningResult::success(worker_id.clone(), provider_id.clone());
        assert!(result.success);
        assert_eq!(result.new_worker_id, Some(worker_id.clone()));
    }

    #[tokio::test]
    async fn job_transfer_result_success() {
        let result = JobTransferResult::success(JobState::Running);
        assert!(result.success);
        assert_eq!(result.new_state, JobState::Running);
    }
}

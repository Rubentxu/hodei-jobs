//!
//! # Command Handlers for saga-engine v4.0 (EPIC-94-C.6)
//!
//! This module provides command handlers that integrate with the new
//! DurableWorkflow-based system for v4.0.
//!
//! ## Key Commands
//!
//! - `ProvisionWorkerCommand`: Provision a worker using v4.0 workflow
//! - `ExecuteJobCommand`: Execute a job using v4.0 workflow
//! - `RecoverWorkerCommand`: Recover a failed worker using v4.0 workflow
//!

use async_trait::async_trait;
use hodei_server_domain::command::{Command, CommandHandler};
use hodei_server_domain::jobs::{Job, JobRepository, JobSpec};
use hodei_server_domain::outbox::{OutboxEventInsert, OutboxRepository};
use hodei_server_domain::providers::ProviderConfigRepository;
use hodei_server_domain::shared_kernel::{JobId, JobState, ProviderId, WorkerId};
use hodei_server_domain::workers::{WorkerRegistry, WorkerSpec};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt::Debug;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::saga::provisioning_workflow_coordinator::{
    ProvisioningWorkflowCoordinator, ProvisioningWorkflowError, ProvisioningWorkflowResult,
};
use crate::saga::workflows::execution_durable::ExecutionWorkflowInput;
use crate::saga::workflows::execution_durable::JobResultData;

// =============================================================================
// Provision Worker Command
// =============================================================================

/// Command to provision a worker using v4.0 DurableWorkflow
///
/// This command uses the ProvisioningWorkflowCoordinator to provision
/// a new worker through the saga-engine v4.0 infrastructure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionWorkerCommand {
    /// Job ID that requires the worker (optional)
    pub job_id: Option<JobId>,
    /// Provider ID to use
    pub provider_id: ProviderId,
    /// Worker specification
    pub worker_spec: WorkerSpec,
    /// Optional metadata for tracing
    pub correlation_id: Option<String>,
}

impl ProvisionWorkerCommand {
    /// Create a new command
    pub fn new(job_id: Option<JobId>, provider_id: ProviderId, worker_spec: WorkerSpec) -> Self {
        Self {
            correlation_id: job_id.as_ref().map(|j| j.to_string()),
            job_id,
            provider_id,
            worker_spec,
        }
    }
}

impl Command for ProvisionWorkerCommand {
    type Output = ProvisionWorkerResult;

    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!(
            "provision-worker-{}:{}",
            self.provider_id,
            self.job_id
                .as_ref()
                .map(|j| j.0.to_string())
                .unwrap_or_else(|| "no-job".to_string())
        ))
    }
}

/// Result of worker provisioning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionWorkerResult {
    /// Whether the operation was successful
    pub success: bool,
    /// The provisioned worker ID
    pub worker_id: Option<WorkerId>,
    /// OTP token for worker registration
    pub otp_token: Option<String>,
    /// Provider ID used
    pub provider_id: ProviderId,
    /// Error message if failed
    pub error_message: Option<String>,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

impl ProvisionWorkerResult {
    /// Create a successful result
    pub fn success(
        worker_id: WorkerId,
        otp_token: String,
        provider_id: ProviderId,
        duration_ms: u64,
    ) -> Self {
        Self {
            success: true,
            worker_id: Some(worker_id),
            otp_token: Some(otp_token),
            provider_id,
            error_message: None,
            duration_ms,
        }
    }

    /// Create a failed result
    pub fn failure(provider_id: ProviderId, message: String, duration_ms: u64) -> Self {
        Self {
            success: false,
            worker_id: None,
            otp_token: None,
            provider_id,
            error_message: Some(message),
            duration_ms,
        }
    }
}

/// Error types for ProvisionWorkerHandler
#[derive(Debug, thiserror::Error)]
pub enum ProvisionWorkerError {
    #[error("Provider {provider_id} not found")]
    ProviderNotFound { provider_id: ProviderId },

    #[error("Workflow execution failed: {0}")]
    WorkflowFailed(String),

    #[error("Infrastructure error: {0}")]
    InfrastructureError(String),
}

/// Handler for ProvisionWorkerCommand using v4.0 workflow
pub struct ProvisionWorkerHandler<C: ?Sized>
where
    C: ProvisioningWorkflowCoordinator,
{
    coordinator: Arc<C>,
    provider_registry: Arc<dyn ProviderConfigRepository>,
    outbox_repository: Option<Arc<dyn OutboxRepository + Send + Sync>>,
}

impl<C: ?Sized> ProvisionWorkerHandler<C>
where
    C: ProvisioningWorkflowCoordinator,
{
    /// Create a new handler
    pub fn new(
        coordinator: Arc<C>,
        provider_registry: Arc<dyn ProviderConfigRepository>,
        outbox_repository: Option<Arc<dyn OutboxRepository + Send + Sync>>,
    ) -> Self {
        Self {
            coordinator,
            provider_registry,
            outbox_repository,
        }
    }
}

#[async_trait]
impl<C: ?Sized> CommandHandler<ProvisionWorkerCommand> for ProvisionWorkerHandler<C>
where
    C: ProvisioningWorkflowCoordinator + Send + Sync + 'static,
{
    type Error = ProvisionWorkerError;

    async fn handle(
        &self,
        command: ProvisionWorkerCommand,
    ) -> Result<ProvisionWorkerResult, Self::Error> {
        let start_time = std::time::Instant::now();
        let job_id_str = command
            .job_id
            .as_ref()
            .map(|j| j.to_string())
            .unwrap_or_default();

        info!(
            job_id = %job_id_str,
            provider_id = %command.provider_id,
            "🔧 ProvisionWorkerCommand: Starting worker provisioning"
        );

        // Validate provider exists
        let provider = self
            .provider_registry
            .find_by_id(&command.provider_id)
            .await
            .map_err(|e| ProvisionWorkerError::InfrastructureError(e.to_string()))?;

        if provider.is_none() {
            return Err(ProvisionWorkerError::ProviderNotFound {
                provider_id: command.provider_id.clone(),
            });
        }

        // Execute provisioning workflow
        match self
            .coordinator
            .provision_worker(
                &command.provider_id,
                &command.worker_spec,
                command.job_id.clone(),
            )
            .await
        {
            Ok(result) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;

                info!(
                    job_id = %job_id_str,
                    worker_id = %result.worker_id,
                    provider_id = %result.provider_id,
                    duration_ms = %duration_ms,
                    "✅ Worker provisioned successfully"
                );

                // Emit event if outbox available
                self.emit_worker_provisioned_event(&result, &job_id_str)
                    .await;

                Ok(ProvisionWorkerResult::success(
                    result.worker_id,
                    result.otp_token,
                    result.provider_id,
                    duration_ms,
                ))
            }
            Err(e) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;
                let error_msg = e.to_string();

                error!(
                    job_id = %job_id_str,
                    provider_id = %command.provider_id,
                    error = %error_msg,
                    duration_ms = %duration_ms,
                    "❌ Worker provisioning failed"
                );

                Err(ProvisionWorkerError::WorkflowFailed(error_msg))
            }
        }
    }
}

impl<C: ?Sized> ProvisionWorkerHandler<C>
where
    C: ProvisioningWorkflowCoordinator,
{
    async fn emit_worker_provisioned_event(
        &self,
        result: &ProvisioningWorkflowResult,
        job_id_str: &str,
    ) {
        if let Some(ref outbox) = self.outbox_repository {
            let event = OutboxEventInsert::for_worker(
                result.worker_id.0.clone(),
                "WorkerProvisioned".to_string(),
                serde_json::json!({
                    "worker_id": result.worker_id.0.to_string(),
                    "provider_id": result.provider_id.0.to_string(),
                    "job_id": job_id_str,
                    "source": "ProvisionWorkerHandler_v4",
                    "workflow_version": "v4.0"
                }),
                None,
                None,
            );

            if let Err(e) = outbox.insert_events(&[event]).await {
                error!("Failed to emit WorkerProvisioned event: {}", e);
            }
        }
    }
}

// =============================================================================
// Execute Job Command
// =============================================================================

/// Command to execute a job using v4.0 DurableWorkflow
///
/// This command uses the ExecutionWorkflow to manage job execution
/// through the saga-engine v4.0 infrastructure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteJobCommand {
    /// The job to execute
    pub job: Job,
    /// Worker ID to dispatch to
    pub worker_id: WorkerId,
    /// Optional metadata for tracing
    pub correlation_id: Option<String>,
}

impl ExecuteJobCommand {
    /// Create a new command
    pub fn new(job: Job, worker_id: WorkerId) -> Self {
        let job_id = job.id.clone();
        Self {
            job,
            worker_id,
            correlation_id: Some(job_id.to_string()),
        }
    }
}

impl Command for ExecuteJobCommand {
    type Output = ExecuteJobResult;

    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!("execute-job-{}", self.job.id))
    }
}

/// Result of job execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteJobResult {
    /// Whether the operation was successful
    pub success: bool,
    /// Job ID
    pub job_id: JobId,
    /// Worker ID
    pub worker_id: WorkerId,
    /// Job result
    pub result: JobResultData,
    /// Exit code
    pub exit_code: i32,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Error message if failed
    pub error_message: Option<String>,
}

impl ExecuteJobResult {
    /// Create a successful result
    pub fn success(
        job_id: JobId,
        worker_id: WorkerId,
        result: JobResultData,
        exit_code: i32,
        duration_ms: u64,
    ) -> Self {
        Self {
            success: true,
            job_id,
            worker_id,
            result,
            exit_code,
            duration_ms,
            error_message: None,
        }
    }

    /// Create a failed result
    pub fn failure(job_id: JobId, worker_id: WorkerId, message: String, duration_ms: u64) -> Self {
        Self {
            success: false,
            job_id,
            worker_id,
            result: JobResultData::Error {
                message: message.clone(),
            },
            exit_code: -1,
            duration_ms,
            error_message: Some(message),
        }
    }
}

/// Error types for ExecuteJobHandler
#[derive(Debug, thiserror::Error)]
pub enum ExecuteJobError {
    #[error("Job {job_id} not found")]
    JobNotFound { job_id: JobId },

    #[error("Worker {worker_id} not found")]
    WorkerNotFound { worker_id: WorkerId },

    #[error("Workflow execution failed: {0}")]
    WorkflowFailed(String),

    #[error("Infrastructure error: {0}")]
    InfrastructureError(String),
}

// =============================================================================
// Cancel Job Command
// =============================================================================

/// Command to cancel a running job using v4.0 workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelJobCommand {
    /// The job ID to cancel
    pub job_id: JobId,
    /// Reason for cancellation
    pub reason: String,
    /// Optional metadata for tracing
    pub correlation_id: Option<String>,
}

impl CancelJobCommand {
    /// Create a new command
    pub fn new(job_id: JobId, reason: impl Into<String>) -> Self {
        let correlation_id = job_id.to_string();
        Self {
            job_id,
            reason: reason.into(),
            correlation_id: Some(correlation_id),
        }
    }
}

impl Command for CancelJobCommand {
    type Output = CancelJobCommandResult;

    fn idempotency_key(&self) -> Cow<'_, str> {
        Cow::Owned(format!("cancel-job-{}", self.job_id))
    }
}

/// Result of job cancellation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelJobCommandResult {
    /// Whether the operation was successful
    pub success: bool,
    /// Job ID
    pub job_id: JobId,
    /// Previous state
    pub previous_state: JobState,
    /// New state
    pub new_state: JobState,
    /// Error message if failed
    pub error_message: Option<String>,
}

impl CancelJobCommandResult {
    /// Create a successful result
    pub fn success(job_id: JobId, previous_state: JobState) -> Self {
        Self {
            success: true,
            job_id,
            previous_state,
            new_state: JobState::Cancelled,
            error_message: None,
        }
    }

    /// Create a failed result
    pub fn failure(job_id: JobId, message: String) -> Self {
        Self {
            success: false,
            job_id,
            previous_state: JobState::Pending,
            new_state: JobState::Pending,
            error_message: Some(message),
        }
    }
}

/// Error types for CancelJobHandler
#[derive(Debug, thiserror::Error)]
pub enum CancelJobError {
    #[error("Job {job_id} not found")]
    JobNotFound { job_id: JobId },

    #[error("Job {job_id} is in terminal state: {state}")]
    TerminalState { job_id: JobId, state: JobState },

    #[error("Cancellation failed: {0}")]
    CancellationFailed(String),
}

// =============================================================================
// Command Handler Builder
// =============================================================================

/// Builder for creating command handlers with common configuration
pub struct CommandHandlerBuilder {
    provisioning_coordinator: Option<Arc<dyn ProvisioningWorkflowCoordinator>>,
    provider_registry: Option<Arc<dyn ProviderConfigRepository>>,
    job_repository: Option<Arc<dyn JobRepository>>,
    worker_registry: Option<Arc<dyn WorkerRegistry>>,
    outbox_repository: Option<Arc<dyn OutboxRepository + Send + Sync>>,
}

impl CommandHandlerBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            provisioning_coordinator: None,
            provider_registry: None,
            job_repository: None,
            worker_registry: None,
            outbox_repository: None,
        }
    }

    /// Set provisioning coordinator
    pub fn with_provisioning_coordinator(
        mut self,
        coordinator: Arc<dyn ProvisioningWorkflowCoordinator>,
    ) -> Self {
        self.provisioning_coordinator = Some(coordinator);
        self
    }

    /// Set provider registry
    pub fn with_provider_registry(
        mut self,
        provider_registry: Arc<dyn ProviderConfigRepository>,
    ) -> Self {
        self.provider_registry = Some(provider_registry);
        self
    }

    /// Set job repository
    pub fn with_job_repository(mut self, job_repository: Arc<dyn JobRepository>) -> Self {
        self.job_repository = Some(job_repository);
        self
    }

    /// Set worker registry
    pub fn with_worker_registry(mut self, worker_registry: Arc<dyn WorkerRegistry>) -> Self {
        self.worker_registry = Some(worker_registry);
        self
    }

    /// Set outbox repository
    pub fn with_outbox_repository(
        mut self,
        outbox_repository: Arc<dyn OutboxRepository + Send + Sync>,
    ) -> Self {
        self.outbox_repository = Some(outbox_repository);
        self
    }

    /// Build ProvisionWorkerHandler
    pub fn build_provision_worker_handler(
        self,
    ) -> Result<ProvisionWorkerHandler<dyn ProvisioningWorkflowCoordinator + Send + Sync>, String>
    {
        Ok(ProvisionWorkerHandler::new(
            self.provisioning_coordinator
                .ok_or("provisioning_coordinator required")?,
            self.provider_registry.ok_or("provider_registry required")?,
            self.outbox_repository,
        ))
    }
}

impl Default for CommandHandlerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::shared_kernel::{JobId, ProviderId, WorkerId};
    use hodei_server_domain::workers::WorkerSpec;

    #[tokio::test]
    async fn test_provision_worker_command_idempotency() {
        let job_id = JobId::new();
        let provider_id = ProviderId::new();
        let spec = WorkerSpec::new("test-image".to_string(), "localhost:50051".to_string());

        let cmd = ProvisionWorkerCommand::new(Some(job_id.clone()), provider_id.clone(), spec);

        let key = cmd.idempotency_key();
        assert!(key.contains(&provider_id.0.to_string()));
        assert!(key.contains(&job_id.0.to_string()));
    }

    #[tokio::test]
    async fn test_provision_worker_command_no_job() {
        let provider_id = ProviderId::new();
        let spec = WorkerSpec::new("test-image".to_string(), "localhost:50051".to_string());

        let cmd = ProvisionWorkerCommand::new(None, provider_id.clone(), spec);

        let key = cmd.idempotency_key();
        assert!(key.contains("no-job"));
    }

    #[tokio::test]
    async fn test_execute_job_command() {
        let job_id = JobId::new();
        let worker_id = WorkerId::new();
        let job = Job::new(job_id.clone(), "test-job".to_string(), JobSpec::new(vec![]));

        let cmd = ExecuteJobCommand::new(job, worker_id.clone());

        assert_eq!(cmd.job.id, job_id);
        assert_eq!(cmd.worker_id, worker_id);
        assert!(cmd.correlation_id.is_some());
    }

    #[tokio::test]
    async fn test_cancel_job_command() {
        let job_id = JobId::new();
        let cmd = CancelJobCommand::new(job_id.clone(), "User requested cancellation");

        assert_eq!(cmd.job_id, job_id);
        assert_eq!(cmd.reason, "User requested cancellation");
    }

    #[tokio::test]
    async fn test_provision_worker_result_success() {
        let worker_id = WorkerId::new();
        let provider_id = ProviderId::new();

        let result = ProvisionWorkerResult::success(
            worker_id.clone(),
            "otp-12345".to_string(),
            provider_id.clone(),
            1500,
        );

        assert!(result.success);
        assert_eq!(result.worker_id, Some(worker_id));
        assert_eq!(result.otp_token, Some("otp-12345".to_string()));
        assert!(result.error_message.is_none());
    }

    #[tokio::test]
    async fn test_provision_worker_result_failure() {
        let provider_id = ProviderId::new();

        let result = ProvisionWorkerResult::failure(
            provider_id.clone(),
            "Provider not available".to_string(),
            500,
        );

        assert!(!result.success);
        assert!(result.worker_id.is_none());
        assert!(result.error_message.is_some());
    }
}

//!
//! # Recovery Workflow for saga-engine v4 (DurableWorkflow)
//!
//! This module provides the [`RecoveryWorkflow`] implementation using the
//! [`DurableWorkflow`] trait for the Workflow-as-Code pattern.
//!
//! This is a migration from the legacy [`super::recovery::RecoveryWorkflow`]
//! which used the step-list pattern (`WorkflowDefinition`).
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

use hodei_server_domain::shared_kernel::{JobId, ProviderId, WorkerId};
use hodei_server_domain::workers::{WorkerProvisioning, WorkerRegistry};
use hodei_shared::states::WorkerState;

use saga_engine_core::workflow::{
    Activity, DurableWorkflow, DurableWorkflowState, ExecuteActivityError, WorkflowConfig,
    WorkflowContext, WorkflowResult,
};

use crate::saga::bridge::worker_lifecycle::TerminateWorkerActivity;

// =============================================================================
// Input/Output Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryInput {
    pub job_id: String,
    pub failed_worker_id: String,
    pub target_provider_id: Option<String>,
    pub reason: String,
}

impl RecoveryInput {
    pub fn new(
        job_id: JobId,
        failed_worker_id: WorkerId,
        target_provider_id: Option<String>,
        reason: impl Into<String>,
    ) -> Self {
        let reason = reason.into();
        Self {
            job_id: job_id.to_string(),
            failed_worker_id: failed_worker_id.to_string(),
            target_provider_id: target_provider_id.map(|id| id.to_string()),
            reason,
        }
    }

    pub fn idempotency_key(&self) -> String {
        format!("recovery:{}:{}", self.job_id, self.failed_worker_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryOutput {
    pub job_id: String,
    pub old_worker_id: String,
    pub new_worker_id: String,
    pub provider_id: Option<String>,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum RecoveryError {
    #[error("Worker validation failed: {0}")]
    WorkerValidationFailed(String),

    #[error("Worker provisioning failed: {0}")]
    WorkerProvisioningFailed(String),

    #[error("Job transfer failed: {0}")]
    JobTransferFailed(String),

    #[error("Worker termination failed: {0}")]
    WorkerTerminationFailed(String),

    #[error("Worker cancellation failed: {0}")]
    WorkerCancellationFailed(String),

    #[error("Activity execution failed: {0}")]
    ActivityFailed(String),

    #[error("Compensation failed: {0}")]
    CompensationFailed(String),
}

// =============================================================================
// Activities
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckConnectivityInput {
    pub worker_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckConnectivityOutput {
    pub worker_id: String,
    pub is_reachable: bool,
    pub last_heartbeat: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum CheckConnectivityError {
    #[error("Worker not found: {worker_id}")]
    WorkerNotFound { worker_id: String },
    #[error("Connectivity check failed: {0}")]
    Failed(String),
}

/// Activity for checking worker connectivity
pub struct CheckConnectivityActivity {
    /// Using underscore to indicate Debug is manually implemented
    _registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl std::fmt::Debug for CheckConnectivityActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckConnectivityActivity")
            .field("_registry", &"<dyn WorkerRegistry>")
            .finish()
    }
}

impl CheckConnectivityActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self {
            _registry: registry,
        }
    }
}

#[async_trait]
impl Activity for CheckConnectivityActivity {
    const TYPE_ID: &'static str = "recovery-check-connectivity";

    type Input = CheckConnectivityInput;
    type Output = CheckConnectivityOutput;
    type Error = CheckConnectivityError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let worker_id = WorkerId::from_string(&input.worker_id).ok_or_else(|| {
            CheckConnectivityError::WorkerNotFound {
                worker_id: input.worker_id.clone(),
            }
        })?;

        let worker = self
            ._registry
            .find_by_id(&worker_id)
            .await
            .map_err(|e| CheckConnectivityError::Failed(e.to_string()))?
            .ok_or_else(|| CheckConnectivityError::WorkerNotFound {
                worker_id: input.worker_id.clone(),
            })?;

        let last_heartbeat = worker.last_heartbeat_at();

        Ok(CheckConnectivityOutput {
            worker_id: input.worker_id,
            is_reachable: *worker.state() != WorkerState::Terminated,
            last_heartbeat,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionReplacementInput {
    pub job_id: String,
    pub target_provider_id: Option<String>,
    pub original_worker_spec: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionReplacementOutput {
    pub new_worker_id: String,
    pub provider_id: String,
}

/// Activity for provisioning replacement worker
pub struct ProvisionReplacementActivity {
    /// Using underscore to indicate Debug is manually implemented
    _provisioning: Arc<dyn WorkerProvisioning + Send + Sync>,
}

impl std::fmt::Debug for ProvisionReplacementActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvisionReplacementActivity")
            .field("_provisioning", &"<dyn WorkerProvisioning>")
            .finish()
    }
}

impl ProvisionReplacementActivity {
    pub fn new(provisioning: Arc<dyn WorkerProvisioning + Send + Sync>) -> Self {
        Self {
            _provisioning: provisioning,
        }
    }
}

#[async_trait]
impl Activity for ProvisionReplacementActivity {
    const TYPE_ID: &'static str = "recovery-provision-replacement";

    type Input = ProvisionReplacementInput;
    type Output = ProvisionReplacementOutput;
    type Error = RecoveryError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let provider_id = if let Some(pid) = input.target_provider_id {
            hodei_server_domain::shared_kernel::ProviderId::from_uuid(
                Uuid::parse_str(&pid)
                    .map_err(|e| RecoveryError::WorkerProvisioningFailed(e.to_string()))?,
            )
        } else {
            ProviderId::new()
        };

        let job_id = JobId::from_string(&input.job_id).ok_or_else(|| {
            RecoveryError::WorkerProvisioningFailed(format!("Invalid job ID: {}", input.job_id))
        })?;

        let spec = hodei_server_domain::workers::WorkerSpec::new(
            "recovery-worker:latest".to_string(),
            "localhost:50051".to_string(),
        );

        let result = self
            ._provisioning
            .provision_worker(&provider_id, spec, job_id)
            .await
            .map_err(|e| RecoveryError::WorkerProvisioningFailed(e.to_string()))?;

        Ok(ProvisionReplacementOutput {
            new_worker_id: result.worker_id.to_string(),
            provider_id: provider_id.to_string(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferJobInput {
    pub job_id: String,
    pub new_worker_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferJobOutput {
    pub job_id: String,
    pub worker_id: String,
    pub transferred: bool,
}

/// Activity for transferring job to new worker
pub struct TransferJobActivity {
    /// Using underscore to indicate Debug is manually implemented
    _registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl std::fmt::Debug for TransferJobActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransferJobActivity")
            .field("_registry", &"<dyn WorkerRegistry>")
            .finish()
    }
}

impl TransferJobActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self {
            _registry: registry,
        }
    }
}

#[async_trait]
impl Activity for TransferJobActivity {
    const TYPE_ID: &'static str = "recovery-transfer-job";

    type Input = TransferJobInput;
    type Output = TransferJobOutput;
    type Error = RecoveryError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let worker_id = WorkerId::from_string(&input.new_worker_id).ok_or_else(|| {
            RecoveryError::JobTransferFailed(format!("Invalid worker ID: {}", input.new_worker_id))
        })?;

        self._registry
            .update_state(&worker_id, WorkerState::Busy)
            .await
            .map_err(|e| RecoveryError::JobTransferFailed(e.to_string()))?;

        Ok(TransferJobOutput {
            job_id: input.job_id,
            worker_id: input.new_worker_id,
            transferred: true,
        })
    }
}

// =============================================================================
// Workflow
// =============================================================================

/// Recovery Workflow using DurableWorkflow (Workflow-as-Code)
///
/// This workflow handles worker failure recovery by:
/// 1. Checking connectivity of failed worker
/// 2. Provisioning a replacement worker
/// 3. Transferring the job to the new worker
/// 4. Terminating the old worker
pub struct RecoveryWorkflow {
    /// Activity for checking worker connectivity
    check_connectivity_activity: CheckConnectivityActivity,
    /// Activity for provisioning replacement worker
    provision_replacement_activity: ProvisionReplacementActivity,
    /// Activity for transferring job to new worker
    transfer_job_activity: TransferJobActivity,
    /// Activity for terminating old worker
    terminate_worker_activity: TerminateWorkerActivity,
    /// Workflow configuration
    config: WorkflowConfig,
}

impl Debug for RecoveryWorkflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryWorkflow")
            .field("config", &self.config)
            .finish()
    }
}

impl RecoveryWorkflow {
    /// Create a new RecoveryWorkflow
    pub fn new(
        registry: Arc<dyn WorkerRegistry + Send + Sync>,
        provisioning: Arc<dyn WorkerProvisioning + Send + Sync>,
    ) -> Self {
        Self {
            check_connectivity_activity: CheckConnectivityActivity::new(registry.clone()),
            provision_replacement_activity: ProvisionReplacementActivity::new(provisioning.clone()),
            transfer_job_activity: TransferJobActivity::new(registry.clone()),
            terminate_worker_activity: TerminateWorkerActivity::new(provisioning, registry),
            config: WorkflowConfig {
                default_timeout_secs: 300,
                max_step_retries: 3,
                retry_backoff_multiplier: 2.0,
                retry_base_delay_ms: 1000,
                task_queue: "recovery".to_string(),
            },
        }
    }

    /// Get the workflow configuration
    pub fn config(&self) -> &WorkflowConfig {
        &self.config
    }
}

#[async_trait]
impl DurableWorkflow for RecoveryWorkflow {
    const TYPE_ID: &'static str = "recovery";
    const VERSION: u32 = 2; // Version 2 = migrated to DurableWorkflow

    type Input = RecoveryInput;
    type Output = RecoveryOutput;
    type Error = RecoveryError;

    /// Execute the recovery workflow
    ///
    /// This method implements the recovery workflow logic using the Workflow-as-Code pattern.
    async fn run(
        &self,
        context: &mut WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        tracing::info!(
            execution_id = %context.execution_id.0,
            job_id = %input.job_id,
            failed_worker_id = %input.failed_worker_id,
            "Starting recovery workflow"
        );

        // Track new worker ID for compensation
        let mut new_worker_id: Option<String> = None;

        // Step 1: Check connectivity of failed worker
        let connectivity_input = CheckConnectivityInput {
            worker_id: input.failed_worker_id.clone(),
        };

        let connectivity_result = context
            .execute_activity(
                &self.check_connectivity_activity,
                connectivity_input.clone(),
            )
            .await;

        match connectivity_result {
            Ok(output) => {
                tracing::info!(
                    worker_id = %output.worker_id,
                    is_reachable = %output.is_reachable,
                    "Connectivity check completed"
                );
                // Continue even if worker is not reachable
            }
            Err(e) if e.is_failed() => {
                return Err(RecoveryError::WorkerValidationFailed(
                    e.as_failed()
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "Unknown error".to_string()),
                ));
            }
            Err(e) if e.is_paused() => {
                return Err(RecoveryError::ActivityFailed(
                    "Workflow paused waiting for connectivity check".to_string(),
                ));
            }
            _ => {}
        }

        // Step 2: Provision replacement worker
        let provision_input = ProvisionReplacementInput {
            job_id: input.job_id.clone(),
            target_provider_id: input.target_provider_id.clone(),
            original_worker_spec: None,
        };

        let provision_result = context
            .execute_activity(
                &self.provision_replacement_activity,
                provision_input.clone(),
            )
            .await;

        match provision_result {
            Ok(output) => {
                tracing::info!(
                    new_worker_id = %output.new_worker_id,
                    "Replacement worker provisioned"
                );
                new_worker_id = Some(output.new_worker_id.clone());
            }
            Err(e) if e.is_failed() => {
                return Err(RecoveryError::WorkerProvisioningFailed(
                    e.as_failed()
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "Unknown error".to_string()),
                ));
            }
            Err(e) if e.is_paused() => {
                return Err(RecoveryError::ActivityFailed(
                    "Workflow paused waiting for replacement provisioning".to_string(),
                ));
            }
            _ => {}
        }

        // Step 3: Transfer job to new worker
        let new_worker_id = new_worker_id.ok_or_else(|| {
            RecoveryError::JobTransferFailed("No new worker ID available".to_string())
        })?;

        let transfer_input = TransferJobInput {
            job_id: input.job_id.clone(),
            new_worker_id: new_worker_id.clone(),
        };

        let transfer_result = context
            .execute_activity(&self.transfer_job_activity, transfer_input.clone())
            .await;

        match transfer_result {
            Ok(output) => {
                tracing::info!(
                    job_id = %output.job_id,
                    new_worker_id = %output.worker_id,
                    "Job transferred to new worker"
                );
            }
            Err(e) if e.is_failed() => {
                return Err(RecoveryError::JobTransferFailed(
                    e.as_failed()
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "Unknown error".to_string()),
                ));
            }
            Err(e) if e.is_paused() => {
                return Err(RecoveryError::ActivityFailed(
                    "Workflow paused waiting for job transfer".to_string(),
                ));
            }
            _ => {}
        }

        // Step 4: Terminate old worker
        let old_worker_id =
            WorkerId::from_string(&input.failed_worker_id).unwrap_or_else(|| WorkerId::new());

        let terminate_input = crate::saga::bridge::worker_lifecycle::TerminateWorkerInput {
            worker_id: old_worker_id,
            reason: format!("Recovery: {}", input.reason),
            destroy_infrastructure: true,
        };

        let terminate_result = context
            .execute_activity(&self.terminate_worker_activity, terminate_input.clone())
            .await;

        match terminate_result {
            Ok(output) => {
                tracing::info!(
                    worker_id = %output.worker_id.0,
                    "Old worker terminated"
                );
            }
            Err(e) if e.is_failed() => {
                // Log but don't fail - the old worker might already be terminated
                tracing::warn!(
                    "Failed to terminate old worker: {}",
                    e.as_failed()
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "Unknown error".to_string())
                );
            }
            Err(e) if e.is_paused() => {
                return Err(RecoveryError::ActivityFailed(
                    "Workflow paused waiting for worker termination".to_string(),
                ));
            }
            _ => {}
        }

        // Return successful recovery output
        Ok(RecoveryOutput {
            job_id: input.job_id,
            old_worker_id: input.failed_worker_id,
            new_worker_id,
            provider_id: input.target_provider_id,
            completed_at: chrono::Utc::now(),
        })
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workers::provisioning::{MockWorkerProvisioning, MockWorkerRegistry};
    use saga_engine_core::workflow::WorkflowContext;
    use std::sync::Arc;

    #[test]
    fn test_recovery_workflow_type_id() {
        assert_eq!(RecoveryWorkflow::TYPE_ID, "recovery");
        assert_eq!(RecoveryWorkflow::VERSION, 2);
    }

    #[test]
    fn test_recovery_input_idempotency_key() {
        let input = RecoveryInput::new(
            JobId::from_string("job-123").unwrap(),
            WorkerId::from_string("worker-456").unwrap(),
            Some("provider-789".to_string()),
            "test failure",
        );

        let key = input.idempotency_key();
        assert!(key.starts_with("recovery:job-123:worker-456"));
    }

    #[test]
    fn test_recovery_workflow_creation() {
        let registry: Arc<dyn WorkerRegistry + Send + Sync> = Arc::new(MockWorkerRegistry::new());
        let provisioning: Arc<dyn WorkerProvisioning + Send + Sync> =
            Arc::new(MockWorkerProvisioning::new());

        let workflow = RecoveryWorkflow::new(registry, provisioning);

        assert_eq!(workflow.config.task_queue, "recovery");
        assert_eq!(workflow.config.default_timeout_secs, 300);
    }
}

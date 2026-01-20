//!
//! # Recovery Workflow for saga-engine v4
//!
//! This module provides RecoveryWorkflow implementation for saga-engine v4.0,
//! handling worker failure recovery.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

use hodei_server_domain::shared_kernel::{JobId, WorkerId};
use hodei_server_domain::workers::{WorkerProvisioning, WorkerRegistry};
use hodei_shared::states::WorkerState;

use saga_engine_core::workflow::{
    DynWorkflowStep, StepCompensationError, StepError, StepErrorKind, StepResult, WorkflowConfig,
    WorkflowContext, WorkflowDefinition, WorkflowExecutor, WorkflowResult, WorkflowStep,
};

use crate::saga::bridge::worker_lifecycle::TerminateWorkerActivity;
use crate::saga::port::types::WorkflowState;

// =============================================================================
// Input/Output Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryWorkflowInput {
    pub job_id: String,
    pub failed_worker_id: String,
    pub target_provider_id: Option<String>,
    pub reason: String,
}

impl RecoveryWorkflowInput {
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
pub struct RecoveryWorkflowOutput {
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
pub enum RecoveryWorkflowError {
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

#[derive(Clone)]
pub struct CheckConnectivityActivity {
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl Debug for CheckConnectivityActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckConnectivityActivity").finish()
    }
}

impl CheckConnectivityActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl saga_engine_core::workflow::Activity for CheckConnectivityActivity {
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
            .registry
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

#[derive(Clone)]
pub struct ProvisionReplacementActivity {
    provisioning: Arc<dyn WorkerProvisioning + Send + Sync>,
}

impl Debug for ProvisionReplacementActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvisionReplacementActivity").finish()
    }
}

impl ProvisionReplacementActivity {
    pub fn new(provisioning: Arc<dyn WorkerProvisioning + Send + Sync>) -> Self {
        Self { provisioning }
    }
}

#[async_trait]
impl saga_engine_core::workflow::Activity for ProvisionReplacementActivity {
    const TYPE_ID: &'static str = "recovery-provision-replacement";

    type Input = ProvisionReplacementInput;
    type Output = ProvisionReplacementOutput;
    type Error = RecoveryWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let provider_id = if let Some(pid) = input.target_provider_id {
            hodei_server_domain::shared_kernel::ProviderId::from_uuid(
                Uuid::parse_str(&pid)
                    .map_err(|e| RecoveryWorkflowError::WorkerProvisioningFailed(e.to_string()))?,
            )
        } else {
            hodei_server_domain::shared_kernel::ProviderId::new()
        };

        let job_id = JobId::from_string(&input.job_id).ok_or_else(|| {
            RecoveryWorkflowError::WorkerProvisioningFailed(format!(
                "Invalid job ID: {}",
                input.job_id
            ))
        })?;

        let spec = hodei_server_domain::workers::WorkerSpec::new(
            "recovery-worker:latest".to_string(),
            "localhost:50051".to_string(),
        );

        let result = self
            .provisioning
            .provision_worker(&provider_id, spec, job_id)
            .await
            .map_err(|e| RecoveryWorkflowError::WorkerProvisioningFailed(e.to_string()))?;

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

#[derive(Clone)]
pub struct TransferJobActivity {
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl Debug for TransferJobActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransferJobActivity").finish()
    }
}

impl TransferJobActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl saga_engine_core::workflow::Activity for TransferJobActivity {
    const TYPE_ID: &'static str = "recovery-transfer-job";

    type Input = TransferJobInput;
    type Output = TransferJobOutput;
    type Error = RecoveryWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let worker_id = WorkerId::from_string(&input.new_worker_id).ok_or_else(|| {
            RecoveryWorkflowError::JobTransferFailed(format!(
                "Invalid worker ID: {}",
                input.new_worker_id
            ))
        })?;
        let _job_id = JobId::from_string(&input.job_id).ok_or_else(|| {
            RecoveryWorkflowError::JobTransferFailed(format!("Invalid job ID: {}", input.job_id))
        })?;

        self.registry
            .update_state(&worker_id, WorkerState::Busy)
            .await
            .map_err(|e| RecoveryWorkflowError::JobTransferFailed(e.to_string()))?;

        Ok(TransferJobOutput {
            job_id: input.job_id,
            worker_id: input.new_worker_id,
            transferred: true,
        })
    }
}

// =============================================================================
// Workflow Steps
// =============================================================================

pub struct CheckConnectivityStep {
    activity: CheckConnectivityActivity,
}

impl Debug for CheckConnectivityStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckConnectivityStep").finish()
    }
}

impl CheckConnectivityStep {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self {
            activity: CheckConnectivityActivity::new(registry),
        }
    }
}

#[async_trait]
impl WorkflowStep for CheckConnectivityStep {
    const TYPE_ID: &'static str = "recovery-check-connectivity";

    async fn execute(
        &self,
        _context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let input: RecoveryWorkflowInput = serde_json::from_value(input).map_err(|e| {
            StepError::new(
                e.to_string(),
                StepErrorKind::InvalidInput,
                Self::TYPE_ID.to_string(),
                false,
            )
        })?;

        let activity_input = CheckConnectivityInput {
            worker_id: input.failed_worker_id.clone(),
        };

        match saga_engine_core::workflow::Activity::execute(&self.activity, activity_input).await {
            Ok(output) => {
                let result_json = serde_json::to_value(&output).map_err(|e| {
                    StepError::new(
                        e.to_string(),
                        StepErrorKind::Unknown,
                        Self::TYPE_ID.to_string(),
                        false,
                    )
                })?;
                Ok(StepResult::Output(result_json))
            }
            Err(e) => Err(StepError::new(
                e.to_string(),
                StepErrorKind::ActivityFailed,
                Self::TYPE_ID.to_string(),
                true,
            )),
        }
    }

    async fn compensate(
        &self,
        _context: &mut WorkflowContext,
        _output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        Ok(())
    }
}

pub struct ProvisionReplacementWorkerStep {
    activity: ProvisionReplacementActivity,
}

impl Debug for ProvisionReplacementWorkerStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProvisionReplacementWorkerStep").finish()
    }
}

impl ProvisionReplacementWorkerStep {
    pub fn new(provisioning: Arc<dyn WorkerProvisioning + Send + Sync>) -> Self {
        Self {
            activity: ProvisionReplacementActivity::new(provisioning),
        }
    }
}

#[async_trait]
impl WorkflowStep for ProvisionReplacementWorkerStep {
    const TYPE_ID: &'static str = "recovery-provision-replacement";

    async fn execute(
        &self,
        context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let input: RecoveryWorkflowInput = serde_json::from_value(input).map_err(|e| {
            StepError::new(
                e.to_string(),
                StepErrorKind::InvalidInput,
                Self::TYPE_ID.to_string(),
                false,
            )
        })?;

        let activity_input = ProvisionReplacementInput {
            job_id: input.job_id.clone(),
            target_provider_id: input.target_provider_id.clone(),
            original_worker_spec: context.step_outputs.get("connectivity_output").cloned(),
        };

        match saga_engine_core::workflow::Activity::execute(&self.activity, activity_input).await {
            Ok(output) => {
                let result_json = serde_json::to_value(&output).map_err(|e| {
                    StepError::new(
                        e.to_string(),
                        StepErrorKind::Unknown,
                        Self::TYPE_ID.to_string(),
                        false,
                    )
                })?;
                context.set_step_output("provision_output".to_string(), result_json.clone());
                Ok(StepResult::Output(result_json))
            }
            Err(e) => Err(StepError::new(
                e.to_string(),
                StepErrorKind::ActivityFailed,
                Self::TYPE_ID.to_string(),
                true,
            )),
        }
    }

    async fn compensate(
        &self,
        _context: &mut WorkflowContext,
        output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        if let Ok(output) = serde_json::from_value::<ProvisionReplacementOutput>(output) {
            if let Some(worker_id) = WorkerId::from_string(&output.new_worker_id) {
                let _ = self.activity.provisioning.destroy_worker(&worker_id).await;
            }
        }
        Ok(())
    }
}

pub struct TransferJobStep {
    activity: TransferJobActivity,
}

impl Debug for TransferJobStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransferJobStep").finish()
    }
}

impl TransferJobStep {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self {
            activity: TransferJobActivity::new(registry),
        }
    }
}

#[async_trait]
impl WorkflowStep for TransferJobStep {
    const TYPE_ID: &'static str = "recovery-transfer-job";

    async fn execute(
        &self,
        context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let input: RecoveryWorkflowInput = serde_json::from_value(input).map_err(|e| {
            StepError::new(
                e.to_string(),
                StepErrorKind::InvalidInput,
                Self::TYPE_ID.to_string(),
                false,
            )
        })?;

        let new_worker_id = context
            .step_outputs
            .get("provision_output")
            .and_then(|v| v.get("new_worker_id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                StepError::new(
                    "Missing provision output".to_string(),
                    StepErrorKind::MissingInput,
                    Self::TYPE_ID.to_string(),
                    false,
                )
            })?
            .to_string();

        let activity_input = TransferJobInput {
            job_id: input.job_id.clone(),
            new_worker_id,
        };

        match saga_engine_core::workflow::Activity::execute(&self.activity, activity_input).await {
            Ok(output) => {
                let result_json = serde_json::to_value(&output).map_err(|e| {
                    StepError::new(
                        e.to_string(),
                        StepErrorKind::Unknown,
                        Self::TYPE_ID.to_string(),
                        false,
                    )
                })?;
                Ok(StepResult::Output(result_json))
            }
            Err(e) => Err(StepError::new(
                e.to_string(),
                StepErrorKind::ActivityFailed,
                Self::TYPE_ID.to_string(),
                true,
            )),
        }
    }

    async fn compensate(
        &self,
        _context: &mut WorkflowContext,
        _output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        Ok(())
    }
}

pub struct TerminateOldWorkerStep {
    activity: TerminateWorkerActivity,
}

impl Debug for TerminateOldWorkerStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminateOldWorkerStep").finish()
    }
}

impl TerminateOldWorkerStep {
    pub fn new(
        provisioning: Arc<dyn WorkerProvisioning + Send + Sync>,
        registry: Arc<dyn WorkerRegistry + Send + Sync>,
    ) -> Self {
        Self {
            activity: TerminateWorkerActivity::new(provisioning, registry),
        }
    }
}

#[async_trait]
impl WorkflowStep for TerminateOldWorkerStep {
    const TYPE_ID: &'static str = "recovery-terminate-old-worker";

    async fn execute(
        &self,
        _context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let input: RecoveryWorkflowInput = serde_json::from_value(input).map_err(|e| {
            StepError::new(
                e.to_string(),
                StepErrorKind::InvalidInput,
                Self::TYPE_ID.to_string(),
                false,
            )
        })?;

        let worker_id =
            WorkerId::from_string(&input.failed_worker_id).unwrap_or_else(|| WorkerId::new());

        let activity_input = crate::saga::bridge::worker_lifecycle::TerminateWorkerInput {
            worker_id,
            reason: format!("Recovery: {}", input.reason),
            destroy_infrastructure: true,
        };

        match saga_engine_core::workflow::Activity::execute(&self.activity, activity_input).await {
            Ok(output) => {
                let result_json = serde_json::to_value(&output).map_err(|e| {
                    StepError::new(
                        e.to_string(),
                        StepErrorKind::Unknown,
                        Self::TYPE_ID.to_string(),
                        false,
                    )
                })?;
                Ok(StepResult::Output(result_json))
            }
            Err(e) => Err(StepError::new(
                e.to_string(),
                StepErrorKind::ActivityFailed,
                Self::TYPE_ID.to_string(),
                false,
            )),
        }
    }

    async fn compensate(
        &self,
        _context: &mut WorkflowContext,
        _output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        Ok(())
    }
}

pub struct CancelOldWorkerStep;

impl Debug for CancelOldWorkerStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancelOldWorkerStep").finish()
    }
}

#[async_trait]
impl WorkflowStep for CancelOldWorkerStep {
    const TYPE_ID: &'static str = "recovery-cancel-old-worker";

    async fn execute(
        &self,
        _context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        Ok(StepResult::Output(input))
    }

    async fn compensate(
        &self,
        _context: &mut WorkflowContext,
        _output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        Ok(())
    }
}

// =============================================================================
// Workflow Definition
// =============================================================================

pub struct RecoveryWorkflow {
    steps: Vec<Box<dyn DynWorkflowStep>>,
    config: WorkflowConfig,
}

impl Debug for RecoveryWorkflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryWorkflow")
            .field("steps", &format!("[{} steps]", self.steps.len()))
            .field("config", &self.config)
            .finish()
    }
}

impl RecoveryWorkflow {
    pub fn new(
        registry: Arc<dyn WorkerRegistry + Send + Sync>,
        provisioning: Arc<dyn WorkerProvisioning + Send + Sync>,
    ) -> Self {
        let steps = vec![
            Box::new(CheckConnectivityStep::new(registry.clone())) as Box<dyn DynWorkflowStep>,
            Box::new(ProvisionReplacementWorkerStep::new(provisioning.clone()))
                as Box<dyn DynWorkflowStep>,
            Box::new(TransferJobStep::new(registry.clone())) as Box<dyn DynWorkflowStep>,
            Box::new(TerminateOldWorkerStep::new(
                provisioning.clone(),
                registry.clone(),
            )) as Box<dyn DynWorkflowStep>,
            Box::new(CancelOldWorkerStep) as Box<dyn DynWorkflowStep>,
        ];

        let config = WorkflowConfig {
            default_timeout_secs: 300,
            max_step_retries: 3,
            retry_backoff_multiplier: 2.0,
            retry_base_delay_ms: 1000,
            task_queue: "recovery".to_string(),
        };

        Self { steps, config }
    }
}

impl Default for RecoveryWorkflow {
    fn default() -> Self {
        Self {
            steps: Vec::new(),
            config: WorkflowConfig {
                default_timeout_secs: 300,
                max_step_retries: 3,
                retry_backoff_multiplier: 2.0,
                retry_base_delay_ms: 1000,
                task_queue: "recovery".to_string(),
            },
        }
    }
}

impl WorkflowDefinition for RecoveryWorkflow {
    const TYPE_ID: &'static str = "recovery";
    const VERSION: u32 = 1;

    type Input = RecoveryWorkflowInput;
    type Output = RecoveryWorkflowOutput;

    fn steps(&self) -> &[Box<dyn DynWorkflowStep>] {
        &self.steps
    }

    fn configuration(&self) -> WorkflowConfig {
        self.config.clone()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_workflow_type_id() {
        assert_eq!(RecoveryWorkflow::TYPE_ID, "recovery");
        assert_eq!(RecoveryWorkflow::VERSION, 1);
    }

    #[test]
    fn test_recovery_workflow_has_five_steps() {
        use crate::workers::provisioning::MockWorkerProvisioning;
        use crate::workers::provisioning::MockWorkerRegistry;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let registry: Arc<dyn WorkerRegistry + Send + Sync> = Arc::new(MockWorkerRegistry::new());
        let provisioning: Arc<dyn WorkerProvisioning + Send + Sync> =
            Arc::new(MockWorkerProvisioning::new());

        let workflow = RecoveryWorkflow::new(registry, provisioning);
        let steps = workflow.steps();

        assert_eq!(steps.len(), 5);
        assert_eq!(steps[0].step_type_id(), "recovery-check-connectivity");
        assert_eq!(steps[1].step_type_id(), "recovery-provision-replacement");
        assert_eq!(steps[2].step_type_id(), "recovery-transfer-job");
        assert_eq!(steps[3].step_type_id(), "recovery-terminate-old-worker");
        assert_eq!(steps[4].step_type_id(), "recovery-cancel-old-worker");
    }

    #[test]
    fn test_workflow_config_default() {
        let config = WorkflowConfig::default();
        assert_eq!(config.default_timeout_secs, 3600);
    }
}

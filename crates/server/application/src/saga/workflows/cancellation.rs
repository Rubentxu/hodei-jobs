//!
//! # Cancellation Workflow for saga-engine v4
//!
//! This module provides CancellationWorkflow implementation for saga-engine v4.0,
//! handling job cancellation.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;

use hodei_server_domain::shared_kernel::{JobId, JobState, WorkerId};
use hodei_server_domain::workers::WorkerRegistry;
use hodei_shared::states::WorkerState;

use saga_engine_core::workflow::{
    DynWorkflowStep, StepCompensationError, StepError, StepErrorKind, StepResult, WorkflowConfig,
    WorkflowContext, WorkflowDefinition, WorkflowStep,
};

use crate::saga::bridge::worker_lifecycle::{SetWorkerBusyActivity, SetWorkerReadyActivity};

// =============================================================================
// Input/Output Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancellationWorkflowInput {
    pub job_id: String,
    pub reason: String,
    pub requester: Option<String>,
}

impl CancellationWorkflowInput {
    pub fn new(job_id: JobId, reason: impl Into<String>) -> Self {
        Self {
            job_id: job_id.to_string(),
            reason: reason.into(),
            requester: None,
        }
    }

    pub fn with_requester(mut self, requester: impl Into<String>) -> Self {
        self.requester = Some(requester.into());
        self
    }

    pub fn idempotency_key(&self) -> String {
        format!("cancellation:{}:{}", self.job_id, self.reason)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancellationWorkflowOutput {
    pub job_id: String,
    pub original_state: JobState,
    pub cancelled_at: chrono::DateTime<chrono::Utc>,
    pub worker_released: bool,
    pub notified: bool,
}

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum CancellationWorkflowError {
    #[error("Job validation failed: {0}")]
    JobValidationFailed(String),

    #[error("Worker notification failed: {0}")]
    WorkerNotificationFailed(String),

    #[error("Job state update failed: {0}")]
    JobStateUpdateFailed(String),

    #[error("Worker release failed: {0}")]
    WorkerReleaseFailed(String),

    #[error("Compensation failed: {0}")]
    CompensationFailed(String),
}

// =============================================================================
// Activities
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateCancellationInput {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateCancellationOutput {
    pub job_id: String,
    pub is_cancellable: bool,
    pub current_state: JobState,
    pub worker_id: Option<String>,
}

#[derive(Clone)]
pub struct ValidateCancellationActivity {
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl Debug for ValidateCancellationActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateCancellationActivity").finish()
    }
}

impl ValidateCancellationActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl saga_engine_core::workflow::Activity for ValidateCancellationActivity {
    const TYPE_ID: &'static str = "cancellation-validate";

    type Input = ValidateCancellationInput;
    type Output = ValidateCancellationOutput;
    type Error = CancellationWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let job_id = JobId::from_string(&input.job_id).ok_or_else(|| {
            CancellationWorkflowError::JobValidationFailed(format!(
                "Invalid job ID: {}",
                input.job_id
            ))
        })?;
        let worker_id = self
            .registry
            .find_by_job(&job_id)
            .await
            .map_err(|e| CancellationWorkflowError::JobValidationFailed(e.to_string()))?;

        let is_cancellable = matches!(
            worker_id.as_ref().map(|w| w.state()),
            Some(WorkerState::Ready) | Some(WorkerState::Creating) | None
        );

        Ok(ValidateCancellationOutput {
            job_id: input.job_id,
            is_cancellable,
            current_state: JobState::Pending,
            worker_id: worker_id.map(|w| w.id().to_string()),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyWorkerInput {
    pub worker_id: String,
    pub job_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyWorkerOutput {
    pub worker_id: String,
    pub notified: bool,
}

#[derive(Clone)]
pub struct NotifyWorkerActivity {
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl Debug for NotifyWorkerActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotifyWorkerActivity").finish()
    }
}

impl NotifyWorkerActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl saga_engine_core::workflow::Activity for NotifyWorkerActivity {
    const TYPE_ID: &'static str = "cancellation-notify-worker";

    type Input = NotifyWorkerInput;
    type Output = NotifyWorkerOutput;
    type Error = CancellationWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let worker_id = WorkerId::from_string(&input.worker_id).ok_or_else(|| {
            CancellationWorkflowError::WorkerNotificationFailed(format!(
                "Invalid worker ID: {}",
                input.worker_id
            ))
        })?;
        let _job_id = JobId::from_string(&input.job_id).ok_or_else(|| {
            CancellationWorkflowError::WorkerNotificationFailed(format!(
                "Invalid job ID: {}",
                input.job_id
            ))
        })?;

        self.registry
            .update_state(&worker_id, WorkerState::Ready)
            .await
            .map_err(|e| CancellationWorkflowError::WorkerNotificationFailed(e.to_string()))?;

        Ok(NotifyWorkerOutput {
            worker_id: input.worker_id,
            notified: true,
        })
    }
}

// =============================================================================
// Workflow Steps
// =============================================================================

pub struct ValidateCancellationStep {
    activity: ValidateCancellationActivity,
}

impl Debug for ValidateCancellationStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateCancellationStep").finish()
    }
}

impl ValidateCancellationStep {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self {
            activity: ValidateCancellationActivity::new(registry),
        }
    }
}

#[async_trait]
impl WorkflowStep for ValidateCancellationStep {
    const TYPE_ID: &'static str = "cancellation-validate";

    async fn execute(
        &self,
        context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let input: CancellationWorkflowInput = serde_json::from_value(input).map_err(|e| {
            StepError::new(
                e.to_string(),
                StepErrorKind::InvalidInput,
                Self::TYPE_ID.to_string(),
                false,
            )
        })?;

        let activity_input = ValidateCancellationInput {
            job_id: input.job_id.clone(),
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
                context.set_step_output("validation_output".to_string(), result_json.clone());
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

pub struct NotifyWorkerStep {
    activity: NotifyWorkerActivity,
}

impl Debug for NotifyWorkerStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotifyWorkerStep").finish()
    }
}

impl NotifyWorkerStep {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self {
            activity: NotifyWorkerActivity::new(registry),
        }
    }
}

#[async_trait]
impl WorkflowStep for NotifyWorkerStep {
    const TYPE_ID: &'static str = "cancellation-notify-worker";

    async fn execute(
        &self,
        context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let input: CancellationWorkflowInput = serde_json::from_value(input).map_err(|e| {
            StepError::new(
                e.to_string(),
                StepErrorKind::InvalidInput,
                Self::TYPE_ID.to_string(),
                false,
            )
        })?;

        let worker_id = context
            .step_outputs
            .get("validation_output")
            .and_then(|v| v.get("worker_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(worker_id) = worker_id {
            let activity_input = NotifyWorkerInput {
                worker_id,
                job_id: input.job_id.clone(),
                reason: input.reason.clone(),
            };

            match saga_engine_core::workflow::Activity::execute(&self.activity, activity_input)
                .await
            {
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
        } else {
            Ok(StepResult::Output(serde_json::json!({
                "worker_id": null,
                "notified": false
            })))
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

pub struct UpdateJobStateStep;

impl Debug for UpdateJobStateStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateJobStateStep").finish()
    }
}

#[async_trait]
impl WorkflowStep for UpdateJobStateStep {
    const TYPE_ID: &'static str = "cancellation-update-job-state";

    async fn execute(
        &self,
        _context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        Ok(StepResult::Output(serde_json::json!({
            "job_id": input["job_id"],
            "state": "CANCELLED",
            "cancelled_at": chrono::Utc::now().to_rfc3339()
        })))
    }

    async fn compensate(
        &self,
        _context: &mut WorkflowContext,
        _output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        Ok(())
    }
}

pub struct ReleaseWorkerStep;

impl Debug for ReleaseWorkerStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReleaseWorkerStep").finish()
    }
}

#[async_trait]
impl WorkflowStep for ReleaseWorkerStep {
    const TYPE_ID: &'static str = "cancellation-release-worker";

    async fn execute(
        &self,
        _context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        Ok(StepResult::Output(serde_json::json!({
            "released": true,
            "released_at": chrono::Utc::now().to_rfc3339()
        })))
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

pub struct CancellationWorkflow {
    steps: Vec<Box<dyn DynWorkflowStep>>,
    config: WorkflowConfig,
}

impl Debug for CancellationWorkflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancellationWorkflow")
            .field("steps", &format!("[{} steps]", self.steps.len()))
            .field("config", &self.config)
            .finish()
    }
}

impl CancellationWorkflow {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        let steps = vec![
            Box::new(ValidateCancellationStep::new(registry.clone())) as Box<dyn DynWorkflowStep>,
            Box::new(NotifyWorkerStep::new(registry.clone())) as Box<dyn DynWorkflowStep>,
            Box::new(UpdateJobStateStep) as Box<dyn DynWorkflowStep>,
            Box::new(ReleaseWorkerStep) as Box<dyn DynWorkflowStep>,
        ];

        let config = WorkflowConfig {
            default_timeout_secs: 30,
            max_step_retries: 3,
            retry_backoff_multiplier: 2.0,
            retry_base_delay_ms: 1000,
            task_queue: "cancellation".to_string(),
        };

        Self { steps, config }
    }
}

impl Default for CancellationWorkflow {
    fn default() -> Self {
        Self {
            steps: Vec::new(),
            config: WorkflowConfig {
                default_timeout_secs: 30,
                max_step_retries: 3,
                retry_backoff_multiplier: 2.0,
                retry_base_delay_ms: 1000,
                task_queue: "cancellation".to_string(),
            },
        }
    }
}

impl WorkflowDefinition for CancellationWorkflow {
    const TYPE_ID: &'static str = "cancellation";
    const VERSION: u32 = 1;

    type Input = CancellationWorkflowInput;
    type Output = CancellationWorkflowOutput;

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
    fn test_cancellation_workflow_type_id() {
        assert_eq!(CancellationWorkflow::TYPE_ID, "cancellation");
        assert_eq!(CancellationWorkflow::VERSION, 1);
    }

    #[test]
    fn test_cancellation_workflow_has_four_steps() {
        use crate::workers::provisioning::MockWorkerRegistry;
        use std::sync::Arc;

        let registry: Arc<dyn WorkerRegistry + Send + Sync> = Arc::new(MockWorkerRegistry::new());

        let workflow = CancellationWorkflow::new(registry);
        let steps = workflow.steps();

        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].step_type_id(), "cancellation-validate");
        assert_eq!(steps[1].step_type_id(), "cancellation-notify-worker");
        assert_eq!(steps[2].step_type_id(), "cancellation-update-job-state");
        assert_eq!(steps[3].step_type_id(), "cancellation-release-worker");
    }
}

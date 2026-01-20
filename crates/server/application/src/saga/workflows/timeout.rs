//!
//! # Timeout Workflow for saga-engine v4
//!
//! This module provides TimeoutWorkflow implementation for saga-engine v4.0,
//! handling job timeout detection and recovery.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

use hodei_server_domain::shared_kernel::{JobId, WorkerId};
use hodei_server_domain::workers::{WorkerProvisioning, WorkerRegistry};
use hodei_shared::states::WorkerState;

use saga_engine_core::workflow::{
    DynWorkflowStep, StepCompensationError, StepError, StepErrorKind, StepResult, WorkflowConfig,
    WorkflowContext, WorkflowDefinition, WorkflowStep,
};

use crate::saga::bridge::worker_lifecycle::TerminateWorkerActivity;

// =============================================================================
// Input/Output Types
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutWorkflowInput {
    pub job_id: String,
    pub timeout_duration: Duration,
    pub reason: String,
}

impl TimeoutWorkflowInput {
    pub fn new(job_id: JobId, timeout_duration: Duration, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            job_id: job_id.to_string(),
            timeout_duration,
            reason,
        }
    }

    pub fn idempotency_key(&self) -> String {
        format!("timeout:{}:{}", self.job_id, self.reason)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutWorkflowOutput {
    pub job_id: String,
    pub worker_terminated: bool,
    pub job_marked_failed: bool,
    pub worker_released: bool,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum TimeoutWorkflowError {
    #[error("Timeout validation failed: {0}")]
    TimeoutValidationFailed(String),

    #[error("Worker termination failed: {0}")]
    WorkerTerminationFailed(String),

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
pub struct ValidateTimeoutInput {
    pub job_id: String,
    pub timeout_duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateTimeoutOutput {
    pub job_id: String,
    pub has_timed_out: bool,
    pub worker_id: Option<String>,
    pub elapsed_seconds: u64,
}

#[derive(Clone)]
pub struct ValidateTimeoutActivity {
    registry: Arc<dyn WorkerRegistry + Send + Sync>,
}

impl Debug for ValidateTimeoutActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateTimeoutActivity").finish()
    }
}

impl ValidateTimeoutActivity {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl saga_engine_core::workflow::Activity for ValidateTimeoutActivity {
    const TYPE_ID: &'static str = "timeout-validate";

    type Input = ValidateTimeoutInput;
    type Output = ValidateTimeoutOutput;
    type Error = TimeoutWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let job_id = JobId::from_string(&input.job_id).ok_or_else(|| {
            TimeoutWorkflowError::TimeoutValidationFailed(format!(
                "Invalid job ID: {}",
                input.job_id
            ))
        })?;
        let worker = self
            .registry
            .find_by_job(&job_id)
            .await
            .map_err(|e| TimeoutWorkflowError::TimeoutValidationFailed(e.to_string()))?;

        let elapsed_seconds = input.timeout_duration.as_secs();

        Ok(ValidateTimeoutOutput {
            job_id: input.job_id,
            has_timed_out: worker.is_some(),
            worker_id: worker.map(|w| w.id().to_string()),
            elapsed_seconds,
        })
    }
}

// =============================================================================
// Workflow Steps
// =============================================================================

pub struct ValidateTimeoutStep {
    activity: ValidateTimeoutActivity,
}

impl Debug for ValidateTimeoutStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateTimeoutStep").finish()
    }
}

impl ValidateTimeoutStep {
    pub fn new(registry: Arc<dyn WorkerRegistry + Send + Sync>) -> Self {
        Self {
            activity: ValidateTimeoutActivity::new(registry),
        }
    }
}

#[async_trait]
impl WorkflowStep for ValidateTimeoutStep {
    const TYPE_ID: &'static str = "timeout-validate";

    async fn execute(
        &self,
        context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let input: TimeoutWorkflowInput = serde_json::from_value(input).map_err(|e| {
            StepError::new(
                e.to_string(),
                StepErrorKind::InvalidInput,
                Self::TYPE_ID.to_string(),
                false,
            )
        })?;

        let activity_input = ValidateTimeoutInput {
            job_id: input.job_id.clone(),
            timeout_duration: input.timeout_duration,
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

pub struct TerminateWorkerStep {
    activity: TerminateWorkerActivity,
}

impl Debug for TerminateWorkerStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminateWorkerStep").finish()
    }
}

impl TerminateWorkerStep {
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
impl WorkflowStep for TerminateWorkerStep {
    const TYPE_ID: &'static str = "timeout-terminate-worker";

    async fn execute(
        &self,
        context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let worker_id_opt = context
            .step_outputs
            .get("validation_output")
            .and_then(|v| v.get("worker_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(worker_id_str) = worker_id_opt {
            let worker_id = WorkerId::from_string(&worker_id_str).ok_or_else(|| {
                StepError::new(
                    format!("Invalid worker ID: {}", worker_id_str),
                    StepErrorKind::InvalidInput,
                    Self::TYPE_ID.to_string(),
                    false,
                )
            })?;
            let activity_input = crate::saga::bridge::worker_lifecycle::TerminateWorkerInput {
                worker_id,
                reason: format!("Timeout: {}", input["reason"].as_str().unwrap_or("unknown")),
                destroy_infrastructure: true,
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
                    false,
                )),
            }
        } else {
            Ok(StepResult::Output(serde_json::json!({
                "worker_id": null,
                "terminated": false
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

pub struct MarkJobFailedStep;

impl Debug for MarkJobFailedStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarkJobFailedStep").finish()
    }
}

#[async_trait]
impl WorkflowStep for MarkJobFailedStep {
    const TYPE_ID: &'static str = "timeout-mark-job-failed";

    async fn execute(
        &self,
        _context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        Ok(StepResult::Output(serde_json::json!({
            "job_id": input["job_id"],
            "state": "FAILED",
            "reason": "TIMEOUT",
            "failed_at": chrono::Utc::now().to_rfc3339()
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
    const TYPE_ID: &'static str = "timeout-release-worker";

    async fn execute(
        &self,
        _context: &mut WorkflowContext,
        _input: serde_json::Value,
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

pub struct TimeoutWorkflow {
    steps: Vec<Box<dyn DynWorkflowStep>>,
    config: WorkflowConfig,
}

impl Debug for TimeoutWorkflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TimeoutWorkflow")
            .field("steps", &format!("[{} steps]", self.steps.len()))
            .field("config", &self.config)
            .finish()
    }
}

impl TimeoutWorkflow {
    pub fn new(
        registry: Arc<dyn WorkerRegistry + Send + Sync>,
        provisioning: Arc<dyn WorkerProvisioning + Send + Sync>,
    ) -> Self {
        let steps = vec![
            Box::new(ValidateTimeoutStep::new(registry.clone())) as Box<dyn DynWorkflowStep>,
            Box::new(TerminateWorkerStep::new(
                provisioning.clone(),
                registry.clone(),
            )) as Box<dyn DynWorkflowStep>,
            Box::new(MarkJobFailedStep) as Box<dyn DynWorkflowStep>,
            Box::new(ReleaseWorkerStep) as Box<dyn DynWorkflowStep>,
        ];

        let config = WorkflowConfig {
            default_timeout_secs: 30,
            max_step_retries: 3,
            retry_backoff_multiplier: 2.0,
            retry_base_delay_ms: 1000,
            task_queue: "timeout".to_string(),
        };

        Self { steps, config }
    }
}

impl Default for TimeoutWorkflow {
    fn default() -> Self {
        Self {
            steps: Vec::new(),
            config: WorkflowConfig {
                default_timeout_secs: 30,
                max_step_retries: 3,
                retry_backoff_multiplier: 2.0,
                retry_base_delay_ms: 1000,
                task_queue: "timeout".to_string(),
            },
        }
    }
}

impl WorkflowDefinition for TimeoutWorkflow {
    const TYPE_ID: &'static str = "timeout";
    const VERSION: u32 = 1;

    type Input = TimeoutWorkflowInput;
    type Output = TimeoutWorkflowOutput;

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
    fn test_timeout_workflow_type_id() {
        assert_eq!(TimeoutWorkflow::TYPE_ID, "timeout");
        assert_eq!(TimeoutWorkflow::VERSION, 1);
    }

    #[test]
    fn test_timeout_workflow_has_four_steps() {
        use crate::workers::provisioning::MockWorkerProvisioning;
        use crate::workers::provisioning::MockWorkerRegistry;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let registry: Arc<dyn WorkerRegistry + Send + Sync> = Arc::new(MockWorkerRegistry::new());
        let provisioning: Arc<dyn WorkerProvisioning + Send + Sync> =
            Arc::new(MockWorkerProvisioning::new());

        let workflow = TimeoutWorkflow::new(registry, provisioning);
        let steps = workflow.steps();

        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].step_type_id(), "timeout-validate");
        assert_eq!(steps[1].step_type_id(), "timeout-terminate-worker");
        assert_eq!(steps[2].step_type_id(), "timeout-mark-job-failed");
        assert_eq!(steps[3].step_type_id(), "timeout-release-worker");
    }
}

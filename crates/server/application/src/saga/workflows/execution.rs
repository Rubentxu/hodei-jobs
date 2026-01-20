//!
//! # Execution Workflow for saga-engine v4
//!
//! This module provides the ExecutionWorkflow implementation for saga-engine v4.0,
//! migrating from the legacy ExecutionSaga to the new Event Sourcing based workflow.
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

use saga_engine_core::workflow::{
    DynWorkflowStep, StepCompensationError, StepError, StepErrorKind, StepResult, WorkflowConfig,
    WorkflowContext, WorkflowDefinition, WorkflowExecutor, WorkflowResult, WorkflowStep,
};

use crate::saga::port::types::WorkflowState;

// =============================================================================
// Input/Output Types
// =============================================================================

/// Input for the Execution Workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionWorkflowInput {
    /// Job ID to execute
    pub job_id: String,
    /// Worker ID to dispatch to
    pub worker_id: String,
    /// Command to execute
    pub command: String,
    /// Arguments for the command
    pub arguments: Vec<String>,
    /// Environment variables
    pub env: Vec<EnvVarData>,
    /// Working directory
    pub working_dir: Option<String>,
    /// Timeout in seconds
    pub timeout_seconds: u64,
    /// Idempotency key for deduplication
    pub idempotency_key: Option<String>,
}

impl ExecutionWorkflowInput {
    /// Create a new input with auto-generated idempotency key
    pub fn new(job_id: String, worker_id: String, command: String, arguments: Vec<String>) -> Self {
        let idempotency_key = Some(format!("execution:{}:{}", worker_id, Uuid::new_v4()));
        Self {
            job_id,
            worker_id,
            command,
            arguments,
            env: vec![],
            working_dir: None,
            timeout_seconds: 3600,
            idempotency_key,
        }
    }
}

/// Environment variable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVarData {
    pub key: String,
    pub value: String,
}

/// Output of the Execution Workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionWorkflowOutput {
    /// The job ID
    pub job_id: String,
    /// The worker ID
    pub worker_id: String,
    /// Job result
    pub result: JobResultData,
    /// Exit code
    pub exit_code: i32,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
}

/// Job result data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobResultData {
    Success,
    Failed { exit_code: i32 },
    Timeout,
    Cancelled,
    Error { message: String },
}

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum ExecutionWorkflowError {
    #[error("Job validation failed: {0}")]
    JobValidationFailed(String),

    #[error("Job dispatch failed: {0}")]
    JobDispatchFailed(String),

    #[error("Result collection failed: {0}")]
    ResultCollectionFailed(String),

    #[error("Compensation failed: {0}")]
    CompensationFailed(String),
}

// =============================================================================
// Execution Port Trait
// =============================================================================

/// Port for job execution operations
///
/// This trait abstracts the job execution operations that the workflow needs.
/// Implementations can use different backends (e.g., saga dispatcher, direct gRPC).
#[async_trait]
pub trait JobExecutionPort: Send + Sync {
    /// Validate that a job can be executed
    async fn validate_job(&self, job_id: &str) -> Result<bool, String>;

    /// Dispatch a job to a worker
    async fn dispatch_job(
        &self,
        job_id: &str,
        worker_id: &str,
        command: &str,
        arguments: &[String],
        timeout_secs: u64,
    ) -> Result<JobResultData, String>;

    /// Collect the result of a job execution
    async fn collect_result(
        &self,
        job_id: &str,
        timeout_secs: u64,
    ) -> Result<CollectedResult, String>;

    /// Cancel a running job
    async fn cancel_job(&self, job_id: &str) -> Result<(), String>;
}

/// Result collected from job execution
#[derive(Debug, Clone)]
pub struct CollectedResult {
    pub job_id: String,
    pub worker_id: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub result: JobResultData,
}

// =============================================================================
// Activities
// =============================================================================

/// Activity for validating job
pub struct ValidateJobActivity {
    port: Arc<dyn JobExecutionPort>,
}

impl std::fmt::Debug for ValidateJobActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateJobActivity")
            .field("port", &"<dyn JobExecutionPort>")
            .finish()
    }
}

impl ValidateJobActivity {
    pub fn new(port: Arc<dyn JobExecutionPort>) -> Self {
        Self { port }
    }
}

#[async_trait]
impl saga_engine_core::workflow::Activity for ValidateJobActivity {
    const TYPE_ID: &'static str = "execution-validate-job";

    type Input = ValidateJobInput;
    type Output = ValidateJobOutput;
    type Error = ExecutionWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        // Validate job exists and is in valid state
        let is_valid = self
            .port
            .validate_job(&input.job_id)
            .await
            .map_err(|e| ExecutionWorkflowError::JobValidationFailed(e))?;

        Ok(ValidateJobOutput {
            job_id: input.job_id,
            is_valid,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateJobInput {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateJobOutput {
    pub job_id: String,
    pub is_valid: bool,
}

/// Activity for dispatching job
pub struct DispatchJobActivity {
    port: Arc<dyn JobExecutionPort>,
}

impl std::fmt::Debug for DispatchJobActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DispatchJobActivity")
            .field("port", &"<dyn JobExecutionPort>")
            .finish()
    }
}

impl DispatchJobActivity {
    pub fn new(port: Arc<dyn JobExecutionPort>) -> Self {
        Self { port }
    }
}

#[async_trait]
impl saga_engine_core::workflow::Activity for DispatchJobActivity {
    const TYPE_ID: &'static str = "execution-dispatch-job";

    type Input = DispatchJobInput;
    type Output = DispatchJobOutput;
    type Error = ExecutionWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        // Dispatch the job
        let result = self
            .port
            .dispatch_job(
                &input.job_id,
                &input.worker_id,
                &input.command,
                &input.arguments,
                input.timeout_seconds,
            )
            .await
            .map_err(|e| ExecutionWorkflowError::JobDispatchFailed(e))?;

        Ok(DispatchJobOutput {
            job_id: input.job_id,
            worker_id: input.worker_id,
            dispatched: true,
            result,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchJobInput {
    pub job_id: String,
    pub worker_id: String,
    pub command: String,
    pub arguments: Vec<String>,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchJobOutput {
    pub job_id: String,
    pub worker_id: String,
    pub dispatched: bool,
    pub result: JobResultData,
}

/// Activity for collecting result
pub struct CollectResultActivity {
    port: Arc<dyn JobExecutionPort>,
}

impl std::fmt::Debug for CollectResultActivity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollectResultActivity")
            .field("port", &"<dyn JobExecutionPort>")
            .finish()
    }
}

impl CollectResultActivity {
    pub fn new(port: Arc<dyn JobExecutionPort>) -> Self {
        Self { port }
    }
}

#[async_trait]
impl saga_engine_core::workflow::Activity for CollectResultActivity {
    const TYPE_ID: &'static str = "execution-collect-result";

    type Input = CollectResultInput;
    type Output = CollectResultOutput;
    type Error = ExecutionWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        // Wait for and collect the result
        let result = self
            .port
            .collect_result(&input.job_id, input.timeout_seconds)
            .await
            .map_err(|e| ExecutionWorkflowError::ResultCollectionFailed(e))?;

        Ok(CollectResultOutput {
            job_id: input.job_id,
            worker_id: result.worker_id,
            exit_code: result.exit_code,
            duration_ms: result.duration_ms,
            result: result.result,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectResultInput {
    pub job_id: String,
    pub worker_id: String,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectResultOutput {
    pub job_id: String,
    pub worker_id: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub result: JobResultData,
}

// =============================================================================
// Workflow Steps
// =============================================================================

/// Step for validating job
pub struct ValidateJobStep {
    activity: ValidateJobActivity,
}

impl std::fmt::Debug for ValidateJobStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidateJobStep")
            .field("activity", &self.activity)
            .finish()
    }
}

impl ValidateJobStep {
    pub fn new(port: Arc<dyn JobExecutionPort>) -> Self {
        Self {
            activity: ValidateJobActivity::new(port),
        }
    }
}

#[async_trait]
impl WorkflowStep for ValidateJobStep {
    const TYPE_ID: &'static str = "execution-validate-job";

    async fn execute(
        &self,
        _context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let input: ExecutionWorkflowInput = serde_json::from_value(input).map_err(|e| {
            StepError::new(
                e.to_string(),
                StepErrorKind::InvalidInput,
                Self::TYPE_ID.to_string(),
                false,
            )
        })?;

        let activity_input = ValidateJobInput {
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
        // No compensation needed for validation step
        Ok(())
    }
}

/// Step for dispatching job
pub struct DispatchJobStep {
    activity: DispatchJobActivity,
}

impl std::fmt::Debug for DispatchJobStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DispatchJobStep")
            .field("activity", &self.activity)
            .finish()
    }
}

impl DispatchJobStep {
    pub fn new(port: Arc<dyn JobExecutionPort>) -> Self {
        Self {
            activity: DispatchJobActivity::new(port),
        }
    }
}

#[async_trait]
impl WorkflowStep for DispatchJobStep {
    const TYPE_ID: &'static str = "execution-dispatch-job";

    async fn execute(
        &self,
        _context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        let input: ExecutionWorkflowInput = serde_json::from_value(input).map_err(|e| {
            StepError::new(
                e.to_string(),
                StepErrorKind::InvalidInput,
                Self::TYPE_ID.to_string(),
                false,
            )
        })?;

        let activity_input = DispatchJobInput {
            job_id: input.job_id.clone(),
            worker_id: input.worker_id.clone(),
            command: input.command,
            arguments: input.arguments,
            timeout_seconds: input.timeout_seconds,
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
        output: serde_json::Value,
    ) -> Result<(), StepCompensationError> {
        // Cancel the job on compensation
        let output: DispatchJobOutput = serde_json::from_value(output).map_err(|e| {
            StepCompensationError::new(e.to_string(), Self::TYPE_ID.to_string(), true)
        })?;

        let _ = self.activity.port.cancel_job(&output.job_id).await;

        Ok(())
    }
}

/// Step for collecting result
pub struct CollectResultStep {
    activity: CollectResultActivity,
}

impl std::fmt::Debug for CollectResultStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollectResultStep")
            .field("activity", &self.activity)
            .finish()
    }
}

impl CollectResultStep {
    pub fn new(port: Arc<dyn JobExecutionPort>) -> Self {
        Self {
            activity: CollectResultActivity::new(port),
        }
    }
}

#[async_trait]
impl WorkflowStep for CollectResultStep {
    const TYPE_ID: &'static str = "execution-collect-result";

    async fn execute(
        &self,
        context: &mut WorkflowContext,
        input: serde_json::Value,
    ) -> Result<StepResult, StepError> {
        // Get job_id and worker_id from the dispatch step output
        let dispatch_output: DispatchJobOutput = context
            .step_outputs
            .get("execution-dispatch-job")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .ok_or_else(|| {
                StepError::new(
                    "Missing dispatch output".to_string(),
                    StepErrorKind::MissingInput,
                    Self::TYPE_ID.to_string(),
                    false,
                )
            })?;

        let input: ExecutionWorkflowInput = serde_json::from_value(input).map_err(|e| {
            StepError::new(
                e.to_string(),
                StepErrorKind::InvalidInput,
                Self::TYPE_ID.to_string(),
                false,
            )
        })?;

        let activity_input = CollectResultInput {
            job_id: dispatch_output.job_id,
            worker_id: dispatch_output.worker_id,
            timeout_seconds: input.timeout_seconds,
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
        // Result collection compensation is typically a no-op
        // The job result has already been recorded
        Ok(())
    }
}

// =============================================================================
// Workflow Definition
// =============================================================================

/// Execution Workflow Definition for saga-engine v4
pub struct ExecutionWorkflow {
    steps: Vec<Box<dyn DynWorkflowStep>>,
    config: WorkflowConfig,
}

impl std::fmt::Debug for ExecutionWorkflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionWorkflow")
            .field("steps", &format!("[{} steps]", self.steps.len()))
            .field("config", &self.config)
            .finish()
    }
}

impl ExecutionWorkflow {
    /// Create a new ExecutionWorkflow
    pub fn new(port: Arc<dyn JobExecutionPort>) -> Self {
        let steps = vec![
            Box::new(ValidateJobStep::new(port.clone())) as Box<dyn DynWorkflowStep>,
            Box::new(DispatchJobStep::new(port.clone())) as Box<dyn DynWorkflowStep>,
            Box::new(CollectResultStep::new(port.clone())) as Box<dyn DynWorkflowStep>,
        ];

        let config = WorkflowConfig {
            default_timeout_secs: 3600,
            max_step_retries: 3,
            retry_backoff_multiplier: 2.0,
            retry_base_delay_ms: 1000,
            task_queue: "execution".to_string(),
        };

        Self { steps, config }
    }
}

impl WorkflowDefinition for ExecutionWorkflow {
    const TYPE_ID: &'static str = "execution";
    const VERSION: u32 = 1;

    type Input = ExecutionWorkflowInput;
    type Output = ExecutionWorkflowOutput;

    fn steps(&self) -> &[Box<dyn DynWorkflowStep>] {
        &self.steps
    }

    fn configuration(&self) -> WorkflowConfig {
        self.config.clone()
    }
}

// =============================================================================
// Workflow Executor Builder
// =============================================================================

/// Builder for ExecutionWorkflow executor
pub struct ExecutionWorkflowExecutorBuilder {
    port: Option<Arc<dyn JobExecutionPort>>,
}

impl std::fmt::Debug for ExecutionWorkflowExecutorBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionWorkflowExecutorBuilder")
            .field("port", &self.port.is_some())
            .finish()
    }
}

impl ExecutionWorkflowExecutorBuilder {
    pub fn new() -> Self {
        Self { port: None }
    }

    pub fn with_port(mut self, port: Arc<dyn JobExecutionPort>) -> Self {
        self.port = Some(port);
        self
    }

    pub fn build(self) -> Result<WorkflowExecutor<ExecutionWorkflow>, BuildError> {
        let port = self.port.ok_or(BuildError::MissingPort)?;

        let workflow = ExecutionWorkflow::new(port);
        Ok(WorkflowExecutor::new(workflow))
    }
}

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("JobExecutionPort is required")]
    MissingPort,
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Mock implementation of JobExecutionPort for testing
    struct MockJobExecutionPort;

    #[async_trait]
    impl JobExecutionPort for MockJobExecutionPort {
        async fn validate_job(&self, _job_id: &str) -> Result<bool, String> {
            Ok(true)
        }

        async fn dispatch_job(
            &self,
            _job_id: &str,
            _worker_id: &str,
            _command: &str,
            _arguments: &[String],
            _timeout_secs: u64,
        ) -> Result<JobResultData, String> {
            Ok(JobResultData::Success)
        }

        async fn collect_result(
            &self,
            _job_id: &str,
            _timeout_secs: u64,
        ) -> Result<CollectedResult, String> {
            Ok(CollectedResult {
                job_id: "job-1".to_string(),
                worker_id: "worker-1".to_string(),
                exit_code: 0,
                duration_ms: 100,
                result: JobResultData::Success,
            })
        }

        async fn cancel_job(&self, _job_id: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn test_execution_workflow_input() {
        let input = ExecutionWorkflowInput::new(
            "job-123".to_string(),
            "worker-456".to_string(),
            "/bin/ls".to_string(),
            vec!["-la".to_string()],
        );

        assert!(input.idempotency_key.is_some());
        assert_eq!(input.job_id, "job-123");
        assert_eq!(input.worker_id, "worker-456");
        assert_eq!(input.command, "/bin/ls");
        assert_eq!(input.arguments, vec!["-la"]);
    }

    #[test]
    fn test_execution_workflow_config() {
        let config = WorkflowConfig::default();
        assert_eq!(config.default_timeout_secs, 3600);
        assert_eq!(config.max_step_retries, 3);
    }

    #[test]
    fn test_job_result_data_variants() {
        assert!(matches!(JobResultData::Success, JobResultData::Success));

        let failed = JobResultData::Failed { exit_code: 1 };
        if let JobResultData::Failed { exit_code } = failed {
            assert_eq!(exit_code, 1);
        } else {
            panic!("Expected Failed variant");
        }

        assert!(matches!(JobResultData::Timeout, JobResultData::Timeout));
        assert!(matches!(JobResultData::Cancelled, JobResultData::Cancelled));

        let error = JobResultData::Error {
            message: "test error".to_string(),
        };
        if let JobResultData::Error { message } = error {
            assert_eq!(message, "test error");
        } else {
            panic!("Expected Error variant");
        }
    }

    #[test]
    fn test_step_result_serialization() {
        let result = StepResult::Output(serde_json::json!({"key": "value"}));
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: StepResult = serde_json::from_str(&json).unwrap();
        match deserialized {
            StepResult::Output(v) => assert_eq!(v, serde_json::json!({"key": "value"})),
            _ => panic!("Expected Output variant"),
        }
    }
}

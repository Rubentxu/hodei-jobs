//!
//! # Execution Workflow for saga-engine v4 (DurableWorkflow)
//!
//! This module provides the [`ExecutionWorkflow`] implementation using the
//! [`DurableWorkflow`] trait for the Workflow-as-Code pattern.
//!
//! ## Key Changes from v3
//!
//! - Uses `JobExecutionPort` trait for all operations
//! - Activities delegate to the port for real command bus integration
//! - No more simulated behavior - all operations use real handlers
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;

use saga_engine_core::workflow::{Activity, DurableWorkflow, WorkflowContext};

use crate::saga::port::types::WorkflowState;

// =============================================================================
// Job Execution Port & Types
// =============================================================================

/// Port trait for job execution operations
/// This allows the workflow to use real command handlers
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectedResult {
    pub job_id: String,
    pub worker_id: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub result: JobResultData,
}

/// Job result data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JobResultData {
    Success,
    Failed { exit_code: i32 },
    Timeout,
    Cancelled,
    Error { message: String },
}

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
}

impl ExecutionWorkflowInput {
    /// Create a new input
    pub fn new(job_id: String, worker_id: String, command: String, arguments: Vec<String>) -> Self {
        Self {
            job_id,
            worker_id,
            command,
            arguments,
            env: vec![],
            working_dir: None,
            timeout_seconds: 3600,
        }
    }

    /// Generate idempotency key for this input
    pub fn idempotency_key(&self) -> String {
        format!("execution:{}:{}", self.worker_id, self.job_id)
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

    #[error("Activity execution failed: {0}")]
    ActivityFailed(String),

    #[error("Compensation failed: {0}")]
    CompensationFailed(String),
}

// =============================================================================
// Activities (using JobExecutionPort)
// =============================================================================

/// Activity for validating job using JobExecutionPort
#[derive(Debug)]
pub struct ValidateJobActivity<P: Debug + Send + Sync + ?Sized> {
    /// Port for job execution operations
    pub port: Arc<P>,
}

impl<P: Debug + Send + Sync + 'static + ?Sized> ValidateJobActivity<P> {
    pub fn new(port: Arc<P>) -> Self {
        Self { port }
    }
}

#[async_trait]
impl<P: Debug + Send + Sync + 'static + ?Sized> Activity for ValidateJobActivity<P>
where
    P: JobExecutionPort + 'static,
{
    const TYPE_ID: &'static str = "execution-validate-job";

    type Input = ValidateJobInput;
    type Output = ValidateJobOutput;
    type Error = ExecutionWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
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

/// Activity for dispatching job using JobExecutionPort
#[derive(Debug)]
pub struct DispatchJobActivity<P: Debug + Send + Sync + ?Sized> {
    /// Port for job execution operations
    pub port: Arc<P>,
}

impl<P: Debug + Send + Sync + 'static + ?Sized> DispatchJobActivity<P> {
    pub fn new(port: Arc<P>) -> Self {
        Self { port }
    }
}

#[async_trait]
impl<P: Debug + Send + Sync + 'static + ?Sized> Activity for DispatchJobActivity<P>
where
    P: JobExecutionPort + 'static,
{
    const TYPE_ID: &'static str = "execution-dispatch-job";

    type Input = DispatchJobInput;
    type Output = DispatchJobOutput;
    type Error = ExecutionWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
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

        // Convert JobResultData to dispatched status
        let dispatched = matches!(result, JobResultData::Success);

        Ok(DispatchJobOutput {
            job_id: input.job_id,
            worker_id: input.worker_id,
            dispatched,
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

/// Activity for collecting result using JobExecutionPort
#[derive(Debug)]
pub struct CollectResultActivity<P: Debug + Send + Sync + ?Sized> {
    /// Port for job execution operations
    pub port: Arc<P>,
}

impl<P: Debug + Send + Sync + 'static + ?Sized> CollectResultActivity<P> {
    pub fn new(port: Arc<P>) -> Self {
        Self { port }
    }
}

#[async_trait]
impl<P: Debug + Send + Sync + 'static + ?Sized> Activity for CollectResultActivity<P>
where
    P: JobExecutionPort + 'static,
{
    const TYPE_ID: &'static str = "execution-collect-result";

    type Input = CollectResultInput;
    type Output = CollectResultOutput;
    type Error = ExecutionWorkflowError;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let result = self
            .port
            .collect_result(&input.job_id, input.timeout_seconds)
            .await
            .map_err(|e| ExecutionWorkflowError::ResultCollectionFailed(e))?;

        let (exit_code, duration_ms, result_data) = match result.result {
            JobResultData::Success => (0, result.duration_ms, JobResultData::Success),
            JobResultData::Failed { exit_code } => (
                exit_code,
                result.duration_ms,
                JobResultData::Failed { exit_code },
            ),
            JobResultData::Timeout => (0, result.duration_ms, JobResultData::Timeout),
            JobResultData::Cancelled => (0, result.duration_ms, JobResultData::Cancelled),
            JobResultData::Error { message } => {
                (1, result.duration_ms, JobResultData::Error { message })
            }
        };

        Ok(CollectResultOutput {
            job_id: result.job_id,
            worker_id: result.worker_id,
            exit_code,
            duration_ms,
            result: result_data,
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
// Execution Workflow (DurableWorkflow Pattern)
// =============================================================================

/// Execution Workflow for saga-engine v4 using DurableWorkflow pattern
///
/// This workflow orchestrates job execution:
/// 1. Validate the job (via JobExecutionPort)
/// 2. Dispatch the job to the worker (via JobExecutionPort)
/// 3. Collect the result (via JobExecutionPort)
#[derive(Debug)]
pub struct ExecutionWorkflow<P: ?Sized>
where
    P: Debug + Send + Sync + JobExecutionPort + 'static,
{
    /// Port for job execution operations
    port: Arc<P>,
}

impl<P: ?Sized> Clone for ExecutionWorkflow<P>
where
    P: Debug + Send + Sync + JobExecutionPort + 'static,
{
    fn clone(&self) -> Self {
        Self {
            port: self.port.clone(),
        }
    }
}

impl<P: ?Sized> ExecutionWorkflow<P>
where
    P: Debug + Send + Sync + JobExecutionPort + 'static,
{
    /// Create a new ExecutionWorkflow
    pub fn new(port: Arc<P>) -> Self {
        Self { port }
    }
}

#[async_trait]
impl<P: ?Sized> DurableWorkflow for ExecutionWorkflow<P>
where
    P: Debug + Send + Sync + JobExecutionPort + 'static,
{
    const TYPE_ID: &'static str = "execution";
    const VERSION: u32 = 1;

    type Input = ExecutionWorkflowInput;
    type Output = ExecutionWorkflowOutput;
    type Error = ExecutionWorkflowError;

    /// Execute the workflow
    ///
    /// This implements the Workflow-as-Code pattern where the workflow logic
    /// is expressed imperatively rather than as a list of steps.
    ///
    /// All activities use `JobExecutionPort` which in production connects
    /// to the CommandBus for real command handling.
    async fn run(
        &self,
        context: &mut WorkflowContext,
        input: Self::Input,
    ) -> Result<Self::Output, Self::Error> {
        let start_time = std::time::Instant::now();

        // Activity 1: Validate the job
        let validate_input = ValidateJobInput {
            job_id: input.job_id.clone(),
        };

        let validate_output = context
            .execute_activity(&ValidateJobActivity::new(self.port.clone()), validate_input)
            .await
            .map_err(|e| ExecutionWorkflowError::ActivityFailed(e.to_string()))?;

        if !validate_output.is_valid {
            return Err(ExecutionWorkflowError::JobValidationFailed(
                "Job validation failed via JobExecutionPort".to_string(),
            ));
        }

        // Activity 2: Dispatch the job to worker
        let dispatch_input = DispatchJobInput {
            job_id: input.job_id.clone(),
            worker_id: input.worker_id.clone(),
            command: input.command,
            arguments: input.arguments,
            timeout_seconds: input.timeout_seconds,
        };

        let dispatch_output = context
            .execute_activity(&DispatchJobActivity::new(self.port.clone()), dispatch_input)
            .await
            .map_err(|e| ExecutionWorkflowError::ActivityFailed(e.to_string()))?;

        if !dispatch_output.dispatched {
            return Err(ExecutionWorkflowError::JobDispatchFailed(
                "Failed to dispatch job via JobExecutionPort".to_string(),
            ));
        }

        // Activity 3: Collect the result
        let collect_input = CollectResultInput {
            job_id: input.job_id.clone(),
            worker_id: input.worker_id.clone(),
            timeout_seconds: input.timeout_seconds,
        };

        let collect_output = context
            .execute_activity(
                &CollectResultActivity::new(self.port.clone()),
                collect_input,
            )
            .await
            .map_err(|e| ExecutionWorkflowError::ActivityFailed(e.to_string()))?;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(ExecutionWorkflowOutput {
            job_id: input.job_id,
            worker_id: input.worker_id,
            result: collect_output.result,
            exit_code: collect_output.exit_code,
            duration_ms,
        })
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Arc;

    // Mock implementation of JobExecutionPort for testing
    #[derive(Debug, Clone)]
    struct MockJobExecutionPort;

    #[async_trait]
    impl JobExecutionPort for MockJobExecutionPort {
        async fn validate_job(&self, job_id: &str) -> Result<bool, String> {
            Ok(!job_id.is_empty())
        }

        async fn dispatch_job(
            &self,
            job_id: &str,
            _worker_id: &str,
            _command: &str,
            _arguments: &[String],
            _timeout_secs: u64,
        ) -> Result<JobResultData, String> {
            Ok(JobResultData::Success)
        }

        async fn collect_result(
            &self,
            job_id: &str,
            _timeout_secs: u64,
        ) -> Result<CollectedResult, String> {
            Ok(CollectedResult {
                job_id: job_id.to_string(),
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

    #[tokio::test]
    async fn test_execution_workflow_input() {
        let input = ExecutionWorkflowInput::new(
            "job-123".to_string(),
            "worker-456".to_string(),
            "/bin/ls".to_string(),
            vec!["-la".to_string()],
        );

        assert_eq!(input.job_id, "job-123");
        assert_eq!(input.worker_id, "worker-456");
        assert_eq!(input.command, "/bin/ls");
        assert_eq!(input.arguments, vec!["-la"]);
    }

    #[tokio::test]
    async fn test_idempotency_key() {
        let input = ExecutionWorkflowInput::new(
            "job-123".to_string(),
            "worker-456".to_string(),
            "/bin/ls".to_string(),
            vec![],
        );

        let key = input.idempotency_key();
        assert!(key.starts_with("execution:worker-456:job-123"));
    }

    #[tokio::test]
    async fn test_validate_job_activity_with_mock() {
        let port = Arc::new(MockJobExecutionPort);
        let activity = ValidateJobActivity::new(port);
        let input = ValidateJobInput {
            job_id: "job-123".to_string(),
        };

        let result = activity.execute(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.job_id, "job-123");
        assert!(output.is_valid);
    }

    #[tokio::test]
    async fn test_validate_job_activity_empty_id() {
        let port = Arc::new(MockJobExecutionPort);
        let activity = ValidateJobActivity::new(port);
        let input = ValidateJobInput {
            job_id: "".to_string(),
        };

        let result = activity.execute(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.is_valid); // Empty job_id should be invalid
    }

    #[tokio::test]
    async fn test_dispatch_job_activity_with_mock() {
        let port = Arc::new(MockJobExecutionPort);
        let activity = DispatchJobActivity::new(port);
        let input = DispatchJobInput {
            job_id: "job-123".to_string(),
            worker_id: "worker-456".to_string(),
            command: "/bin/ls".to_string(),
            arguments: vec!["-la".to_string()],
            timeout_seconds: 3600,
        };

        let result = activity.execute(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.dispatched);
        assert!(matches!(output.result, JobResultData::Success));
    }

    #[tokio::test]
    async fn test_collect_result_activity_with_mock() {
        let port = Arc::new(MockJobExecutionPort);
        let activity = CollectResultActivity::new(port);
        let input = CollectResultInput {
            job_id: "job-123".to_string(),
            worker_id: "worker-456".to_string(),
            timeout_seconds: 3600,
        };

        let result = activity.execute(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(matches!(output.result, JobResultData::Success));
    }

    #[tokio::test]
    async fn test_execution_workflow_with_mock_port() {
        let port = Arc::new(MockJobExecutionPort);
        let _workflow = ExecutionWorkflow::new(port);

        // Test that workflow can be created with the mock port
        // Note: Full workflow execution requires ActivityRegistry setup
        // which is beyond the scope of this unit test
        assert_eq!(
            ExecutionWorkflow::<MockJobExecutionPort>::TYPE_ID,
            "execution"
        );
        assert_eq!(ExecutionWorkflow::<MockJobExecutionPort>::VERSION, 1);

        // Test individual activities instead of full workflow
        let port = Arc::new(MockJobExecutionPort);
        let activity = ValidateJobActivity::new(port);
        let input = ValidateJobInput {
            job_id: "job-123".to_string(),
        };

        let result = activity.execute(input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.is_valid);
    }

    #[tokio::test]
    async fn test_job_result_data_variants() {
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
}

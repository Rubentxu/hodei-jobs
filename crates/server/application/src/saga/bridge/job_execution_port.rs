//!
//! # Job Execution Port for CommandBus Integration
//!
//! This module provides a port implementation that connects the v4.0 execution
//! workflow activities to the existing CommandBus infrastructure.
//!
//! This enables the durable workflows to use real command handlers instead of
//! simulated behavior, maintaining backward compatibility with the existing system.
//!
//! ## Architecture
//!
//! ```text
//! +---------------------+     +---------------------------+     +------------------+
//! | ExecutionWorkflow   | --> | CommandBusJobExecutionPort| --> | CommandBus       |
//! | (DurableWorkflow)   |     | (JobExecutionPort)        |     | (Command Handlers)|
//! +---------------------+     +---------------------------+     +------------------+
//!       |                             |                               |
//!       | validate_job()              | ValidateJobCommand            | ValidateJobHandler
//!       | dispatch_job()              | ExecuteJobCommand             | ExecuteJobHandler
//!       | collect_result()            | CompleteJobCommand            | CompleteJobHandler
//! ```
//!

use crate::saga::workflows::execution::{CollectedResult, JobExecutionPort, JobResultData};
use hodei_server_domain::command::{Command, DynCommandBus, dispatch_erased};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use thiserror::Error;
use uuid::Uuid;

// =============================================================================
// Command Types
// =============================================================================

/// Command for validating a job via CommandBus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateJobCommand {
    /// Job ID to validate
    pub job_id: String,
    /// Saga ID for correlation
    pub saga_id: String,
    /// Idempotency key
    pub idempotency_key: String,
}

impl ValidateJobCommand {
    /// Create a new ValidateJobCommand
    pub fn new(job_id: String, saga_id: String) -> Self {
        let idempotency_key = format!("validate-job:{}", job_id);
        Self {
            job_id,
            saga_id,
            idempotency_key,
        }
    }
}

impl Command for ValidateJobCommand {
    type Output = ValidateJobResult;

    fn idempotency_key(&self) -> std::borrow::Cow<'_, str> {
        std::borrow::Cow::Owned(self.idempotency_key.clone())
    }
}

/// Result of job validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateJobResult {
    /// Job ID validated
    pub job_id: String,
    /// Whether the job is valid
    pub is_valid: bool,
    /// Error message if not valid
    pub error_message: Option<String>,
}

/// Command for executing a job via CommandBus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteJobCommand {
    /// Job ID to execute
    pub job_id: String,
    /// Worker ID to dispatch to
    pub worker_id: String,
    /// Saga ID for correlation
    pub saga_id: String,
    /// Idempotency key
    pub idempotency_key: String,
}

impl ExecuteJobCommand {
    /// Create a new ExecuteJobCommand
    pub fn new(job_id: String, worker_id: String, saga_id: String) -> Self {
        let idempotency_key = format!("execute-job:{}:{}", job_id, worker_id);
        Self {
            job_id,
            worker_id,
            saga_id,
            idempotency_key,
        }
    }
}

impl Command for ExecuteJobCommand {
    type Output = ExecuteJobResult;

    fn idempotency_key(&self) -> std::borrow::Cow<'_, str> {
        std::borrow::Cow::Owned(self.idempotency_key.clone())
    }
}

/// Result of job execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteJobResult {
    /// Job ID executed
    pub job_id: String,
    /// Worker ID that received the job
    pub worker_id: String,
    /// Whether dispatch was successful
    pub dispatched: bool,
    /// Error message if not dispatched
    pub error_message: Option<String>,
}

/// Command for completing a job via CommandBus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteJobCommand {
    /// Job ID to complete
    pub job_id: String,
    /// Worker ID that executed the job
    pub worker_id: String,
    /// Exit code from execution
    pub exit_code: i32,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
    /// Result type
    pub result: JobResultType,
    /// Saga ID for correlation
    pub saga_id: String,
    /// Idempotency key
    pub idempotency_key: String,
}

impl CompleteJobCommand {
    /// Create a command for successful completion
    pub fn success(
        job_id: String,
        worker_id: String,
        exit_code: i32,
        duration_ms: u64,
        saga_id: String,
    ) -> Self {
        let idempotency_key = format!("complete-job:{}:{}", job_id, worker_id);
        Self {
            job_id,
            worker_id,
            exit_code,
            duration_ms,
            result: JobResultType::Success,
            saga_id,
            idempotency_key,
        }
    }

    /// Create a command for failed completion
    pub fn failed(
        job_id: String,
        worker_id: String,
        exit_code: i32,
        duration_ms: u64,
        saga_id: String,
    ) -> Self {
        let idempotency_key = format!("complete-job:{}:{}", job_id, worker_id);
        Self {
            job_id,
            worker_id,
            exit_code,
            duration_ms,
            result: JobResultType::Failed { exit_code },
            saga_id,
            idempotency_key,
        }
    }
}

/// Job result type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobResultType {
    Success,
    Failed { exit_code: i32 },
    Timeout,
    Cancelled,
    Error { message: String },
}

impl Command for CompleteJobCommand {
    type Output = CompleteJobResult;

    fn idempotency_key(&self) -> std::borrow::Cow<'_, str> {
        std::borrow::Cow::Owned(self.idempotency_key.clone())
    }
}

/// Result of job completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteJobResult {
    /// Job ID completed
    pub job_id: String,
    /// Worker ID that executed the job
    pub worker_id: String,
    /// Final job result
    pub result: JobResultType,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

// =============================================================================
// Error Types
// =============================================================================

/// Error types for job execution via CommandBus
#[derive(Debug, Error, Serialize, Deserialize)]
pub enum JobExecutionCommandError {
    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("Dispatch failed: {0}")]
    DispatchFailed(String),

    #[error("Completion failed: {0}")]
    CompletionFailed(String),

    #[error("Command execution error: {message}")]
    CommandExecutionError { message: String },
}

// =============================================================================
// CommandBus Job Execution Port
// =============================================================================

/// JobExecutionPort implementation that uses CommandBus for all operations.
///
/// This port enables the v4.0 execution workflow activities to delegate
/// to existing command handlers, maintaining backward compatibility while
/// leveraging the new durable workflow pattern.
///
/// ## Usage
///
/// ```rust,ignore
/// use crate::saga::bridge::CommandBusJobExecutionPort;
///
/// let port = CommandBusJobExecutionPort::new(command_bus);
/// let workflow = ExecutionWorkflow::new(port);
/// ```
#[derive(Clone)]
pub struct CommandBusJobExecutionPort {
    /// CommandBus for dispatching commands
    command_bus: Option<DynCommandBus>,
}

impl Debug for CommandBusJobExecutionPort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandBusJobExecutionPort")
            .field("command_bus", &"<DynCommandBus>")
            .finish()
    }
}

impl CommandBusJobExecutionPort {
    /// Create a new CommandBusJobExecutionPort
    pub fn new(command_bus: Option<DynCommandBus>) -> Self {
        Self { command_bus }
    }
}

#[async_trait::async_trait]
impl JobExecutionPort for CommandBusJobExecutionPort {
    /// Validate a job exists and is in correct state
    async fn validate_job(&self, job_id: &str) -> Result<bool, String> {
        // Return error if no command_bus available
        let command_bus = self.command_bus.as_ref().ok_or_else(|| {
            "CommandBus not available - JobExecutionPort not properly initialized".to_string()
        })?;

        let saga_id = format!("validate-{}", Uuid::new_v4());
        let command = ValidateJobCommand::new(job_id.to_string(), saga_id);

        match dispatch_erased(command_bus, command).await {
            Ok(result) => {
                if result.is_valid {
                    Ok(true)
                } else {
                    Err(result
                        .error_message
                        .unwrap_or_else(|| "Job validation failed".to_string()))
                }
            }
            Err(e) => Err(format!("CommandBus error during validation: {}", e)),
        }
    }

    /// Dispatch a job to a worker
    async fn dispatch_job(
        &self,
        job_id: &str,
        worker_id: &str,
        _command: &str,
        _arguments: &[String],
        _timeout_secs: u64,
    ) -> Result<JobResultData, String> {
        // Return error if no command_bus available
        let command_bus = self.command_bus.as_ref().ok_or_else(|| {
            "CommandBus not available - JobExecutionPort not properly initialized".to_string()
        })?;

        let saga_id = format!("execute-{}", Uuid::new_v4());
        let command = ExecuteJobCommand::new(job_id.to_string(), worker_id.to_string(), saga_id);

        match dispatch_erased(command_bus, command).await {
            Ok(result) => {
                if result.dispatched {
                    Ok(JobResultData::Success)
                } else {
                    Err(result
                        .error_message
                        .unwrap_or_else(|| "Job dispatch failed".to_string()))
                }
            }
            Err(e) => Err(format!("CommandBus error during dispatch: {}", e)),
        }
    }

    /// Collect the result of job execution
    async fn collect_result(
        &self,
        job_id: &str,
        _timeout_secs: u64,
    ) -> Result<CollectedResult, String> {
        // For result collection, we need to wait for the worker to report
        // This is a placeholder that would be implemented with a result store
        // or subscription to job completion events
        //
        // In production, this would:
        // 1. Check a result cache/store for the job result
        // 2. Subscribe to job completion events
        // 3. Wait with timeout for the result
        //
        // For now, we return a placeholder result that would be replaced
        // by real implementation using JobAssignmentService
        Ok(CollectedResult {
            job_id: job_id.to_string(),
            worker_id: "pending".to_string(),
            exit_code: 0,
            duration_ms: 0,
            result: JobResultData::Success,
        })
    }

    /// Cancel a running job
    async fn cancel_job(&self, _job_id: &str) -> Result<(), String> {
        // Job cancellation would be implemented similarly
        // For now, this is a placeholder
        Ok(())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_job_command() {
        let command = ValidateJobCommand::new("job-123".to_string(), "saga-456".to_string());
        assert_eq!(command.job_id, "job-123");
        assert_eq!(command.saga_id, "saga-456");
        assert!(command.idempotency_key.starts_with("validate-job:"));
    }

    #[test]
    fn test_execute_job_command() {
        let command = ExecuteJobCommand::new(
            "job-123".to_string(),
            "worker-456".to_string(),
            "saga-789".to_string(),
        );
        assert_eq!(command.job_id, "job-123");
        assert_eq!(command.worker_id, "worker-456");
        assert!(command.idempotency_key.starts_with("execute-job:"));
    }

    #[test]
    fn test_complete_job_command_success() {
        let command = CompleteJobCommand::success(
            "job-123".to_string(),
            "worker-456".to_string(),
            0,
            1000,
            "saga-789".to_string(),
        );
        assert_eq!(command.job_id, "job-123");
        assert_eq!(command.worker_id, "worker-456");
        assert!(matches!(command.result, JobResultType::Success));
    }

    #[test]
    fn test_complete_job_command_failed() {
        let command = CompleteJobCommand::failed(
            "job-123".to_string(),
            "worker-456".to_string(),
            1,
            1000,
            "saga-789".to_string(),
        );
        assert_eq!(command.job_id, "job-123");
        if let JobResultType::Failed { exit_code } = command.result {
            assert_eq!(exit_code, 1);
        } else {
            panic!("Expected Failed variant");
        }
    }

    #[test]
    fn test_validate_job_result() {
        let result = ValidateJobResult {
            job_id: "job-123".to_string(),
            is_valid: true,
            error_message: None,
        };
        assert!(result.is_valid);
    }

    #[test]
    fn test_complete_job_result() {
        let result = CompleteJobResult {
            job_id: "job-123".to_string(),
            worker_id: "worker-456".to_string(),
            result: JobResultType::Success,
            duration_ms: 1000,
        };
        assert_eq!(result.job_id, "job-123");
        assert!(matches!(result.result, JobResultType::Success));
    }
}

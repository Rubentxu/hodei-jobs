//! Common types for the Saga Port abstraction layer.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Newtype wrapper for saga execution ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SagaExecutionId(pub String);

impl SagaExecutionId {
    /// Create a new execution ID with a randomly generated UUID
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Create an execution ID from an existing string
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Get the underlying string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SagaExecutionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SagaExecutionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Represents the possible states of a workflow execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkflowState {
    /// Workflow is currently executing
    Running {
        /// Current step being executed
        current_step: Option<u32>,
        /// Elapsed time since start
        elapsed: Option<std::time::Duration>,
    },

    /// Workflow completed successfully
    Completed {
        /// The output produced by the workflow
        output: serde_json::Value,
        /// Total number of steps executed
        steps_executed: u32,
        /// Total duration
        duration: std::time::Duration,
    },

    /// Workflow failed during execution
    Failed {
        /// Error message describing the failure
        error_message: String,
        /// Step where the failure occurred
        failed_step: Option<u32>,
        /// Total number of steps executed before failure
        steps_executed: u32,
        /// Number of compensations executed
        compensations_executed: u32,
    },

    /// Workflow was compensated (rolled back) after partial execution
    Compensated {
        /// Error that triggered compensation
        error_message: String,
        /// Step where the failure occurred
        failed_step: Option<u32>,
        /// Number of steps executed before compensation
        steps_executed: u32,
        /// Number of compensations executed
        compensations_executed: u32,
    },

    /// Workflow was cancelled by user or system
    Cancelled {
        /// Reason for cancellation
        reason: String,
        /// Number of steps executed before cancellation
        steps_executed: u32,
    },

    /// Workflow is currently compensating (rolling back)
    Compensating {
        /// Error that triggered compensation
        error_message: String,
        /// Current compensation step
        current_step: u32,
    },
}

impl WorkflowState {
    /// Check if the workflow is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            WorkflowState::Completed { .. }
                | WorkflowState::Failed { .. }
                | WorkflowState::Compensated { .. }
                | WorkflowState::Cancelled { .. }
        )
    }

    /// Check if the workflow completed successfully
    pub fn is_success(&self) -> bool {
        matches!(self, WorkflowState::Completed { .. })
    }

    /// Check if the workflow failed
    pub fn is_failed(&self) -> bool {
        matches!(
            self,
            WorkflowState::Failed { .. } | WorkflowState::Compensated { .. }
        )
    }
}

/// Result of a saga port operation that includes metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaPortResult {
    /// The execution ID
    pub execution_id: SagaExecutionId,
    /// The final state
    pub state: WorkflowState,
    /// Timestamp when the operation started
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Timestamp when the operation completed
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl SagaPortResult {
    /// Create a new result for a running workflow
    pub fn new(execution_id: SagaExecutionId) -> Self {
        Self {
            execution_id,
            state: WorkflowState::Running {
                current_step: None,
                elapsed: None,
            },
            started_at: chrono::Utc::now(),
            completed_at: None,
        }
    }
}

/// Configuration for saga port operations
#[derive(Debug, Clone, Default)]
pub struct SagaPortConfig {
    /// Default timeout for workflow execution
    pub default_timeout: std::time::Duration,
    /// Poll interval for state queries
    pub poll_interval: std::time::Duration,
    /// Maximum retries for transient failures
    pub max_retries: u32,
    /// Retry delay
    pub retry_delay: std::time::Duration,
}

impl SagaPortConfig {
    /// Create a production configuration
    pub fn production() -> Self {
        Self {
            default_timeout: std::time::Duration::from_secs(300),
            poll_interval: std::time::Duration::from_millis(100),
            max_retries: 3,
            retry_delay: std::time::Duration::from_millis(50),
        }
    }

    /// Create a test configuration
    pub fn test() -> Self {
        Self {
            default_timeout: std::time::Duration::from_secs(30),
            poll_interval: std::time::Duration::from_millis(10),
            max_retries: 1,
            retry_delay: std::time::Duration::from_millis(5),
        }
    }
}

//! Workflow execution events.
//!
//! Events related to the lifecycle of a workflow execution.

use serde::{Deserialize, Serialize};

/// Workflow execution event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowEventType {
    /// Workflow execution has started.
    Started,
    /// Workflow execution completed successfully.
    Completed,
    /// Workflow execution failed with an error.
    Failed,
    /// Workflow execution timed out.
    TimedOut,
    /// Workflow execution was canceled.
    Canceled,
    /// Workflow execution continued as a new execution.
    ContinuedAsNew,
    /// Workflow execution was terminated.
    Terminated,
}

impl WorkflowEventType {
    /// Returns the event type as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Started => "workflow_execution_started",
            Self::Completed => "workflow_execution_completed",
            Self::Failed => "workflow_execution_failed",
            Self::TimedOut => "workflow_execution_timed_out",
            Self::Canceled => "workflow_execution_canceled",
            Self::ContinuedAsNew => "workflow_execution_continued_as_new",
            Self::Terminated => "workflow_execution_terminated",
        }
    }
}

impl std::fmt::Display for WorkflowEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

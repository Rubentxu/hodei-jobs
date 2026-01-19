//! Child workflow events.
//!
//! Events related to the lifecycle of child workflow executions.

use serde::{Deserialize, Serialize};

/// Child workflow event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildWorkflowEventType {
    /// Child workflow execution has started.
    ExecutionStarted,
    /// Child workflow execution completed successfully.
    ExecutionCompleted,
    /// Child workflow execution failed.
    ExecutionFailed,
    /// Child workflow execution was canceled.
    ExecutionCanceled,
    /// Child workflow execution timed out.
    ExecutionTimedOut,
    /// Child workflow execution was terminated.
    ExecutionTerminated,
    /// Child workflow execution continued as new.
    ExecutionContinuedAsNew,
    /// Start child workflow execution has been initiated.
    StartInitiated,
    /// Child workflow execution cancel has been requested.
    CancelRequested,
}

impl ChildWorkflowEventType {
    /// Returns the event type as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ExecutionStarted => "child_workflow_execution_started",
            Self::ExecutionCompleted => "child_workflow_execution_completed",
            Self::ExecutionFailed => "child_workflow_execution_failed",
            Self::ExecutionCanceled => "child_workflow_execution_canceled",
            Self::ExecutionTimedOut => "child_workflow_execution_timed_out",
            Self::ExecutionTerminated => "child_workflow_execution_terminated",
            Self::ExecutionContinuedAsNew => "child_workflow_execution_continued_as_new",
            Self::StartInitiated => "start_child_workflow_execution_initiated",
            Self::CancelRequested => "child_workflow_execution_cancel_requested",
        }
    }
}

impl std::fmt::Display for ChildWorkflowEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

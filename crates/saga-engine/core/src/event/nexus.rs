//! Nexus events.
//!
//! Events related to Nexus operations (cross-namespace).

use serde::{Deserialize, Serialize};

/// Nexus operation event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NexusEventType {
    /// Nexus operation has started.
    OperationStarted,
    /// Nexus operation completed successfully.
    OperationCompleted,
    /// Nexus operation failed.
    OperationFailed,
    /// Nexus operation was canceled.
    OperationCanceled,
    /// Nexus operation timed out.
    OperationTimedOut,
    /// Start Nexus operation has been initiated.
    StartInitiated,
    /// Nexus operation cancel has been requested.
    CancelRequested,
}

impl NexusEventType {
    /// Returns the event type as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OperationStarted => "nexus_operation_started",
            Self::OperationCompleted => "nexus_operation_completed",
            Self::OperationFailed => "nexus_operation_failed",
            Self::OperationCanceled => "nexus_operation_canceled",
            Self::OperationTimedOut => "nexus_operation_timed_out",
            Self::StartInitiated => "start_nexus_operation_initiated",
            Self::CancelRequested => "nexus_operation_cancel_requested",
        }
    }
}

impl std::fmt::Display for NexusEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

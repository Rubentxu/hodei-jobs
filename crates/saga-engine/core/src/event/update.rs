//! Workflow update events.
//!
//! Events related to proposed and accepted workflow updates.

use serde::{Deserialize, Serialize};

/// Workflow update event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateEventType {
    /// Workflow update has been accepted.
    Accepted,
    /// Workflow update has been rejected.
    Rejected,
    /// Workflow update has completed.
    Completed,
    /// Workflow update has been validated.
    Validated,
    /// Workflow update has been rolled back.
    RolledBack,
}

impl UpdateEventType {
    /// Returns the event type as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Accepted => "workflow_update_accepted",
            Self::Rejected => "workflow_update_rejected",
            Self::Completed => "workflow_update_completed",
            Self::Validated => "workflow_update_validated",
            Self::RolledBack => "workflow_update_rolled_back",
        }
    }
}

impl std::fmt::Display for UpdateEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

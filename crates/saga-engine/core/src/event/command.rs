//! Command events.
//!
//! Events for command issuance and completion.

use serde::{Deserialize, Serialize};

/// Command event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandEventType {
    /// A command has been issued.
    Issued,
    /// A command has completed.
    Completed,
    /// A command has failed.
    Failed,
}

impl CommandEventType {
    /// Returns the event type as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Issued => "command_issued",
            Self::Completed => "command_completed",
            Self::Failed => "command_failed",
        }
    }
}

impl std::fmt::Display for CommandEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

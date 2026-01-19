//! Activity task events.
//!
//! Events related to the lifecycle of activity task execution.

use serde::{Deserialize, Serialize};

/// Activity task event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityEventType {
    /// Activity task has been scheduled.
    Scheduled,
    /// Activity task has started execution.
    Started,
    /// Activity task completed successfully.
    Completed,
    /// Activity task failed.
    Failed,
    /// Activity task timed out.
    TimedOut,
    /// Activity task was canceled.
    Canceled,
}

impl ActivityEventType {
    /// Returns the event type as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Scheduled => "activity_task_scheduled",
            Self::Started => "activity_task_started",
            Self::Completed => "activity_task_completed",
            Self::Failed => "activity_task_failed",
            Self::TimedOut => "activity_task_timed_out",
            Self::Canceled => "activity_task_canceled",
        }
    }
}

impl std::fmt::Display for ActivityEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

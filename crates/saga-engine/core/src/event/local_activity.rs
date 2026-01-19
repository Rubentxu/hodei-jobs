//! Local activity events.
//!
//! Events related to local activity execution (short-running, no heartbeat).

use serde::{Deserialize, Serialize};

/// Local activity event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalActivityEventType {
    /// Local activity has been scheduled.
    Scheduled,
    /// Local activity has started.
    Started,
    /// Local activity completed successfully.
    Completed,
    /// Local activity failed.
    Failed,
    /// Local activity timed out.
    TimedOut,
    /// Local activity was canceled.
    Canceled,
}

impl LocalActivityEventType {
    /// Returns the event type as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Scheduled => "local_activity_scheduled",
            Self::Started => "local_activity_started",
            Self::Completed => "local_activity_completed",
            Self::Failed => "local_activity_failed",
            Self::TimedOut => "local_activity_timed_out",
            Self::Canceled => "local_activity_canceled",
        }
    }
}

impl std::fmt::Display for LocalActivityEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

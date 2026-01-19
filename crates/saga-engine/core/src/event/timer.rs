//! Timer events.
//!
//! Events related to timer creation, firing, and cancellation.

use serde::{Deserialize, Serialize};

/// Timer event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerEventType {
    /// Timer has been created.
    Created,
    /// Timer has fired (deadline reached).
    Fired,
    /// Timer has been canceled.
    Canceled,
}

impl TimerEventType {
    /// Returns the event type as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "timer_created",
            Self::Fired => "timer_fired",
            Self::Canceled => "timer_canceled",
        }
    }
}

impl std::fmt::Display for TimerEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

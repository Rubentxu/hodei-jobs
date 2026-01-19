//! Signal events.
//!
//! Events related to external signal handling.

use serde::{Deserialize, Serialize};

/// Signal event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalEventType {
    /// An external signal was received.
    Received,
}

impl SignalEventType {
    /// Returns the event type as a string.
    pub fn as_str(&self) -> &'static str {
        "signal_received"
    }
}

impl std::fmt::Display for SignalEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

//! Marker events.
//!
//! User-defined marker events for workflow tracking and debugging.

use serde::{Deserialize, Serialize};

/// Marker event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerEventType {
    /// A marker has been recorded.
    Recorded,
}

impl MarkerEventType {
    /// Returns the event type as a string.
    pub fn as_str(&self) -> &'static str {
        "marker_recorded"
    }
}

impl std::fmt::Display for MarkerEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

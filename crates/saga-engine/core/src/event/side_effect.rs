//! Side effect events.
//!
//! Events related to deterministic side effects.

use serde::{Deserialize, Serialize};

/// Side effect event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectEventType {
    /// A side effect has been recorded.
    Recorded,
}

impl SideEffectEventType {
    /// Returns the event type as a string.
    pub fn as_str(&self) -> &'static str {
        "side_effect_recorded"
    }
}

impl std::fmt::Display for SideEffectEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

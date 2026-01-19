//! Snapshot events.
//!
//! Events for state reconstruction and workflow snapshots.

use serde::{Deserialize, Serialize};

/// Snapshot event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotEventType {
    /// A snapshot has been created.
    Created,
}

impl SnapshotEventType {
    /// Returns the event type as a string.
    pub fn as_str(&self) -> &'static str {
        "snapshot_created"
    }
}

impl std::fmt::Display for SnapshotEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

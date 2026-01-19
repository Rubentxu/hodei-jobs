//! Search attribute events.
//!
//! Events related to search attribute upserts.

use serde::{Deserialize, Serialize};

/// Search attribute event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchAttributeEventType {
    /// Search attributes have been upserted.
    Upserted,
}

impl SearchAttributeEventType {
    /// Returns the event type as a string.
    pub fn as_str(&self) -> &'static str {
        "upsert_search_attributes"
    }
}

impl std::fmt::Display for SearchAttributeEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

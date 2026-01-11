//! Client Commands sent to WebSocket Server

use serde::{Deserialize, Serialize};

/// Commands sent from client to server
///
/// Used for subscribing/unsubscribing to topics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum ClientCommand {
    /// Subscribe to a topic (e.g., "jobs:all", "workers:all", "agg:{uuid}")
    #[serde(rename = "sub")]
    Subscribe { topic: String, request_id: String },

    /// Unsubscribe from a topic
    #[serde(rename = "unsub")]
    Unsubscribe { topic: String },

    /// Ping to keep connection alive (auto-handled by WebSocket)
    #[serde(rename = "ping")]
    Ping,
}

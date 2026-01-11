//! Realtime WebSocket Infrastructure
//!
//! Provides real-time event streaming to WebSocket clients.

pub mod bridge;
pub mod connection_manager;
pub mod metrics;
pub mod session;

// Re-exports
pub use bridge::{EventBridgeConfig, RealtimeEventBridge};
pub use connection_manager::{ConnectionManager, ConnectionManagerMetrics};
pub use metrics::{RealtimeMetrics, RealtimeMetricsSnapshot};
pub use session::{
    BACKPRESSURE_THRESHOLD, SESSION_CHANNEL_CAPACITY, Session, SessionError, SessionId,
    SessionMetrics,
};

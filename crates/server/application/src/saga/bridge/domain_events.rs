//!
//! # Domain Event to Signal Bridge (Simplified)
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;

/// Signal notification for saga-engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalNotification {
    pub saga_id: String,
    pub signal_type: String,
    pub payload: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Trait for saga-engine signal dispatcher.
#[async_trait]
pub trait SignalDispatcher: Send + Sync {
    async fn notify(&self, notification: &SignalNotification) -> Result<(), String>;
}

/// Bridge configuration.
#[derive(Debug, Default)]
pub struct DomainEventBridgeConfig {
    pub default_signal_type: String,
    pub batch_size: usize,
    pub poll_interval_ms: u64,
}

impl DomainEventBridgeConfig {
    pub fn new(default_signal_type: String) -> Self {
        Self {
            default_signal_type,
            batch_size: 100,
            poll_interval_ms: 100,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("Subscription failed")]
    SubscriptionFailed,
    #[error("Signal dispatch failed")]
    SignalDispatchFailed,
    #[error("Bridge shutdown failed")]
    ShutdownFailed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestNotification {
        pub saga_id: String,
        pub signal_type: String,
    }

    #[tokio::test]
    async fn test_signal_notification_creation() {
        let notification = SignalNotification {
            saga_id: "test-saga".to_string(),
            signal_type: "test-signal".to_string(),
            payload: serde_json::json!({ "key": "value" }),
            timestamp: chrono::Utc::now(),
        };
        assert_eq!(notification.saga_id, "test-saga");
        assert_eq!(notification.signal_type, "test-signal");
    }

    #[tokio::test]
    async fn test_bridge_config() {
        let config = DomainEventBridgeConfig::new("default".to_string());
        assert_eq!(config.default_signal_type, "default");
        assert_eq!(config.batch_size, 100);
    }

    #[tokio::test]
    async fn test_bridge_error_display() {
        let err = BridgeError::SubscriptionFailed;
        assert!(err.to_string().contains("Subscription"));
    }
}

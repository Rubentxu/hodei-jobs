//! Watchdog Component Trait
//!
//! Extends [`HealthCheck`] with recovery capabilities for automatic
//! component restart by watchdog system.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::time::Duration;

use super::health_check::{HealthCheck, HealthInfo};

/// Result of a recovery attempt
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryResult {
    /// Recovery succeeded
    Success,
    /// Recovery failed with error
    Failed(String),
    /// Recovery not needed (component already healthy)
    NotNeeded,
    /// Recovery not supported by component
    NotSupported,
}

/// Watchdog component trait with recovery capabilities
///
/// Components implement this trait to allow the watchdog to
/// automatically restart them when they fail.
#[async_trait::async_trait]
pub trait WatchdogComponent: HealthCheck + Send + Sync {
    /// Get component name
    fn name(&self) -> &str;

    /// Restart the component
    ///
    /// This method is called by the watchdog when the component
    /// is unhealthy and needs to be restarted.
    async fn restart(&self) -> Result<(), String>;

    /// Get component start time
    async fn start_time(&self) -> Option<DateTime<Utc>>;

    /// Get last activity timestamp
    async fn last_activity(&self) -> Option<DateTime<Utc>>;

    /// Check if component is stalled (no activity for too long)
    ///
    /// Returns true if no activity has been detected for the
    /// specified duration.
    async fn is_stalled(&self, stall_threshold: Duration) -> bool {
        if let Some(last_activity) = self.last_activity().await {
            let now = Utc::now();
            let elapsed = now.signed_duration_since(last_activity);
            elapsed.to_std().unwrap_or(Duration::ZERO) > stall_threshold
        } else {
            false // No activity recorded yet
        }
    }

    /// Perform graceful shutdown
    async fn shutdown(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Builder for creating watchdog components
pub trait WatchdogComponentBuilder: Send + Sync {
    type Component: WatchdogComponent + Send + Sync + 'static;

    /// Build the component
    fn build(self) -> Self::Component;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockComponent {
        name: String,
        healthy: bool,
        start_time: Option<DateTime<Utc>>,
        last_activity: Option<DateTime<Utc>>,
    }

    #[async_trait::async_trait]
    impl HealthCheck for MockComponent {
        async fn health(&self) -> HealthInfo {
            HealthInfo::new(&self.name, if self.healthy { HealthStatus::Healthy } else { HealthStatus::Unhealthy })
        }

        async fn uptime(&self) -> Duration {
            if let Some(start) = self.start_time {
                let now = Utc::now();
                now.signed_duration_since(start).to_std().unwrap_or(Duration::ZERO)
            } else {
                Duration::ZERO
            }
        }

        async fn last_error(&self) -> Option<String> {
            if self.healthy {
                None
            } else {
                Some("Mock error".to_string())
            }
        }

        async fn liveness(&self) -> bool {
            self.healthy
        }

        async fn readiness(&self) -> bool {
            self.healthy
        }
    }

    #[async_trait::async_trait]
    impl WatchdogComponent for MockComponent {
        fn name(&self) -> &str {
            &self.name
        }

        async fn restart(&self) -> Result<(), String> {
            Ok(())
        }

        async fn start_time(&self) -> Option<DateTime<Utc>> {
            self.start_time
        }

        async fn last_activity(&self) -> Option<DateTime<Utc>> {
            self.last_activity
        }
    }

    #[test]
    fn test_recovery_result_variants() {
        assert!(matches!(RecoveryResult::Success, RecoveryResult::Success));
        assert!(matches!(RecoveryResult::Failed("test".to_string()), RecoveryResult::Failed(_)));
        assert!(matches!(RecoveryResult::NotNeeded, RecoveryResult::NotNeeded));
        assert!(matches!(RecoveryResult::NotSupported, RecoveryResult::NotSupported));
    }

    #[test]
    fn test_mock_component_health() {
        let component = MockComponent {
            name: "TestComponent".to_string(),
            healthy: true,
            start_time: Some(Utc::now()),
            last_activity: Some(Utc::now()),
        };

        assert_eq!(component.name(), "TestComponent");
        assert_eq!(component.start_time().await, component.start_time);
        assert_eq!(component.last_activity().await, component.last_activity);
    }

    #[test]
    fn test_mock_component_is_stalled() {
        let mut component = MockComponent {
            name: "TestComponent".to_string(),
            healthy: true,
            start_time: Some(Utc::now()),
            last_activity: Some(Utc::now()),
        };

        // Not stalled (activity just now)
        assert!(!component.is_stalled(Duration::from_secs(10)).await);

        // Make last activity old
        component.last_activity = Some(Utc::now() - chrono::Duration::seconds(30));

        // Stalled (activity 30s ago, threshold 10s)
        assert!(component.is_stalled(Duration::from_secs(10)).await);

        // Not stalled (activity 30s ago, threshold 60s)
        assert!(!component.is_stalled(Duration::from_secs(60)).await);
    }

    #[tokio::test]
    async fn test_mock_component_restart() {
        let component = MockComponent {
            name: "TestComponent".to_string(),
            healthy: false,
            start_time: Some(Utc::now()),
            last_activity: Some(Utc::now()),
        };

        let result = component.restart().await;
        assert!(result.is_ok());
    }
}

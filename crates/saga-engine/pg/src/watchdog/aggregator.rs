//! Health Aggregator
//!
//! Bridge between auto-polling watchdog and on-demand HTTP health endpoints.
//! Maintains health cache for fast responses to health queries.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::health_check::{HealthInfo, HealthStatus};

/// Overall health status for the entire saga engine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverallHealthStatus {
    /// All components are healthy
    Healthy,
    /// Some components are degraded but still functional
    Degraded,
    /// Some components are unhealthy
    Unhealthy,
    /// Overall status is unknown
    Unknown,
}

impl OverallHealthStatus {
    /// Convert from component health status to overall status
    pub fn from_component_status(status: HealthStatus) -> Self {
        match status {
            HealthStatus::Healthy => Self::Healthy,
            HealthStatus::Degraded => Self::Degraded,
            HealthStatus::Unhealthy => Self::Unhealthy,
            HealthStatus::Unknown => Self::Unknown,
        }
    }
}

impl std::fmt::Display for OverallHealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Unhealthy => "unhealthy",
            Self::Unknown => "unknown",
        };
        write!(f, "{}", status)
    }
}

/// Readiness check status
#[derive(Debug, Clone)]
pub struct ReadinessCheck {
    /// Check name
    pub name: String,
    /// Check description
    pub description: String,
    /// Whether check passed
    pub passed: bool,
}

/// Health aggregator maintains cached health information
///
/// Serves as bridge between auto-polling watchdog (updates cache)
/// and on-demand HTTP endpoints (reads cache).
pub struct HealthAggregator {
    /// Cached health information per component
    health_cache: Arc<Mutex<HashMap<String, HealthInfo>>>,
    /// Overall health status
    overall_status: Arc<Mutex<OverallHealthStatus>>,
    /// Readiness checks
    readiness_checks: Arc<Mutex<Vec<ReadinessCheck>>>,
    /// Last update timestamp
    last_update: Arc<Mutex<Option<DateTime<Utc>>>>,
}

impl HealthAggregator {
    /// Create new health aggregator
    pub fn new() -> Self {
        Self {
            health_cache: Arc::new(Mutex::new(HashMap::new())),
            overall_status: Arc::new(Mutex::new(OverallHealthStatus::Unknown)),
            readiness_checks: Arc::new(Mutex::new(Vec::new())),
            last_update: Arc::new(Mutex::new(None)),
        }
    }

    /// Update health information for a component
    ///
    /// Called by watchdog auto-polling loop
    pub async fn update_component_health(&self, component_name: String, health: HealthInfo) {
        let mut cache = self.health_cache.lock().await;
        cache.insert(component_name, health);

        // Update last update timestamp
        *self.last_update.lock().await = Some(Utc::now());

        // Recalculate overall status
        self.recalculate_overall_status().await;
    }

    /// Batch update health information for multiple components
    ///
    /// Called by watchdog after checking all components
    pub async fn update_all_components(&self, health_updates: HashMap<String, HealthInfo>) {
        let mut cache = self.health_cache.lock().await;
        for (name, health) in health_updates {
            cache.insert(name, health);
        }

        *self.last_update.lock().await = Some(Utc::now());
        self.recalculate_overall_status().await;
    }

    /// Update readiness check
    pub async fn update_readiness_check(&self, check: ReadinessCheck) {
        let mut checks = self.readiness_checks.lock().await;

        // Update existing check or add new
        if let Some(existing) = checks.iter_mut().find(|c| c.name == check.name) {
            *existing = check;
        } else {
            checks.push(check);
        }

        *self.last_update.lock().await = Some(Utc::now());
    }

    /// Update multiple readiness checks
    pub async fn update_readiness_checks(&self, checks: Vec<ReadinessCheck>) {
        let mut readiness_checks = self.readiness_checks.lock().await;
        *readiness_checks = checks;

        *self.last_update.lock().await = Some(Utc::now());
    }

    /// Get health information for a specific component
    pub async fn get_component_health(&self, component_name: &str) -> Option<HealthInfo> {
        let cache = self.health_cache.lock().await;
        cache.get(component_name).cloned()
    }

    /// Get all component health information
    pub async fn get_all_components(&self) -> Vec<HealthInfo> {
        let cache = self.health_cache.lock().await;
        cache.values().cloned().collect()
    }

    /// Get overall health status
    pub async fn get_overall_status(&self) -> OverallHealthStatus {
        *self.overall_status.lock().await
    }

    /// Check if overall system is healthy
    pub async fn is_healthy(&self) -> bool {
        matches!(
            self.get_overall_status().await,
            OverallHealthStatus::Healthy
        )
    }

    /// Check if overall system is operational (healthy or degraded)
    pub async fn is_operational(&self) -> bool {
        matches!(
            self.get_overall_status().await,
            OverallHealthStatus::Healthy | OverallHealthStatus::Degraded
        )
    }

    /// Check if system is ready (all readiness checks passed)
    pub async fn is_ready(&self) -> bool {
        let checks = self.readiness_checks.lock().await;
        checks.iter().all(|check| check.passed)
    }

    /// Get all readiness checks
    pub async fn get_readiness_checks(&self) -> Vec<ReadinessCheck> {
        let checks = self.readiness_checks.lock().await;
        checks.clone()
    }

    /// Get last update timestamp
    pub async fn get_last_update(&self) -> Option<DateTime<Utc>> {
        *self.last_update.lock().await
    }

    /// Check if health data is stale (older than max_age)
    pub async fn is_stale(&self, max_age: chrono::Duration) -> bool {
        if let Some(last_update) = self.get_last_update().await {
            let now = Utc::now();
            let elapsed = now.signed_duration_since(last_update);
            elapsed > max_age
        } else {
            true // No data yet
        }
    }

    /// Clear all health information
    pub async fn clear(&self) {
        self.health_cache.lock().await.clear();
        self.readiness_checks.lock().await.clear();
        *self.last_update.lock().await = None;
        *self.overall_status.lock().await = OverallHealthStatus::Unknown;
    }

    /// Recalculate overall health status from all components
    async fn recalculate_overall_status(&self) {
        let cache = self.health_cache.lock().await;

        if cache.is_empty() {
            *self.overall_status.lock().await = OverallHealthStatus::Unknown;
            return;
        }

        let mut has_degraded = false;
        let mut has_unhealthy = false;

        for health in cache.values() {
            match health.status {
                HealthStatus::Degraded => has_degraded = true,
                HealthStatus::Unhealthy => has_unhealthy = true,
                HealthStatus::Healthy | HealthStatus::Unknown => {}
            }
        }

        let overall_status = if has_unhealthy {
            OverallHealthStatus::Unhealthy
        } else if has_degraded {
            OverallHealthStatus::Degraded
        } else {
            OverallHealthStatus::Healthy
        };

        *self.overall_status.lock().await = overall_status;
    }
}

impl Default for HealthAggregator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overall_health_status_variants() {
        assert_eq!(
            OverallHealthStatus::from_component_status(HealthStatus::Healthy),
            OverallHealthStatus::Healthy
        );
        assert_eq!(
            OverallHealthStatus::from_component_status(HealthStatus::Degraded),
            OverallHealthStatus::Degraded
        );
        assert_eq!(
            OverallHealthStatus::from_component_status(HealthStatus::Unhealthy),
            OverallHealthStatus::Unhealthy
        );
        assert_eq!(
            OverallHealthStatus::from_component_status(HealthStatus::Unknown),
            OverallHealthStatus::Unknown
        );
    }

    #[test]
    fn test_overall_health_status_display() {
        assert_eq!(format!("{}", OverallHealthStatus::Healthy), "healthy");
        assert_eq!(format!("{}", OverallHealthStatus::Degraded), "degraded");
        assert_eq!(format!("{}", OverallHealthStatus::Unhealthy), "unhealthy");
        assert_eq!(format!("{}", OverallHealthStatus::Unknown), "unknown");
    }

    #[tokio::test]
    async fn test_health_aggregator_update() {
        let aggregator = HealthAggregator::new();

        let health = HealthInfo::new("TestComponent", HealthStatus::Healthy);
        aggregator.update_component_health("TestComponent".to_string(), health).await;

        let retrieved = aggregator.get_component_health("TestComponent").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().component, "TestComponent");
    }

    #[tokio::test]
    async fn test_health_aggregator_get_all() {
        let aggregator = HealthAggregator::new();

        aggregator
            .update_component_health(
                "Component1".to_string(),
                HealthInfo::new("Component1", HealthStatus::Healthy),
            )
            .await;
        aggregator
            .update_component_health(
                "Component2".to_string(),
                HealthInfo::new("Component2", HealthStatus::Unhealthy),
            )
            .await;

        let all = aggregator.get_all_components().await;
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_health_aggregator_overall_status() {
        let aggregator = HealthAggregator::new();

        // No components -> Unknown
        assert_eq!(
            aggregator.get_overall_status().await,
            OverallHealthStatus::Unknown
        );

        // All healthy -> Healthy
        aggregator
            .update_component_health(
                "C1".to_string(),
                HealthInfo::new("C1", HealthStatus::Healthy),
            )
            .await;
        aggregator
            .update_component_health(
                "C2".to_string(),
                HealthInfo::new("C2", HealthStatus::Healthy),
            )
            .await;
        assert_eq!(
            aggregator.get_overall_status().await,
            OverallHealthStatus::Healthy
        );

        // Some degraded -> Degraded
        aggregator
            .update_component_health(
                "C3".to_string(),
                HealthInfo::new("C3", HealthStatus::Degraded),
            )
            .await;
        assert_eq!(
            aggregator.get_overall_status().await,
            OverallHealthStatus::Degraded
        );

        // Some unhealthy -> Unhealthy
        aggregator
            .update_component_health(
                "C4".to_string(),
                HealthInfo::new("C4", HealthStatus::Unhealthy),
            )
            .await;
        assert_eq!(
            aggregator.get_overall_status().await,
            OverallHealthStatus::Unhealthy
        );
    }

    #[tokio::test]
    async fn test_health_aggregator_is_healthy() {
        let aggregator = HealthAggregator::new();

        aggregator
            .update_component_health(
                "C1".to_string(),
                HealthInfo::new("C1", HealthStatus::Healthy),
            )
            .await;

        assert!(aggregator.is_healthy().await);

        aggregator
            .update_component_health(
                "C2".to_string(),
                HealthInfo::new("C2", HealthStatus::Unhealthy),
            )
            .await;

        assert!(!aggregator.is_healthy().await);
    }

    #[tokio::test]
    async fn test_health_aggregator_is_operational() {
        let aggregator = HealthAggregator::new();

        aggregator
            .update_component_health(
                "C1".to_string(),
                HealthInfo::new("C1", HealthStatus::Healthy),
            )
            .await;

        assert!(aggregator.is_operational().await);

        aggregator
            .update_component_health(
                "C2".to_string(),
                HealthInfo::new("C2", HealthStatus::Degraded),
            )
            .await;

        // Degraded is still operational
        assert!(aggregator.is_operational().await);
    }

    #[tokio::test]
    async fn test_health_aggregator_readiness() {
        let aggregator = HealthAggregator::new();

        // No checks -> ready (empty)
        assert!(aggregator.is_ready().await);

        let check1 = ReadinessCheck {
            name: "Check1".to_string(),
            description: "Test check".to_string(),
            passed: true,
        };
        aggregator.update_readiness_check(check1).await;
        assert!(aggregator.is_ready().await);

        let check2 = ReadinessCheck {
            name: "Check2".to_string(),
            description: "Test check 2".to_string(),
            passed: false,
        };
        aggregator.update_readiness_check(check2).await;
        assert!(!aggregator.is_ready().await);
    }

    #[tokio::test]
    async fn test_health_aggregator_is_stale() {
        let aggregator = HealthAggregator::new();

        // No data -> stale
        assert!(aggregator.is_stale(chrono::Duration::seconds(60)).await);

        // Recent data -> not stale
        aggregator
            .update_component_health(
                "C1".to_string(),
                HealthInfo::new("C1", HealthStatus::Healthy),
            )
            .await;
        assert!(!aggregator.is_stale(chrono::Duration::seconds(60)).await);

        // Old data (simulate by waiting)
        // Note: This test assumes time passes, in real test use mock time
        assert!(aggregator.is_stale(chrono::Duration::milliseconds(1)).await);
    }

    #[tokio::test]
    async fn test_health_aggregator_clear() {
        let aggregator = HealthAggregator::new();

        aggregator
            .update_component_health(
                "C1".to_string(),
                HealthInfo::new("C1", HealthStatus::Healthy),
            )
            .await;
        aggregator.update_readiness_check(ReadinessCheck {
            name: "Check1".to_string(),
            description: "Test".to_string(),
            passed: true,
        }).await;

        // Verify data exists
        assert!(aggregator.get_component_health("C1").await.is_some());
        assert!(!aggregator.get_readiness_checks().await.is_empty());

        // Clear
        aggregator.clear().await;

        // Verify data cleared
        assert!(aggregator.get_component_health("C1").await.is_none());
        assert!(aggregator.get_readiness_checks().await.is_empty());
        assert_eq!(
            aggregator.get_overall_status().await,
            OverallHealthStatus::Unknown
        );
    }
}

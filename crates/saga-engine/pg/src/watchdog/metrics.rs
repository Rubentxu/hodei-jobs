//! Watchdog Metrics
//!
//! Prometheus metrics for watchdog system monitoring.

use prometheus::{Counter, CounterVec, Gauge, Histogram, HistogramOpts, Opts, Registry};

/// Watchdog metrics for Prometheus monitoring
#[derive(Clone)]
pub struct WatchdogMetrics {
    /// Total health checks performed
    pub health_checks_total: Counter,
    /// Total unhealthy components detected
    pub unhealthy_components_total: Counter,
    /// Total restarts performed
    pub restarts_total: Counter,
    /// Total recovery failures
    pub recovery_failures_total: Counter,
    /// Total stalls detected
    pub stalls_detected_total: Counter,
    /// Total deadlocks detected
    pub deadlocks_detected_total: Counter,
    /// Current number of healthy components
    pub healthy_components: Gauge,
    /// Current number of unhealthy components
    pub unhealthy_components: Gauge,
    /// Health check duration histogram
    pub health_check_duration: Histogram,
}

impl WatchdogMetrics {
    /// Register metrics with Prometheus registry
    pub fn register(registry: &Registry) -> Result<Self, prometheus::Error> {
        let health_checks_total = Counter::new(
            "saga_watchdog_health_checks_total",
            "Total health checks performed by watchdog",
        )?;

        let unhealthy_components_total = Counter::new(
            "saga_watchdog_unhealthy_components_total",
            "Total unhealthy components detected",
        )?;

        let restarts_total = Counter::new(
            "saga_watchdog_restarts_total",
            "Total component restarts performed",
        )?;

        let recovery_failures_total = Counter::new(
            "saga_watchdog_recovery_failures_total",
            "Total recovery failures",
        )?;

        let stalls_detected_total = Counter::new(
            "saga_watchdog_stalls_detected_total",
            "Total stalls detected",
        )?;

        let deadlocks_detected_total = Counter::new(
            "saga_watchdog_deadlocks_detected_total",
            "Total deadlocks detected",
        )?;

        let healthy_components = Gauge::new(
            "saga_watchdog_healthy_components",
            "Current number of healthy components",
        )?;

        let unhealthy_components = Gauge::new(
            "saga_watchdog_unhealthy_components",
            "Current number of unhealthy components",
        )?;

        let health_check_duration = Histogram::with_opts(
            HistogramOpts::new(
                "saga_watchdog_health_check_duration_seconds",
                "Health check duration in seconds",
            )
            .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0]),
        )?;

        registry.register(Box::new(health_checks_total.clone()))?;
        registry.register(Box::new(unhealthy_components_total.clone()))?;
        registry.register(Box::new(restarts_total.clone()))?;
        registry.register(Box::new(recovery_failures_total.clone()))?;
        registry.register(Box::new(stalls_detected_total.clone()))?;
        registry.register(Box::new(deadlocks_detected_total.clone()))?;
        registry.register(Box::new(healthy_components.clone()))?;
        registry.register(Box::new(unhealthy_components.clone()))?;
        registry.register(Box::new(health_check_duration.clone()))?;

        Ok(Self {
            health_checks_total,
            unhealthy_components_total,
            restarts_total,
            recovery_failures_total,
            stalls_detected_total,
            deadlocks_detected_total,
            healthy_components,
            unhealthy_components,
            health_check_duration,
        })
    }

    /// Increment health check counter
    pub fn inc_health_checks(&self) {
        self.health_checks_total.inc();
    }

    /// Increment unhealthy components counter
    pub fn inc_unhealthy_components(&self) {
        self.unhealthy_components_total.inc();
    }

    /// Increment restarts counter
    pub fn inc_restarts(&self) {
        self.restarts_total.inc();
    }

    /// Increment recovery failures counter
    pub fn inc_recovery_failures(&self) {
        self.recovery_failures_total.inc();
    }

    /// Increment stalls detected counter
    pub fn inc_stalls_detected(&self) {
        self.stalls_detected_total.inc();
    }

    /// Increment deadlocks detected counter
    pub fn inc_deadlocks_detected(&self) {
        self.deadlocks_detected_total.inc();
    }

    /// Update healthy components gauge
    pub fn set_healthy_components(&self, count: u64) {
        self.healthy_components.set(count as f64);
    }

    /// Update unhealthy components gauge
    pub fn set_unhealthy_components(&self, count: u64) {
        self.unhealthy_components.set(count as f64);
    }

    /// Record health check duration
    pub fn observe_health_check_duration(&self, duration_seconds: f64) {
        self.health_check_duration.observe(duration_seconds);
    }
}

impl Default for WatchdogMetrics {
    fn default() -> Self {
        let registry = Registry::new();
        Self::register(&registry).expect("Failed to create default metrics")
    }
}

impl Default for WatchdogActionMetrics {
    fn default() -> Self {
        let registry = Registry::new();
        Self::register(&registry).expect("Failed to create default action metrics")
    }
}

/// Watchdog action for metrics tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogAction {
    HealthCheck,
    UnhealthyDetected,
    ComponentRestarted,
    RecoveryFailed,
    StallDetected,
    DeadlockDetected,
}

/// Watchdog action metrics (labeled)
#[derive(Clone)]
pub struct WatchdogActionMetrics {
    actions: CounterVec,
}

impl WatchdogActionMetrics {
    /// Register action metrics with Prometheus registry
    pub fn register(registry: &Registry) -> Result<Self, prometheus::Error> {
        let opts = Opts::new(
            "saga_watchdog_actions_total",
            "Total watchdog actions performed",
        );
        let actions = CounterVec::new(opts, &["action"])?;
        registry.register(Box::new(actions.clone()))?;

        Ok(Self { actions })
    }

    /// Record watchdog action
    pub fn record_action(&self, action: WatchdogAction) {
        let action_label = match action {
            WatchdogAction::HealthCheck => "health_check",
            WatchdogAction::UnhealthyDetected => "unhealthy_detected",
            WatchdogAction::ComponentRestarted => "component_restarted",
            WatchdogAction::RecoveryFailed => "recovery_failed",
            WatchdogAction::StallDetected => "stall_detected",
            WatchdogAction::DeadlockDetected => "deadlock_detected",
        };
        self.actions.with_label_values(&[action_label]).inc();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watchdog_action_variants() {
        let _ = WatchdogAction::HealthCheck;
        let _ = WatchdogAction::UnhealthyDetected;
        let _ = WatchdogAction::ComponentRestarted;
        let _ = WatchdogAction::RecoveryFailed;
        let _ = WatchdogAction::StallDetected;
        let _ = WatchdogAction::DeadlockDetected;
    }

    #[test]
    fn test_watchdog_metrics_create() {
        let registry = Registry::new();
        let metrics = WatchdogMetrics::register(&registry);

        assert!(metrics.is_ok());
    }

    #[test]
    fn test_watchdog_metrics_increment() {
        let registry = Registry::new();
        let metrics = WatchdogMetrics::register(&registry).unwrap();

        metrics.inc_health_checks();
        metrics.inc_unhealthy_components();
        metrics.inc_restarts();
        metrics.inc_recovery_failures();
        metrics.inc_stalls_detected();
        metrics.inc_deadlocks_detected();

        metrics.set_healthy_components(5);
        metrics.set_unhealthy_components(2);

        metrics.observe_health_check_duration(0.5);

        // Verify metrics were recorded
        assert_eq!(metrics.health_checks_total.get(), 1.0);
        assert_eq!(metrics.unhealthy_components_total.get(), 1.0);
        assert_eq!(metrics.restarts_total.get(), 1.0);
        assert_eq!(metrics.recovery_failures_total.get(), 1.0);
        assert_eq!(metrics.stalls_detected_total.get(), 1.0);
        assert_eq!(metrics.deadlocks_detected_total.get(), 1.0);
        assert_eq!(metrics.healthy_components.get(), 5.0);
        assert_eq!(metrics.unhealthy_components.get(), 2.0);
    }

    #[test]
    fn test_watchdog_action_metrics_create() {
        let registry = Registry::new();
        let action_metrics = WatchdogActionMetrics::register(&registry);

        assert!(action_metrics.is_ok());
    }

    #[test]
    fn test_watchdog_action_metrics_record() {
        let registry = Registry::new();
        let action_metrics = WatchdogActionMetrics::register(&registry).unwrap();

        action_metrics.record_action(WatchdogAction::HealthCheck);
        action_metrics.record_action(WatchdogAction::UnhealthyDetected);
        action_metrics.record_action(WatchdogAction::ComponentRestarted);
        action_metrics.record_action(WatchdogAction::StallDetected);
        action_metrics.record_action(WatchdogAction::DeadlockDetected);

        // Verify actions were recorded
        let health_check_count = action_metrics
            .actions
            .with_label_values(&["health_check"])
            .get();
        let unhealthy_count = action_metrics
            .actions
            .with_label_values(&["unhealthy_detected"])
            .get();
        let restarted_count = action_metrics
            .actions
            .with_label_values(&["component_restarted"])
            .get();

        assert_eq!(health_check_count, 1.0);
        assert_eq!(unhealthy_count, 1.0);
        assert_eq!(restarted_count, 1.0);
    }
}

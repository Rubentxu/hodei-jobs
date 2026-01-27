//! Saga Engine Watchdog Supervisor
//!
//! Auto-polling watchdog that monitors components, detects failures,
//! and performs automatic recovery.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use futures::executor;

use super::aggregator::{HealthAggregator, OverallHealthStatus};
use super::deadlock_detector::{DeadlockDetector, DeadlockDetectorConfig, DeadlockStatus};
use super::health_check::{HealthCheck, HealthInfo, HealthStatus};
use super::metrics::{WatchdogAction, WatchdogActionMetrics, WatchdogMetrics};
use super::stall_detector::{StallDetector, StallDetectorConfig, StallStatus};
use super::watchdog_component::{WatchdogComponent};

/// Watchdog configuration
#[derive(Debug, Clone)]
pub struct WatchdogConfig {
    /// Poll interval for health checks (default: 30s)
    pub poll_interval: Duration,
    /// Stall detector configuration
    pub stall_config: StallDetectorConfig,
    /// Deadlock detector configuration
    pub deadlock_config: DeadlockDetectorConfig,
    /// Enable automatic restart of failed components
    pub auto_restart: bool,
    /// Delay before restart after failure (default: 1s)
    pub restart_delay: Duration,
    /// Max restarts before giving up (default: 3)
    pub max_restarts: u32,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(30),
            stall_config: StallDetectorConfig::default(),
            deadlock_config: DeadlockDetectorConfig::default(),
            auto_restart: true,
            restart_delay: Duration::from_secs(1),
            max_restarts: 3,
        }
    }
}

impl WatchdogConfig {
    /// Create builder
    pub fn builder() -> WatchdogConfigBuilder {
        WatchdogConfigBuilder::default()
    }
}

/// Builder for watchdog configuration
#[derive(Default)]
pub struct WatchdogConfigBuilder {
    poll_interval: Option<Duration>,
    stall_config: Option<StallDetectorConfig>,
    deadlock_config: Option<DeadlockDetectorConfig>,
    auto_restart: Option<bool>,
    restart_delay: Option<Duration>,
    max_restarts: Option<u32>,
}

impl WatchdogConfigBuilder {
    /// Set poll interval
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = Some(interval);
        self
    }

    /// Set stall detector configuration
    pub fn with_stall_config(mut self, config: StallDetectorConfig) -> Self {
        self.stall_config = Some(config);
        self
    }

    /// Set deadlock detector configuration
    pub fn with_deadlock_config(mut self, config: DeadlockDetectorConfig) -> Self {
        self.deadlock_config = Some(config);
        self
    }

    /// Enable or disable auto restart
    pub fn with_auto_restart(mut self, enabled: bool) -> Self {
        self.auto_restart = Some(enabled);
        self
    }

    /// Set restart delay
    pub fn with_restart_delay(mut self, delay: Duration) -> Self {
        self.restart_delay = Some(delay);
        self
    }

    /// Set max restarts
    pub fn with_max_restarts(mut self, max: u32) -> Self {
        self.max_restarts = Some(max);
        self
    }

    /// Build configuration
    pub fn build(self) -> WatchdogConfig {
        WatchdogConfig {
            poll_interval: self.poll_interval.unwrap_or(Duration::from_secs(30)),
            stall_config: self.stall_config.unwrap_or_default(),
            deadlock_config: self.deadlock_config.unwrap_or_default(),
            auto_restart: self.auto_restart.unwrap_or(true),
            restart_delay: self.restart_delay.unwrap_or(Duration::from_secs(1)),
            max_restarts: self.max_restarts.unwrap_or(3),
        }
    }
}

/// Saga Engine Watchdog Supervisor
///
/// Auto-polling watchdog that:
/// - Monitors component health every N seconds (configurable, default: 30s)
/// - Detects stalled components (no activity for extended periods)
/// - Detects deadlocked workflows/timers
/// - Automatically restarts failed components
/// - Updates health aggregator for HTTP endpoints
pub struct SagaEngineWatchdog {
    /// Watched components
    components: Vec<Arc<dyn WatchdogComponent>>,
    /// Health aggregator for HTTP endpoints
    aggregator: Arc<HealthAggregator>,
    /// Watchdog configuration
    config: WatchdogConfig,
    /// Stall detector
    stall_detector: StallDetector,
    /// Deadlock detector
    deadlock_detector: DeadlockDetector,
    /// Watchdog metrics
    metrics: WatchdogMetrics,
    /// Action metrics
    action_metrics: WatchdogActionMetrics,
    /// Running flag
    running: Arc<AtomicBool>,
    /// Restart counters per component
    restart_counts: Arc<tokio::sync::Mutex<HashMap<String, u32>>>,
}

impl SagaEngineWatchdog {
    /// Create new watchdog
    pub fn new(config: WatchdogConfig) -> Self {
        let aggregator = Arc::new(HealthAggregator::new());
        let stall_detector = StallDetector::new(config.stall_config.clone());
        let deadlock_detector = DeadlockDetector::new(config.deadlock_config.clone());

        Self {
            components: Vec::new(),
            aggregator,
            config,
            stall_detector,
            deadlock_detector,
            metrics: WatchdogMetrics::default(),
            action_metrics: WatchdogActionMetrics::default(),
            running: Arc::new(AtomicBool::new(false)),
            restart_counts: Arc::new(tokio::sync::Mutex::new(
                HashMap::new(),
            )),
        }
    }

    /// Create watchdog with default configuration
    pub fn with_defaults() -> Self {
        Self::new(WatchdogConfig::default())
    }

    /// Get health aggregator (for HTTP endpoints)
    pub fn aggregator(&self) -> Arc<HealthAggregator> {
        self.aggregator.clone()
    }

    /// Register a component for monitoring
    pub fn register_component(&mut self, component: Arc<dyn WatchdogComponent>) {
        info!(
            "Watchdog: Registering component {}",
            component.name()
        );
        self.components.push(component);
    }

    /// Start watchdog monitoring loop
    ///
    /// Returns shutdown receiver that can be used to stop the watchdog
    pub async fn start(&self) -> anyhow::Result<mpsc::Receiver<()>> {
        if self.running.load(Ordering::Relaxed) {
            warn!("Watchdog: Already running");
            return Err(anyhow::anyhow!("Watchdog already running"));
        }

        info!(
            "Watchdog: Starting monitoring loop (interval: {:?})",
            self.config.poll_interval
        );

        self.running.store(true, Ordering::Relaxed);

        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        // Clone components before spawning
        let components = self.components.clone();

        // Spawn monitoring task
        let aggregator = self.aggregator.clone();
        let config = self.config.clone();
        let stall_detector = self.stall_detector.clone();
        let deadlock_detector = self.deadlock_detector.clone();
        let metrics = Arc::new(self.metrics.clone());
        let action_metrics = Arc::new(self.action_metrics.clone());
        let restart_counts = self.restart_counts.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.poll_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        Self::monitor_components(
                            components.clone(),
                            &aggregator,
                            &config,
                            &stall_detector,
                            &deadlock_detector,
                            &metrics,
                            &action_metrics,
                            &restart_counts,
                        ).await;
                    }
                    _ = shutdown_tx.closed() => {
                        info!("Watchdog: Received shutdown signal");
                        break;
                    }
                }
            }

            running.store(false, Ordering::Relaxed);
            info!("Watchdog: Monitoring loop ended");
        });

        Ok(shutdown_rx)
    }

    /// Stop watchdog monitoring
    pub async fn stop(&self) {
        info!("Watchdog: Stopping");
        self.running.store(false, Ordering::Relaxed);
    }

    /// Check if watchdog is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Internal: Monitor all components
    async fn monitor_components(
        components: Vec<Arc<dyn WatchdogComponent>>,
        aggregator: &Arc<HealthAggregator>,
        config: &WatchdogConfig,
        stall_detector: &StallDetector,
        deadlock_detector: &DeadlockDetector,
        metrics: &Arc<WatchdogMetrics>,
        action_metrics: &Arc<WatchdogActionMetrics>,
        restart_counts: &Arc<tokio::sync::Mutex<HashMap<String, u32>>>,
    ) {
        action_metrics.record_action(WatchdogAction::HealthCheck);
        metrics.inc_health_checks();

        let mut health_updates = HashMap::new();
        let mut restart_actions = Vec::new();

        // Check all components
        for component in components {
            let component_name = component.name().to_string();

            // Get health info
            let health = component.health().await;

            // Record health check duration
            // Note: In production, measure actual duration
            metrics.observe_health_check_duration(0.5);

            // Detect stalls
            let last_activity = component.last_activity().await;
            let stall_status = stall_detector.detect_stall(last_activity, None);

            if matches!(stall_status, StallStatus::Stalled { .. }) {
                warn!(
                    "Watchdog: Component {} is stalled",
                    component_name
                );
                action_metrics.record_action(WatchdogAction::StallDetected);
                metrics.inc_stalls_detected();

                // Update health info with stall
                let mut stalled_health = health.clone();
                stalled_health.context.insert(
                    "stall_detected".to_string(),
                    "true".to_string(),
                );
                health_updates.insert(component_name.clone(), stalled_health);
            }

            // Detect deadlocks (simplified for timers)
            // TODO: Implement workflow-specific deadlock detection
            let deadlock_status = deadlock_detector.detect_deadlock(
                last_activity,
                "Processing",
                false, // TODO: Get actual completion status
                None,
            );

            if matches!(deadlock_status, DeadlockStatus::Deadlocked { .. }) {
                warn!(
                    "Watchdog: Component {} appears deadlocked",
                    component_name
                );
                action_metrics.record_action(WatchdogAction::DeadlockDetected);
                metrics.inc_deadlocks_detected();

                let mut deadlocked_health = health.clone();
                deadlocked_health.context.insert(
                    "deadlock_detected".to_string(),
                    "true".to_string(),
                );
                health_updates.insert(component_name.clone(), deadlocked_health);
            }

            // Check if component is unhealthy
            if !health.is_healthy() {
                warn!(
                    "Watchdog: Component {} is unhealthy: {:?}",
                    component_name,
                    health.status
                );
                action_metrics.record_action(WatchdogAction::UnhealthyDetected);
                metrics.inc_unhealthy_components();

                restart_actions.push(component_name);
            } else {
                health_updates.insert(component_name, health);
            }
        }

        // Update aggregator with health status
        aggregator.update_all_components(health_updates).await;

        // Perform restarts if auto-restart enabled
        if config.auto_restart {
            Self::perform_restarts(
                components,
                restart_actions,
                config,
                restart_counts,
                metrics,
                action_metrics,
            ).await;
        }

        // Update metrics
        let total_components = components.len();
        let healthy_count = components
            .iter()
            .filter(|c| {
                let health = c.health();
                executor::block_on(health).unwrap_or(false)
            })
            .count();
        let unhealthy_count = total_components - healthy_count;

        metrics.set_healthy_components(healthy_count as u64);
        metrics.set_unhealthy_components(unhealthy_count as u64);
    }

    /// Internal: Perform component restarts
    async fn perform_restarts(
        components: Vec<Arc<dyn WatchdogComponent>>,
        restart_actions: Vec<String>,
        config: &WatchdogConfig,
        restart_counts: &Arc<tokio::sync::Mutex<HashMap<String, u32>>>,
        metrics: &Arc<WatchdogMetrics>,
        action_metrics: &Arc<WatchdogActionMetrics>,
    ) {
        let mut counts = restart_counts.lock().await;

        for component_name in restart_actions {
            let restart_count = counts.entry(component_name.clone()).or_insert(0);

            if *restart_count >= config.max_restarts {
                error!(
                    "Watchdog: Component {} has reached max restarts ({}), giving up",
                    component_name,
                    config.max_restarts
                );
                continue;
            }

            // Find component and restart it
            if let Some(component) = components
                .iter()
                .find(|c| c.name() == component_name)
            {
                info!(
                    "Watchdog: Restarting component {} (restart #{})",
                    component_name,
                    *restart_count + 1
                );

                // Wait before restart
                tokio::time::sleep(config.restart_delay).await;

                // Perform restart
                match component.restart().await {
                    Ok(()) => {
                        *restart_count += 1;
                        action_metrics.record_action(WatchdogAction::ComponentRestarted);
                        metrics.inc_restarts();
                        info!(
                            "Watchdog: Component {} restarted successfully",
                            component_name
                        );
                    }
                    Err(error) => {
                        action_metrics.record_action(WatchdogAction::RecoveryFailed);
                        metrics.inc_recovery_failures();
                        error!(
                            "Watchdog: Failed to restart component {}: {}",
                            component_name,
                            error
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watchdog_config_default() {
        let config = WatchdogConfig::default();

        assert_eq!(config.poll_interval, Duration::from_secs(30));
        assert_eq!(config.auto_restart, true);
        assert_eq!(config.restart_delay, Duration::from_secs(1));
        assert_eq!(config.max_restarts, 3);
    }

    #[test]
    fn test_watchdog_config_builder() {
        let config = WatchdogConfig::builder()
            .with_poll_interval(Duration::from_secs(60))
            .with_auto_restart(false)
            .with_restart_delay(Duration::from_secs(5))
            .with_max_restarts(5)
            .build();

        assert_eq!(config.poll_interval, Duration::from_secs(60));
        assert_eq!(config.auto_restart, false);
        assert_eq!(config.restart_delay, Duration::from_secs(5));
        assert_eq!(config.max_restarts, 5);
    }

    #[test]
    fn test_watchdog_create() {
        let watchdog = SagaEngineWatchdog::new(WatchdogConfig::default());

        assert!(!watchdog.is_running());
        assert_eq!(watchdog.components.len(), 0);
    }

    #[test]
    fn test_watchdog_aggregator() {
        let watchdog = SagaEngineWatchdog::with_defaults();
        let aggregator = watchdog.aggregator();

        assert_eq!(aggregator.get_overall_status().await, OverallHealthStatus::Unknown);
    }

    #[tokio::test]
    async fn test_watchdog_start_stop() {
        let watchdog = SagaEngineWatchdog::with_defaults();

        let shutdown_rx = watchdog.start().await.unwrap();

        assert!(watchdog.is_running());

        // Stop watchdog
        watchdog.stop().await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(!watchdog.is_running());
    }

    #[tokio::test]
    async fn test_watchdog_already_running() {
        let watchdog = SagaEngineWatchdog::with_defaults();

        watchdog.start().await.unwrap();

        let result = watchdog.start().await;

        assert!(result.is_err());

        watchdog.stop().await;
    }
}

//! ReconcilerMetrics - Prometheus metrics for reconciliation components
//!
//! This module provides Prometheus metrics for:
//! - DatabaseReaper activities (jobs failed, workers terminated)
//! - InfrastructureReconciler activities (zombies, ghosts)
//! - Alert thresholds and conditions
//!
//! EPIC-43: Sprint 4 - Reconciliación (Red de Seguridad)
//! US-EDA-403: Configurar alertas de producción

use prometheus::{IntCounter, IntGauge, Registry};
use std::sync::Arc;

/// Metrics for DatabaseReaper
#[derive(Debug)]
pub struct DatabaseReaperMetrics {
    /// Jobs marked as failed
    pub jobs_failed: IntCounter,

    /// Workers terminated
    pub workers_terminated: IntCounter,

    /// Reconciliation cycles run
    pub cycles_total: IntCounter,

    /// Reconciliation errors
    pub errors: IntCounter,
}

/// Metrics for InfrastructureReconciler
#[derive(Debug)]
pub struct InfrastructureReconcilerMetrics {
    /// Zombie workers destroyed
    pub zombies_destroyed: IntCounter,

    /// Ghost workers handled
    pub ghosts_handled: IntCounter,

    /// Jobs recovered from ghosts
    pub jobs_recovered: IntCounter,

    /// Reconciliation cycles run
    pub cycles_total: IntCounter,

    /// Reconciliation errors
    pub errors: IntCounter,
}

/// Alert configuration for production monitoring
#[derive(Debug, Clone)]
pub struct AlertConfig {
    /// Alert if zombie workers detected
    pub zombie_worker_threshold: u64,

    /// Alert if jobs marked as failed by reaper
    pub job_failure_threshold: u64,

    /// Alert if DLQ has messages
    pub dlq_size_threshold: u64,

    /// Alert if jobs are stuck in RUNNING state
    pub stuck_job_threshold: u64,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            // Alert if more than 5 zombie workers detected
            zombie_worker_threshold: 5,

            // Alert if more than 10 jobs failed in last 5 minutes
            job_failure_threshold: 10,

            // Alert if DLQ has more than 100 messages
            dlq_size_threshold: 100,

            // Alert if jobs are stuck in RUNNING for more than 5 minutes
            stuck_job_threshold: 5,
        }
    }
}

/// ReconcilerMetrics - Shared metrics for reconciliation components
#[derive(Debug)]
pub struct ReconcilerMetrics {
    /// Registry for Prometheus metrics
    registry: Registry,

    /// Database reaper metrics
    pub db_reaper: DatabaseReaperMetrics,

    /// Infrastructure reconciler metrics
    pub infra_reconciler: InfrastructureReconcilerMetrics,

    /// Current zombie worker count (for alerts)
    pub current_zombie_count: IntGauge,

    /// Current job failure count (for alerts)
    pub current_job_failure_count: IntGauge,

    /// Current DLQ size (for alerts)
    pub current_dlq_size: IntGauge,
}

impl ReconcilerMetrics {
    /// Creates a new metrics registry
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        // DatabaseReaper metrics
        let jobs_failed = IntCounter::new(
            "hodei_reaper_jobs_failed_total",
            "Total jobs marked as failed by DatabaseReaper",
        )?;
        let workers_terminated = IntCounter::new(
            "hodei_reaper_workers_terminated_total",
            "Total workers terminated by DatabaseReaper",
        )?;
        let cycles_total = IntCounter::new(
            "hodei_reaper_cycles_total",
            "Total DatabaseReaper cycles run",
        )?;
        let errors = IntCounter::new("hodei_reaper_errors_total", "Total DatabaseReaper errors")?;

        registry.register(Box::new(jobs_failed.clone()))?;
        registry.register(Box::new(workers_terminated.clone()))?;
        registry.register(Box::new(cycles_total.clone()))?;
        registry.register(Box::new(errors.clone()))?;

        // InfrastructureReconciler metrics
        let zombies_destroyed = IntCounter::new(
            "hodei_reconciler_zombies_destroyed_total",
            "Total zombie workers destroyed",
        )?;
        let ghosts_handled = IntCounter::new(
            "hodei_reconciler_ghosts_handled_total",
            "Total ghost workers handled",
        )?;
        let jobs_recovered = IntCounter::new(
            "hodei_reconciler_jobs_recovered_total",
            "Total jobs recovered from ghosts",
        )?;
        let infra_cycles = IntCounter::new(
            "hodei_reconciler_cycles_total",
            "Total InfrastructureReconciler cycles run",
        )?;
        let infra_errors = IntCounter::new(
            "hodei_reconciler_errors_total",
            "Total InfrastructureReconciler errors",
        )?;

        registry.register(Box::new(zombies_destroyed.clone()))?;
        registry.register(Box::new(ghosts_handled.clone()))?;
        registry.register(Box::new(jobs_recovered.clone()))?;
        registry.register(Box::new(infra_cycles.clone()))?;
        registry.register(Box::new(infra_errors.clone()))?;

        // Current state gauges for alerting
        let current_zombie_count = IntGauge::new(
            "hodei_reconciler_current_zombies",
            "Current number of zombie workers detected",
        )?;
        let current_job_failure_count = IntGauge::new(
            "hodei_reaper_current_job_failures",
            "Current number of job failures detected",
        )?;
        let current_dlq_size = IntGauge::new(
            "hodei_outbox_dlq_size",
            "Current size of the Dead Letter Queue",
        )?;

        registry.register(Box::new(current_zombie_count.clone()))?;
        registry.register(Box::new(current_job_failure_count.clone()))?;
        registry.register(Box::new(current_dlq_size.clone()))?;

        Ok(Self {
            registry,
            db_reaper: DatabaseReaperMetrics {
                jobs_failed,
                workers_terminated,
                cycles_total,
                errors,
            },
            infra_reconciler: InfrastructureReconcilerMetrics {
                zombies_destroyed,
                ghosts_handled,
                jobs_recovered,
                cycles_total: infra_cycles,
                errors: infra_errors,
            },
            current_zombie_count,
            current_job_failure_count,
            current_dlq_size,
        })
    }

    /// Returns the Prometheus registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

/// Alert evaluator for production monitoring
#[derive(Debug)]
pub struct AlertEvaluator {
    config: AlertConfig,
    metrics: Arc<ReconcilerMetrics>,
}

impl AlertEvaluator {
    /// Creates a new alert evaluator
    pub fn new(config: AlertConfig, metrics: Arc<ReconcilerMetrics>) -> Self {
        Self { config, metrics }
    }

    /// Checks if any alerts should be triggered
    pub fn check_alerts(&self) -> Vec<Alert> {
        let mut alerts = Vec::new();

        // Check zombie worker alert
        let zombie_count = self.metrics.current_zombie_count.get() as u64;
        if zombie_count >= self.config.zombie_worker_threshold {
            alerts.push(Alert {
                name: "ZombieWorkerAlert".to_string(),
                severity: AlertSeverity::Warning,
                message: format!(
                    "High number of zombie workers detected: {} (threshold: {})",
                    zombie_count, self.config.zombie_worker_threshold
                ),
                timestamp: chrono::Utc::now(),
            });
        }

        // Check job failure alert
        let failed_count = self.metrics.current_job_failure_count.get() as u64;
        if failed_count >= self.config.job_failure_threshold {
            alerts.push(Alert {
                name: "JobFailureAlert".to_string(),
                severity: AlertSeverity::Critical,
                message: format!(
                    "High number of jobs marked as failed by reaper: {} (threshold: {})",
                    failed_count, self.config.job_failure_threshold
                ),
                timestamp: chrono::Utc::now(),
            });
        }

        // Check DLQ alert
        let dlq_size = self.metrics.current_dlq_size.get() as u64;
        if dlq_size >= self.config.dlq_size_threshold {
            alerts.push(Alert {
                name: "DLQSizeAlert".to_string(),
                severity: AlertSeverity::Critical,
                message: format!(
                    "Dead Letter Queue has grown large: {} messages (threshold: {})",
                    dlq_size, self.config.dlq_size_threshold
                ),
                timestamp: chrono::Utc::now(),
            });
        }

        alerts
    }
}

/// An alert to be sent to monitoring
#[derive(Debug, Clone)]
pub struct Alert {
    /// Alert name
    pub name: String,

    /// Alert severity
    pub severity: AlertSeverity,

    /// Alert message
    pub message: String,

    /// When the alert was triggered
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Alert severity levels
#[derive(Debug, Clone, PartialEq)]
pub enum AlertSeverity {
    /// Warning level alert
    Warning,

    /// Critical level alert
    Critical,
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertSeverity::Warning => write!(f, "WARNING"),
            AlertSeverity::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Shared state for metrics and alerts
#[derive(Debug, Clone)]
pub struct ReconcilerMonitoring {
    /// Metrics
    pub metrics: Arc<ReconcilerMetrics>,

    /// Alert configuration
    pub alert_config: Arc<AlertConfig>,

    /// Alert evaluator
    pub evaluator: Arc<AlertEvaluator>,
}

impl ReconcilerMonitoring {
    /// Creates new monitoring state
    pub fn new(alert_config: AlertConfig) -> Result<Self, prometheus::Error> {
        let metrics = Arc::new(ReconcilerMetrics::new()?);
        let evaluator = Arc::new(AlertEvaluator::new(alert_config.clone(), metrics.clone()));

        Ok(Self {
            metrics,
            alert_config: Arc::new(alert_config),
            evaluator,
        })
    }

    /// Returns the database reaper metrics
    pub fn db_reaper(&self) -> &DatabaseReaperMetrics {
        &self.metrics.db_reaper
    }

    /// Returns the infrastructure reconciler metrics
    pub fn infra_reconciler(&self) -> &InfrastructureReconcilerMetrics {
        &self.metrics.infra_reconciler
    }

    /// Checks for alerts
    pub fn check_alerts(&self) -> Vec<Alert> {
        self.evaluator.check_alerts()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_alert_config_defaults() {
        let config = AlertConfig::default();

        assert_eq!(config.zombie_worker_threshold, 5);
        assert_eq!(config.job_failure_threshold, 10);
        assert_eq!(config.dlq_size_threshold, 100);
        assert_eq!(config.stuck_job_threshold, 5);
    }

    #[tokio::test]
    async fn test_metrics_creation() {
        let metrics = ReconcilerMetrics::new();
        assert!(metrics.is_ok());
    }

    #[tokio::test]
    async fn test_alert_evaluation() {
        let config = AlertConfig {
            zombie_worker_threshold: 1, // Low threshold for testing
            job_failure_threshold: 10,
            dlq_size_threshold: 100,
            stuck_job_threshold: 5,
        };

        let metrics = Arc::new(ReconcilerMetrics::new().unwrap());
        let monitoring = ReconcilerMonitoring::new(config).unwrap();

        // Initially no alerts
        assert!(monitoring.check_alerts().is_empty());

        // Increment zombie counter above threshold
        monitoring.metrics.current_zombie_count.inc();
        monitoring.metrics.current_zombie_count.inc();

        let alerts = monitoring.check_alerts();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].name, "ZombieWorkerAlert");
        assert_eq!(alerts[0].severity, AlertSeverity::Warning);
    }

    #[tokio::test]
    async fn test_alert_severity_display() {
        assert_eq!(AlertSeverity::Warning.to_string(), "WARNING");
        assert_eq!(AlertSeverity::Critical.to_string(), "CRITICAL");
    }
}

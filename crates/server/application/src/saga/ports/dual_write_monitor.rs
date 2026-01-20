//!
//! # Dual-Write Consistency Monitor
//!
//! Monitors consistency between legacy saga system and saga-engine v4
//! during the dual-write migration mode.
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::time;

/// Configuration for dual-write consistency monitoring
#[derive(Debug, Clone)]
pub struct DualWriteConfig {
    /// How often to run consistency checks
    pub check_interval: Duration,
    /// Whether to retry on detected inconsistencies
    pub retry_on_inconsistency: bool,
    /// Maximum number of inconsistencies before alerting
    pub max_inconsistencies: u32,
    /// Enable auto-heal for minor inconsistencies
    pub auto_heal: bool,
    /// Threshold for auto-heal (number of retries)
    pub auto_heal_threshold: u32,
}

impl Default for DualWriteConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(60),
            retry_on_inconsistency: true,
            max_inconsistencies: 100,
            auto_heal: false,
            auto_heal_threshold: 3,
        }
    }
}

/// Types of inconsistencies that can be detected
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InconsistencyType {
    /// Workflow state mismatch
    StateMismatch,
    /// Entity state mismatch
    EntityStateMismatch,
    /// Event log inconsistency
    EventLogMismatch,
    /// Missing workflow in one system
    MissingWorkflow,
    /// Compensation status mismatch
    CompensationMismatch,
}

/// Details of a detected inconsistency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inconsistency {
    pub saga_id: String,
    pub inconsistency_type: InconsistencyType,
    pub legacy_value: serde_json::Value,
    pub v4_value: serde_json::Value,
    pub detected_at: chrono::DateTime<chrono::Utc>,
    pub retry_count: u32,
    pub auto_healed: bool,
}

/// Report of a consistency check run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyReport {
    pub check_id: String,
    pub ran_at: chrono::DateTime<chrono::Utc>,
    pub duration_ms: u64,
    pub total_workflows_checked: usize,
    pub inconsistencies_found: usize,
    pub resolved_count: usize,
    pub new_inconsistencies: usize,
    pub details: Vec<Inconsistency>,
    pub metrics: ConsistencyMetrics,
}

/// Metrics collected during consistency checks
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsistencyMetrics {
    pub total_checks: u64,
    pub total_inconsistencies: u64,
    pub resolved_inconsistencies: u64,
    pub auto_healed_count: u64,
    pub last_check_duration_ms: u64,
    pub average_check_duration_ms: f64,
    pub success_rate: f64,
}

/// Error types for dual-write monitoring
#[derive(Debug, Error, Clone)]
#[error("{0}")]
pub struct DualWriteError(pub String);

/// Trait for workflow state comparison
#[async_trait]
pub trait WorkflowStateComparator: Send + Sync {
    async fn get_legacy_state(&self, saga_id: &str) -> Option<serde_json::Value>;
    async fn get_v4_state(&self, saga_id: &str) -> Option<serde_json::Value>;
    async fn are_states_equivalent(
        &self,
        legacy: &serde_json::Value,
        v4: &serde_json::Value,
    ) -> bool;
    /// Get all saga IDs from both systems
    async fn get_all_saga_ids(&self) -> Vec<String>;
}

/// Metrics recorder for dual-write monitoring
#[async_trait]
pub trait ConsistencyMetricsRecorder: Send + Sync {
    async fn record_check(&self, report: &ConsistencyReport);
    async fn record_inconsistency(&self, inconsistency: &Inconsistency);
    async fn record_resolution(&self, saga_id: &str);
    async fn get_metrics(&self) -> ConsistencyMetrics;
}

/// In-memory implementation of consistency metrics recorder
#[derive(Debug, Default)]
pub struct InMemoryMetricsRecorder {
    metrics: Arc<RwLock<ConsistencyMetrics>>,
}

#[async_trait]
impl ConsistencyMetricsRecorder for InMemoryMetricsRecorder {
    async fn record_check(&self, report: &ConsistencyReport) {
        let mut metrics = self.metrics.write().await;
        metrics.total_checks += 1;
        metrics.total_inconsistencies += report.inconsistencies_found as u64;
        metrics.last_check_duration_ms = report.duration_ms;

        // Update average
        let total = metrics.total_checks as f64;
        metrics.average_check_duration_ms =
            (metrics.average_check_duration_ms * (total - 1.0) + report.duration_ms as f64) / total;
    }

    async fn record_inconsistency(&self, inconsistency: &Inconsistency) {
        let mut metrics = self.metrics.write().await;
        metrics.total_inconsistencies += 1;
    }

    async fn record_resolution(&self, _saga_id: &str) {
        let mut metrics = self.metrics.write().await;
        metrics.resolved_inconsistencies += 1;

        // Update success rate
        let total = metrics.total_inconsistencies.max(1);
        metrics.success_rate = metrics.resolved_inconsistencies as f64 / total as f64;
    }

    async fn get_metrics(&self) -> ConsistencyMetrics {
        self.metrics.read().await.clone()
    }
}

/// Main dual-write consistency monitor
pub struct DualWriteMonitor<C: WorkflowStateComparator> {
    comparator: Arc<C>,
    config: DualWriteConfig,
    metrics_recorder: Arc<dyn ConsistencyMetricsRecorder>,
    active_inconsistencies: Arc<RwLock<Vec<Inconsistency>>>,
    check_interval: Duration,
    running: Arc<RwLock<bool>>,
}

impl<C: WorkflowStateComparator> DualWriteMonitor<C> {
    /// Create a new dual-write monitor
    pub fn new(
        comparator: Arc<C>,
        config: DualWriteConfig,
        metrics_recorder: Arc<dyn ConsistencyMetricsRecorder>,
    ) -> Self {
        let check_interval = config.check_interval;
        Self {
            comparator,
            config,
            metrics_recorder,
            active_inconsistencies: Arc::new(RwLock::new(Vec::new())),
            check_interval,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Run a single consistency check
    pub async fn run_check(&self) -> Result<ConsistencyReport, DualWriteError> {
        let start = tokio::time::Instant::now();

        // Get all saga IDs from both systems
        let saga_ids = self.comparator.get_all_saga_ids().await;

        let mut inconsistencies = Vec::new();
        let mut resolved = 0;
        let mut new_inconsistencies = 0;

        // Check each saga for consistency
        for saga_id in &saga_ids {
            let legacy_state = self.comparator.get_legacy_state(saga_id).await;
            let v4_state = self.comparator.get_v4_state(saga_id).await;

            match (legacy_state, v4_state) {
                // Both present - check consistency
                (Some(legacy), Some(v4)) => {
                    if !self.comparator.are_states_equivalent(&legacy, &v4).await {
                        // Check if this is a known inconsistency
                        let is_known = {
                            let active = self.active_inconsistencies.read().await;
                            active.iter().any(|i| i.saga_id == *saga_id)
                        };

                        if is_known {
                            // Increment retry count
                            let mut active = self.active_inconsistencies.write().await;
                            if let Some(inconsistency) =
                                active.iter_mut().find(|i| i.saga_id == *saga_id)
                            {
                                inconsistency.retry_count += 1;
                            }
                        } else {
                            // New inconsistency
                            new_inconsistencies += 1;
                            let inconsistency = Inconsistency {
                                saga_id: saga_id.clone(),
                                inconsistency_type: InconsistencyType::StateMismatch,
                                legacy_value: legacy.clone(),
                                v4_value: v4,
                                detected_at: chrono::Utc::now(),
                                retry_count: 0,
                                auto_healed: false,
                            };
                            inconsistencies.push(inconsistency.clone());
                            self.active_inconsistencies
                                .write()
                                .await
                                .push(inconsistency);
                        }
                    } else {
                        // States are consistent - check if this was a known issue
                        let mut active = self.active_inconsistencies.write().await;
                        if let Some(pos) = active.iter().position(|i| i.saga_id == *saga_id) {
                            active.remove(pos);
                            resolved += 1;
                            self.metrics_recorder.record_resolution(saga_id).await;
                        }
                    }
                }
                // Only in legacy - missing in v4
                (Some(_), None) => {
                    let inconsistency = Inconsistency {
                        saga_id: saga_id.clone(),
                        inconsistency_type: InconsistencyType::MissingWorkflow,
                        legacy_value: serde_json::json!({"status": "present"}),
                        v4_value: serde_json::json!({"status": "missing"}),
                        detected_at: chrono::Utc::now(),
                        retry_count: 0,
                        auto_healed: false,
                    };
                    inconsistencies.push(inconsistency.clone());
                    self.active_inconsistencies
                        .write()
                        .await
                        .push(inconsistency);
                    new_inconsistencies += 1;
                }
                // Only in v4 - missing in legacy
                (None, Some(_)) => {
                    let inconsistency = Inconsistency {
                        saga_id: saga_id.clone(),
                        inconsistency_type: InconsistencyType::MissingWorkflow,
                        legacy_value: serde_json::json!({"status": "missing"}),
                        v4_value: serde_json::json!({"status": "present"}),
                        detected_at: chrono::Utc::now(),
                        retry_count: 0,
                        auto_healed: false,
                    };
                    inconsistencies.push(inconsistency.clone());
                    self.active_inconsistencies
                        .write()
                        .await
                        .push(inconsistency);
                    new_inconsistencies += 1;
                }
                // Neither has it - skip
                (None, None) => {}
            }
        }

        let duration = start.elapsed();
        let duration_ms = duration.as_millis() as u64;

        // Auto-heal if configured
        if self.config.auto_heal {
            self.auto_heal_inconsistencies().await;
        }

        let report = ConsistencyReport {
            check_id: format!("check-{}", chrono::Utc::now().timestamp()),
            ran_at: chrono::Utc::now(),
            duration_ms,
            total_workflows_checked: saga_ids.len(),
            inconsistencies_found: inconsistencies.len(),
            resolved_count: resolved,
            new_inconsistencies,
            details: inconsistencies,
            metrics: self.metrics_recorder.get_metrics().await,
        };

        // Record metrics
        self.metrics_recorder.record_check(&report).await;

        Ok(report)
    }

    /// Auto-heal minor inconsistencies
    async fn auto_heal_inconsistencies(&self) {
        let threshold = self.config.auto_heal_threshold;

        let mut to_remove = Vec::new();
        let mut to_heal = Vec::new();

        {
            let active = self.active_inconsistencies.read().await;
            for (idx, inconsistency) in active.iter().enumerate() {
                if inconsistency.retry_count >= threshold
                    && inconsistency.inconsistency_type == InconsistencyType::StateMismatch
                {
                    to_remove.push(idx);
                    to_heal.push(inconsistency.saga_id.clone());
                }
            }
        }

        for saga_id in to_heal {
            // Attempt to heal by re-synchronizing from legacy to v4
            self.heal_inconsistency(&saga_id).await;
        }

        // Remove healed inconsistencies
        let mut active = self.active_inconsistencies.write().await;
        for idx in to_remove.iter().rev() {
            active.remove(*idx);
        }
    }

    /// Heal a specific inconsistency
    async fn heal_inconsistency(&self, saga_id: &str) {
        // In a real implementation, this would:
        // 1. Fetch the correct state from legacy
        // 2. Apply it to saga-engine v4
        // 3. Verify the fix

        // For now, mark as auto-healed
        let mut active = self.active_inconsistencies.write().await;
        if let Some(inconsistency) = active.iter_mut().find(|i| i.saga_id == *saga_id) {
            inconsistency.auto_healed = true;
        }
    }

    /// Get all saga IDs to check (simplified - would query databases in real impl)
    async fn get_all_saga_ids(&self) -> Result<Vec<String>, DualWriteError> {
        // This is a placeholder - real implementation would query both DBs
        Ok(Vec::new())
    }

    /// Get current active inconsistencies
    pub async fn get_active_inconsistencies(&self) -> Vec<Inconsistency> {
        self.active_inconsistencies.read().await.clone()
    }

    /// Start background monitoring
    pub async fn start(&self) {
        *self.running.write().await = true;

        let mut interval = time::interval(self.check_interval);

        while *self.running.read().await {
            interval.tick().await;
            if let Err(e) = self.run_check().await {
                tracing::error!("Consistency check failed: {}", e);
            }
        }
    }

    /// Stop background monitoring
    pub async fn stop(&self) {
        *self.running.write().await = false;
    }
}

/// Background task that runs consistency checks
pub struct ConsistencyMonitorTask<C: WorkflowStateComparator> {
    monitor: Arc<DualWriteMonitor<C>>,
}

impl<C: WorkflowStateComparator> ConsistencyMonitorTask<C> {
    pub fn new(monitor: Arc<DualWriteMonitor<C>>) -> Self {
        Self { monitor }
    }

    pub async fn run(&self) {
        self.monitor.start().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    /// Mock comparator for testing
    #[derive(Debug)]
    struct MockComparator {
        states: std::collections::HashMap<
            String,
            (Option<serde_json::Value>, Option<serde_json::Value>),
        >,
    }

    impl MockComparator {
        fn new() -> Self {
            Self {
                states: std::collections::HashMap::new(),
            }
        }

        fn add_saga(
            &mut self,
            saga_id: String,
            legacy: Option<serde_json::Value>,
            v4: Option<serde_json::Value>,
        ) {
            self.states.insert(saga_id, (legacy, v4));
        }
    }

    #[async_trait]
    impl WorkflowStateComparator for MockComparator {
        async fn get_legacy_state(&self, saga_id: &str) -> Option<serde_json::Value> {
            self.states.get(saga_id).map(|(l, _)| l.clone()).flatten()
        }

        async fn get_v4_state(&self, saga_id: &str) -> Option<serde_json::Value> {
            self.states.get(saga_id).map(|(_, v)| v.clone()).flatten()
        }

        async fn are_states_equivalent(
            &self,
            legacy: &serde_json::Value,
            v4: &serde_json::Value,
        ) -> bool {
            legacy == v4
        }

        async fn get_all_saga_ids(&self) -> Vec<String> {
            self.states.keys().cloned().collect()
        }
    }

    #[tokio::test]
    async fn test_consistent_states() {
        let mut comparator = MockComparator::new();
        comparator.add_saga(
            "saga-1".to_string(),
            Some(serde_json::json!({"status": "completed"})),
            Some(serde_json::json!({"status": "completed"})),
        );

        let monitor = DualWriteMonitor::new(
            Arc::new(comparator),
            DualWriteConfig::default(),
            Arc::new(InMemoryMetricsRecorder::default()),
        );

        let report = monitor.run_check().await.unwrap();

        assert_eq!(report.total_workflows_checked, 1);
        assert_eq!(report.inconsistencies_found, 0);
    }

    #[tokio::test]
    async fn test_inconsistent_states() {
        let mut comparator = MockComparator::new();
        comparator.add_saga(
            "saga-1".to_string(),
            Some(serde_json::json!({"status": "completed"})),
            Some(serde_json::json!({"status": "running"})),
        );

        let monitor = DualWriteMonitor::new(
            Arc::new(comparator),
            DualWriteConfig::default(),
            Arc::new(InMemoryMetricsRecorder::default()),
        );

        let report = monitor.run_check().await.unwrap();

        assert_eq!(report.total_workflows_checked, 1);
        assert_eq!(report.inconsistencies_found, 1);
        assert_eq!(report.details[0].saga_id, "saga-1");
    }

    #[tokio::test]
    async fn test_missing_in_legacy() {
        let mut comparator = MockComparator::new();
        comparator.add_saga(
            "saga-1".to_string(),
            None,
            Some(serde_json::json!({"status": "running"})),
        );

        let monitor = DualWriteMonitor::new(
            Arc::new(comparator),
            DualWriteConfig::default(),
            Arc::new(InMemoryMetricsRecorder::default()),
        );

        let report = monitor.run_check().await.unwrap();

        assert_eq!(report.inconsistencies_found, 1);
        assert_eq!(report.new_inconsistencies, 1);
    }

    #[tokio::test]
    async fn test_metrics_recording() {
        let comparator = MockComparator::new();
        let recorder = Arc::new(InMemoryMetricsRecorder::default());

        let monitor = DualWriteMonitor::new(
            Arc::new(comparator),
            DualWriteConfig::default(),
            recorder.clone(),
        );

        monitor.run_check().await.unwrap();

        let metrics = recorder.get_metrics().await;
        assert_eq!(metrics.total_checks, 1);
    }

    #[tokio::test]
    async fn test_auto_heal_threshold() {
        let mut comparator = MockComparator::new();
        comparator.add_saga(
            "saga-1".to_string(),
            Some(serde_json::json!({"status": "completed"})),
            Some(serde_json::json!({"status": "running"})),
        );

        let mut config = DualWriteConfig::default();
        config.auto_heal = true;
        config.auto_heal_threshold = 1;

        let monitor = DualWriteMonitor::new(
            Arc::new(comparator),
            config,
            Arc::new(InMemoryMetricsRecorder::default()),
        );

        // First check - creates inconsistency
        monitor.run_check().await.unwrap();

        // Second check - should auto-heal
        monitor.run_check().await.unwrap();

        let active = monitor.get_active_inconsistencies().await;
        assert!(active.is_empty() || active.iter().all(|i| i.auto_healed));
    }
}

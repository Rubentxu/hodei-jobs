//!
//! # Stuck Detection Port
//!
//! Detects sagas or saga steps that have been stuck for too long,
//! potentially indicating hangs or deadlocks.
//!

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;

/// Configuration for stuck detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StuckDetectionConfig {
    pub saga_timeout: std::time::Duration,
    pub step_timeout: std::time::Duration,
    pub check_interval: std::time::Duration,
    pub enabled: bool,
}

impl Default for StuckDetectionConfig {
    fn default() -> Self {
        Self {
            saga_timeout: std::time::Duration::from_secs(3600), // 1 hour
            step_timeout: std::time::Duration::from_secs(300),  // 5 minutes
            check_interval: std::time::Duration::from_secs(60), // 1 minute
            enabled: true,
        }
    }
}

/// Stuck detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StuckDetectionResult {
    pub saga_id: String,
    pub step_id: Option<String>,
    pub stuck_duration: std::time::Duration,
    pub current_state: String,
    pub detected_at: chrono::DateTime<chrono::Utc>,
    pub recommendation: String,
}

impl StuckDetectionResult {
    pub fn new(
        saga_id: String,
        step_id: Option<String>,
        stuck_duration: std::time::Duration,
        current_state: String,
        recommendation: String,
    ) -> Self {
        Self {
            saga_id,
            step_id,
            stuck_duration,
            current_state,
            detected_at: chrono::Utc::now(),
            recommendation,
        }
    }
}

/// Stuck detection error
#[derive(Debug, Error, Clone)]
#[error("Stuck detection error: {0}")]
pub struct StuckDetectionError(pub String);

/// Saga status for stuck detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaStatusInfo {
    pub saga_id: String,
    pub saga_type: String,
    pub current_state: String,
    pub current_step: Option<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
}

/// Stuck detection port
#[async_trait]
pub trait SagaStuckDetector: Send + Sync {
    /// Checks if a saga is stuck
    async fn is_saga_stuck(&self, saga_status: &SagaStatusInfo) -> Option<StuckDetectionResult>;
    /// Checks if a step is stuck
    async fn is_step_stuck(
        &self,
        saga_status: &SagaStatusInfo,
        step_id: &str,
    ) -> Option<StuckDetectionResult>;
    /// Runs detection on all known sagas
    async fn detect_all(&self, active_sagas: &[SagaStatusInfo]) -> Vec<StuckDetectionResult>;
}

/// In-memory stuck detector
#[derive(Debug)]
pub struct InMemoryStuckDetector {
    config: StuckDetectionConfig,
    last_check: Arc<std::sync::RwLock<chrono::DateTime<chrono::Utc>>>,
}

impl InMemoryStuckDetector {
    pub fn new(config: StuckDetectionConfig) -> Self {
        Self {
            config,
            last_check: Arc::new(std::sync::RwLock::new(chrono::Utc::now())),
        }
    }

    fn calculate_stuck_duration(
        &self,
        started_at: chrono::DateTime<chrono::Utc>,
    ) -> std::time::Duration {
        let elapsed = chrono::Utc::now().signed_duration_since(started_at);
        std::time::Duration::from_secs(elapsed.num_seconds() as u64)
    }
}

#[async_trait]
impl SagaStuckDetector for InMemoryStuckDetector {
    async fn is_saga_stuck(&self, saga_status: &SagaStatusInfo) -> Option<StuckDetectionResult> {
        if !self.config.enabled {
            return None;
        }

        let stuck_duration = self.calculate_stuck_duration(saga_status.started_at);

        if stuck_duration > self.config.saga_timeout {
            Some(StuckDetectionResult::new(
                saga_status.saga_id.clone(),
                None,
                stuck_duration,
                saga_status.current_state.clone(),
                "Saga has exceeded maximum timeout. Consider canceling and restarting.".to_string(),
            ))
        } else {
            None
        }
    }

    async fn is_step_stuck(
        &self,
        saga_status: &SagaStatusInfo,
        step_id: &str,
    ) -> Option<StuckDetectionResult> {
        if !self.config.enabled {
            return None;
        }

        let step_start = saga_status.last_activity;
        let elapsed = chrono::Utc::now().signed_duration_since(step_start);
        let stuck_duration = std::time::Duration::from_secs(elapsed.num_seconds() as u64);
        let seconds = elapsed.num_seconds();

        if stuck_duration > self.config.step_timeout {
            Some(StuckDetectionResult::new(
                saga_status.saga_id.clone(),
                Some(step_id.to_string()),
                stuck_duration,
                saga_status.current_state.clone(),
                format!(
                    "Step '{}' has been executing for {} seconds. Consider investigating.",
                    step_id, seconds,
                ),
            ))
        } else {
            None
        }
    }

    async fn detect_all(&self, active_sagas: &[SagaStatusInfo]) -> Vec<StuckDetectionResult> {
        if !self.config.enabled {
            return vec![];
        }

        let mut results = Vec::new();

        for saga in active_sagas {
            if let Some(stuck) = self.is_saga_stuck(saga).await {
                results.push(stuck);
            }
        }

        let mut last_check = self.last_check.write().unwrap();
        *last_check = chrono::Utc::now();

        results
    }
}

/// Background stuck detection task
pub struct StuckDetectionTask {
    detector: Arc<dyn SagaStuckDetector>,
    saga_provider: Arc<dyn SagaStatusProvider>,
    config: StuckDetectionConfig,
}

#[async_trait]
pub trait SagaStatusProvider: Send + Sync {
    async fn get_active_sagas(&self) -> Vec<SagaStatusInfo>;
}

impl StuckDetectionTask {
    pub fn new(
        detector: Arc<dyn SagaStuckDetector>,
        saga_provider: Arc<dyn SagaStatusProvider>,
        config: StuckDetectionConfig,
    ) -> Self {
        Self {
            detector,
            saga_provider,
            config,
        }
    }

    pub async fn run(&self) -> Vec<StuckDetectionResult> {
        let active_sagas = self.saga_provider.get_active_sagas().await;
        self.detector.detect_all(&active_sagas).await
    }
}

#[cfg(test)]
mod stuck_detection_tests {
    use super::*;

    fn create_test_saga_status(state: &str, hours_ago: i64) -> SagaStatusInfo {
        SagaStatusInfo {
            saga_id: "test-saga".to_string(),
            saga_type: "test".to_string(),
            current_state: state.to_string(),
            current_step: Some("test-step".to_string()),
            started_at: chrono::Utc::now() - chrono::Duration::hours(hours_ago),
            last_activity: chrono::Utc::now() - chrono::Duration::hours(hours_ago),
        }
    }

    #[tokio::test]
    async fn test_detector_identifies_stuck_saga() {
        let config = StuckDetectionConfig {
            saga_timeout: std::time::Duration::from_secs(60),
            step_timeout: std::time::Duration::from_secs(30),
            check_interval: std::time::Duration::from_secs(10),
            enabled: true,
        };
        let detector = Arc::new(InMemoryStuckDetector::new(config));

        // Create a saga that started 2 hours ago
        let saga = create_test_saga_status("Running", 2);

        let result = detector.is_saga_stuck(&saga).await;

        assert!(result.is_some());
        let stuck = result.unwrap();
        assert_eq!(stuck.saga_id, "test-saga");
        assert!(stuck.stuck_duration > std::time::Duration::from_secs(60 * 60));
    }

    #[tokio::test]
    async fn test_detector_allows_normal_saga() {
        let config = StuckDetectionConfig::default();
        let detector = Arc::new(InMemoryStuckDetector::new(config));

        // Create a saga that started 5 minutes ago
        let saga = create_test_saga_status("Running", 0);

        let result = detector.is_saga_stuck(&saga).await;

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_detect_all_returns_stuck_sagas() {
        let config = StuckDetectionConfig {
            saga_timeout: std::time::Duration::from_secs(1),
            step_timeout: std::time::Duration::from_secs(1),
            check_interval: std::time::Duration::from_secs(10),
            enabled: true,
        };
        let detector = Arc::new(InMemoryStuckDetector::new(config));

        let sagas = vec![
            create_test_saga_status("Running", 0),
            create_test_saga_status("Running", 2), // Stuck
        ];

        let results = detector.detect_all(&sagas).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].saga_id, "test-saga");
    }

    #[tokio::test]
    async fn test_disabled_detector_returns_nothing() {
        let config = StuckDetectionConfig {
            saga_timeout: std::time::Duration::from_secs(1),
            step_timeout: std::time::Duration::from_secs(1),
            check_interval: std::time::Duration::from_secs(10),
            enabled: false,
        };
        let detector = Arc::new(InMemoryStuckDetector::new(config));

        let sagas = vec![create_test_saga_status("Running", 2)];

        let results = detector.detect_all(&sagas).await;

        assert!(results.is_empty());
    }
}

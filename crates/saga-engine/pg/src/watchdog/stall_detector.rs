//! Stall Detection
//!
//! Detects when components are stalled (no activity for extended periods).
//! This is critical for the auto-recovery system.

use chrono::{DateTime, Utc};
use std::time::Duration;

/// Stall status for a component
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StallStatus {
    /// Component is active and processing
    Active,
    /// Component may be stalled (grace period exceeded but not yet critical)
    Degraded,
    /// Component is definitely stalled (no activity for too long)
    Stalled { age_seconds: u64 },
}

/// Stall detector configuration
#[derive(Debug, Clone)]
pub struct StallDetectorConfig {
    /// Time before component is considered degraded (default: 30s)
    pub degraded_threshold: Duration,
    /// Time before component is considered stalled (default: 60s)
    pub stalled_threshold: Duration,
}

impl Default for StallDetectorConfig {
    fn default() -> Self {
        Self {
            degraded_threshold: Duration::from_secs(30),
            stalled_threshold: Duration::from_secs(60),
        }
    }
}

impl StallDetectorConfig {
    /// Create builder
    pub fn builder() -> StallDetectorConfigBuilder {
        StallDetectorConfigBuilder::default()
    }
}

/// Builder for stall detector configuration
#[derive(Default)]
pub struct StallDetectorConfigBuilder {
    degraded_threshold: Option<Duration>,
    stalled_threshold: Option<Duration>,
}

impl StallDetectorConfigBuilder {
    /// Set degraded threshold
    pub fn with_degraded_threshold(mut self, threshold: Duration) -> Self {
        self.degraded_threshold = Some(threshold);
        self
    }

    /// Set stalled threshold
    pub fn with_stalled_threshold(mut self, threshold: Duration) -> Self {
        self.stalled_threshold = Some(threshold);
        self
    }

    /// Build configuration
    pub fn build(self) -> StallDetectorConfig {
        StallDetectorConfig {
            degraded_threshold: self.degraded_threshold.unwrap_or(Duration::from_secs(30)),
            stalled_threshold: self.stalled_threshold.unwrap_or(Duration::from_secs(60)),
        }
    }
}

/// Stall detector for components
///
/// Detects when components have not processed any events or timers
/// for extended periods.
#[derive(Clone)]
pub struct StallDetector {
    config: StallDetectorConfig,
}

impl StallDetector {
    /// Create new stall detector
    pub fn new(config: StallDetectorConfig) -> Self {
        Self { config }
    }

    /// Create stall detector with default configuration
    pub fn with_defaults() -> Self {
        Self::new(StallDetectorConfig::default())
    }

    /// Detect stall status for a component
    ///
    /// # Arguments
    ///
    /// * `last_activity` - Last activity timestamp
    /// * `now` - Current timestamp (defaults to Utc::now())
    ///
    /// # Returns
    ///
    /// Stall status based on activity age
    pub fn detect_stall(
        &self,
        last_activity: Option<DateTime<Utc>>,
        now: Option<DateTime<Utc>>,
    ) -> StallStatus {
        let now = now.unwrap_or_else(Utc::now);

        match last_activity {
            Some(last_activity) => {
                let elapsed = now.signed_duration_since(last_activity);
                let elapsed_secs = elapsed.num_seconds().max(0) as u64;
                let elapsed_duration = Duration::from_secs(elapsed_secs);

                if elapsed_duration >= self.config.stalled_threshold {
                    StallStatus::Stalled {
                        age_seconds: elapsed_secs,
                    }
                } else if elapsed_duration >= self.config.degraded_threshold {
                    StallStatus::Degraded
                } else {
                    StallStatus::Active
                }
            }
            None => StallStatus::Stalled {
                age_seconds: u64::MAX,
            }, // Never had activity
        }
    }

    /// Check if component is stalled
    pub fn is_stalled(&self, last_activity: Option<DateTime<Utc>>) -> bool {
        matches!(
            self.detect_stall(last_activity, None),
            StallStatus::Stalled { .. }
        )
    }

    /// Check if component is degraded
    pub fn is_degraded(&self, last_activity: Option<DateTime<Utc>>) -> bool {
        matches!(
            self.detect_stall(last_activity, None),
            StallStatus::Degraded | StallStatus::Stalled { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stall_status_variants() {
        let active = StallStatus::Active;
        let degraded = StallStatus::Degraded;
        let stalled = StallStatus::Stalled { age_seconds: 100 };

        assert!(matches!(active, StallStatus::Active));
        assert!(matches!(degraded, StallStatus::Degraded));
        assert!(matches!(stalled, StallStatus::Stalled { .. }));
    }

    #[test]
    fn test_stall_detector_active() {
        let detector = StallDetector::with_defaults();
        let now = Utc::now();
        let last_activity = Some(now - chrono::Duration::seconds(10));

        let status = detector.detect_stall(last_activity, Some(now));

        assert!(matches!(status, StallStatus::Active));
    }

    #[test]
    fn test_stall_detector_degraded() {
        let detector = StallDetector::with_defaults();
        let now = Utc::now();
        let last_activity = Some(now - chrono::Duration::seconds(45));

        let status = detector.detect_stall(last_activity, Some(now));

        assert!(matches!(status, StallStatus::Degraded));
    }

    #[test]
    fn test_stall_detector_stalled() {
        let detector = StallDetector::with_defaults();
        let now = Utc::now();
        let last_activity = Some(now - chrono::Duration::seconds(90));

        let status = detector.detect_stall(last_activity, Some(now));

        assert!(matches!(
            status,
            StallStatus::Stalled {
                age_seconds: 90..=120
            }
        ));
    }

    #[test]
    fn test_stall_detector_no_activity() {
        let detector = StallDetector::with_defaults();
        let last_activity: Option<DateTime<Utc>> = None;

        let status = detector.detect_stall(last_activity, None);

        assert!(matches!(
            status,
            StallStatus::Stalled {
                age_seconds: u64::MAX
            }
        ));
    }

    #[test]
    fn test_stall_detector_is_stalled() {
        let detector = StallDetector::with_defaults();
        let now = Utc::now();

        // Not stalled
        let recent_activity = Some(now - chrono::Duration::seconds(10));
        assert!(!detector.is_stalled(recent_activity));

        // Stalled
        let old_activity = Some(now - chrono::Duration::seconds(90));
        assert!(detector.is_stalled(old_activity));
    }

    #[test]
    fn test_stall_detector_is_degraded() {
        let detector = StallDetector::with_defaults();
        let now = Utc::now();

        // Not degraded
        let recent_activity = Some(now - chrono::Duration::seconds(10));
        assert!(!detector.is_degraded(recent_activity));

        // Degraded
        let degraded_activity = Some(now - chrono::Duration::seconds(45));
        assert!(detector.is_degraded(degraded_activity));

        // Also degraded (stalled implies degraded)
        let stalled_activity = Some(now - chrono::Duration::seconds(90));
        assert!(detector.is_degraded(stalled_activity));
    }

    #[test]
    fn test_stall_detector_config_builder() {
        let config = StallDetectorConfig::builder()
            .with_degraded_threshold(Duration::from_secs(20))
            .with_stalled_threshold(Duration::from_secs(40))
            .build();

        assert_eq!(config.degraded_threshold, Duration::from_secs(20));
        assert_eq!(config.stalled_threshold, Duration::from_secs(40));
    }

    #[test]
    fn test_stall_detector_custom_thresholds() {
        let config = StallDetectorConfig {
            degraded_threshold: Duration::from_secs(20),
            stalled_threshold: Duration::from_secs(40),
        };
        let detector = StallDetector::new(config);

        let now = Utc::now();

        // Active (10s < 20s degraded threshold)
        let active = detector.detect_stall(Some(now - chrono::Duration::seconds(10)), Some(now));
        assert!(matches!(active, StallStatus::Active));

        // Degraded (25s >= 20s, < 40s)
        let degraded = detector.detect_stall(Some(now - chrono::Duration::seconds(25)), Some(now));
        assert!(matches!(degraded, StallStatus::Degraded));

        // Stalled (50s >= 40s stalled threshold)
        let stalled = detector.detect_stall(Some(now - chrono::Duration::seconds(50)), Some(now));
        assert!(matches!(
            stalled,
            StallStatus::Stalled {
                age_seconds: 50..=60
            }
        ));
    }
}

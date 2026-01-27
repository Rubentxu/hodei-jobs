//! Deadlock Detection
//!
//! Detects when workflows, timers, or components are stuck in
//! a non-progressing state (deadlocked).

use chrono::{DateTime, Utc};
use std::time::Duration;

/// Deadlock status for a component
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlockStatus {
    /// Component is processing normally
    Processing,
    /// Component may be deadlocked (warning threshold exceeded)
    Warning { age_seconds: u64 },
    /// Component is definitely deadlocked
    Deadlocked { age_seconds: u64 },
}

/// Deadlock detector configuration
#[derive(Debug, Clone)]
pub struct DeadlockDetectorConfig {
    /// Time before warning is issued (default: 120s / 2min)
    pub warning_threshold: Duration,
    /// Time before deadlock is declared (default: 300s / 5min)
    pub deadlock_threshold: Duration,
}

impl Default for DeadlockDetectorConfig {
    fn default() -> Self {
        Self {
            warning_threshold: Duration::from_secs(120),
            deadlock_threshold: Duration::from_secs(300),
        }
    }
}

impl DeadlockDetectorConfig {
    /// Create builder
    pub fn builder() -> DeadlockDetectorConfigBuilder {
        DeadlockDetectorConfigBuilder::default()
    }
}

/// Builder for deadlock detector configuration
#[derive(Default)]
pub struct DeadlockDetectorConfigBuilder {
    warning_threshold: Option<Duration>,
    deadlock_threshold: Option<Duration>,
}

impl DeadlockDetectorConfigBuilder {
    /// Set warning threshold
    pub fn with_warning_threshold(mut self, threshold: Duration) -> Self {
        self.warning_threshold = Some(threshold);
        self
    }

    /// Set deadlock threshold
    pub fn with_deadlock_threshold(mut self, threshold: Duration) -> Self {
        self.deadlock_threshold = Some(threshold);
        self
    }

    /// Build configuration
    pub fn build(self) -> DeadlockDetectorConfig {
        DeadlockDetectorConfig {
            warning_threshold: self.warning_threshold.unwrap_or(Duration::from_secs(120)),
            deadlock_threshold: self.deadlock_threshold.unwrap_or(Duration::from_secs(300)),
        }
    }
}

/// Deadlock detector for components
///
/// Detects when workflows, timers, or other components
/// are stuck in a non-progressing state (deadlocked).
#[derive(Clone)]
pub struct DeadlockDetector {
    config: DeadlockDetectorConfig,
}

impl DeadlockDetector {
    /// Create new deadlock detector
    pub fn new(config: DeadlockDetectorConfig) -> Self {
        Self { config }
    }

    /// Create deadlock detector with default configuration
    pub fn with_defaults() -> Self {
        Self::new(DeadlockDetectorConfig::default())
    }

    /// Detect deadlock status for a workflow/timer
    ///
    /// # Arguments
    ///
    /// * `last_progress` - Last time progress was made
    /// * `current_state` - Current state (e.g., "Processing", "Pending")
    /// * `is_completed` - Whether the workflow/timer is completed
    /// * `now` - Current timestamp (defaults to Utc::now())
    ///
    /// # Returns
    ///
    /// Deadlock status based on time since last progress
    pub fn detect_deadlock(
        &self,
        last_progress: Option<DateTime<Utc>>,
        current_state: &str,
        is_completed: bool,
        now: Option<DateTime<Utc>>,
    ) -> DeadlockStatus {
        // If completed, not deadlocked
        if is_completed {
            return DeadlockStatus::Processing;
        }

        let now = now.unwrap_or_else(Utc::now);

        match last_progress {
            Some(last_progress) => {
                let elapsed = now.signed_duration_since(last_progress);
                let elapsed_secs = elapsed.num_seconds().max(0) as u64;
                let elapsed_duration = Duration::from_secs(elapsed_secs);

                if elapsed_duration >= self.config.deadlock_threshold {
                    DeadlockStatus::Deadlocked {
                        age_seconds: elapsed_secs,
                    }
                } else if elapsed_duration >= self.config.warning_threshold {
                    DeadlockStatus::Warning {
                        age_seconds: elapsed_secs,
                    }
                } else {
                    DeadlockStatus::Processing
                }
            }
            None => {
                // No progress recorded - check if in transient state
                if Self::is_transient_state(current_state) {
                    DeadlockStatus::Processing
                } else {
                    // No progress and not in transient state -> potential deadlock
                    DeadlockStatus::Deadlocked {
                        age_seconds: u64::MAX,
                    }
                }
            }
        }
    }

    /// Check if state is transient (expected to be temporary)
    fn is_transient_state(state: &str) -> bool {
        let state_lower = state.to_lowercase();
        matches!(
            state_lower.as_str(),
            "initializing" | "connecting" | "starting" | "pending"
        )
    }

    /// Check if component is deadlocked
    pub fn is_deadlocked(&self, last_progress: Option<DateTime<Utc>>) -> bool {
        matches!(
            self.detect_deadlock(last_progress, "Unknown", false, None),
            DeadlockStatus::Deadlocked { .. }
        )
    }

    /// Check if component has deadlock warning
    pub fn has_warning(&self, last_progress: Option<DateTime<Utc>>) -> bool {
        matches!(
            self.detect_deadlock(last_progress, "Unknown", false, None),
            DeadlockStatus::Warning { .. } | DeadlockStatus::Deadlocked { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deadlock_status_variants() {
        let processing = DeadlockStatus::Processing;
        let warning = DeadlockStatus::Warning { age_seconds: 150 };
        let deadlocked = DeadlockStatus::Deadlocked { age_seconds: 400 };

        assert!(matches!(processing, DeadlockStatus::Processing));
        assert!(matches!(
            warning,
            DeadlockStatus::Warning { age_seconds: 150 }
        ));
        assert!(matches!(
            deadlocked,
            DeadlockStatus::Deadlocked { age_seconds: 400 }
        ));
    }

    #[test]
    fn test_deadlock_detector_processing() {
        let detector = DeadlockDetector::with_defaults();
        let now = Utc::now();
        let last_progress = Some(now - chrono::Duration::seconds(60));

        let status = detector.detect_deadlock(last_progress, "Processing", false, Some(now));

        assert!(matches!(status, DeadlockStatus::Processing));
    }

    #[test]
    fn test_deadlock_detector_warning() {
        let detector = DeadlockDetector::with_defaults();
        let now = Utc::now();
        let last_progress = Some(now - chrono::Duration::seconds(180));

        let status = detector.detect_deadlock(last_progress, "Processing", false, Some(now));

        assert!(matches!(
            status,
            DeadlockStatus::Warning {
                age_seconds: 180..=299
            }
        ));
    }

    #[test]
    fn test_deadlock_detector_deadlocked() {
        let detector = DeadlockDetector::with_defaults();
        let now = Utc::now();
        let last_progress = Some(now - chrono::Duration::seconds(400));

        let status = detector.detect_deadlock(last_progress, "Processing", false, Some(now));

        assert!(matches!(
            status,
            DeadlockStatus::Deadlocked {
                age_seconds: 400..=500
            }
        ));
    }

    #[test]
    fn test_deadlock_detector_completed() {
        let detector = DeadlockDetector::with_defaults();
        let now = Utc::now();
        let last_progress = Some(now - chrono::Duration::seconds(1000));

        let status = detector.detect_deadlock(last_progress, "Processing", true, Some(now));

        // Should not be deadlocked even if old progress
        assert!(matches!(status, DeadlockStatus::Processing));
    }

    #[test]
    fn test_deadlock_detector_no_progress_transient() {
        let detector = DeadlockDetector::with_defaults();
        let last_progress: Option<DateTime<Utc>> = None;

        let status = detector.detect_deadlock(last_progress, "Initializing", false, None);

        // Transient state, no progress -> not deadlocked
        assert!(matches!(status, DeadlockStatus::Processing));
    }

    #[test]
    fn test_deadlock_detector_no_progress_non_transient() {
        let detector = DeadlockDetector::with_defaults();
        let last_progress: Option<DateTime<Utc>> = None;

        let status = detector.detect_deadlock(last_progress, "Processing", false, None);

        // Non-transient state, no progress -> deadlocked
        assert!(matches!(
            status,
            DeadlockStatus::Deadlocked {
                age_seconds: u64::MAX
            }
        ));
    }

    #[test]
    fn test_deadlock_detector_is_deadlocked() {
        let detector = DeadlockDetector::with_defaults();
        let now = Utc::now();

        // Not deadlocked (recent progress)
        let recent = Some(now - chrono::Duration::seconds(60));
        assert!(!detector.is_deadlocked(recent));

        // Deadlocked (old progress)
        let old = Some(now - chrono::Duration::seconds(400));
        assert!(detector.is_deadlocked(old));
    }

    #[test]
    fn test_deadlock_detector_has_warning() {
        let detector = DeadlockDetector::with_defaults();
        let now = Utc::now();

        // No warning (recent progress)
        let recent = Some(now - chrono::Duration::seconds(60));
        assert!(!detector.has_warning(recent));

        // Warning (moderate old progress)
        let warning = Some(now - chrono::Duration::seconds(180));
        assert!(detector.has_warning(warning));

        // Also warning (deadlocked implies warning)
        let deadlocked = Some(now - chrono::Duration::seconds(400));
        assert!(detector.has_warning(deadlocked));
    }

    #[test]
    fn test_deadlock_detector_config_builder() {
        let config = DeadlockDetectorConfig::builder()
            .with_warning_threshold(Duration::from_secs(90))
            .with_deadlock_threshold(Duration::from_secs(180))
            .build();

        assert_eq!(config.warning_threshold, Duration::from_secs(90));
        assert_eq!(config.deadlock_threshold, Duration::from_secs(180));
    }

    #[test]
    fn test_deadlock_detector_custom_thresholds() {
        let config = DeadlockDetectorConfig {
            warning_threshold: Duration::from_secs(90),
            deadlock_threshold: Duration::from_secs(180),
        };
        let detector = DeadlockDetector::new(config);

        let now = Utc::now();

        // Processing (60s < 90s warning threshold)
        let processing = detector.detect_deadlock(
            Some(now - chrono::Duration::seconds(60)),
            "Processing",
            false,
            Some(now),
        );
        assert!(matches!(processing, DeadlockStatus::Processing));

        // Warning (120s >= 90s, < 180s)
        let warning = detector.detect_deadlock(
            Some(now - chrono::Duration::seconds(120)),
            "Processing",
            false,
            Some(now),
        );
        assert!(matches!(
            warning,
            DeadlockStatus::Warning {
                age_seconds: 120..=179
            }
        ));

        // Deadlocked (200s >= 180s)
        let deadlocked = detector.detect_deadlock(
            Some(now - chrono::Duration::seconds(200)),
            "Processing",
            false,
            Some(now),
        );
        assert!(matches!(
            deadlocked,
            DeadlockStatus::Deadlocked {
                age_seconds: 200..=300
            }
        ));
    }
}

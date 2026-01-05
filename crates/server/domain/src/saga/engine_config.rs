//! Saga Engine Configuration
//!
//! Configuration struct for saga engine with feature flags for gradual rollout.
//! EPIC-45 Gap 2: SagaEngineConfig for feature flags.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Saga Engine Configuration
///
/// Provides feature flags for gradual rollout of saga capabilities.
/// This configuration allows enabling/disabling features without code changes.
///
/// # Example
/// ```rust
/// use hodei_server_domain::saga::SagaEngineConfig;
///
/// let config = SagaEngineConfig::default();
/// assert!(config.enable_real_compensation());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaEngineConfig {
    /// Enable real compensation (rollback) for saga steps
    /// When disabled, compensation steps are skipped
    #[serde(default = "default_true")]
    pub enable_real_compensation: bool,

    /// Enable saga resume from persisted state
    /// When disabled, sagas always start from step 0
    #[serde(default = "default_true")]
    pub enable_saga_resume: bool,

    /// Enable locked polling for stuck saga recovery
    /// When disabled, stuck sagas won't be recovered automatically
    #[serde(default = "default_true")]
    pub enable_locked_polling: bool,

    /// Timeout in seconds before a saga is considered "stuck"
    #[serde(default = "default_stuck_timeout")]
    pub stuck_saga_timeout_seconds: u64,

    /// Maximum number of compensation retries before giving up
    #[serde(default = "default_max_retries")]
    pub max_compensation_retries: u32,

    /// Safety polling interval for stuck saga recovery
    #[serde(default = "default_polling_interval")]
    pub safety_polling_interval_seconds: u64,
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_stuck_timeout() -> u64 {
    300 // 5 minutes
}

fn default_max_retries() -> u32 {
    3
}

fn default_polling_interval() -> u64 {
    300 // 5 minutes
}

impl Default for SagaEngineConfig {
    fn default() -> Self {
        Self {
            enable_real_compensation: true,
            enable_saga_resume: true,
            enable_locked_polling: true,
            stuck_saga_timeout_seconds: default_stuck_timeout(),
            max_compensation_retries: default_max_retries(),
            safety_polling_interval_seconds: default_polling_interval(),
        }
    }
}

impl SagaEngineConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable real compensation
    pub fn with_real_compensation(mut self, enabled: bool) -> Self {
        self.enable_real_compensation = enabled;
        self
    }

    /// Enable saga resume
    pub fn with_saga_resume(mut self, enabled: bool) -> Self {
        self.enable_saga_resume = enabled;
        self
    }

    /// Enable locked polling
    pub fn with_locked_polling(mut self, enabled: bool) -> Self {
        self.enable_locked_polling = enabled;
        self
    }

    /// Set stuck saga timeout
    pub fn with_stuck_timeout(mut self, seconds: u64) -> Self {
        self.stuck_saga_timeout_seconds = seconds;
        self
    }

    /// Set max compensation retries
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_compensation_retries = retries;
        self
    }

    /// Set safety polling interval
    pub fn with_polling_interval(mut self, seconds: u64) -> Self {
        self.safety_polling_interval_seconds = seconds;
        self
    }

    /// Check if real compensation is enabled
    pub fn enable_real_compensation(&self) -> bool {
        self.enable_real_compensation
    }

    /// Check if saga resume is enabled
    pub fn enable_saga_resume(&self) -> bool {
        self.enable_saga_resume
    }

    /// Check if locked polling is enabled
    pub fn enable_locked_polling(&self) -> bool {
        self.enable_locked_polling
    }

    /// Get stuck saga timeout as Duration
    pub fn stuck_saga_timeout(&self) -> Duration {
        Duration::from_secs(self.stuck_saga_timeout_seconds)
    }

    /// Get safety polling interval as Duration
    pub fn safety_polling_interval(&self) -> Duration {
        Duration::from_secs(self.safety_polling_interval_seconds)
    }

    /// Get max compensation retries
    pub fn max_compensation_retries(&self) -> u32 {
        self.max_compensation_retries
    }

    /// Check if a feature is enabled for gradual rollout
    pub fn is_feature_enabled(&self, feature: SagaFeature) -> bool {
        match feature {
            SagaFeature::RealCompensation => self.enable_real_compensation,
            SagaFeature::SagaResume => self.enable_saga_resume,
            SagaFeature::LockedPolling => self.enable_locked_polling,
        }
    }
}

/// Saga features for granular feature flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SagaFeature {
    /// Real compensation (rollback) feature
    RealCompensation,
    /// Saga resume from persisted state
    SagaResume,
    /// Locked polling for stuck saga recovery
    LockedPolling,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SagaEngineConfig::default();
        assert!(config.enable_real_compensation());
        assert!(config.enable_saga_resume());
        assert!(config.enable_locked_polling());
        assert_eq!(config.stuck_saga_timeout_seconds, 300);
        assert_eq!(config.max_compensation_retries, 3);
    }

    #[test]
    fn test_builder_pattern() {
        let config = SagaEngineConfig::new()
            .with_real_compensation(false)
            .with_saga_resume(false)
            .with_locked_polling(false)
            .with_stuck_timeout(600)
            .with_max_retries(5)
            .with_polling_interval(600);

        assert!(!config.enable_real_compensation());
        assert!(!config.enable_saga_resume());
        assert!(!config.enable_locked_polling());
        assert_eq!(config.stuck_saga_timeout(), Duration::from_secs(600));
        assert_eq!(config.max_compensation_retries(), 5);
    }

    #[test]
    fn test_is_feature_enabled() {
        let config = SagaEngineConfig::default();
        assert!(config.is_feature_enabled(SagaFeature::RealCompensation));
        assert!(config.is_feature_enabled(SagaFeature::SagaResume));
        assert!(config.is_feature_enabled(SagaFeature::LockedPolling));

        let config = SagaEngineConfig::new()
            .with_real_compensation(false)
            .with_saga_resume(false)
            .with_locked_polling(false);

        assert!(!config.is_feature_enabled(SagaFeature::RealCompensation));
        assert!(!config.is_feature_enabled(SagaFeature::SagaResume));
        assert!(!config.is_feature_enabled(SagaFeature::LockedPolling));
    }

    #[test]
    fn test_duration_conversions() {
        let config = SagaEngineConfig::new()
            .with_stuck_timeout(600)
            .with_polling_interval(900);

        assert_eq!(config.stuck_saga_timeout(), Duration::from_secs(600));
        assert_eq!(config.safety_polling_interval(), Duration::from_secs(900));
    }
}

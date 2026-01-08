//! BackoffConfig - Shared exponential backoff configuration
//!
//! This module provides a reusable exponential backoff strategy for both
//! Command and Event outboxes. It includes configurable base delay, max delay,
//! jitter, and retry limits.
//!
//! # Architecture
//!
//! ```text
//! Retry Count    Delay (base=5s)    With jitter (±10%)
//! ──────────────────────────────────────────────────────
//!     0              5s              4.5s - 5.5s
//!     1             10s              9s - 11s
//!     2             20s             18s - 22s
//!     3             40s             36s - 44s
//!     4             80s             72s - 88s
//!     5            160s            144s - 176s
//!    >5         DEAD LETTER        Max retries exceeded
//! ```
//!
//! # Usage
//!
//! ```rust
//! use backoff::BackoffConfig;
//!
//! let config = BackoffConfig::standard();
//!
//! // Calculate delay for retry 2
//! let delay = config.calculate_delay(2);
//! // Returns Duration::seconds(20) ± 10%
//!
//! // Check if we can retry
//! if config.can_retry(3) {
//!     // Schedule retry
//! }
//! ```

use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Default base delay in seconds
const DEFAULT_BASE_DELAY_SECS: i64 = 5;

/// Default max delay in seconds (30 minutes)
const DEFAULT_MAX_DELAY_SECS: i64 = 1800;

/// Default jitter factor (10%)
const DEFAULT_JITTER_FACTOR: f64 = 0.1;

/// Default max retries before dead letter
const DEFAULT_MAX_RETRIES: i32 = 5;

/// Configuración de exponential backoff reutilizable.
///
/// Esta estructura define la estrategia de reintentos con exponential backoff
/// que puede ser compartida entre Command y Event outboxes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffConfig {
    /// Delay base en segundos (default: 5)
    #[serde(default = "default_base_delay")]
    pub base_delay_secs: i64,

    /// Delay máximo en segundos (default: 1800 = 30min)
    #[serde(default = "default_max_delay")]
    pub max_delay_secs: i64,

    /// Factor de jitter (0.0-1.0, default: 0.1 = ±10%)
    #[serde(default = "default_jitter")]
    pub jitter_factor: f64,

    /// Máximo reintentos antes de dead letter
    #[serde(default = "default_max_retries")]
    pub max_retries: i32,
}

fn default_base_delay() -> i64 {
    DEFAULT_BASE_DELAY_SECS
}

fn default_max_delay() -> i64 {
    DEFAULT_MAX_DELAY_SECS
}

fn default_jitter() -> f64 {
    DEFAULT_JITTER_FACTOR
}

fn default_max_retries() -> i32 {
    DEFAULT_MAX_RETRIES
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            base_delay_secs: DEFAULT_BASE_DELAY_SECS,
            max_delay_secs: DEFAULT_MAX_DELAY_SECS,
            jitter_factor: DEFAULT_JITTER_FACTOR,
            max_retries: DEFAULT_MAX_RETRIES,
        }
    }
}

impl fmt::Display for BackoffConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BackoffConfig(base_delay={}s, max_delay={}s, jitter={:.1}%, max_retries={})",
            self.base_delay_secs,
            self.max_delay_secs,
            self.jitter_factor * 100.0,
            self.max_retries
        )
    }
}

impl BackoffConfig {
    /// Create a standard backoff configuration.
    ///
    /// This is the default configuration with:
    /// - 5 second base delay
    /// - 30 minute max delay
    /// - 10% jitter
    /// - 5 max retries
    pub fn standard() -> Self {
        Self::default()
    }

    /// Create an aggressive backoff configuration.
    ///
    /// Use this for critical events that need faster recovery:
    /// - 1 second base delay
    /// - 5 minute max delay
    /// - 20% jitter
    /// - 3 max retries
    pub fn aggressive() -> Self {
        Self {
            base_delay_secs: 1,
            max_delay_secs: 300,
            jitter_factor: 0.2,
            max_retries: 3,
        }
    }

    /// Create a conservative backoff configuration.
    ///
    /// Use this for non-critical operations:
    /// - 10 second base delay
    /// - 1 hour max delay
    /// - 15% jitter
    /// - 10 max retries
    pub fn conservative() -> Self {
        Self {
            base_delay_secs: 10,
            max_delay_secs: 3600,
            jitter_factor: 0.15,
            max_retries: 10,
        }
    }

    /// Create a custom backoff configuration.
    ///
    /// # Arguments
    /// * `base_delay_secs` - Initial delay in seconds
    /// * `max_delay_secs` - Maximum delay in seconds
    /// * `jitter_factor` - Jitter as a fraction (0.1 = 10%)
    /// * `max_retries` - Maximum retry attempts
    pub fn new(base_delay_secs: i64, max_delay_secs: i64, jitter_factor: f64, max_retries: i32) -> Self {
        Self {
            base_delay_secs,
            max_delay_secs,
            jitter_factor,
            max_retries,
        }
    }

    /// Calculate delay for a specific retry attempt.
    ///
    /// The delay follows the formula:
    /// `delay = min(base_delay * 2^retry_count, max_delay) + jitter`
    ///
    /// # Arguments
    /// * `retry_count` - The current retry attempt (0-indexed)
    ///
    /// # Returns
    /// A `Duration` representing the delay before the next attempt
    ///
    /// # Examples
    ///
    /// ```rust
    /// let config = BackoffConfig::standard();
    ///
    /// // First retry: ~5 seconds ± 10%
    /// let delay0 = config.calculate_delay(0);
    ///
    /// // Second retry: ~10 seconds ± 10%
    /// let delay1 = config.calculate_delay(1);
    /// ```
    pub fn calculate_delay(&self, retry_count: i32) -> Duration {
        // Exponential: base * 2^retry_count
        let raw_delay = self.base_delay_secs * 2i64.pow(retry_count as u32);
        let delay = raw_delay.min(self.max_delay_secs);

        // Apply jitter
        let jitter_range = (delay as f64 * self.jitter_factor) as i64;
        let jitter = if jitter_range > 0 {
            let mut rng = rand::thread_rng();
            rng.gen_range(-jitter_range..=jitter_range)
        } else {
            0
        };

        Duration::seconds(delay + jitter)
    }

    /// Calculate the timestamp for the next retry.
    ///
    /// # Arguments
    /// * `retry_count` - The current retry attempt
    ///
    /// # Returns
    /// A `DateTime<Utc>` representing when the next attempt should occur
    pub fn next_retry_at(&self, retry_count: i32) -> DateTime<Utc> {
        Utc::now() + self.calculate_delay(retry_count)
    }

    /// Check if retry is allowed.
    ///
    /// # Arguments
    /// * `retry_count` - The current retry attempt
    ///
    /// # Returns
    /// `true` if more retries are allowed, `false` otherwise
    pub fn can_retry(&self, retry_count: i32) -> bool {
        retry_count < self.max_retries
    }

    /// Get the maximum number of retries.
    pub fn max_retries(&self) -> i32 {
        self.max_retries
    }

    /// Get the base delay in seconds.
    pub fn base_delay_secs(&self) -> i64 {
        self.base_delay_secs
    }

    /// Get the max delay in seconds.
    pub fn max_delay_secs(&self) -> i64 {
        self.max_delay_secs
    }

    /// Get the jitter factor.
    pub fn jitter_factor(&self) -> f64 {
        self.jitter_factor
    }

    /// Calculate all delays up to max_retries.
    ///
    /// Useful for testing and logging the full backoff schedule.
    ///
    /// # Returns
    /// A vector of delays for each retry attempt
    pub fn delay_schedule(&self) -> Vec<Duration> {
        (0..self.max_retries)
            .map(|i| self.calculate_delay(i))
            .collect()
    }

    /// Calculate the total delay if all retries are exhausted.
    ///
    /// This is the sum of all delays in the backoff schedule.
    pub fn total_delay_if_exhausted(&self) -> Duration {
        self.delay_schedule().iter().sum()
    }
}

/// Statistics for backoff operations.
#[derive(Debug, Clone, Default)]
pub struct BackoffStats {
    pub total_retries: u64,
    pub max_retries_hit: u64,
    pub avg_delay_ms: f64,
}

impl BackoffStats {
    /// Create new empty stats.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a retry.
    pub fn record_retry(&mut self, delay_ms: u64) {
        self.total_retries += 1;
        // Simple running average
        let n = self.total_retries as f64;
        self.avg_delay_ms = (self.avg_delay_ms * (n - 1.0) + delay_ms as f64) / n;
    }

    /// Record max retries hit.
    pub fn record_max_retries_hit(&mut self) {
        self.max_retries_hit += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[tokio::test]
    fn test_backoff_standard_defaults() {
        let config = BackoffConfig::standard();

        assert_eq!(config.base_delay_secs, 5);
        assert_eq!(config.max_delay_secs, 1800);
        assert_eq!(config.jitter_factor, 0.1);
        assert_eq!(config.max_retries, 5);
    }

    #[tokio::test]
    fn test_backoff_aggressive() {
        let config = BackoffConfig::aggressive();

        assert_eq!(config.base_delay_secs, 1);
        assert_eq!(config.max_delay_secs, 300);
        assert_eq!(config.jitter_factor, 0.2);
        assert_eq!(config.max_retries, 3);
    }

    #[tokio::test]
    fn test_backoff_conservative() {
        let config = BackoffConfig::conservative();

        assert_eq!(config.base_delay_secs, 10);
        assert_eq!(config.max_delay_secs, 3600);
        assert_eq!(config.jitter_factor, 0.15);
        assert_eq!(config.max_retries, 10);
    }

    #[tokio::test]
    fn test_calculate_delay_exponential() {
        let config = BackoffConfig::standard();

        // Retry 0: ~5 segundos (±10%)
        let delay0 = config.calculate_delay(0);
        let secs0 = delay0.num_seconds();
        assert!(secs0 >= 4 && secs0 <= 6, "Retry 0: expected ~5s, got {}s", secs0);

        // Retry 1: ~10 segundos
        let delay1 = config.calculate_delay(1);
        let secs1 = delay1.num_seconds();
        assert!(secs1 >= 8 && secs1 <= 12, "Retry 1: expected ~10s, got {}s", secs1);

        // Retry 2: ~20 segundos
        let delay2 = config.calculate_delay(2);
        let secs2 = delay2.num_seconds();
        assert!(secs2 >= 16 && secs2 <= 24, "Retry 2: expected ~20s, got {}s", secs2);

        // Retry 3: ~40 segundos
        let delay3 = config.calculate_delay(3);
        let secs3 = delay3.num_seconds();
        assert!(secs3 >= 32 && secs3 <= 48, "Retry 3: expected ~40s, got {}s", secs3);
    }

    #[tokio::test]
    fn test_calculate_delay_max_cap() {
        let config = BackoffConfig::standard();

        // Large retry count should be capped at max_delay
        let delay = config.calculate_delay(20);
        let secs = delay.num_seconds();
        assert_eq!(secs, 1800); // Should be capped at 30 minutes
    }

    #[tokio::test]
    fn test_calculate_delay_jitter_variation() {
        let config = BackoffConfig {
            base_delay_secs: 100,
            jitter_factor: 0.2, // ±20%
            ..Default::default()
        };

        // Collect multiple delays and check for variation
        let delays: Vec<i64> = (0..20)
            .map(|i| config.calculate_delay(i).num_seconds())
            .collect();

        // Verify jitter produces different values
        let unique_delays: HashSet<_> = delays.iter().collect();
        // With jitter, we expect at least some variation
        assert!(
            unique_delays.len() > 1,
            "Jitter should produce different delays, got {} unique values",
            unique_delays.len()
        );
    }

    #[tokio::test]
    fn test_can_retry() {
        let config = BackoffConfig::standard();

        assert!(config.can_retry(0));
        assert!(config.can_retry(1));
        assert!(config.can_retry(4));
        assert!(!config.can_retry(5));
        assert!(!config.can_retry(10));
    }

    #[tokio::test]
    fn test_next_retry_at() {
        let config = BackoffConfig::standard();
        let now = Utc::now();

        let next = config.next_retry_at(0);

        // Should be in the future
        assert!(next > now);

        // Should be roughly 5 seconds in the future
        let diff = next.signed_duration_since(now);
        assert!(diff.num_seconds() >= 4 && diff.num_seconds() <= 6);
    }

    #[tokio::test]
    fn test_delay_schedule() {
        let config = BackoffConfig {
            base_delay_secs: 5,
            max_retries: 3,
            jitter_factor: 0.0, // No jitter for predictable test
            ..Default::default()
        };

        let schedule = config.delay_schedule();

        assert_eq!(schedule.len(), 3);
        assert_eq!(schedule[0].num_seconds(), 5);
        assert_eq!(schedule[1].num_seconds(), 10);
        assert_eq!(schedule[2].num_seconds(), 20);
    }

    #[tokio::test]
    fn test_total_delay_if_exhausted() {
        let config = BackoffConfig {
            base_delay_secs: 5,
            max_retries: 3,
            jitter_factor: 0.0, // No jitter for predictable test
            ..Default::default()
        };

        let total = config.total_delay_if_exhausted();
        // 5 + 10 + 20 = 35 seconds
        assert_eq!(total.num_seconds(), 35);
    }

    #[tokio::test]
    fn test_display_format() {
        let config = BackoffConfig::standard();
        let display = format!("{}", config);

        assert!(display.contains("5s"));
        assert!(display.contains("1800s"));
        assert!(display.contains("10%"));
        assert!(display.contains("5"));
    }

    #[tokio::test]
    fn test_backoff_stats() {
        let mut stats = BackoffStats::new();

        stats.record_retry(5000);
        assert_eq!(stats.total_retries, 1);
        assert_eq!(stats.avg_delay_ms as u64, 5000);

        stats.record_retry(10000);
        assert_eq!(stats.total_retries, 2);
        // Average of 5000 and 10000
        assert_eq!(stats.avg_delay_ms as u64, 7500);

        stats.record_max_retries_hit();
        assert_eq!(stats.max_retries_hit, 1);
    }

    #[tokio::test]
    fn test_custom_config() {
        let config = BackoffConfig::new(3, 600, 0.15, 7);

        assert_eq!(config.base_delay_secs, 3);
        assert_eq!(config.max_delay_secs, 600);
        assert_eq!(config.jitter_factor, 0.15);
        assert_eq!(config.max_retries, 7);
    }

    #[tokio::test]
    fn test_serde_serialization() {
        let config = BackoffConfig::standard();

        // Serialize
        let json = serde_json::to_string(&config).expect("Failed to serialize");

        // Deserialize
        let deserialized: BackoffConfig = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(config.base_delay_secs, deserialized.base_delay_secs);
        assert_eq!(config.max_delay_secs, deserialized.max_delay_secs);
        assert_eq!(config.jitter_factor, deserialized.jitter_factor);
        assert_eq!(config.max_retries, deserialized.max_retries);
    }
}

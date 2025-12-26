//! Backpressure Management
//!
//! Defines backpressure strategies and events for the global log buffer.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Backpressure strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackpressureStrategy {
    /// Timeout for eviction operations
    pub timeout_ms: Option<u64>,
    /// Maximum number of eviction iterations
    pub max_evictions: Option<usize>,
    /// Whether to block or drop logs when backpressure occurs
    pub policy: BackpressurePolicy,
}

impl BackpressureStrategy {
    pub fn new(
        timeout: Option<Duration>,
        max_evictions: Option<usize>,
        policy: BackpressurePolicy,
    ) -> Self {
        Self {
            timeout_ms: timeout.map(|t| t.as_millis() as u64),
            max_evictions,
            policy,
        }
    }

    /// Create a strategy with a specific timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            timeout_ms: Some(timeout.as_millis() as u64),
            max_evictions: Some(100),
            policy: BackpressurePolicy::Block,
        }
    }

    pub fn timeout(&self) -> Option<Duration> {
        self.timeout_ms.map(Duration::from_millis)
    }

    pub fn max_evictions(&self) -> Option<usize> {
        self.max_evictions
    }

    pub fn policy(&self) -> &BackpressurePolicy {
        &self.policy
    }
}

impl Default for BackpressureStrategy {
    fn default() -> Self {
        Self {
            timeout_ms: Some(5000), // 5 seconds
            max_evictions: Some(100),
            policy: BackpressurePolicy::Block,
        }
    }
}

/// Backpressure policy
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BackpressurePolicy {
    /// Block until space is available (default)
    Block,
    /// Drop oldest logs to make space
    DropOldest,
    /// Drop newest logs if space is needed
    DropNewest,
    /// Return error to caller
    Error,
}

impl std::fmt::Display for BackpressurePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackpressurePolicy::Block => write!(f, "Block"),
            BackpressurePolicy::DropOldest => write!(f, "DropOldest"),
            BackpressurePolicy::DropNewest => write!(f, "DropNewest"),
            BackpressurePolicy::Error => write!(f, "Error"),
        }
    }
}

/// Backpressure event types
#[derive(Debug, Clone)]
pub enum BackpressureEvent {
    /// Buffer is approaching capacity
    ThresholdReached {
        current_bytes: u64,
        max_bytes: u64,
        utilization: f64,
    },
    /// Eviction started
    EvictionStarted { target_bytes: u64, reason: String },
    /// Eviction completed
    EvictionCompleted {
        evicted_bytes: u64,
        evicted_jobs: usize,
    },
    /// Backpressure operation timed out
    Timeout { attempted_bytes: u64 },
    /// Backpressure strategy applied
    StrategyApplied { strategy: String, result: String },
}

impl BackpressureEvent {
    pub fn threshold_reached(current_bytes: u64, max_bytes: u64) -> Self {
        let utilization = (current_bytes as f64 / max_bytes as f64) * 100.0;
        Self::ThresholdReached {
            current_bytes,
            max_bytes,
            utilization,
        }
    }

    pub fn eviction_started(target_bytes: u64, reason: String) -> Self {
        Self::EvictionStarted {
            target_bytes,
            reason,
        }
    }

    pub fn eviction_completed(evicted_bytes: u64, evicted_jobs: usize) -> Self {
        Self::EvictionCompleted {
            evicted_bytes,
            evicted_jobs,
        }
    }

    pub fn timeout(attempted_bytes: u64) -> Self {
        Self::Timeout { attempted_bytes }
    }

    pub fn strategy_applied(strategy: &BackpressureStrategy, result: String) -> Self {
        Self::StrategyApplied {
            strategy: format!("{:?}", strategy),
            result,
        }
    }
}

impl std::fmt::Display for BackpressureEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackpressureEvent::ThresholdReached {
                current_bytes,
                max_bytes,
                utilization,
            } => {
                write!(
                    f,
                    "Threshold reached: {}/{} bytes ({:.1}% utilized)",
                    current_bytes, max_bytes, utilization
                )
            }
            BackpressureEvent::EvictionStarted {
                target_bytes,
                reason,
            } => {
                write!(
                    f,
                    "Eviction started: target={} bytes, reason={}",
                    target_bytes, reason
                )
            }
            BackpressureEvent::EvictionCompleted {
                evicted_bytes,
                evicted_jobs,
            } => {
                write!(
                    f,
                    "Eviction completed: {} bytes from {} jobs",
                    evicted_bytes, evicted_jobs
                )
            }
            BackpressureEvent::Timeout { attempted_bytes } => {
                write!(
                    f,
                    "Backpressure timeout: attempted={} bytes",
                    attempted_bytes
                )
            }
            BackpressureEvent::StrategyApplied { strategy, result } => {
                write!(f, "Strategy applied: {} -> {}", strategy, result)
            }
        }
    }
}

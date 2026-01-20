//!
//! # Saga Ports for Legacy Infrastructure
//!
//! This module provides adapter ports that allow the saga-engine v4.0 to
//! integrate with legacy infrastructure components like rate limiters,
//! circuit breakers, and stuck detection systems.
//!

use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Rate limiter port for saga-engine integration
#[async_trait]
pub trait SagaRateLimiter: Send + Sync {
    /// Attempts to acquire a rate limit permit
    async fn try_acquire(&self, key: &str) -> Result<RateLimitPermit, RateLimitError>;
}

/// Result of a rate limit check
#[derive(Debug, Clone)]
pub struct RateLimitPermit {
    pub allowed: bool,
    pub remaining: u64,
    pub reset_at: chrono::DateTime<chrono::Utc>,
    pub retry_after: Option<std::time::Duration>,
}

impl RateLimitPermit {
    pub fn allowed(remaining: u64, reset_at: chrono::DateTime<chrono::Utc>) -> Self {
        Self {
            allowed: true,
            remaining,
            reset_at,
            retry_after: None,
        }
    }

    pub fn denied(
        retry_after: std::time::Duration,
        reset_at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            allowed: false,
            remaining: 0,
            reset_at,
            retry_after: Some(retry_after),
        }
    }
}

#[derive(Debug, Error, Clone)]
#[error("Rate limit exceeded for key {key}: {message}")]
pub struct RateLimitError {
    pub key: String,
    pub message: String,
    pub retry_after: std::time::Duration,
}

/// Configuration for rate limiting
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub max_requests: u64,
    pub window: std::time::Duration,
    pub key_prefix: String,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window: std::time::Duration::from_secs(60),
            key_prefix: "saga".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
struct RateLimitWindow {
    count: u64,
    window_start: chrono::DateTime<chrono::Utc>,
}

/// In-memory rate limiter implementation
#[derive(Debug)]
pub struct InMemoryRateLimiter {
    windows: Arc<RwLock<HashMap<String, RateLimitWindow>>>,
    config: RateLimitConfig,
}

impl InMemoryRateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            windows: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Cleanup expired windows (call periodically)
    pub async fn cleanup(&self) {
        let mut windows = self.windows.write().await;
        let window_dur = chrono::Duration::from_std(self.config.window).unwrap();
        let now = chrono::Utc::now();

        windows.retain(|_, window| now - window.window_start < window_dur);
    }
}

#[async_trait]
impl SagaRateLimiter for InMemoryRateLimiter {
    async fn try_acquire(&self, key: &str) -> Result<RateLimitPermit, RateLimitError> {
        let full_key = format!("{}:{}", self.config.key_prefix, key);
        let now = chrono::Utc::now();
        let window_dur = chrono::Duration::from_std(self.config.window).unwrap();
        let window_end = now + window_dur;

        let mut windows = self.windows.write().await;

        let window = windows
            .get_mut(&full_key)
            .map(|w| {
                if now - w.window_start > window_dur {
                    // Window expired, reset
                    w.count = 1;
                    w.window_start = now;
                } else {
                    w.count += 1;
                }
                w.clone()
            })
            .unwrap_or_else(|| RateLimitWindow {
                count: 1,
                window_start: now,
            });

        // Store updated window
        windows.insert(full_key.clone(), window.clone());

        let remaining = self.config.max_requests.saturating_sub(window.count);

        if window.count > self.config.max_requests {
            let retry_after = self.config.window;
            Err(RateLimitError {
                key: full_key,
                message: format!(
                    "Rate limit exceeded: {} requests per {:?}",
                    self.config.max_requests, self.config.window
                ),
                retry_after,
            })
        } else {
            Ok(RateLimitPermit::allowed(remaining, window_end))
        }
    }
}

#[cfg(test)]
mod rate_limiter_tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_allows_requests() {
        let limiter = InMemoryRateLimiter::new(RateLimitConfig {
            max_requests: 10,
            window: std::time::Duration::from_secs(60),
            key_prefix: "test".to_string(),
        });

        for i in 0..10 {
            let result = limiter.try_acquire("key1").await;
            assert!(result.is_ok());
            let permit = result.unwrap();
            assert!(permit.allowed);
            assert_eq!(permit.remaining, 10 - i - 1);
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks_excess() {
        let limiter = InMemoryRateLimiter::new(RateLimitConfig {
            max_requests: 3,
            window: std::time::Duration::from_secs(60),
            key_prefix: "test".to_string(),
        });

        // First 3 should succeed
        for _ in 0..3 {
            assert!(limiter.try_acquire("key2").await.is_ok());
        }

        // 4th should fail
        let result = limiter.try_acquire("key2").await;
        assert!(result.is_err());
        assert!(!result.unwrap_err().to_string().is_empty());
    }
}

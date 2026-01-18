//! Retry Policy with Exponential Backoff - EPIC-46
//!
//! Implements retry logic for saga operations with exponential backoff
//! to handle transient failures gracefully (EPIC-46 Section 13.3).

use std::time::Duration;

/// Retry policy configuration for saga operations (EPIC-46 Section 13.3)
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial delay before first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for exponential backoff
    pub multiplier: f64,
    /// Optional jitter factor (0.0 to 1.0) to add randomness
    pub jitter_factor: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

impl RetryPolicy {
    /// Creates a new RetryPolicy with the specified configuration.
    ///
    /// # Arguments
    /// * `max_attempts` - Maximum number of retry attempts (including first attempt)
    /// * `initial_delay` - Initial delay before first retry
    /// * `max_delay` - Maximum delay between retries
    /// * `multiplier` - Multiplier for exponential backoff
    #[inline]
    pub fn new(
        max_attempts: u32,
        initial_delay: Duration,
        max_delay: Duration,
        multiplier: f64,
    ) -> Self {
        Self {
            max_attempts,
            initial_delay,
            max_delay,
            multiplier,
            jitter_factor: 0.1, // Default 10% jitter
        }
    }

    /// Creates a RetryPolicy optimized for saga step retries.
    ///
    /// Default: 3 attempts, 100ms initial, 30s max, 2x multiplier
    #[inline]
    pub fn for_saga_step() -> Self {
        Self::default()
    }

    /// Creates a RetryPolicy optimized for saga compensation retries.
    ///
    /// More conservative: fewer attempts, longer delays
    #[inline]
    pub fn for_compensation() -> Self {
        Self {
            max_attempts: 2,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            jitter_factor: 0.2,
        }
    }

    /// Creates a RetryPolicy optimized for infrastructure operations.
    ///
    /// Conservative: fewer attempts due to potential side effects
    #[inline]
    pub fn for_infrastructure() -> Self {
        Self {
            max_attempts: 2,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            jitter_factor: 0.15,
        }
    }

    /// Returns the delay for a specific attempt number.
    ///
    /// Uses exponential backoff: initial_delay * (multiplier ^ (attempt - 1))
    ///
    /// # Arguments
    /// * `attempt` - The attempt number (1-indexed, 1 = first attempt)
    #[inline]
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt <= 1 {
            return self.initial_delay;
        }

        // Calculate exponential delay
        let delay = self.initial_delay.as_secs_f64() * self.multiplier.powf(attempt as f64 - 1.0);

        // Apply max delay cap
        let delay = delay.min(self.max_delay.as_secs_f64());

        // Apply jitter for distributed systems (using pseudo-random based on attempt)
        let pseudo_jitter = 1.0 + ((attempt % 10) as f64 / 10.0 - 0.5) * 2.0 * self.jitter_factor;
        let delay = delay * pseudo_jitter;

        Duration::from_secs_f64(delay.max(self.initial_delay.as_secs_f64()))
    }

    /// Returns true if another attempt should be made.
    ///
    /// # Arguments
    /// * `attempt` - The current attempt number (1-indexed)
    #[inline]
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_attempts
    }

    /// Returns the total number of attempts allowed.
    #[inline]
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// Returns the maximum total time for all retries.
    #[inline]
    pub fn max_total_time(&self) -> Duration {
        let mut total = Duration::ZERO;
        for attempt in 1..=self.max_attempts {
            if attempt > 1 {
                total += self.delay_for_attempt(attempt);
            }
        }
        total
    }
}

/// Retry outcome after attempting an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryOutcome {
    /// Operation succeeded on first attempt
    Success,
    /// Operation succeeded after retries
    SuccessAfterRetries(u32),
    /// Operation failed after all retries
    Failed,
    /// Operation timed out before completing
    TimedOut,
}

/// Result of a retryable operation.
pub type RetryResult<T> = std::result::Result<T, T>;

/// Trait for operations that can be retried.
#[async_trait::async_trait]
pub trait RetryableOperation {
    /// The type of result produced by the operation.
    type Output;

    /// Execute the operation once.
    async fn execute(&self) -> Result<Self::Output, Self::Output>;
}

/// Executes an operation with retry policy.
///
/// # Arguments
/// * `operation` - The operation to execute
/// * `policy` - The retry policy to use
/// * `timeout` - Optional overall timeout for all retries
///
/// # Returns
/// Ok(RetryOutcome::Success) on first success
/// Ok(RetryOutcome::SuccessAfterRetries(n)) on success after n retries
/// Err(RetryOutcome::Failed) if all retries exhausted
/// Err(RetryOutcome::TimedOut) if overall timeout exceeded
pub async fn execute_with_retry<Op: RetryableOperation>(
    operation: &Op,
    policy: RetryPolicy,
    timeout: Option<Duration>,
) -> RetryResult<RetryOutcome> {
    let start_time = std::time::Instant::now();
    let mut attempt = 1u32;

    loop {
        // Check overall timeout
        if let Some(timeout_duration) = timeout
            && start_time.elapsed() >= timeout_duration
        {
            return Err(RetryOutcome::TimedOut);
        }

        match operation.execute().await {
            Ok(_result) => {
                return if attempt == 1 {
                    Ok(RetryOutcome::Success)
                } else {
                    Ok(RetryOutcome::SuccessAfterRetries(attempt - 1))
                };
            }
            Err(_error) => {
                if !policy.should_retry(attempt) {
                    return Err(RetryOutcome::Failed);
                }

                // Calculate delay for next attempt
                let delay = policy.delay_for_attempt(attempt + 1);

                // Check if we have time for the delay
                if let Some(timeout_duration) = timeout {
                    let elapsed = start_time.elapsed();
                    if elapsed + delay >= timeout_duration {
                        return Err(RetryOutcome::TimedOut);
                    }
                }

                // Wait before retrying
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

/// Builder for RetryPolicy with fluent API.
#[derive(Debug, Clone)]
pub struct RetryPolicyBuilder {
    max_attempts: u32,
    initial_delay: Duration,
    max_delay: Duration,
    multiplier: f64,
    jitter_factor: f64,
}

impl Default for RetryPolicyBuilder {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

impl RetryPolicyBuilder {
    /// Creates a new builder with default values.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum number of attempts.
    #[inline]
    pub fn with_max_attempts(mut self, attempts: u32) -> Self {
        self.max_attempts = attempts;
        self
    }

    /// Sets the initial delay.
    #[inline]
    pub fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Sets the maximum delay.
    #[inline]
    pub fn with_max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Sets the backoff multiplier.
    #[inline]
    pub fn with_multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier;
        self
    }

    /// Sets the jitter factor (0.0 to 1.0).
    #[inline]
    pub fn with_jitter_factor(mut self, factor: f64) -> Self {
        self.jitter_factor = factor.clamp(0.0, 1.0);
        self
    }

    /// Builds the RetryPolicy.
    #[inline]
    pub fn build(self) -> RetryPolicy {
        RetryPolicy {
            max_attempts: self.max_attempts,
            initial_delay: self.initial_delay,
            max_delay: self.max_delay,
            multiplier: self.multiplier,
            jitter_factor: self.jitter_factor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_retry_policy_default_values() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.initial_delay, Duration::from_millis(100));
        assert_eq!(policy.max_delay, Duration::from_secs(30));
        assert_eq!(policy.multiplier, 2.0);
    }

    #[tokio::test]
    async fn test_delay_for_attempt() {
        let policy = RetryPolicy::new(5, Duration::from_millis(100), Duration::from_secs(30), 2.0);

        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(100));
        // Attempt 2: 100ms * 2^1 = 200ms (with jitter, between 180-220ms)
        let delay_2 = policy.delay_for_attempt(2);
        assert!(delay_2 >= Duration::from_millis(180));
        assert!(delay_2 <= Duration::from_millis(220));

        // Attempt 3: 100ms * 2^2 = 400ms (with jitter)
        let delay_3 = policy.delay_for_attempt(3);
        assert!(delay_3 >= Duration::from_millis(360));
        assert!(delay_3 <= Duration::from_millis(440));
    }

    #[tokio::test]
    async fn test_should_retry() {
        let policy = RetryPolicy::new(3, Duration::from_millis(100), Duration::from_secs(30), 2.0);

        assert!(policy.should_retry(1)); // 1 < 3, can retry
        assert!(policy.should_retry(2)); // 2 < 3, can retry
        assert!(!policy.should_retry(3)); // 3 >= 3, no more retries
    }

    #[tokio::test]
    async fn test_max_total_time() {
        let policy = RetryPolicy::new(4, Duration::from_millis(100), Duration::from_secs(30), 2.0);

        let total = policy.max_total_time();
        // Should include delays for attempts 2, 3, 4
        // Approximate: 200ms + 400ms + 800ms = 1400ms (with jitter range)
        assert!(total >= Duration::from_millis(1000));
        assert!(total <= Duration::from_millis(2000));
    }

    #[tokio::test]
    async fn test_builder() {
        let policy = RetryPolicyBuilder::new()
            .with_max_attempts(5)
            .with_initial_delay(Duration::from_millis(50))
            .with_max_delay(Duration::from_secs(10))
            .with_multiplier(3.0)
            .with_jitter_factor(0.2)
            .build();

        assert_eq!(policy.max_attempts, 5);
        assert_eq!(policy.initial_delay, Duration::from_millis(50));
        assert_eq!(policy.max_delay, Duration::from_secs(10));
        assert_eq!(policy.multiplier, 3.0);
        assert_eq!(policy.jitter_factor, 0.2);
    }

    #[tokio::test]
    async fn test_execute_with_retry_success_first() {
        struct SuccessOperation;
        #[async_trait::async_trait]
        impl RetryableOperation for SuccessOperation {
            type Output = String;
            async fn execute(&self) -> Result<Self::Output, Self::Output> {
                Ok("success".to_string())
            }
        }

        let policy = RetryPolicy::default();
        let result = execute_with_retry(&SuccessOperation, policy, None).await;

        match result {
            Ok(RetryOutcome::Success) => {}
            _ => panic!("Expected success on first attempt"),
        }
    }

    #[tokio::test]
    async fn test_execute_with_retry_failure() {
        struct FailOperation;
        #[async_trait::async_trait]
        impl RetryableOperation for FailOperation {
            type Output = &'static str;
            async fn execute(&self) -> Result<Self::Output, Self::Output> {
                Err("always fails")
            }
        }

        let policy = RetryPolicy::new(2, Duration::from_millis(10), Duration::from_secs(1), 2.0);
        let result = execute_with_retry(&FailOperation, policy, None).await;

        match result {
            Err(RetryOutcome::Failed) => {}
            _ => panic!("Expected failure after retries"),
        }
    }
}

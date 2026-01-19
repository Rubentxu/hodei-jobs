//! TimerStore port for durable timers with PostgreSQL.
//!
//! This module defines the [`TimerStore`] trait for managing durable
//! timers that survive process restarts.

use super::super::event::SagaId;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::time::Duration;

/// Errors from timer store operations.
#[derive(Debug, thiserror::Error)]
pub enum TimerStoreError<E> {
    #[error("Timer creation failed: {0}")]
    Create(E),

    #[error("Timer cancellation failed: {0}")]
    Cancel(E),

    #[error("Timer retrieval failed: {0}")]
    Retrieve(E),

    #[error("Timer update failed: {0}")]
    Update(E),

    #[error("Timer not found: {0}")]
    NotFound(String),
}

impl<E> TimerStoreError<E> {
    /// Create a not found error.
    pub fn not_found(timer_id: impl Into<String>) -> Self {
        Self::NotFound(timer_id.into())
    }
}

/// Status of a timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerStatus {
    /// Timer is pending and waiting to fire.
    Pending,

    /// Timer has been claimed by a scheduler.
    Processing,

    /// Timer has fired successfully.
    Fired,

    /// Timer was cancelled before firing.
    Cancelled,

    /// Timer failed to fire after max retries.
    Failed,
}

/// Type of timer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerType {
    /// Workflow-level timeout.
    WorkflowTimeout,

    /// Activity-level timeout.
    ActivityTimeout,

    /// User-defined delay.
    Sleep,

    /// Cron-style scheduled execution.
    Scheduled,

    /// Retry backoff timer.
    RetryBackoff,

    /// Custom timer type.
    Custom(String),
}

impl TimerType {
    /// Get the string representation.
    pub fn as_str(&self) -> &str {
        match self {
            TimerType::WorkflowTimeout => "workflow_timeout",
            TimerType::ActivityTimeout => "activity_timeout",
            TimerType::Sleep => "sleep",
            TimerType::Scheduled => "scheduled",
            TimerType::RetryBackoff => "retry_backoff",
            TimerType::Custom(s) => s.as_str(),
        }
    }
}

/// A durable timer that persists across process restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DurableTimer {
    /// Unique timer identifier.
    pub timer_id: String,

    /// The saga this timer belongs to.
    pub saga_id: SagaId,

    /// The workflow run this timer belongs to.
    pub run_id: String,

    /// Type of timer.
    pub timer_type: TimerType,

    /// When the timer should fire.
    pub fire_at: chrono::DateTime<chrono::Utc>,

    /// When the timer was created.
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Timer attributes (correlation data).
    #[serde(default)]
    pub attributes: Vec<u8>,

    /// Current timer status.
    pub status: TimerStatus,

    /// Number of firing attempts.
    pub attempt: u32,

    /// Maximum firing attempts.
    pub max_attempts: u32,
}

impl DurableTimer {
    /// Create a new timer.
    pub fn new(
        saga_id: SagaId,
        run_id: String,
        timer_type: TimerType,
        fire_at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            timer_id: uuid::Uuid::new_v4().to_string(),
            saga_id,
            run_id,
            timer_type,
            fire_at,
            created_at: chrono::Utc::now(),
            attributes: vec![],
            status: TimerStatus::Pending,
            attempt: 0,
            max_attempts: 1,
        }
    }

    /// Set timer attributes.
    pub fn with_attributes(mut self, attrs: Vec<u8>) -> Self {
        self.attributes = attrs;
        self
    }

    /// Set max attempts.
    pub fn with_max_attempts(mut self, max: u32) -> Self {
        self.max_attempts = max;
        self
    }

    /// Check if the timer can be fired.
    pub fn can_fire(&self) -> bool {
        self.status == TimerStatus::Pending
            && self.attempt < self.max_attempts
            && self.fire_at <= chrono::Utc::now()
    }

    /// Check if the timer should be retried.
    pub fn should_retry(&self) -> bool {
        self.status == TimerStatus::Processing && self.attempt < self.max_attempts
    }

    /// Mark timer as processing.
    pub fn mark_processing(&mut self) {
        self.status = TimerStatus::Processing;
        self.attempt += 1;
    }

    /// Mark timer as fired.
    pub fn mark_fired(&mut self) {
        self.status = TimerStatus::Fired;
    }

    /// Mark timer as cancelled.
    pub fn mark_cancelled(&mut self) {
        // Cannot cancel a timer that has already fired
        if self.status == TimerStatus::Fired {
            return;
        }
        self.status = TimerStatus::Cancelled;
    }

    /// Calculate backoff for next retry.
    pub fn next_fire_at(&self, base_delay: Duration) -> chrono::DateTime<chrono::Utc> {
        let base_ms = base_delay.as_millis();
        let multiplier = 2_u128.pow(self.attempt.saturating_sub(1) as u32);
        let delay_ms = base_ms * multiplier;
        self.fire_at + Duration::from_millis(delay_ms.min(u64::MAX as u128) as u64)
    }
}

/// Result of claiming expired timers.
#[derive(Debug)]
pub struct TimerClaimResult {
    /// The claimed timers.
    pub timers: Vec<DurableTimer>,
    /// The scheduler ID that claimed them.
    pub scheduler_id: String,
}

/// Trait for durable timer storage.
///
/// The TimerStore provides:
/// - Persistent timers that survive restarts
/// - Efficient polling for expired timers
/// - Timer claiming to prevent duplicate firing
/// - Transactional timer + event creation
///
/// # Timer Flow
///
/// ```ignore
/// // 1. Create timer
/// timer_store.create_timer(&timer).await?;
///
/// // 2. Scheduler polls for expired timers
/// let expired = timer_store.get_expired_timers(100).await?;
///
/// // 3. Claim timers (prevents duplicate firing)
/// for timer in &expired {
///     timer_store.claim_timer(&timer.timer_id, &scheduler_id).await?;
/// }
///
/// // 4. Fire timers (create TimerFired event, notify SignalDispatcher)
/// for timer in expired {
///     // Create TimerFired event
///     event_store.append_event(...).await?;
///     signal_dispatcher.notify_timer_fired(...).await?;
/// }
/// ```
#[async_trait::async_trait]
pub trait TimerStore: Send + Sync {
    /// The error type for this implementation.
    type Error: Debug + Send + Sync + 'static;

    /// Create a new timer.
    ///
    /// # Arguments
    ///
    /// * `timer` - The timer to create.
    async fn create_timer(&self, timer: &DurableTimer) -> Result<(), TimerStoreError<Self::Error>>;

    /// Cancel a timer.
    ///
    /// # Arguments
    ///
    /// * `timer_id` - The timer to cancel.
    async fn cancel_timer(&self, timer_id: &str) -> Result<(), TimerStoreError<Self::Error>>;

    /// Get expired timers that are ready to fire.
    ///
    /// This is the main method called by the timer scheduler.
    /// Returns timers that are:
    /// - In PENDING status
    /// - Past their fire_at time
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of timers to return.
    /// * `scheduler_id` - The scheduler calling this method.
    ///
    /// # Note
    ///
    /// This method does NOT claim the timers. Use `claim_timers`
    /// after calling this to prevent duplicate firing.
    async fn get_expired_timers(
        &self,
        limit: u64,
    ) -> Result<Vec<DurableTimer>, TimerStoreError<Self::Error>>;

    /// Claim expired timers for processing.
    ///
    /// Marks timers as PROCESSING to prevent other schedulers
    /// from firing the same timer.
    ///
    /// # Arguments
    ///
    /// * `timer_ids` - The timers to claim.
    /// * `scheduler_id` - The scheduler claiming them.
    async fn claim_timers(
        &self,
        timer_ids: &[String],
        scheduler_id: &str,
    ) -> Result<Vec<DurableTimer>, TimerStoreError<Self::Error>>;

    /// Update timer status.
    ///
    /// # Arguments
    ///
    /// * `timer_id` - The timer to update.
    /// * `status` - The new status.
    async fn update_timer_status(
        &self,
        timer_id: &str,
        status: TimerStatus,
    ) -> Result<(), TimerStoreError<Self::Error>>;

    /// Get timers for a specific saga.
    ///
    /// # Arguments
    ///
    /// * `saga_id` - The saga to get timers for.
    /// * `include_fired` - Whether to include fired/cancelled timers.
    async fn get_timers_for_saga(
        &self,
        saga_id: &SagaId,
        include_fired: bool,
    ) -> Result<Vec<DurableTimer>, TimerStoreError<Self::Error>>;

    /// Get a specific timer by ID.
    ///
    /// # Arguments
    ///
    /// * `timer_id` - The timer to retrieve.
    async fn get_timer(
        &self,
        timer_id: &str,
    ) -> Result<Option<DurableTimer>, TimerStoreError<Self::Error>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timer_creation() {
        // Timer en el futuro no puede dispararse todavía
        let future_timer = DurableTimer::new(
            SagaId("saga-1".to_string()),
            "run-1".to_string(),
            TimerType::Sleep,
            chrono::Utc::now() + Duration::from_secs(60),
        );
        assert_eq!(future_timer.saga_id.0, "saga-1");
        assert_eq!(future_timer.status, TimerStatus::Pending);
        assert!(!future_timer.can_fire()); // No puede dispararse todavía

        // Timer en el pasado SÍ puede dispararse
        let past_timer = DurableTimer::new(
            SagaId("saga-2".to_string()),
            "run-2".to_string(),
            TimerType::Sleep,
            chrono::Utc::now() - Duration::from_secs(1),
        );
        assert!(past_timer.can_fire()); // Puede dispararse porque ya pasó el tiempo
    }

    #[test]
    fn test_timer_backoff() {
        let mut timer = DurableTimer::new(
            SagaId("saga-1".to_string()),
            "run-1".to_string(),
            TimerType::RetryBackoff,
            chrono::Utc::now(),
        );
        timer.max_attempts = 3;

        timer.mark_processing();
        let next = timer.next_fire_at(Duration::from_secs(1));
        assert!(next > timer.fire_at);

        timer.mark_processing();
        let next2 = timer.next_fire_at(Duration::from_secs(1));
        assert!(next2 > next);
    }

    #[test]
    fn test_timer_status_transitions() {
        let mut timer = DurableTimer::new(
            SagaId("saga-1".to_string()),
            "run-1".to_string(),
            TimerType::Sleep,
            chrono::Utc::now(),
        );

        assert_eq!(timer.status, TimerStatus::Pending);

        timer.mark_processing();
        assert_eq!(timer.status, TimerStatus::Processing);
        assert_eq!(timer.attempt, 1);

        timer.mark_fired();
        assert_eq!(timer.status, TimerStatus::Fired);

        // Cannot cancel a fired timer
        timer.mark_cancelled();
        assert_eq!(timer.status, TimerStatus::Fired);
    }
}

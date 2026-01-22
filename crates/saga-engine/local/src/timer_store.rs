//! # In-Memory TimerStore Implementation
//!
//! This module provides [`InMemoryTimerStore`] for lightweight timer storage
//! suitable for local applications and testing.

use async_trait::async_trait;
use chrono::Utc;
use saga_engine_core::event::SagaId;
use saga_engine_core::port::timer_store::{
    DurableTimer, TimerStore, TimerStoreError, TimerStatus,
};
use std::collections::BTreeMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

/// Configuration for [`InMemoryTimerStore`].
#[derive(Debug, Clone)]
pub struct InMemoryTimerStoreConfig {
    /// Maximum number of timers to return per fetch.
    pub fetch_batch_size: usize,
}

impl Default for InMemoryTimerStoreConfig {
    fn default() -> Self {
        Self {
            fetch_batch_size: 100,
        }
    }
}

/// In-memory TimerStore implementation.
///
/// This implementation provides:
/// - Zero-config timer storage
/// - Efficient lookup by fire time
/// - Perfect for testing and local development
///
/// # Examples
///
/// ```rust
/// use saga_engine_local::timer_store::InMemoryTimerStore;
///
/// let timer_store = InMemoryTimerStore::new();
/// ```
#[derive(Debug, Clone, Default)]
pub struct InMemoryTimerStore {
    timers: Arc<Mutex<BTreeMap<String, DurableTimer>>>,
}

impl InMemoryTimerStore {
    /// Create a new builder.
    pub fn builder() -> InMemoryTimerStoreBuilder {
        InMemoryTimerStoreBuilder::new()
    }

    /// Create a new [`InMemoryTimerStore`] with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            timers: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Create a new [`InMemoryTimerStore`] with custom configuration.
    #[must_use]
    pub fn with_config(_config: InMemoryTimerStoreConfig) -> Self {
        Self {
            timers: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
}

#[async_trait]
impl TimerStore for InMemoryTimerStore {
    type Error = InMemoryTimerStoreError;

    async fn create_timer(&self, timer: &DurableTimer) -> Result<(), TimerStoreError<Self::Error>> {
        let mut timers = self.timers.lock().await;
        timers.insert(timer.timer_id.clone(), timer.clone());
        Ok(())
    }

    async fn cancel_timer(&self, timer_id: &str) -> Result<(), TimerStoreError<Self::Error>> {
        let mut timers = self.timers.lock().await;
        if let Some(timer) = timers.get_mut(timer_id) {
            if timer.status == TimerStatus::Pending {
                timer.status = TimerStatus::Cancelled;
            }
        }
        Ok(())
    }

    async fn get_expired_timers(
        &self,
        limit: u64,
    ) -> Result<Vec<DurableTimer>, TimerStoreError<Self::Error>> {
        let timers = self.timers.lock().await;

        let expired_timers: Vec<DurableTimer> = timers
            .values()
            .filter(|t| t.status == TimerStatus::Pending && t.fire_at <= Utc::now())
            .take(limit as usize)
            .cloned()
            .collect();

        Ok(expired_timers)
    }

    async fn claim_timers(
        &self,
        timer_ids: &[String],
        _scheduler_id: &str,
    ) -> Result<Vec<DurableTimer>, TimerStoreError<Self::Error>> {
        let mut timers = self.timers.lock().await;
        let mut claimed = Vec::new();

        for timer_id in timer_ids {
            if let Some(timer) = timers.get_mut(timer_id) {
                if timer.status == TimerStatus::Pending {
                    timer.status = TimerStatus::Processing;
                    timer.attempt += 1;
                    claimed.push(timer.clone());
                }
            }
        }

        Ok(claimed)
    }

    async fn update_timer_status(
        &self,
        timer_id: &str,
        status: TimerStatus,
    ) -> Result<(), TimerStoreError<Self::Error>> {
        let mut timers = self.timers.lock().await;
        if let Some(timer) = timers.get_mut(timer_id) {
            timer.status = status;
        }
        Ok(())
    }

    async fn get_timers_for_saga(
        &self,
        saga_id: &SagaId,
        include_fired: bool,
    ) -> Result<Vec<DurableTimer>, TimerStoreError<Self::Error>> {
        let timers = self.timers.lock().await;

        let saga_timers: Vec<DurableTimer> = timers
            .values()
            .filter(|t| t.saga_id == *saga_id)
            .filter(|t| include_fired || (t.status != TimerStatus::Fired && t.status != TimerStatus::Cancelled))
            .cloned()
            .collect();

        Ok(saga_timers)
    }

    async fn get_timer(
        &self,
        timer_id: &str,
    ) -> Result<Option<DurableTimer>, TimerStoreError<Self::Error>> {
        let timers = self.timers.lock().await;
        Ok(timers.get(timer_id).cloned())
    }
}

/// Builder for [`InMemoryTimerStore`].
#[derive(Debug, Default)]
pub struct InMemoryTimerStoreBuilder {
    config: InMemoryTimerStoreConfig,
}

impl InMemoryTimerStoreBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            config: InMemoryTimerStoreConfig::default(),
        }
    }

    /// Set fetch batch size.
    pub fn fetch_batch_size(mut self, size: usize) -> Self {
        self.config.fetch_batch_size = size;
        self
    }

    /// Build the [`InMemoryTimerStore`].
    #[must_use]
    pub fn build(self) -> InMemoryTimerStore {
        InMemoryTimerStore::with_config(self.config)
    }
}

/// Errors from [`InMemoryTimerStore`] operations.
#[derive(Debug, Error)]
pub enum InMemoryTimerStoreError {
    /// Timer not found.
    #[error("Timer not found: {0}")]
    NotFound(String),

    /// Timer creation failed.
    #[error("Timer creation failed: {0}")]
    CreationFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use saga_engine_core::{SagaId, TimerType};

    #[tokio::test]
    async fn test_create_timer() {
        let store = InMemoryTimerStore::new();

        let timer = DurableTimer::new(
            SagaId::new(),
            "run-1".to_string(),
            TimerType::WorkflowTimeout,
            Utc::now(),
        );

        store.create_timer(&timer).await.unwrap();

        let fetched = store.get_timer(&timer.timer_id).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().timer_id, timer.timer_id);
    }

    #[tokio::test]
    async fn test_get_expired_timers() {
        let store = InMemoryTimerStore::new();

        // Create a timer that's already due
        let due_timer = DurableTimer::new(
            SagaId::new(),
            "run-1".to_string(),
            TimerType::WorkflowTimeout,
            Utc::now() - chrono::Duration::seconds(1),
        );

        // Create a timer that's not due yet
        let future_timer = DurableTimer::new(
            SagaId::new(),
            "run-2".to_string(),
            TimerType::WorkflowTimeout,
            Utc::now() + chrono::Duration::hours(1),
        );

        store.create_timer(&due_timer).await.unwrap();
        store.create_timer(&future_timer).await.unwrap();

        let expired = store.get_expired_timers(100).await.unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].timer_id, due_timer.timer_id);
    }

    #[tokio::test]
    async fn test_cancel_timer() {
        let store = InMemoryTimerStore::new();

        let timer = DurableTimer::new(
            SagaId::new(),
            "run-1".to_string(),
            TimerType::WorkflowTimeout,
            Utc::now(),
        );

        store.create_timer(&timer).await.unwrap();
        store.cancel_timer(&timer.timer_id).await.unwrap();

        let fetched = store.get_timer(&timer.timer_id).await.unwrap().unwrap();
        assert_eq!(fetched.status, TimerStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_update_timer_status() {
        let store = InMemoryTimerStore::new();

        let timer = DurableTimer::new(
            SagaId::new(),
            "run-1".to_string(),
            TimerType::WorkflowTimeout,
            Utc::now(),
        );

        store.create_timer(&timer).await.unwrap();
        store
            .update_timer_status(&timer.timer_id, TimerStatus::Fired)
            .await
            .unwrap();

        let fetched = store.get_timer(&timer.timer_id).await.unwrap().unwrap();
        assert_eq!(fetched.status, TimerStatus::Fired);
    }

    #[tokio::test]
    async fn test_get_timers_for_saga() {
        let store = InMemoryTimerStore::new();
        let saga_id = SagaId::new();

        let timer1 = DurableTimer::new(
            saga_id.clone(),
            "run-1".to_string(),
            TimerType::WorkflowTimeout,
            Utc::now(),
        );

        let timer2 = DurableTimer::new(
            saga_id.clone(),
            "run-2".to_string(),
            TimerType::ActivityTimeout,
            Utc::now(),
        );

        store.create_timer(&timer1).await.unwrap();
        store.create_timer(&timer2).await.unwrap();

        let saga_timers = store.get_timers_for_saga(&saga_id, false).await.unwrap();
        assert_eq!(saga_timers.len(), 2);
    }

    #[tokio::test]
    async fn test_claim_timers() {
        let store = InMemoryTimerStore::new();

        let timer1 = DurableTimer::new(
            SagaId::new(),
            "run-1".to_string(),
            TimerType::WorkflowTimeout,
            Utc::now(),
        );

        let timer2 = DurableTimer::new(
            SagaId::new(),
            "run-2".to_string(),
            TimerType::WorkflowTimeout,
            Utc::now(),
        );

        store.create_timer(&timer1).await.unwrap();
        store.create_timer(&timer2).await.unwrap();

        let claimed = store
            .claim_timers(&[timer1.timer_id.clone()], "scheduler-1")
            .await
            .unwrap();

        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].status, TimerStatus::Processing);
    }
}

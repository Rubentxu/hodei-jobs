//! In-memory implementation of TimerStore for testing.

use parking_lot::RwLock;
use saga_engine_core::event::SagaId;
use saga_engine_core::port::timer_store::{
    DurableTimer, TimerStatus, TimerStore, TimerStoreError, TimerType,
};
use std::collections::HashMap;
use std::sync::Arc;

/// In-memory timer store implementation.
#[derive(Debug, Default, Clone)]
pub struct InMemoryTimerStore {
    inner: Arc<InnerStore>,
}

#[derive(Debug, Default)]
struct InnerStore {
    timers: RwLock<HashMap<String, DurableTimer>>,
}

impl InMemoryTimerStore {
    /// Create a new in-memory timer store.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(InnerStore::default()),
        }
    }

    /// Clear all data.
    pub fn clear(&self) {
        let mut timers = self.inner.timers.write();
        timers.clear();
    }

    /// Get the number of timers stored.
    pub fn timer_count(&self) -> usize {
        self.inner.timers.read().len()
    }
}

#[async_trait::async_trait]
impl TimerStore for InMemoryTimerStore {
    type Error = InMemoryTimerStoreError;

    async fn create_timer(&self, timer: &DurableTimer) -> Result<(), TimerStoreError<Self::Error>> {
        let mut timers = self.inner.timers.write();
        timers.insert(timer.timer_id.clone(), timer.clone());
        Ok(())
    }

    async fn cancel_timer(&self, timer_id: &str) -> Result<(), TimerStoreError<Self::Error>> {
        let mut timers = self.inner.timers.write();
        if let Some(timer) = timers.get_mut(timer_id) {
            timer.mark_cancelled();
            Ok(())
        } else {
            Err(TimerStoreError::not_found(timer_id.to_string()))
        }
    }

    async fn get_expired_timers(
        &self,
        limit: u64,
    ) -> Result<Vec<DurableTimer>, TimerStoreError<Self::Error>> {
        let now = chrono::Utc::now();
        let mut timers = self.inner.timers.write();
        let mut expired = Vec::new();

        for timer in timers.values_mut() {
            if timer.can_fire() && timer.fire_at <= now {
                timer.mark_processing();
                expired.push(timer.clone());

                if expired.len() >= limit as usize {
                    break;
                }
            }
        }

        Ok(expired)
    }

    async fn claim_timers(
        &self,
        timer_ids: &[String],
        _scheduler_id: &str,
    ) -> Result<Vec<DurableTimer>, TimerStoreError<Self::Error>> {
        let mut timers = self.inner.timers.write();
        let mut claimed = Vec::new();

        for id in timer_ids {
            if let Some(timer) = timers.get_mut(id) {
                if timer.status == TimerStatus::Pending {
                    timer.mark_processing();
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
        let mut timers = self.inner.timers.write();
        if let Some(timer) = timers.get_mut(timer_id) {
            timer.status = status;
            Ok(())
        } else {
            Err(TimerStoreError::not_found(timer_id.to_string()))
        }
    }

    async fn get_timers_for_saga(
        &self,
        saga_id: &SagaId,
        _include_fired: bool,
    ) -> Result<Vec<DurableTimer>, TimerStoreError<Self::Error>> {
        let timers = self.inner.timers.read();
        let mut result: Vec<DurableTimer> = timers
            .values()
            .filter(|t| &t.saga_id == saga_id)
            .cloned()
            .collect();

        result.sort_by(|a, b| a.fire_at.cmp(&b.fire_at));
        Ok(result)
    }

    async fn get_timer(
        &self,
        timer_id: &str,
    ) -> Result<Option<DurableTimer>, TimerStoreError<Self::Error>> {
        let timers = self.inner.timers.read();
        Ok(timers.get(timer_id).cloned())
    }
}

/// Error type for InMemoryTimerStore operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InMemoryTimerStoreError {
    #[error("Timer not found: {0}")]
    NotFound(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<String> for InMemoryTimerStoreError {
    fn from(s: String) -> Self {
        Self::Internal(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[tokio::test]
    async fn test_create_and_get_timer() {
        let store = InMemoryTimerStore::new();
        let saga_id = SagaId("test-saga-1".to_string());

        let timer = DurableTimer::new(
            saga_id.clone(),
            "run-1".to_string(),
            TimerType::Sleep,
            Utc::now() + Duration::seconds(60),
        );

        store.create_timer(&timer).await.unwrap();

        let retrieved = store.get_timer(&timer.timer_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().status, TimerStatus::Pending);
    }

    #[tokio::test]
    async fn test_get_expired_timers() {
        let store = InMemoryTimerStore::new();
        let saga_id = SagaId("test-saga-2".to_string());

        // Create expired timer
        let expired_timer = DurableTimer::new(
            saga_id.clone(),
            "run-1".to_string(),
            TimerType::Sleep,
            Utc::now() - Duration::seconds(1),
        );
        store.create_timer(&expired_timer).await.unwrap();

        // Create future timer
        let future_timer = DurableTimer::new(
            saga_id.clone(),
            "run-2".to_string(),
            TimerType::Sleep,
            Utc::now() + Duration::seconds(60),
        );
        store.create_timer(&future_timer).await.unwrap();

        let expired = store.get_expired_timers(10).await.unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].timer_id, expired_timer.timer_id);
    }

    #[tokio::test]
    async fn test_cancel_timer() {
        let store = InMemoryTimerStore::new();
        let saga_id = SagaId("test-saga-3".to_string());

        let timer = DurableTimer::new(
            saga_id.clone(),
            "run-1".to_string(),
            TimerType::Sleep,
            Utc::now() + Duration::seconds(60),
        );
        store.create_timer(&timer).await.unwrap();

        store.cancel_timer(&timer.timer_id).await.unwrap();

        let retrieved = store.get_timer(&timer.timer_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().status, TimerStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_get_timers_for_saga() {
        let store = InMemoryTimerStore::new();
        let saga_id = SagaId("test-saga-4".to_string());

        for i in 0..5 {
            let timer = DurableTimer::new(
                saga_id.clone(),
                format!("run-{}", i),
                TimerType::Sleep,
                Utc::now() + Duration::seconds(i as i64),
            );
            store.create_timer(&timer).await.unwrap();
        }

        let timers = store.get_timers_for_saga(&saga_id, true).await.unwrap();
        assert_eq!(timers.len(), 5);
    }

    #[tokio::test]
    async fn test_update_timer_status() {
        let store = InMemoryTimerStore::new();
        let saga_id = SagaId("test-saga-5".to_string());

        let timer = DurableTimer::new(
            saga_id.clone(),
            "run-1".to_string(),
            TimerType::Sleep,
            Utc::now() + Duration::seconds(60),
        );
        store.create_timer(&timer).await.unwrap();

        store
            .update_timer_status(&timer.timer_id, TimerStatus::Fired)
            .await
            .unwrap();

        let retrieved = store.get_timer(&timer.timer_id).await.unwrap();
        assert_eq!(retrieved.unwrap().status, TimerStatus::Fired);
    }

    #[tokio::test]
    async fn test_clear_store() {
        let store = InMemoryTimerStore::new();
        let saga_id = SagaId("test-saga-6".to_string());

        let timer = DurableTimer::new(
            saga_id.clone(),
            "run-1".to_string(),
            TimerType::Sleep,
            Utc::now() + Duration::seconds(60),
        );
        store.create_timer(&timer).await.unwrap();

        assert_eq!(store.timer_count(), 1);

        store.clear();

        assert_eq!(store.timer_count(), 0);
    }
}

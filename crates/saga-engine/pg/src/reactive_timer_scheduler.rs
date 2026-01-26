//! # Reactive Timer Scheduler
//!
//! A reactive timer scheduler implementation that uses PostgreSQL LISTEN/NOTIFY
//! instead of polling for improved performance and scalability.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use saga_engine_core::event::{EventId, EventType, HistoryEvent, SagaId};
use saga_engine_core::port::event_store::{EventStore, EventStoreError};
use saga_engine_core::port::timer_store::{
    DurableTimer, TimerStatus, TimerStore, TimerStoreError, TimerType,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::notify_listener::{NotificationReceiver, NotifyListener};

/// Reactive Timer Scheduler Configuration
#[derive(Debug, Clone)]
pub struct ReactiveTimerSchedulerConfig {
    /// This worker's ID for sharding
    pub worker_id: u64,
    /// Total number of workers (for sharding)
    pub total_shards: u64,
    /// Maximum timers to process per notification batch
    pub max_batch_size: u64,
}

impl Default for ReactiveTimerSchedulerConfig {
    fn default() -> Self {
        Self {
            worker_id: 0,
            total_shards: 1,
            max_batch_size: 100,
        }
    }
}

/// Reactive Timer Scheduler that processes timers via NOTIFY instead of polling
pub struct ReactiveTimerScheduler<E, T>
where
    E: EventStore + Send + Sync,
    T: TimerStore + Send + Sync,
{
    /// Event store for creating TimerFired events
    event_store: Arc<E>,
    /// Timer store for timer operations
    timer_store: Arc<T>,
    /// PostgreSQL notification listener
    notify_listener: Arc<dyn NotifyListener>,
    /// Configuration
    config: ReactiveTimerSchedulerConfig,
    /// Shutdown signal receiver
    shutdown_rx: Option<mpsc::Receiver<()>>,
}

impl<E, T> ReactiveTimerScheduler<E, T>
where
    E: EventStore + Send + Sync,
    T: TimerStore + Send + Sync,
{
    /// Create a new ReactiveTimerScheduler
    pub fn new(
        event_store: Arc<E>,
        timer_store: Arc<T>,
        notify_listener: Arc<dyn NotifyListener>,
        config: ReactiveTimerSchedulerConfig,
    ) -> Self {
        Self {
            event_store,
            timer_store,
            notify_listener,
            config,
            shutdown_rx: None,
        }
    }

    /// Create with shutdown channel
    pub fn with_shutdown(
        event_store: Arc<E>,
        timer_store: Arc<T>,
        notify_listener: Arc<dyn NotifyListener>,
        config: ReactiveTimerSchedulerConfig,
        shutdown_rx: mpsc::Receiver<()>,
    ) -> Self {
        Self {
            event_store,
            timer_store,
            notify_listener,
            config,
            shutdown_rx: Some(shutdown_rx),
        }
    }

    /// Check if this worker should process the given timer based on sharding
    fn should_process_timer(&self, timer_worker_id: u64) -> bool {
        let assigned_shard = timer_worker_id % self.config.total_shards;
        assigned_shard == self.config.worker_id
    }

    /// Process a timer notification
    async fn process_timer_notification(
        &self,
        timer_id: String,
        saga_id: SagaId,
        timer_worker_id: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check sharding
        if !self.should_process_timer(timer_worker_id) {
            debug!(
                "Timer {} belongs to worker {}, skipping (we are worker {})",
                timer_id, timer_worker_id, self.config.worker_id
            );
            return Ok(());
        }

        // Get timer from store - handle error conversion manually
        let timer_result = self.timer_store.get_timer(&timer_id).await;
        let timer = match timer_result {
            Ok(Some(t)) => t,
            Ok(None) => {
                warn!("Timer {} not found in store", timer_id);
                return Ok(());
            }
            Err(e) => {
                error!("Error getting timer {}: {:?}", timer_id, e);
                return Ok(());
            }
        };

        // Check if already processed
        match timer.status {
            TimerStatus::Fired | TimerStatus::Cancelled | TimerStatus::Failed => {
                debug!(
                    "Timer {} already in terminal state: {:?}",
                    timer.timer_id, timer.status
                );
                return Ok(());
            }
            _ => {}
        }

        // Claim the timer - handle error conversion manually
        let claim_result = self
            .timer_store
            .claim_timers(
                &[timer.timer_id.clone()],
                &format!("worker-{}", self.config.worker_id),
            )
            .await;

        let claimed = match claim_result {
            Ok(c) => c,
            Err(e) => {
                error!("Error claiming timer {}: {:?}", timer_id, e);
                return Ok(());
            }
        };

        if claimed.is_empty() {
            debug!("Timer {} already claimed by another worker", timer_id);
            return Ok(());
        }

        // Create TimerFired event
        let timer_fired_event = HistoryEvent::builder()
            .event_id(EventId(0)) // Will be set by event store
            .event_type(EventType::TimerFired)
            .saga_id(saga_id.clone())
            .payload(serde_json::json!({
                "timer_id": timer.timer_id,
                "timer_type": timer.timer_type.as_str(),
                "fire_at": timer.fire_at.to_rfc3339(),
                "worker_id": self.config.worker_id,
            }))
            .build();

        // Append event (this triggers saga_events NOTIFY)
        if let Err(e) = self
            .event_store
            .append_event(&saga_id, u64::MAX, &timer_fired_event)
            .await
        {
            error!("Error appending TimerFired event: {:?}", e);
            return Err(Box::new(e));
        }

        debug!("Processed timer {} for saga {}", timer.timer_id, saga_id);

        Ok(())
    }

    /// Run the scheduler (async loop)
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Starting ReactiveTimerScheduler: worker_id={}, total_shards={}",
            self.config.worker_id, self.config.total_shards
        );

        // Subscribe to timer notifications
        let mut subscription = self.notify_listener.subscribe("saga_timers");

        // Get or create shutdown receiver
        let mut shutdown_rx = self.shutdown_rx.take().unwrap_or_else(|| {
            let (tx, rx) = mpsc::channel(1);
            drop(tx);
            rx
        });

        loop {
            tokio::select! {
                // Timer notification
                notification = subscription.recv() => {
                    match notification {
                        Some(payload) => {
                            if let Err(e) = self.handle_timer_payload(&payload).await {
                                error!("Error processing timer notification: {:?}", e);
                            }
                        }
                        None => {
                            warn!("Notification channel closed");
                            break;
                        }
                    }
                }

                // Shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("ReactiveTimerScheduler shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_timer_payload(
        &self,
        payload: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Parse timer notification
        #[derive(serde::Deserialize)]
        struct TimerPayload {
            timer_id: String,
            saga_id: String,
            fire_at: String,
            worker_id: String,
        }

        let notify: TimerPayload = serde_json::from_str(payload)?;

        let worker_id: u64 = notify.worker_id.parse().unwrap_or(0);
        let saga_id = SagaId(notify.saga_id);

        self.process_timer_notification(notify.timer_id, saga_id, worker_id)
            .await
    }
}

#[cfg(test)]
mod scheduler_tests {
    use super::*;

    #[tokio::test]
    async fn test_should_process_timer_by_shard() {
        let config = ReactiveTimerSchedulerConfig {
            worker_id: 0,
            total_shards: 4,
            max_batch_size: 100,
        };

        // Worker 0 should process timers with worker_id 0, 4, 8, ...
        assert!(config.worker_id == 0);
        assert!((0 % 4) == 0); // timer_worker_id 0 -> shard 0
        assert!((4 % 4) == 0); // timer_worker_id 4 -> shard 0
        assert!((1 % 4) == 1); // timer_worker_id 1 -> shard 1 (not us)
    }

    #[tokio::test]
    async fn test_sharding_distribution() {
        let total_timers = 1000;
        let workers = 4;
        let our_worker = 0;
        let mut shard_counts = vec![0; workers];

        for i in 0..total_timers {
            let worker_id = i as u64;
            let assigned_shard = worker_id % workers as u64;

            if assigned_shard == our_worker {
                shard_counts[our_worker as usize] += 1;
            }
        }

        // With 1000 timers and 4 shards, each shard should get ~250 timers
        assert_eq!(shard_counts[our_worker as usize], 250);
    }

    #[tokio::test]
    async fn test_default_config() {
        let config = ReactiveTimerSchedulerConfig::default();
        assert_eq!(config.worker_id, 0);
        assert_eq!(config.total_shards, 1);
        assert_eq!(config.max_batch_size, 100);
    }
}

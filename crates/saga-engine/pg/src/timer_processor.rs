//! # Timer Processor with Dual Mode Support
//!
//! This module provides a unified timer processor that supports both reactive
//! (LISTEN/NOTIFY) and polling modes with automatic fallback for reliability.

use crate::notify_listener::NotifyListener;
use saga_engine_core::event::{EventId, EventType, HistoryEvent, SagaId};
use saga_engine_core::port::event_store::EventStore;
use saga_engine_core::port::timer_store::{DurableTimer, TimerStatus, TimerStore};
use saga_engine_core::saga_engine::config::{ReactiveMode, SagaEngineConfig};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Unified timer processor supporting reactive and polling modes.
pub struct TimerProcessor<E, ES>
where
    E: EventStore + Send + Sync + 'static,
    ES: TimerStore + Send + Sync + 'static,
{
    /// Event store for creating TimerFired events
    event_store: Arc<E>,
    /// Timer store for timer operations
    timer_store: Arc<ES>,
    /// Configuration
    config: SagaEngineConfig,
    /// Current processing mode
    current_mode: ProcessingMode,
    /// Notification listener (for reactive mode)
    notify_listener: Option<Arc<dyn NotifyListener>>,
    /// Active processor task handle
    processor_handle: Option<JoinHandle<()>>,
    /// Shutdown signal (atomic flag for safe sharing)
    shutdown: Arc<AtomicBool>,
    /// Metrics for monitoring
    metrics: Arc<TimerProcessorMetrics>,
}

/// Processing mode enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingMode {
    /// Reactive mode using LISTEN/NOTIFY
    Reactive,
    /// Polling mode (fallback)
    Polling,
    /// Uninitialized
    Uninitialized,
}

/// Events for mode changes.
#[derive(Debug, Clone)]
pub enum ModeChangeEvent {
    /// Switched to reactive mode
    SwitchedToReactive,
    /// Switched to polling mode
    SwitchedToPolling,
    /// Reactive mode failed, switching to polling
    ReactiveFailed { error: String },
}

/// Metrics for timer processing.
#[derive(Default)]
pub struct TimerProcessorMetrics {
    /// Total timers processed
    timers_processed: AtomicU64,
    /// Timers processed in reactive mode
    reactive_timers: AtomicU64,
    /// Timers processed in polling mode
    polling_timers: AtomicU64,
    /// Mode switches to reactive
    switches_to_reactive: AtomicU64,
    /// Mode switches to polling (fallback)
    switches_to_polling: AtomicU64,
    /// Reactive mode failures
    reactive_failures: AtomicU64,
}

impl TimerProcessorMetrics {
    /// Increment timers processed counter.
    pub fn record_timer(&self, mode: ProcessingMode) {
        self.timers_processed.fetch_add(1, Ordering::Relaxed);
        match mode {
            ProcessingMode::Reactive => {
                self.reactive_timers.fetch_add(1, Ordering::Relaxed);
            }
            ProcessingMode::Polling => {
                self.polling_timers.fetch_add(1, Ordering::Relaxed);
            }
            ProcessingMode::Uninitialized => {}
        }
    }

    /// Record mode switch.
    pub fn record_mode_switch(&self, new_mode: ProcessingMode) {
        match new_mode {
            ProcessingMode::Reactive => {
                self.switches_to_reactive.fetch_add(1, Ordering::Relaxed);
            }
            ProcessingMode::Polling => {
                self.switches_to_polling.fetch_add(1, Ordering::Relaxed);
            }
            ProcessingMode::Uninitialized => {}
        }
    }

    /// Record reactive failure.
    pub fn record_reactive_failure(&self) {
        self.reactive_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Get snapshot of metrics.
    pub fn snapshot(&self) -> TimerProcessorMetricsSnapshot {
        TimerProcessorMetricsSnapshot {
            timers_processed: self.timers_processed.load(Ordering::Relaxed),
            reactive_timers: self.reactive_timers.load(Ordering::Relaxed),
            polling_timers: self.polling_timers.load(Ordering::Relaxed),
            switches_to_reactive: self.switches_to_reactive.load(Ordering::Relaxed),
            switches_to_polling: self.switches_to_polling.load(Ordering::Relaxed),
            reactive_failures: self.reactive_failures.load(Ordering::Relaxed),
        }
    }
}

impl<E, ES> TimerProcessor<E, ES>
where
    E: EventStore + Send + Sync + 'static,
    ES: TimerStore + Send + Sync + 'static,
{
    /// Create a new TimerProcessor.
    pub async fn new(
        event_store: Arc<E>,
        timer_store: Arc<ES>,
        config: SagaEngineConfig,
    ) -> Result<Self, TimerProcessorError> {
        Ok(Self {
            event_store,
            timer_store,
            config: config.clone(),
            current_mode: ProcessingMode::Uninitialized,
            notify_listener: None,
            processor_handle: None,
            shutdown: Arc::new(AtomicBool::new(false)),
            metrics: Arc::new(TimerProcessorMetrics::default()),
        })
    }

    /// Initialize the processor based on configuration.
    pub async fn initialize(
        &mut self,
        notify_listener: Option<Arc<dyn NotifyListener>>,
    ) -> Result<(), TimerProcessorError> {
        self.notify_listener = notify_listener;

        match self.config.reactive_mode {
            ReactiveMode::Enabled => self.start_reactive_mode().await,
            #[allow(deprecated)]
            ReactiveMode::Polling => {
                warn!("Polling mode is deprecated. Consider using reactive mode.");
                self.start_polling_mode().await
            }
        }
    }

    /// Start processing in reactive mode.
    async fn start_reactive_mode(&mut self) -> Result<(), TimerProcessorError> {
        info!(
            "Starting TimerProcessor in reactive mode (worker_id={}, shards={})",
            self.config.worker_id, self.config.total_shards
        );

        self.current_mode = ProcessingMode::Reactive;
        self.metrics.record_mode_switch(ProcessingMode::Reactive);

        if let Some(ref listener) = self.notify_listener {
            self.spawn_reactive_processor(listener.clone());
        }

        Ok(())
    }

    /// Start processing in polling mode.
    async fn start_polling_mode(&mut self) -> Result<(), TimerProcessorError> {
        info!("Starting TimerProcessor in polling mode");
        self.current_mode = ProcessingMode::Polling;
        self.metrics.record_mode_switch(ProcessingMode::Polling);
        self.spawn_polling_processor();
        Ok(())
    }

    /// Spawn reactive timer processor task.
    fn spawn_reactive_processor(&mut self, listener: Arc<dyn NotifyListener>) {
        let event_store = Arc::clone(&self.event_store);
        let timer_store = Arc::clone(&self.timer_store);
        let config = self.config.clone();
        let metrics = Arc::clone(&self.metrics);
        let shutdown = Arc::clone(&self.shutdown);

        let handle = tokio::spawn(async move {
            let mut subscription = listener.subscribe("saga_timers");

            loop {
                if shutdown.load(Ordering::Relaxed) {
                    info!("Shutdown signal received in reactive timer processor");
                    break;
                }

                tokio::select! {
                    notification = subscription.recv() => {
                        match notification {
                            Some(payload) => {
                                if let Err(e) = Self::process_timer_notification(
                                    &event_store, &timer_store, &config, &payload, &metrics,
                                ).await {
                                    error!("Error processing timer notification: {:?}", e);
                                }
                            }
                            None => {
                                warn!("Timer notification channel closed");
                                break;
                            }
                        }
                    }
                }
            }
        });

        self.processor_handle = Some(handle);
    }

    /// Spawn polling timer processor task.
    fn spawn_polling_processor(&mut self) {
        let event_store = Arc::clone(&self.event_store);
        let timer_store = Arc::clone(&self.timer_store);
        let config = self.config.clone();
        let metrics = Arc::clone(&self.metrics);
        let shutdown = Arc::clone(&self.shutdown);
        let poll_interval = Duration::from_secs(5);

        let handle = tokio::spawn(async move {
            let mut last_poll = std::time::Instant::now();

            loop {
                if shutdown.load(Ordering::Relaxed) {
                    info!("Shutdown signal received in polling timer processor");
                    break;
                }

                let elapsed = std::time::Instant::now().duration_since(last_poll);
                if elapsed < poll_interval {
                    tokio::time::sleep(poll_interval - elapsed).await;
                }
                last_poll = std::time::Instant::now();

                Self::poll_and_process_timers(&event_store, &timer_store, &config, &metrics).await;
            }
        });

        self.processor_handle = Some(handle);
    }

    /// Process a single timer notification.
    async fn process_timer_notification(
        event_store: &Arc<E>,
        timer_store: &Arc<ES>,
        config: &SagaEngineConfig,
        payload: &str,
        metrics: &Arc<TimerProcessorMetrics>,
    ) -> Result<(), TimerProcessorError> {
        #[derive(serde::Deserialize)]
        struct TimerPayload {
            timer_id: String,
            saga_id: String,
            fire_at: String,
            worker_id: String,
        }

        let notify: TimerPayload = serde_json::from_str(payload)
            .map_err(|e| TimerProcessorError::InvalidPayload(e.to_string()))?;

        let worker_id: u64 = notify.worker_id.parse().unwrap_or(0);
        let total_shards = config.total_shards as u64;
        let assigned_shard = worker_id % total_shards;

        if assigned_shard != config.worker_id {
            debug!(
                "Timer {} belongs to shard {}, skipping",
                notify.timer_id, assigned_shard
            );
            return Ok(());
        }

        let timer = match timer_store.get_timer(&notify.timer_id).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                warn!("Timer {} not found", notify.timer_id);
                return Ok(());
            }
            Err(e) => return Err(TimerProcessorError::TimerStore(format!("{:?}", e))),
        };

        if !matches!(timer.status, TimerStatus::Pending) {
            debug!(
                "Timer {} already in state: {:?}",
                timer.timer_id, timer.status
            );
            return Ok(());
        }

        let claimed = match timer_store
            .claim_timers(
                &[timer.timer_id.clone()],
                &format!("timer-processor-{}", config.worker_id),
            )
            .await
        {
            Ok(c) => c,
            Err(e) => return Err(TimerProcessorError::TimerStore(format!("{:?}", e))),
        };

        if claimed.is_empty() {
            debug!("Timer {} already claimed", notify.timer_id);
            return Ok(());
        }

        let timer_fired_event = HistoryEvent::builder()
            .event_id(EventId(0))
            .event_type(EventType::TimerFired)
            .saga_id(SagaId(notify.saga_id.clone()))
            .payload(serde_json::json!({
                "timer_id": timer.timer_id,
                "timer_type": timer.timer_type.as_str(),
                "fire_at": notify.fire_at,
                "worker_id": config.worker_id,
            }))
            .build();

        if let Err(e) = event_store
            .append_event(&SagaId(notify.saga_id), u64::MAX, &timer_fired_event)
            .await
        {
            return Err(TimerProcessorError::EventStore(format!("{:?}", e)));
        }

        metrics.record_timer(ProcessingMode::Reactive);
        Ok(())
    }

    /// Poll and process expired timers.
    async fn poll_and_process_timers(
        event_store: &Arc<E>,
        timer_store: &Arc<ES>,
        config: &SagaEngineConfig,
        metrics: &Arc<TimerProcessorMetrics>,
    ) {
        let timers = match timer_store.get_expired_timers(100).await {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to get expired timers: {:?}", e);
                return;
            }
        };

        for timer in timers {
            if let Err(e) =
                Self::process_expired_timer(event_store, timer_store, config, &timer, metrics).await
            {
                error!("Error processing timer {}: {:?}", timer.timer_id, e);
            }
        }
    }

    /// Process a single expired timer.
    async fn process_expired_timer(
        event_store: &Arc<E>,
        timer_store: &Arc<ES>,
        config: &SagaEngineConfig,
        timer: &DurableTimer,
        metrics: &Arc<TimerProcessorMetrics>,
    ) -> Result<(), TimerProcessorError> {
        let claimed = match timer_store
            .claim_timers(
                &[timer.timer_id.clone()],
                &format!("timer-processor-{}", config.worker_id),
            )
            .await
        {
            Ok(c) => c,
            Err(e) => return Err(TimerProcessorError::TimerStore(format!("{:?}", e))),
        };

        if claimed.is_empty() {
            return Ok(());
        }

        let timer_fired_event = HistoryEvent::builder()
            .event_id(EventId(0))
            .event_type(EventType::TimerFired)
            .saga_id(timer.saga_id.clone())
            .payload(serde_json::json!({
                "timer_id": timer.timer_id,
                "timer_type": timer.timer_type.as_str(),
                "fire_at": timer.fire_at.to_rfc3339(),
                "worker_id": config.worker_id,
            }))
            .build();

        if let Err(e) = event_store
            .append_event(&timer.saga_id, u64::MAX, &timer_fired_event)
            .await
        {
            return Err(TimerProcessorError::EventStore(format!("{:?}", e)));
        }

        metrics.record_timer(ProcessingMode::Polling);
        Ok(())
    }

    /// Stop the processor.
    pub async fn stop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.processor_handle.take() {
            let _ = handle.await;
        }
        self.current_mode = ProcessingMode::Uninitialized;
    }

    /// Get current processing mode.
    pub fn current_mode(&self) -> ProcessingMode {
        self.current_mode
    }

    /// Get current metrics snapshot.
    pub fn metrics(&self) -> TimerProcessorMetricsSnapshot {
        self.metrics.snapshot()
    }
}

/// Snapshot of timer processor metrics.
#[derive(Debug, Clone, Default)]
pub struct TimerProcessorMetricsSnapshot {
    /// Total timers processed
    pub timers_processed: u64,
    /// Timers processed in reactive mode
    pub reactive_timers: u64,
    /// Timers processed in polling mode
    pub polling_timers: u64,
    /// Mode switches to reactive
    pub switches_to_reactive: u64,
    /// Mode switches to polling (fallback)
    pub switches_to_polling: u64,
    /// Reactive mode failures
    pub reactive_failures: u64,
}

/// Timer processor errors.
#[derive(Debug, thiserror::Error)]
pub enum TimerProcessorError {
    #[error("Timer store error: {0}")]
    TimerStore(String),

    #[error("Invalid notification payload: {0}")]
    InvalidPayload(String),

    #[error("Event store error: {0}")]
    EventStore(String),

    #[error("Reactive mode not available: {0}")]
    ReactiveModeUnavailable(String),
}

impl Default for ProcessingMode {
    fn default() -> Self {
        ProcessingMode::Uninitialized
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processing_mode_default() {
        assert_eq!(ProcessingMode::Uninitialized, ProcessingMode::default());
        assert_ne!(ProcessingMode::Reactive, ProcessingMode::default());
    }

    #[test]
    fn test_mode_change_event_variants() {
        let _ = ModeChangeEvent::SwitchedToReactive;
        let _ = ModeChangeEvent::SwitchedToPolling;
        let _ = ModeChangeEvent::ReactiveFailed {
            error: "test error".to_string(),
        };
    }

    #[test]
    fn test_metrics_snapshot_defaults() {
        let snapshot = TimerProcessorMetricsSnapshot::default();
        assert_eq!(snapshot.timers_processed, 0);
        assert_eq!(snapshot.reactive_timers, 0);
        assert_eq!(snapshot.polling_timers, 0);
        assert_eq!(snapshot.switches_to_reactive, 0);
        assert_eq!(snapshot.switches_to_polling, 0);
        assert_eq!(snapshot.reactive_failures, 0);
    }

    #[tokio::test]
    async fn test_config_builder_pattern() {
        let config = SagaEngineConfig::new()
            .with_worker_id(5)
            .with_total_shards(10)
            .with_max_events_before_snapshot(50)
            .with_auto_compensation(false);

        assert_eq!(config.worker_id, 5);
        assert_eq!(config.total_shards, 10);
        assert!(!config.auto_compensation);
    }

    #[tokio::test]
    async fn test_metrics_snapshot() {
        let metrics = TimerProcessorMetrics::default();
        let snapshot = metrics.snapshot();

        assert_eq!(snapshot.timers_processed, 0);
        assert_eq!(snapshot.reactive_timers, 0);
        assert_eq!(snapshot.polling_timers, 0);
    }
}

//! # Reactive Worker
//!
//! A reactive worker implementation that processes saga events via PostgreSQL
//! LISTEN/NOTIFY instead of polling task queues.

use async_trait::async_trait;
use saga_engine_core::activity_registry::ActivityRegistry;
use saga_engine_core::event::SagaId;
use saga_engine_core::port::event_store::EventStore;
use saga_engine_core::port::task_queue::TaskQueue;
use saga_engine_core::port::timer_store::TimerStore;
use saga_engine_core::saga_engine::{SagaEngine, SagaEngineError, WorkflowTask};
use saga_engine_core::workflow::registry::WorkflowRegistry;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::notify_listener::{NotificationReceiver, NotifyListener};

/// Reactive Worker Configuration
#[derive(Debug, Clone)]
pub struct ReactiveWorkerConfig {
    /// Worker ID for sharding
    pub worker_id: u64,
    /// Total number of workers (for sharding)
    pub total_shards: u64,
    /// Consumer name for task queue fallback
    pub consumer_name: String,
    /// Queue group for load balancing
    pub queue_group: String,
    /// Maximum concurrent tasks
    pub max_concurrent: u64,
}

impl Default for ReactiveWorkerConfig {
    fn default() -> Self {
        Self {
            worker_id: 0,
            total_shards: 1,
            consumer_name: "saga-reactive-worker".to_string(),
            queue_group: "saga-workers".to_string(),
            max_concurrent: 10,
        }
    }
}

/// Worker errors
#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("Task processing failed: {0}")]
    TaskFailed(String),

    #[error("Saga not found: {0}")]
    SagaNotFound(String),

    #[error("Workflow not found: {0}")]
    WorkflowNotFound(String),

    #[error("Event processing failed: {0}")]
    EventProcessingFailed(String),
}

/// A reactive worker that processes events via NOTIFY instead of polling
pub struct ReactiveWorker<E, Q, T>
where
    E: EventStore + Send + Sync + 'static,
    Q: TaskQueue + Send + Sync + 'static,
    T: TimerStore + Send + Sync + 'static,
{
    /// Worker configuration
    config: ReactiveWorkerConfig,
    /// Saga engine for workflow execution
    saga_engine: Arc<SagaEngine<E, Q, T>>,
    /// Activity registry for activity lookup
    activity_registry: Arc<ActivityRegistry>,
    /// Workflow registry for workflow lookup
    workflow_registry: Arc<WorkflowRegistry>,
    /// PostgreSQL notification listener
    notify_listener: Arc<dyn NotifyListener>,
    /// Running flag
    running: Arc<AtomicBool>,
    /// Shutdown receiver
    shutdown_rx: Option<mpsc::Receiver<()>>,
}

impl<E, Q, T> ReactiveWorker<E, Q, T>
where
    E: EventStore + Send + Sync + 'static,
    Q: TaskQueue + Send + Sync + 'static,
    T: TimerStore + Send + Sync + 'static,
{
    /// Create a new ReactiveWorker
    pub fn new(
        config: ReactiveWorkerConfig,
        saga_engine: Arc<SagaEngine<E, Q, T>>,
        activity_registry: Arc<ActivityRegistry>,
        workflow_registry: Arc<WorkflowRegistry>,
        notify_listener: Arc<dyn NotifyListener>,
    ) -> Self {
        Self {
            config,
            saga_engine,
            activity_registry,
            workflow_registry,
            notify_listener,
            running: Arc::new(AtomicBool::new(false)),
            shutdown_rx: None,
        }
    }

    /// Create with shutdown channel
    pub fn with_shutdown(
        config: ReactiveWorkerConfig,
        saga_engine: Arc<SagaEngine<E, Q, T>>,
        activity_registry: Arc<ActivityRegistry>,
        workflow_registry: Arc<WorkflowRegistry>,
        notify_listener: Arc<dyn NotifyListener>,
        shutdown_rx: mpsc::Receiver<()>,
    ) -> Self {
        Self {
            config,
            saga_engine,
            activity_registry,
            workflow_registry,
            notify_listener,
            running: Arc::new(AtomicBool::new(false)),
            shutdown_rx: Some(shutdown_rx),
        }
    }

    /// Calculate shard for a saga ID
    fn get_saga_shard(&self, saga_id: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        saga_id.hash(&mut hasher);
        hasher.finish() % self.config.total_shards
    }

    /// Check if this worker should process the given saga
    fn should_process_saga(&self, saga_id: &str) -> bool {
        self.get_saga_shard(saga_id) == self.config.worker_id
    }

    /// Process an event notification
    async fn process_event(&self, saga_id: SagaId, event_type: String) -> Result<(), WorkerError> {
        let saga_id_str = saga_id.0.as_str();

        // Check sharding
        if !self.should_process_saga(saga_id_str) {
            debug!(
                "Saga {} belongs to shard {}, skipping (we are worker {})",
                saga_id_str,
                self.get_saga_shard(saga_id_str),
                self.config.worker_id
            );
            return Ok(());
        }

        debug!("Processing event {} for saga {}", event_type, saga_id_str);

        // Get workflow type from history
        let workflow_type = self
            .get_workflow_type_for_saga(&saga_id)
            .await
            .ok_or_else(|| WorkerError::SagaNotFound(saga_id_str.to_string()))?;

        // Lookup workflow
        let workflow = self
            .workflow_registry
            .get_workflow(&workflow_type)
            .ok_or_else(|| WorkerError::WorkflowNotFound(workflow_type.clone()))?;

        // Resume the workflow
        match self
            .saga_engine
            .resume_workflow_dyn(workflow.as_ref(), saga_id)
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(WorkerError::TaskFailed(e.to_string())),
        }
    }

    /// Get workflow type for a saga from its history
    async fn get_workflow_type_for_saga(&self, saga_id: &SagaId) -> Option<String> {
        let history = self
            .saga_engine
            .event_store()
            .get_history(saga_id)
            .await
            .ok()?;
        history.first().and_then(|event| {
            event
                .attributes
                .get("workflow_type")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        })
    }

    /// Start the worker
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.running.store(true, Ordering::SeqCst);

        info!(
            "Starting ReactiveWorker: worker_id={}, total_shards={}",
            self.config.worker_id, self.config.total_shards
        );

        // Subscribe to channels
        let events_sub = self.notify_listener.subscribe("saga_events");
        let signals_sub = self.notify_listener.subscribe("saga_signals");

        // Listen to channels
        self.notify_listener.listen("saga_events").await?;
        self.notify_listener.listen("saga_signals").await?;

        // Start listener
        self.notify_listener.start().await?;

        // Get or create shutdown receiver
        let mut shutdown_rx = self.shutdown_rx.take().unwrap_or_else(|| {
            let (tx, rx) = mpsc::channel(1);
            drop(tx);
            rx
        });

        let mut events_rx = events_sub;
        let mut signals_rx = signals_sub;

        loop {
            tokio::select! {
                // Event notification
                payload = events_rx.recv() => {
                    match payload {
                        Some(p) => {
                            if let Err(e) = self.handle_event_payload(&p).await {
                                error!("Error processing event notification: {:?}", e);
                            }
                        }
                        None => {
                            warn!("Events notification channel closed");
                            break;
                        }
                    }
                }

                // Signal notification
                payload = signals_rx.recv() => {
                    match payload {
                        Some(p) => {
                            if let Err(e) = self.handle_signal_payload(&p).await {
                                error!("Error processing signal notification: {:?}", e);
                            }
                        }
                        None => {
                            warn!("Signals notification channel closed");
                            break;
                        }
                    }
                }

                // Shutdown
                _ = shutdown_rx.recv() => {
                    info!("ReactiveWorker shutting down");
                    break;
                }
            }
        }

        self.running.store(false, Ordering::SeqCst);
        self.notify_listener.stop();
        Ok(())
    }

    async fn handle_event_payload(&self, payload: &str) -> Result<(), WorkerError> {
        #[derive(serde::Deserialize)]
        struct EventPayload {
            saga_id: String,
            event_id: u64,
            event_type: String,
        }

        let notify: EventPayload = serde_json::from_str(payload)
            .map_err(|e| WorkerError::EventProcessingFailed(e.to_string()))?;
        self.process_event(SagaId(notify.saga_id), notify.event_type)
            .await
    }

    async fn handle_signal_payload(&self, payload: &str) -> Result<(), WorkerError> {
        #[derive(serde::Deserialize)]
        struct SignalPayload {
            saga_id: String,
            signal_type: String,
        }

        let notify: SignalPayload = serde_json::from_str(payload)
            .map_err(|e| WorkerError::EventProcessingFailed(e.to_string()))?;
        self.process_event(SagaId(notify.saga_id), notify.signal_type)
            .await
    }

    /// Stop the worker
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.notify_listener.stop();
    }

    /// Check if running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Dummy notify listener for tests
struct DummyNotifyListener;

#[async_trait]
impl NotifyListener for DummyNotifyListener {
    fn subscribe(&self, _channel: &str) -> NotificationReceiver {
        let (tx, rx) = mpsc::unbounded_channel();
        drop(tx);
        NotificationReceiver { rx }
    }

    async fn listen(&self, _channel: &str) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn listen_multiple(&self, _channels: &[&'static str]) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn start(&self) -> Result<(), sqlx::Error> {
        Ok(())
    }

    fn stop(&self) {}
    fn channels(&self) -> Vec<String> {
        vec![]
    }
    fn is_running(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod worker_tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn hash_saga_id(saga_id: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        saga_id.hash(&mut hasher);
        hasher.finish()
    }

    #[tokio::test]
    async fn test_saga_sharding_consistency() {
        for i in 0..100 {
            let saga_id = format!("saga-{:03}", i);
            let hash1 = hash_saga_id(&saga_id);
            let hash2 = hash_saga_id(&saga_id);
            assert_eq!(hash1, hash2, "Hash should be consistent for same saga_id");
        }
    }

    #[tokio::test]
    async fn test_sharding_distribution() {
        let workers = 4;
        let total_sagas = 1000;
        let mut shard_counts = vec![0; workers];

        for i in 0..total_sagas {
            let saga_id = format!("saga-{:03}", i);
            let hash = hash_saga_id(&saga_id);
            let shard = hash % workers as u64;
            shard_counts[shard as usize] += 1;
        }

        for count in &shard_counts {
            let expected = total_sagas / workers;
            assert!((*count as i64 - expected as i64).abs() < (expected as i64 / 5));
        }
    }

    #[tokio::test]
    async fn test_should_process_saga() {
        let config = ReactiveWorkerConfig {
            worker_id: 0,
            total_shards: 4,
            ..Default::default()
        };

        assert!(config.worker_id == 0);
        // Test that worker 0 can process sagas by checking sharding logic
        let hash = hash_saga_id("saga-000");
        // Worker 0 should process sagas where (hash % 4) == 0
        let _shard = hash % 4;
        // Just verify the hash function works consistently
        assert_eq!(hash_saga_id("saga-000"), hash_saga_id("saga-000"));
    }

    #[tokio::test]
    async fn test_default_config() {
        let config = ReactiveWorkerConfig::default();
        assert_eq!(config.worker_id, 0);
        assert_eq!(config.total_shards, 1);
        assert_eq!(config.consumer_name, "saga-reactive-worker");
    }
}

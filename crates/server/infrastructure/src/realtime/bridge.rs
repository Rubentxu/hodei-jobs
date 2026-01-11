//! Realtime Event Bridge - Connects Domain Events to WebSocket Clients

use crate::realtime::connection_manager::{BroadcastError, ConnectionManager};
use crate::realtime::metrics::RealtimeMetrics;
use hodei_server_domain::events::DomainEvent;
use hodei_shared::realtime::messages::ClientEvent;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};
use tracing::{debug, error, info, warn};

/// Configuration for the RealtimeEventBridge.
#[derive(Debug, Clone)]
pub struct EventBridgeConfig {
    pub subject_prefix: String,
    pub health_check_interval: Duration,
    pub max_batch_size: usize,
    pub enable_projection: bool,
}

impl Default for EventBridgeConfig {
    fn default() -> Self {
        Self {
            subject_prefix: "hodei.events".to_string(),
            health_check_interval: Duration::from_secs(30),
            max_batch_size: 100,
            enable_projection: true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EventBridgeError {
    #[error("Failed to subscribe to event bus: {0}")]
    SubscriptionFailed(String),

    #[error("Failed to process event: {0}")]
    ProcessingFailed(String),

    #[error("Bridge is already running")]
    AlreadyRunning,

    #[error("Connection manager error: {0}")]
    ConnectionManagerError(String),
}

/// A simplified event subscriber interface for the bridge.
#[async_trait::async_trait]
pub trait EventSubscriber: Send + Sync {
    async fn next(
        &mut self,
    ) -> Option<Result<DomainEvent, Box<dyn std::error::Error + Send + Sync>>>;
}

/// The RealtimeEventBridge subscribes to domain events and broadcasts them to WebSocket clients.
#[derive(Debug)]
pub struct RealtimeEventBridge<S: EventSubscriber> {
    subscriber: S,
    connection_manager: Arc<ConnectionManager>,
    metrics: Arc<RealtimeMetrics>,
    shutdown_rx: Option<mpsc::Receiver<()>>,
    config: EventBridgeConfig,
}

impl<S: EventSubscriber> RealtimeEventBridge<S> {
    pub fn new(
        subscriber: S,
        connection_manager: Arc<ConnectionManager>,
        shutdown_rx: Option<mpsc::Receiver<()>>,
        metrics: Arc<RealtimeMetrics>,
        config: Option<EventBridgeConfig>,
    ) -> Self {
        Self {
            subscriber,
            connection_manager,
            metrics,
            shutdown_rx,
            config: config.unwrap_or_default(),
        }
    }

    pub fn with_subscriber(
        subscriber: S,
        connection_manager: Arc<ConnectionManager>,
        shutdown_rx: Option<mpsc::Receiver<()>>,
        metrics: Arc<RealtimeMetrics>,
    ) -> Self {
        Self::new(subscriber, connection_manager, shutdown_rx, metrics, None)
    }

    pub async fn start(&mut self) -> Result<(), EventBridgeError> {
        info!(subject_prefix = %self.config.subject_prefix, "RealtimeEventBridge started");

        let mut health_interval = interval(self.config.health_check_interval);

        loop {
            tokio::select! {
                event_result = self.subscriber.next() => {
                    match event_result {
                        Some(Ok(event)) => {
                            self.process_event(event).await;
                        }
                        Some(Err(e)) => {
                            error!(error = %e, "Failed to receive domain event");
                        }
                        None => {
                            info!("Event subscriber closed");
                            break;
                        }
                    }
                }
                _ = health_interval.tick() => {
                    self.perform_health_check().await;
                }
                _ = async {
                    if let Some(mut rx) = self.shutdown_rx.take() {
                        let _ = rx.recv().await;
                    }
                } => {
                    info!("RealtimeEventBridge shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn process_event(&self, event: DomainEvent) {
        let start_time = Instant::now();
        self.metrics.record_domain_event_received();

        let client_event = if self.config.enable_projection {
            match project_domain_event(event.clone()) {
                Some(ce) => ce,
                None => {
                    self.metrics.record_domain_event_filtered();
                    debug!(?event, "Event filtered (internal)");
                    return;
                }
            }
        } else {
            return;
        };

        let topics = client_event.topics();
        let duration_us = start_time.elapsed().as_secs_f64() * 1_000_000.0;
        self.metrics.record_client_event_projected(duration_us);

        if let Err(e) = self
            .connection_manager
            .broadcast(&client_event, &topics)
            .await
        {
            match e {
                BroadcastError::AllSessionsClosed => {
                    debug!("No active sessions to broadcast to");
                }
                BroadcastError::Partial(failed, total) => {
                    warn!(failed = failed, total = total, "Partial broadcast failure");
                }
                BroadcastError::Serialization(e) => {
                    error!(error = %e, "Serialization error during broadcast");
                }
            }
        }
    }

    async fn perform_health_check(&self) {
        let manager_metrics = self.connection_manager.metrics();
        debug!(
            active_sessions = manager_metrics.active_sessions,
            "EventBridge health check"
        );
    }

    pub fn config(&self) -> &EventBridgeConfig {
        &self.config
    }

    pub fn connection_manager(&self) -> &ConnectionManager {
        &self.connection_manager
    }
}

/// Projects a domain event to a client event.
/// Returns None if the event should be filtered (internal event).
fn project_domain_event(event: DomainEvent) -> Option<ClientEvent> {
    use hodei_shared::realtime::ClientEventPayload;

    match event {
        DomainEvent::JobCreated {
            job_id,
            occurred_at,
            ..
        } => Some(ClientEvent {
            event_type: "job.created".to_string(),
            event_version: 1,
            aggregate_id: job_id.to_string(),
            payload: ClientEventPayload::JobCreated {
                job_id: job_id.to_string(),
                timestamp: occurred_at.timestamp_millis(),
            },
            occurred_at: occurred_at.timestamp_millis(),
        }),

        DomainEvent::JobStatusChanged {
            job_id,
            old_state,
            new_state,
            occurred_at,
            ..
        } => Some(ClientEvent {
            event_type: "job.status_changed".to_string(),
            event_version: 1,
            aggregate_id: job_id.to_string(),
            payload: ClientEventPayload::JobStatusChanged {
                job_id: job_id.to_string(),
                old_status: old_state.to_string(),
                new_status: new_state.to_string(),
                timestamp: occurred_at.timestamp_millis(),
            },
            occurred_at: occurred_at.timestamp_millis(),
        }),

        DomainEvent::WorkerReady {
            worker_id,
            provider_id,
            job_id,
            ready_at,
            ..
        } => Some(ClientEvent {
            event_type: "worker.ready".to_string(),
            event_version: 1,
            aggregate_id: worker_id.to_string(),
            payload: ClientEventPayload::WorkerReady {
                worker_id: worker_id.to_string(),
                provider_id: provider_id.to_string(),
                current_job_id: job_id.map(|id| id.to_string()),
                timestamp: ready_at.timestamp_millis(),
            },
            occurred_at: ready_at.timestamp_millis(),
        }),

        DomainEvent::WorkerHeartbeat {
            worker_id,
            state,
            load_average,
            memory_usage_mb,
            current_job_id,
            occurred_at,
            ..
        } => Some(ClientEvent {
            event_type: "worker.heartbeat".to_string(),
            event_version: 1,
            aggregate_id: worker_id.to_string(),
            payload: ClientEventPayload::WorkerHeartbeat {
                worker_id: worker_id.to_string(),
                state: state.to_string(),
                cpu: load_average.map(|l| l as f32),
                memory_mb: memory_usage_mb,
                active_jobs: if current_job_id.is_some() { 1 } else { 0 },
                timestamp: occurred_at.timestamp_millis(),
            },
            occurred_at: occurred_at.timestamp_millis(),
        }),

        // Internal events that should be filtered
        DomainEvent::JobDispatchAcknowledged { .. }
        | DomainEvent::RunJobReceived { .. }
        | DomainEvent::JobQueued { .. }
        | DomainEvent::JobQueueDepthChanged { .. }
        | DomainEvent::WorkerStatusChanged { .. }
        | DomainEvent::WorkerProvisioned { .. }
        | DomainEvent::WorkerProvisioningRequested { .. }
        | DomainEvent::WorkerProvisioningError { .. }
        | DomainEvent::WorkerReadyForJob { .. }
        | DomainEvent::WorkerStateUpdated { .. }
        | DomainEvent::WorkerEphemeralCreated { .. }
        | DomainEvent::WorkerEphemeralReady { .. }
        | DomainEvent::WorkerEphemeralTerminating { .. }
        | DomainEvent::WorkerEphemeralTerminated { .. }
        | DomainEvent::WorkerEphemeralCleanedUp { .. }
        | DomainEvent::WorkerEphemeralIdle { .. }
        | DomainEvent::OrphanWorkerDetected { .. }
        | DomainEvent::GarbageCollectionCompleted { .. }
        | DomainEvent::WorkerRecoveryFailed { .. }
        | DomainEvent::WorkerSelfTerminated { .. }
        | DomainEvent::SchedulingDecisionFailed { .. }
        | DomainEvent::JobExecutionError { .. }
        | DomainEvent::JobDispatchFailed { .. }
        | DomainEvent::JobRetried { .. }
        | DomainEvent::JobAssigned { .. }
        | DomainEvent::JobAccepted { .. }
        | DomainEvent::ProviderRegistered { .. }
        | DomainEvent::ProviderUpdated { .. }
        | DomainEvent::ProviderHealthChanged { .. }
        | DomainEvent::ProviderRecovered { .. }
        | DomainEvent::AutoScalingTriggered { .. }
        | DomainEvent::ProviderSelected { .. }
        | DomainEvent::ProviderExecutionError { .. }
        | DomainEvent::TemplateCreated { .. }
        | DomainEvent::TemplateUpdated { .. }
        | DomainEvent::TemplateDisabled { .. }
        | DomainEvent::TemplateRunCreated { .. }
        | DomainEvent::ScheduledJobCreated { .. }
        | DomainEvent::ScheduledJobTriggered { .. }
        | DomainEvent::ScheduledJobMissed { .. }
        | DomainEvent::ScheduledJobError { .. }
        | DomainEvent::WorkerDisconnected { .. }
        | DomainEvent::WorkerReconnected { .. }
        | DomainEvent::JobCancelled { .. }
        | DomainEvent::WorkerTerminated { .. }
        | DomainEvent::ExecutionRecorded { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realtime::session::Session;
    use hodei_server_domain::events::DomainEvent;
    use hodei_server_domain::shared_kernel::{JobId, JobState};
    use std::sync::Arc;
    use tokio::sync::mpsc;

    struct MockSubscriber {
        events: std::sync::Mutex<Vec<DomainEvent>>,
    }

    impl MockSubscriber {
        fn new(events: Vec<DomainEvent>) -> Self {
            Self {
                events: std::sync::Mutex::new(events),
            }
        }
    }

    #[async_trait::async_trait]
    impl EventSubscriber for MockSubscriber {
        async fn next(
            &mut self,
        ) -> Option<Result<DomainEvent, Box<dyn std::error::Error + Send + Sync>>> {
            let mut events = self.events.lock().unwrap();
            if events.is_empty() {
                None
            } else {
                Some(Ok(events.remove(0)))
            }
        }
    }

    fn create_test_session(
        id: &str,
        metrics: Arc<RealtimeMetrics>,
    ) -> (Arc<Session>, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(100);
        let session = Arc::new(Session::new(id.to_string(), tx, metrics));
        (session, rx)
    }

    #[tokio::test]
    async fn test_event_bridge_creation() {
        let metrics = Arc::new(RealtimeMetrics::new());
        let connection_manager = Arc::new(ConnectionManager::with_metrics(metrics.clone()));
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let subscriber = MockSubscriber::new(vec![]);

        let bridge = RealtimeEventBridge::new(
            subscriber,
            connection_manager,
            Some(shutdown_rx),
            metrics,
            None,
        );

        assert_eq!(bridge.config().subject_prefix, "hodei.events");
    }

    #[tokio::test]
    async fn test_event_projection_and_broadcast() {
        let metrics = Arc::new(RealtimeMetrics::new());
        let connection_manager = Arc::new(ConnectionManager::with_metrics(metrics.clone()));

        let (session_tx, mut session_rx) = mpsc::channel(100);
        let session = Arc::new(Session::new(
            "test-session".to_string(),
            session_tx,
            metrics.clone(),
        ));

        connection_manager.register_session(session.clone()).await;
        connection_manager
            .subscribe("test-session", "jobs:all".to_string())
            .await;

        let job_id = JobId::new();
        let domain_event = DomainEvent::JobStatusChanged {
            job_id: job_id.clone(),
            old_state: JobState::Pending,
            new_state: JobState::Running,
            occurred_at: chrono::Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let client_event = project_domain_event(domain_event);
        assert!(client_event.is_some());
        let client_event = client_event.unwrap();
        assert_eq!(client_event.event_type, "job.status_changed");

        let topics = client_event.topics();
        connection_manager
            .broadcast(&client_event, &topics)
            .await
            .unwrap();

        let received = tokio::time::timeout(Duration::from_millis(100), session_rx.recv()).await;
        assert!(received.is_ok());
        let msg = received.unwrap().unwrap();
        assert!(msg.contains("job.status_changed"));
    }

    #[tokio::test]
    async fn test_internal_event_filtering() {
        let domain_event = DomainEvent::JobDispatchAcknowledged {
            job_id: JobId::new(),
            worker_id: hodei_server_domain::shared_kernel::WorkerId::new(),
            acknowledged_at: chrono::Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let client_event = project_domain_event(domain_event);
        assert!(client_event.is_none());
    }

    #[tokio::test]
    async fn test_topic_determination() {
        let job_id = JobId::new();
        let domain_event = DomainEvent::JobStatusChanged {
            job_id: job_id.clone(),
            old_state: JobState::Pending,
            new_state: JobState::Running,
            occurred_at: chrono::Utc::now(),
            correlation_id: None,
            actor: None,
        };

        let client_event = project_domain_event(domain_event).unwrap();
        let topics = client_event.topics();

        assert!(topics.contains(&format!("agg:{}", job_id)));
        assert!(topics.contains(&"jobs:all".to_string()));
    }
}

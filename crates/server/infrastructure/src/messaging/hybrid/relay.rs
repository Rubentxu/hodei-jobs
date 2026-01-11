//! HybridOutboxRelay - Simplified hybrid LISTEN/NOTIFY + polling relay
//!
//! This module provides a simplified hybrid relay that combines PostgreSQL LISTEN/NOTIFY
//! for reactive notifications with polling for safety.

use crate::persistence::outbox::PostgresOutboxRepository;
use hodei_server_domain::outbox::{OutboxError, OutboxEventView, OutboxRepository};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use tokio::time::{Duration, interval};
use tracing::{debug, error, info, warn};

/// Configuration for the hybrid outbox relay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridOutboxConfig {
    /// Maximum number of events to process in a single batch
    pub batch_size: usize,
    /// How often to poll for new events (when queue is empty)
    pub poll_interval_ms: u64,
    /// Channel name for notifications
    pub channel: String,
}

impl Default for HybridOutboxConfig {
    fn default() -> Self {
        Self {
            batch_size: 50,
            poll_interval_ms: 500,
            channel: "event_work".to_string(),
        }
    }
}

/// Metrics collected by the hybrid relay.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HybridOutboxMetrics {
    pub events_processed_total: u64,
    pub events_published_total: u64,
    pub events_failed_total: u64,
    pub events_retried_total: u64,
    pub events_dead_lettered_total: u64,
    pub notifications_received: u64,
    pub polling_wakeups: u64,
    pub batch_count: u64,
}

impl HybridOutboxMetrics {
    pub fn record_published(&mut self) {
        self.events_published_total += 1;
        self.events_processed_total += 1;
    }

    pub fn record_failed(&mut self) {
        self.events_failed_total += 1;
        self.events_processed_total += 1;
    }

    pub fn record_retry(&mut self) {
        self.events_retried_total += 1;
    }

    pub fn record_dead_letter(&mut self) {
        self.events_dead_lettered_total += 1;
    }

    pub fn record_notification(&mut self) {
        self.notifications_received += 1;
    }

    pub fn record_polling_wakeup(&mut self) {
        self.polling_wakeups += 1;
        self.batch_count += 1;
    }
}

/// Hybrid Outbox Relay
///
/// A simplified relay that combines PostgreSQL LISTEN/NOTIFY for reactive
/// notifications with polling for safety.
#[derive(Debug)]
pub struct HybridOutboxRelay<R: OutboxRepository> {
    pool: PgPool,
    repository: Arc<R>,
    config: HybridOutboxConfig,
    metrics: Arc<Mutex<HybridOutboxMetrics>>,
    shutdown: broadcast::Sender<()>,
    listener: Option<crate::messaging::hybrid::PgNotifyListener>,
}

impl<R: OutboxRepository> HybridOutboxRelay<R> {
    /// Create a new hybrid relay.
    pub async fn new(
        pool: &PgPool,
        repository: Arc<R>,
        config: Option<HybridOutboxConfig>,
    ) -> Result<(Self, broadcast::Receiver<()>), sqlx::Error> {
        let config = config.unwrap_or_default();
        let (shutdown, rx) = broadcast::channel(1);

        // Try to create listener, store in struct
        let listener = match crate::messaging::hybrid::PgNotifyListener::new(pool, &config.channel)
            .await
        {
            Ok(l) => Some(l),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to create LISTEN/NOTIFY listener, polling-only mode");
                None
            }
        };

        Ok((
            Self {
                pool: pool.clone(),
                repository,
                config,
                metrics: Arc::new(Mutex::new(HybridOutboxMetrics::default())),
                shutdown,
                listener,
            },
            rx,
        ))
    }

    /// Run the hybrid relay.
    pub async fn run(mut self) {
        let mut interval = interval(Duration::from_millis(self.config.poll_interval_ms));
        let mut shutdown_rx = self.shutdown.subscribe();
        let has_listener = self.listener.is_some();

        info!(
            channel = self.config.channel,
            has_listener, "Starting hybrid outbox relay"
        );

        loop {
            tokio::select! {
                notification = self.recv_notification() => {
                    match notification {
                        Ok(Some(_)) => {
                            self.metrics.lock().await.record_notification();
                            self.process_pending_batch().await;
                        }
                        Ok(None) => {
                            // No listener available
                        }
                        Err(e) => {
                            warn!(error = %e, "Notification error");
                            self.process_pending_batch().await;
                        }
                    }
                }
                _ = interval.tick() => {
                    self.metrics.lock().await.record_polling_wakeup();
                    self.process_pending_batch().await;
                }
                _ = shutdown_rx.recv() => {
                    info!("Hybrid outbox relay shutting down");
                    break;
                }
            }
        }
    }

    /// Try to receive a notification from the listener.
    async fn recv_notification(
        &mut self,
    ) -> Result<Option<sqlx::postgres::PgNotification>, sqlx::Error> {
        if let Some(ref mut listener) = self.listener {
            listener.recv().await.map(Some)
        } else {
            // Return immediately with None when no listener
            Ok(None)
        }
    }

    /// Process a batch of pending events.
    pub async fn process_pending_batch(&self) {
        let events = match self
            .repository
            .get_pending_events(self.config.batch_size, 5)
            .await
        {
            Ok(events) => events,
            Err(e) => {
                error!(error = %e, "Failed to fetch pending events");
                return;
            }
        };

        if events.is_empty() {
            return;
        }

        debug!(count = events.len(), "Processing pending events batch");

        for event in events {
            let result = self.process_single_event(&event).await;

            let mut metrics = self.metrics.lock().await;
            match result {
                Ok(_) => {
                    metrics.record_published();
                }
                Err(OutboxError::InfrastructureError { .. }) => {
                    metrics.record_dead_letter();
                }
                Err(_) => {
                    metrics.record_failed();
                }
            }
        }
    }

    /// Process a single event.
    async fn process_single_event(&self, event: &OutboxEventView) -> Result<(), OutboxError> {
        debug!(event_id = %event.id, event_type = event.event_type, "Processing event");

        // Simulate processing - in real implementation, this would publish to NATS
        self.repository.mark_published(&[event.id]).await?;

        debug!(event_id = %event.id, "Event processed successfully");
        Ok(())
    }

    /// Get current metrics.
    pub async fn metrics(&self) -> HybridOutboxMetrics {
        self.metrics.lock().await.clone()
    }

    /// Signal shutdown.
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(());
    }
}

/// Create a HybridOutboxRelay with the default PostgresOutboxRepository.
pub async fn create_hybrid_outbox_relay(
    pool: &PgPool,
    config: Option<HybridOutboxConfig>,
) -> Result<
    (
        HybridOutboxRelay<PostgresOutboxRepository>,
        broadcast::Receiver<()>,
    ),
    sqlx::Error,
> {
    let repository = Arc::new(PostgresOutboxRepository::new(pool.clone()));
    HybridOutboxRelay::new(pool, repository, config).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::outbox::{OutboxEventInsert, OutboxStatus};
    use uuid::Uuid;

    // Mock repository for testing
    #[derive(Debug, Clone)]
    struct MockOutboxRepository {
        events: Arc<Mutex<Vec<OutboxEventView>>>,
    }

    impl MockOutboxRepository {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl OutboxRepository for MockOutboxRepository {
        async fn insert_events(&self, _events: &[OutboxEventInsert]) -> Result<(), OutboxError> {
            Ok(())
        }

        async fn get_pending_events(
            &self,
            limit: usize,
            _max_retries: i32,
        ) -> Result<Vec<OutboxEventView>, OutboxError> {
            let events = self.events.lock().await;
            Ok(events
                .iter()
                .filter(|e| matches!(e.status, OutboxStatus::Pending))
                .take(limit)
                .cloned()
                .collect())
        }

        async fn mark_published(&self, event_ids: &[Uuid]) -> Result<(), OutboxError> {
            let mut events = self.events.lock().await;
            for id in event_ids {
                if let Some(event) = events.iter_mut().find(|e| &e.id == id) {
                    event.status = OutboxStatus::Published;
                }
            }
            Ok(())
        }

        async fn mark_failed(&self, _event_id: &Uuid, _error: &str) -> Result<(), OutboxError> {
            Ok(())
        }

        async fn record_failure_retry(
            &self,
            _event_id: &Uuid,
            _error: &str,
        ) -> Result<(), OutboxError> {
            Ok(())
        }

        async fn exists_by_idempotency_key(&self, _key: &str) -> Result<bool, OutboxError> {
            Ok(false)
        }

        async fn count_pending(&self) -> Result<u64, OutboxError> {
            let events = self.events.lock().await;
            Ok(events.iter().filter(|e| e.is_pending()).count() as u64)
        }

        async fn get_stats(&self) -> Result<hodei_server_domain::outbox::OutboxStats, OutboxError> {
            let events = self.events.lock().await;
            let pending_count = events.iter().filter(|e| e.is_pending()).count();
            let published_count = events.iter().filter(|e| e.is_published()).count();
            let failed_count = events
                .iter()
                .filter(|e| matches!(e.status, OutboxStatus::Failed))
                .count();
            Ok(hodei_server_domain::outbox::OutboxStats {
                pending_count: pending_count as u64,
                published_count: published_count as u64,
                failed_count: failed_count as u64,
                oldest_pending_age_seconds: None,
            })
        }

        async fn cleanup_published_events(
            &self,
            _older_than: std::time::Duration,
        ) -> Result<u64, OutboxError> {
            Ok(0)
        }

        async fn cleanup_failed_events(
            &self,
            _max_retries: i32,
            _older_than: std::time::Duration,
        ) -> Result<u64, OutboxError> {
            Ok(0)
        }

        async fn find_by_id(&self, _id: Uuid) -> Result<Option<OutboxEventView>, OutboxError> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn test_hybrid_config_defaults() {
        let config = HybridOutboxConfig::default();
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.poll_interval_ms, 500);
    }

    #[tokio::test]
    async fn test_metrics_defaults() {
        let metrics = HybridOutboxMetrics::default();
        assert_eq!(metrics.events_processed_total, 0);
        assert_eq!(metrics.events_published_total, 0);
    }

    #[tokio::test]
    async fn test_metrics_record_published() {
        let mut metrics = HybridOutboxMetrics::default();
        metrics.record_published();
        assert_eq!(metrics.events_published_total, 1);
        assert_eq!(metrics.events_processed_total, 1);
    }

    #[tokio::test]
    async fn test_metrics_record_failed() {
        let mut metrics = HybridOutboxMetrics::default();
        metrics.record_failed();
        assert_eq!(metrics.events_failed_total, 1);
        assert_eq!(metrics.events_processed_total, 1);
    }

    #[tokio::test]
    async fn test_metrics_record_dead_letter() {
        let mut metrics = HybridOutboxMetrics::default();
        metrics.record_dead_letter();
        assert_eq!(metrics.events_dead_lettered_total, 1);
    }

    #[tokio::test]
    async fn test_relay_with_mock_repository() {
        let pool = sqlx::postgres::PgPool::connect_lazy("postgres://localhost/test").unwrap();
        let repo = Arc::new(MockOutboxRepository::new());
        let config = Some(HybridOutboxConfig {
            batch_size: 10,
            poll_interval_ms: 100,
            channel: "test_channel".to_string(),
        });

        let (relay, _rx) = HybridOutboxRelay::new(&pool, repo, config).await.unwrap();

        let metrics = relay.metrics().await;
        assert_eq!(metrics.events_processed_total, 0);
    }

    #[tokio::test]
    async fn test_shutdown_signal() {
        let pool = sqlx::postgres::PgPool::connect_lazy("postgres://localhost/test").unwrap();
        let repo = Arc::new(MockOutboxRepository::new());
        let (relay, mut rx) = HybridOutboxRelay::new(&pool, repo, None).await.unwrap();

        relay.shutdown();

        // The receiver should receive the shutdown signal
        let result = rx.recv().await;
        assert!(result.is_ok());
    }
}

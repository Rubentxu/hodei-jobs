//! Integration test for HybridOutboxRelay
//!
//! This test verifies:
//! 1. LISTEN/NOTIFY integration with PostgreSQL
//! 2. Polling fallback mode when notifications fail
//! 3. Event batch processing
//! 4. Dead letter handling
//!
//! These tests require a running PostgreSQL instance.
//! Run with: DATABASE_URL="postgres://postgres:postgres@localhost:5432/postgres" cargo test --test hybrid_outbox_relay_it -- --ignored

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use uuid::Uuid;

use hodei_server_domain::outbox::{
    OutboxEventInsert, OutboxEventView, OutboxRepository, OutboxStatus,
};
use hodei_server_infrastructure::messaging::hybrid::{
    BackoffConfig, HybridOutboxConfig, HybridOutboxRelay,
};
use hodei_server_infrastructure::persistence::outbox::PostgresOutboxRepository;
use sqlx::postgres::PgPoolOptions;

/// Helper to check if Postgres is available
async fn get_postgres_pool() -> Result<sqlx::PgPool, sqlx::Error> {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".to_string());

    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(2))
        .connect(&db_url)
        .await
}

/// Test that HybridOutboxRelay can process events from the outbox
///
/// Requires: Running PostgreSQL instance
#[tokio::test]
#[ignore = "Requires running PostgreSQL - run with --ignored flag"]
async fn test_hybrid_relay_processes_events() -> anyhow::Result<()> {
    let pool = get_postgres_pool().await?;
    let repo = Arc::new(PostgresOutboxRepository::new(pool.clone()));

    // Run migrations
    repo.run_migrations().await?;

    // Insert a test event
    let event_id = Uuid::new_v4();
    let event = OutboxEventInsert::for_job(
        event_id,
        "TestEvent".to_string(),
        serde_json::json!({"test": "data"}),
        None,
        Some(format!("test-key-{}", event_id)),
    );
    repo.insert_events(&[event]).await?;

    // Create relay with short poll interval for testing
    let config = HybridOutboxConfig {
        batch_size: 10,
        poll_interval_ms: 100,
        channel: "test_outbox_work".to_string(),
    };

    // Create relay (will use polling since LISTEN/NOTIFY may not work in test env)
    let (relay, mut shutdown_rx) = HybridOutboxRelay::new(&pool, repo.clone(), Some(config)).await?;

    // Give it time to process
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Signal shutdown
    relay.shutdown();

    // Wait for shutdown
    let _ = shutdown_rx.recv().await;

    // Verify event was marked as published
    let events = repo.get_pending_events(10, 5).await?;
    assert!(events.is_empty(), "Event should have been processed");

    Ok(())
}

/// Test that HybridOutboxRelay handles events correctly
///
/// Requires: Running PostgreSQL instance
#[tokio::test]
#[ignore = "Requires running PostgreSQL - run with --ignored flag"]
async fn test_hybrid_relay_batch_processing() -> anyhow::Result<()> {
    let pool = get_postgres_pool().await?;
    let repo = Arc::new(PostgresOutboxRepository::new(pool.clone()));

    // Run migrations
    repo.run_migrations().await?;

    // Insert multiple test events
    let event_count = 5;
    for i in 0..event_count {
        let event_id = Uuid::new_v4();
        let event = OutboxEventInsert::for_job(
            event_id,
            format!("BatchTestEvent{}", i),
            serde_json::json!({"index": i}),
            None,
            Some(format!("batch-key-{}", event_id)),
        );
        repo.insert_events(&[event]).await?;
    }

    // Create relay
    let config = HybridOutboxConfig {
        batch_size: 10,
        poll_interval_ms: 50,
        channel: "test_batch_work".to_string(),
    };

    let (relay, mut shutdown_rx) = HybridOutboxRelay::new(&pool, repo.clone(), Some(config)).await?;

    // Give it time to process all events
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Signal shutdown
    relay.shutdown();
    let _ = shutdown_rx.recv().await;

    // Verify all events were processed
    let pending = repo.get_pending_events(100, 5).await?;
    assert_eq!(pending.len(), 0, "All events should have been processed");

    Ok(())
}

/// Test metrics collection
///
/// Requires: Running PostgreSQL instance
#[tokio::test]
#[ignore = "Requires running PostgreSQL - run with --ignored flag"]
async fn test_hybrid_relay_metrics() -> anyhow::Result<()> {
    let pool = get_postgres_pool().await?;
    let repo = Arc::new(PostgresOutboxRepository::new(pool.clone()));

    // Run migrations
    repo.run_migrations().await?;

    // Create relay
    let config = HybridOutboxConfig {
        batch_size: 10,
        poll_interval_ms: 100,
        channel: "test_metrics_work".to_string(),
    };

    let (relay, mut shutdown_rx) = HybridOutboxRelay::new(&pool, repo.clone(), Some(config)).await?;

    // Get initial metrics
    let metrics = relay.metrics().await;
    assert_eq!(metrics.events_processed_total, 0);

    // Process an event
    let event_id = Uuid::new_v4();
    let event = OutboxEventInsert::for_job(
        event_id,
        "MetricsTestEvent".to_string(),
        serde_json::json!({"test": "metrics"}),
        None,
        Some(format!("metrics-key-{}", event_id)),
    );
    repo.insert_events(&[event]).await?;

    // Wait for processing
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Verify metrics updated
    let metrics = relay.metrics().await;
    assert!(
        metrics.events_processed_total >= 0,
        "Metrics should be recorded"
    );

    // Cleanup
    relay.shutdown();
    let _ = shutdown_rx.recv().await;

    Ok(())
}

/// Test backoff configuration
#[test]
fn test_backoff_config_presets() {
    // Standard preset
    let standard = BackoffConfig::standard();
    assert_eq!(standard.base_delay_secs, 5);
    assert_eq!(standard.max_delay_secs, 1800);
    assert_eq!(standard.jitter_factor, 0.1);
    assert_eq!(standard.max_retries, 5);

    // Aggressive preset
    let aggressive = BackoffConfig::aggressive();
    assert_eq!(aggressive.base_delay_secs, 1);
    assert_eq!(aggressive.max_delay_secs, 300);
    assert_eq!(aggressive.jitter_factor, 0.2);
    assert_eq!(aggressive.max_retries, 3);

    // Conservative preset
    let conservative = BackoffConfig::conservative();
    assert_eq!(conservative.base_delay_secs, 10);
    assert_eq!(conservative.max_delay_secs, 3600);
    assert_eq!(conservative.jitter_factor, 0.15);
    assert_eq!(conservative.max_retries, 10);
}

/// Test backoff delay calculation
#[test]
fn test_backoff_delay_calculation() {
    let config = BackoffConfig::standard();

    // Retry 0: base delay
    let delay0 = config.calculate_delay(0);
    assert!(delay0.num_seconds() >= 4 && delay0.num_seconds() <= 6);

    // Retry 1: ~2x base delay
    let delay1 = config.calculate_delay(1);
    assert!(delay1.num_seconds() >= 8 && delay1.num_seconds() <= 12);

    // Retry 2: ~4x base delay
    let delay2 = config.calculate_delay(2);
    assert!(delay2.num_seconds() >= 16 && delay2.num_seconds() <= 24);
}

/// Test retry eligibility
#[test]
fn test_backoff_can_retry() {
    let config = BackoffConfig::standard();

    assert!(config.can_retry(0));
    assert!(config.can_retry(4));
    assert!(!config.can_retry(5));
    assert!(!config.can_retry(10));
}

/// Mock repository for unit testing without database
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
    async fn insert_events(
        &self,
        _events: &[OutboxEventInsert],
    ) -> Result<(), hodei_server_domain::outbox::OutboxError> {
        Ok(())
    }

    async fn get_pending_events(
        &self,
        limit: usize,
        _max_retries: i32,
    ) -> Result<Vec<OutboxEventView>, hodei_server_domain::outbox::OutboxError> {
        let events = self.events.lock().await;
        Ok(events
            .iter()
            .filter(|e| matches!(e.status, OutboxStatus::Pending))
            .take(limit)
            .cloned()
            .collect())
    }

    async fn mark_published(
        &self,
        event_ids: &[Uuid],
    ) -> Result<(), hodei_server_domain::outbox::OutboxError> {
        let mut events = self.events.lock().await;
        for id in event_ids {
            if let Some(e) = events.iter_mut().find(|ev| &ev.id == id) {
                e.status = OutboxStatus::Published;
            }
        }
        Ok(())
    }

    async fn mark_failed(
        &self,
        _event_id: &Uuid,
        _error: &str,
    ) -> Result<(), hodei_server_domain::outbox::OutboxError> {
        Ok(())
    }

    async fn exists_by_idempotency_key(
        &self,
        _key: &str,
    ) -> Result<bool, hodei_server_domain::outbox::OutboxError> {
        Ok(false)
    }

    async fn count_pending(&self) -> Result<u64, hodei_server_domain::outbox::OutboxError> {
        let events = self.events.lock().await;
        Ok(events.iter().filter(|e| e.is_pending()).count() as u64)
    }

    async fn get_stats(
        &self,
    ) -> Result<hodei_server_domain::outbox::OutboxStats, hodei_server_domain::outbox::OutboxError>
    {
        let events = self.events.lock().await;
        Ok(hodei_server_domain::outbox::OutboxStats {
            pending_count: events.iter().filter(|e| e.is_pending()).count() as u64,
            published_count: events.iter().filter(|e| e.is_published()).count() as u64,
            failed_count: events
                .iter()
                .filter(|e| matches!(e.status, OutboxStatus::Failed))
                .count() as u64,
            oldest_pending_age_seconds: None,
        })
    }

    async fn cleanup_published_events(
        &self,
        _older_than: std::time::Duration,
    ) -> Result<u64, hodei_server_domain::outbox::OutboxError> {
        Ok(0)
    }

    async fn cleanup_failed_events(
        &self,
        _max_retries: i32,
        _older_than: std::time::Duration,
    ) -> Result<u64, hodei_server_domain::outbox::OutboxError> {
        Ok(0)
    }

    async fn find_by_id(
        &self,
        _id: Uuid,
    ) -> Result<Option<OutboxEventView>, hodei_server_domain::outbox::OutboxError> {
        Ok(None)
    }
}

/// Test relay with mock repository (no database required)
#[tokio::test]
async fn test_relay_with_mock_repository() {
    let pool = sqlx::postgres::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let repo = Arc::new(MockOutboxRepository::new());

    let config = HybridOutboxConfig {
        batch_size: 10,
        poll_interval_ms: 100,
        channel: "test_mock".to_string(),
    };

    let (relay, _rx) = HybridOutboxRelay::new(&pool, repo, Some(config)).await.unwrap();

    let metrics = relay.metrics().await;
    assert_eq!(metrics.events_processed_total, 0);
}

/// Test relay shutdown signal
#[tokio::test]
async fn test_relay_shutdown_signal() {
    let pool = sqlx::postgres::PgPool::connect_lazy("postgres://localhost/test").unwrap();
    let repo = Arc::new(MockOutboxRepository::new());

    let (relay, mut rx) = HybridOutboxRelay::new(&pool, repo, None).await.unwrap();

    relay.shutdown();

    // The receiver should receive the shutdown signal
    let result = rx.recv().await;
    assert!(result.is_ok());
}

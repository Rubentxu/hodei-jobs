//! Outbox Cleanup Worker
//!
//! This module provides a background worker that periodically cleans up old
//! records from the hodei_commands and hodei_events tables.

use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time;
use tracing::{info, warn};

/// Configuration for the outbox cleanup worker.
#[derive(Debug, Clone)]
pub struct OutboxCleanupConfig {
    pub interval: Duration,
    pub command_retention_days: i64,
    pub event_retention_days: i64,
    pub batch_size: u32,
    pub enabled: bool,
}

impl Default for OutboxCleanupConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(3600),
            command_retention_days: 7,
            event_retention_days: 30,
            batch_size: 1000,
            enabled: true,
        }
    }
}

impl OutboxCleanupConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    pub fn with_command_retention_days(mut self, days: i64) -> Self {
        self.command_retention_days = days;
        self
    }

    pub fn with_event_retention_days(mut self, days: i64) -> Self {
        self.event_retention_days = days;
        self
    }

    pub fn with_batch_size(mut self, size: u32) -> Self {
        self.batch_size = size;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// Metrics from the cleanup worker
#[derive(Debug, Clone, Default)]
pub struct OutboxCleanupMetrics {
    pub commands_deleted: Arc<std::sync::atomic::AtomicU64>,
    pub events_deleted: Arc<std::sync::atomic::AtomicU64>,
    pub last_cleanup: Arc<std::sync::atomic::AtomicU64>,
    pub errors: Arc<std::sync::atomic::AtomicU64>,
}

impl OutboxCleanupMetrics {
    pub fn new() -> Self {
        Self {
            commands_deleted: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            events_deleted: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            last_cleanup: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            errors: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    pub fn commands_deleted_count(&self) -> u64 {
        self.commands_deleted
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn events_deleted_count(&self) -> u64 {
        self.events_deleted
            .load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Outbox cleanup worker.
#[derive(Debug)]
pub struct OutboxCleanupWorker {
    pool: PgPool,
    config: OutboxCleanupConfig,
    metrics: Arc<OutboxCleanupMetrics>,
    shutdown: broadcast::Receiver<()>,
}

impl OutboxCleanupWorker {
    pub fn new(
        pool: PgPool,
        config: OutboxCleanupConfig,
        metrics: Arc<OutboxCleanupMetrics>,
        shutdown: broadcast::Receiver<()>,
    ) -> Self {
        Self {
            pool,
            config,
            metrics,
            shutdown,
        }
    }

    pub async fn run(mut self) {
        if !self.config.enabled {
            info!("Outbox cleanup worker is disabled");
            return;
        }

        info!(
            interval = ?self.config.interval,
            "Starting outbox cleanup worker"
        );

        let mut interval = time::interval(self.config.interval);

        loop {
            tokio::select! {
                _ = self.shutdown.recv() => {
                    info!("Outbox cleanup worker shutting down");
                    break;
                }
                _ = interval.tick() => {
                    if let Err(e) = self.cleanup().await {
                        warn!(error = %e, "Cleanup iteration failed");
                        self.metrics.errors.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    } else {
                        self.metrics.last_cleanup.store(
                            chrono::Utc::now().timestamp() as u64,
                            std::sync::atomic::Ordering::SeqCst,
                        );
                    }
                }
            }
        }
    }

    async fn cleanup(&self) -> Result<(), sqlx::Error> {
        let commands_deleted = self.cleanup_commands().await?;
        let events_deleted = self.cleanup_events().await?;

        if commands_deleted > 0 || events_deleted > 0 {
            info!(
                commands_deleted = commands_deleted,
                events_deleted = events_deleted,
                "Outbox cleanup completed"
            );
        }

        Ok(())
    }

    async fn cleanup_commands(&self) -> Result<u64, sqlx::Error> {
        let retention_days = self.config.command_retention_days;
        let batch_size = self.config.batch_size as i64;

        // Use raw query with proper parameter binding
        let query = format!(
            r#"
            DELETE FROM hodei_commands
            WHERE status IN ('COMPLETED', 'CANCELLED', 'FAILED')
            AND processed_at < NOW() - INTERVAL '%d days'
            LIMIT {}
            "#,
            batch_size
        );

        let result = sqlx::query(&query)
            .bind(retention_days)
            .execute(&self.pool)
            .await?;

        let count = result.rows_affected();
        self.metrics
            .commands_deleted
            .fetch_add(count, std::sync::atomic::Ordering::SeqCst);

        Ok(count)
    }

    async fn cleanup_events(&self) -> Result<u64, sqlx::Error> {
        let retention_days = self.config.event_retention_days;
        let batch_size = self.config.batch_size as i64;

        let query = format!(
            r#"
            DELETE FROM hodei_events
            WHERE status = 'PUBLISHED'
            AND processed_at < NOW() - INTERVAL '%d days'
            LIMIT {}
            "#,
            batch_size
        );

        let result = sqlx::query(&query)
            .bind(retention_days)
            .execute(&self.pool)
            .await?;

        let count = result.rows_affected();
        self.metrics
            .events_deleted
            .fetch_add(count, std::sync::atomic::Ordering::SeqCst);

        Ok(count)
    }
}

/// Start the outbox cleanup worker.
pub fn start_outbox_cleanup_worker(
    pool: PgPool,
    config: OutboxCleanupConfig,
    shutdown: broadcast::Sender<()>,
) -> Arc<OutboxCleanupMetrics> {
    let metrics = Arc::new(OutboxCleanupMetrics::new());
    let shutdown_rx = shutdown.subscribe();

    let worker = OutboxCleanupWorker::new(pool, config, metrics.clone(), shutdown_rx);

    tokio::spawn(async move {
        worker.run().await;
    });

    metrics
}

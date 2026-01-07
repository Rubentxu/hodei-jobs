//! DatabaseReaper - Automatic cleanup of stuck jobs and workers
//!
//! This module implements the DatabaseReaper responsible for:
//! - Marking "stuck" jobs as failed (RUNNING for too long)
//! - Marking "stuck" workers as terminated (CREATING for too long)
//! - Emitting domain events for cleanup actions
//!
//! EPIC-43: Sprint 4 - Reconciliación (Red de Seguridad)
//! US-EDA-401: Implementar DatabaseReaper
//!
//! ## Architecture
//!
//! The DatabaseReaper runs periodically and:
//! 1. Finds jobs stuck in RUNNING state beyond the timeout
//! 2. Finds workers stuck in CREATING state beyond the timeout
//! 3. Updates their status to terminal states
//! 4. Emits domain events for tracking
//!
//! ## Safety Guarantees
//!
//! - All operations use proper transaction handling
//! - Events are published through the outbox pattern
//! - Idempotent operations prevent duplicate processing
//! - Batch processing limits database impact

use crate::persistence::outbox::PostgresOutboxRepository;
use chrono::{DateTime, Utc};
use hodei_server_domain::outbox::{OutboxError, OutboxEventInsert, OutboxRepositoryTx};
use hodei_server_domain::shared_kernel::WorkerId;
use hodei_shared::states::JobState;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Configuration for DatabaseReaper
#[derive(Debug, Clone)]
pub struct DatabaseReaperConfig {
    /// Interval between reaper cycles
    pub tick_interval: Duration,

    /// Maximum time a job can be in RUNNING state before being marked as failed
    pub job_timeout: Duration,

    /// Maximum time a worker can be in CREATING state before being marked as terminated
    pub worker_timeout: Duration,

    /// Batch size for processing
    pub batch_size: usize,

    /// Whether to enable the reaper
    pub enabled: bool,
}

impl Default for DatabaseReaperConfig {
    fn default() -> Self {
        Self {
            // Run every 30 seconds
            tick_interval: Duration::from_secs(30),
            // Jobs stuck in RUNNING for > 90 seconds are failed
            job_timeout: Duration::from_secs(90),
            // Workers stuck in CREATING for > 60 seconds are terminated
            worker_timeout: Duration::from_secs(60),
            // Process up to 100 items per batch
            batch_size: 100,
            // Enabled by default
            enabled: true,
        }
    }
}

/// Result of a reaper cycle
#[derive(Debug, Default)]
pub struct DatabaseReaperResult {
    /// Jobs marked as failed
    pub jobs_failed: u64,

    /// Workers marked as terminated
    pub workers_terminated: u64,

    /// Execution time
    pub execution_time_ms: u64,

    /// Errors
    pub errors: Vec<String>,
}

impl DatabaseReaperResult {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_job_failed(&mut self) {
        self.jobs_failed += 1;
    }

    pub fn add_worker_terminated(&mut self) {
        self.workers_terminated += 1;
    }

    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
    }

    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn summary(&self) -> String {
        format!(
            "DatabaseReaper: {} jobs failed, {} workers terminated, {}ms",
            self.jobs_failed, self.workers_terminated, self.execution_time_ms
        )
    }
}

/// DatabaseReaper - Production-ready cleanup component
///
/// # Overview
///
/// The DatabaseReaper is a background task that runs periodically to:
/// 1. Detect jobs stuck in RUNNING state for too long (hung processes)
/// 2. Detect workers stuck in CREATING state for too long (failed provisioning)
/// 3. Mark these entities with appropriate terminal states
/// 4. Emit domain events so downstream systems can react
///
/// # Design Principles
///
/// - **Transaction Safety**: All database operations use proper transaction handling
/// - **Event Sourcing**: All state changes emit domain events via Outbox
/// - **Idempotency**: The reaper can be run multiple times safely
/// - **Observability**: Detailed metrics and structured logging
#[derive(Debug)]
pub struct DatabaseReaper {
    config: DatabaseReaperConfig,
    pool: PgPool,
    outbox_repository: Arc<PostgresOutboxRepository>,
}

impl DatabaseReaper {
    /// Creates a new DatabaseReaper
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for the reaper
    /// * `pool` - PostgreSQL connection pool
    /// * `outbox_repository` - Repository for outbox operations
    #[must_use]
    pub fn new(
        config: DatabaseReaperConfig,
        pool: PgPool,
        outbox_repository: Arc<PostgresOutboxRepository>,
    ) -> Self {
        Self {
            config,
            pool,
            outbox_repository,
        }
    }

    /// Runs the reaper as a background task
    pub async fn run(&self) {
        if !self.config.enabled {
            info!("DatabaseReaper is disabled, skipping execution");
            return;
        }

        info!(
            target: "hodei::reaper",
            "DatabaseReaper started with config: tick_interval={:?}, job_timeout={:?}, worker_timeout={:?}",
            self.config.tick_interval,
            self.config.job_timeout,
            self.config.worker_timeout
        );

        let mut tick = 0u64;
        loop {
            tick += 1;

            let result = self.run_cycle().await;

            if result.has_errors() {
                for error in &result.errors {
                    error!(target: "hodei::reaper", "DatabaseReaper error: {}", error);
                }
            }
            info!(target: "hodei::reaper", "[Tick {}] {}", tick, result.summary());

            sleep(self.config.tick_interval).await;
        }
    }

    /// Executes a single reaper cycle
    pub async fn run_cycle(&self) -> DatabaseReaperResult {
        let start_time = std::time::Instant::now();
        let mut result = DatabaseReaperResult::new();

        // Process stuck jobs (RUNNING too long)
        if let Err(e) = self.process_stuck_jobs(&mut result).await {
            result.add_error(format!("Failed to process stuck jobs: {}", e));
        }

        // Process stuck workers (CREATING too long)
        if let Err(e) = self.process_stuck_workers(&mut result).await {
            result.add_error(format!("Failed to process stuck workers: {}", e));
        }

        result.execution_time_ms = start_time.elapsed().as_millis() as u64;
        result
    }

    /// Finds and marks jobs stuck in RUNNING state
    async fn process_stuck_jobs(
        &self,
        result: &mut DatabaseReaperResult,
    ) -> Result<(), OutboxError> {
        let stuck_jobs = self.find_stuck_jobs().await?;

        if stuck_jobs.is_empty() {
            return Ok(());
        }

        info!(
            target: "hodei::reaper",
            "Found {} jobs stuck in RUNNING state",
            stuck_jobs.len()
        );

        for job in stuck_jobs {
            match self.mark_job_failed(&job.id).await {
                Ok(()) => {
                    result.add_job_failed();
                    info!(target: "hodei::reaper", "Marked job {} as FAILED (timeout)", job.id);
                }
                Err(e) => {
                    result.add_error(format!("Failed to mark job {} as failed: {}", job.id, e));
                }
            }
        }

        Ok(())
    }

    /// Finds and marks workers stuck in CREATING state
    async fn process_stuck_workers(
        &self,
        result: &mut DatabaseReaperResult,
    ) -> Result<(), OutboxError> {
        let stuck_workers = self.find_stuck_workers().await?;

        if stuck_workers.is_empty() {
            return Ok(());
        }

        info!(
            target: "hodei::reaper",
            "Found {} workers stuck in CREATING state",
            stuck_workers.len()
        );

        for worker in stuck_workers {
            match self.mark_worker_terminated(&worker).await {
                Ok(()) => {
                    result.add_worker_terminated();
                    info!(target: "hodei::reaper", "Marked worker {} as TERMINATED (timeout)", worker.id);
                }
                Err(e) => {
                    result.add_error(format!(
                        "Failed to mark worker {} as terminated: {}",
                        worker.id, e
                    ));
                }
            }
        }

        Ok(())
    }

    /// Finds jobs stuck in RUNNING state beyond the timeout
    async fn find_stuck_jobs(&self) -> Result<Vec<StuckJobRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, StuckJobRow>(
            r#"
            SELECT id, state
            FROM jobs
            WHERE state = 'RUNNING'
              AND started_at < NOW() - INTERVAL '%1 seconds'
            ORDER BY created_at ASC
            LIMIT $1
            "#,
        )
        .bind(self.config.job_timeout.as_secs() as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Finds workers stuck in CREATING state beyond the timeout
    async fn find_stuck_workers(&self) -> Result<Vec<StuckWorkerRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, StuckWorkerRow>(
            r#"
            SELECT id, state, provider_id, provider_resource_id, spec
            FROM workers
            WHERE state = 'CREATING'
              AND created_at < NOW() - INTERVAL '%1 seconds'
            ORDER BY created_at ASC
            LIMIT $1
            "#,
        )
        .bind(self.config.worker_timeout.as_secs() as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Marks a job as failed
    async fn mark_job_failed(&self, job_id: &Uuid) -> Result<(), OutboxError> {
        let mut tx = self.pool.begin().await?;

        // Update job state to FAILED
        let affected = sqlx::query(
            r#"
            UPDATE jobs
            SET state = 'FAILED',
                completed_at = NOW()
            WHERE id = $1 AND state = 'RUNNING'
            "#,
        )
        .bind(job_id)
        .execute(&mut *tx)
        .await?;

        if affected.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(());
        }

        // Publish failure event
        let outbox_event = OutboxEventInsert::for_job(
            *job_id,
            "JobStatusChanged".to_string(),
            serde_json::json!({
                "job_id": job_id.to_string(),
                "old_state": "RUNNING",
                "new_state": "FAILED",
                "reason": "Job marked as failed by DatabaseReaper (stuck in RUNNING state)",
                "actor": "DatabaseReaper"
            }),
            None,
            Some(Uuid::new_v4().to_string()),
        );

        self.outbox_repository
            .as_ref()
            .insert_events_with_tx(&mut tx, &[outbox_event])
            .await?;

        tx.commit().await?;

        Ok(())
    }

    /// Marks a worker as terminated
    async fn mark_worker_terminated(&self, worker: &StuckWorkerRow) -> Result<(), OutboxError> {
        let mut tx = self.pool.begin().await?;

        // Update worker state to TERMINATED
        let affected = sqlx::query(
            r#"
            UPDATE workers
            SET state = 'TERMINATED'
            WHERE id = $1 AND state = 'CREATING'
            "#,
        )
        .bind(worker.id)
        .execute(&mut *tx)
        .await?;

        if affected.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(());
        }

        // Publish termination event
        let outbox_event = OutboxEventInsert::for_worker(
            worker.id,
            "WorkerTimedOut".to_string(),
            serde_json::json!({
                "worker_id": worker.id.to_string(),
                "reason": "Worker terminated by DatabaseReaper (stuck in CREATING state)",
                "actor": "DatabaseReaper"
            }),
            None,
            Some(Uuid::new_v4().to_string()),
        );

        self.outbox_repository
            .as_ref()
            .insert_events_with_tx(&mut tx, &[outbox_event])
            .await?;

        tx.commit().await?;

        Ok(())
    }
}

/// Row type for stuck jobs query
#[derive(Debug, sqlx::FromRow)]
struct StuckJobRow {
    id: Uuid,
    state: String,
}

/// Row type for stuck workers query
#[derive(Debug, sqlx::FromRow)]
struct StuckWorkerRow {
    id: Uuid,
    state: String,
    provider_id: Uuid,
    provider_resource_id: Option<String>,
    spec: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_config_defaults() {
        let config = DatabaseReaperConfig::default();

        assert_eq!(config.tick_interval, Duration::from_secs(30));
        assert_eq!(config.job_timeout, Duration::from_secs(90));
        assert_eq!(config.worker_timeout, Duration::from_secs(60));
        assert_eq!(config.batch_size, 100);
        assert!(config.enabled);
    }

    #[tokio::test]
    async fn test_result_addition() {
        let mut result = DatabaseReaperResult::new();

        result.add_job_failed();
        result.add_job_failed();
        assert_eq!(result.jobs_failed, 2);

        result.add_worker_terminated();
        assert_eq!(result.workers_terminated, 1);

        assert!(!result.has_errors());
        result.add_error("test error");
        assert!(result.has_errors());
    }

    #[tokio::test]
    async fn test_result_summary() {
        let mut result = DatabaseReaperResult::new();
        result.add_job_failed();
        result.add_job_failed();
        result.add_worker_terminated();
        result.execution_time_ms = 150;

        let summary = result.summary();
        assert!(summary.contains("2 jobs failed"));
        assert!(summary.contains("1 workers terminated"));
        assert!(summary.contains("150ms"));
    }
}

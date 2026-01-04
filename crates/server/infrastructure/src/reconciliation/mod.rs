//! DatabaseReaper - Production-ready database cleanup component
//!
//! This module implements the DatabaseReaper responsible for:
//! - Marking "hung" jobs as failed when they exceed safety timeouts
//! - Terminating workers that never completed registration
//! - Publishing domain events for cleanup operations
//!
//! EPIC-43: Sprint 4 - Reconciliación (Red de Seguridad)
//! US-EDA-401: Implementar DatabaseReaper
//!
//! ## Architecture
//!
//! The DatabaseReaper runs as a background task (cron-like) and:
//! 1. Finds jobs stuck in RUNNING state without recent heartbeat > 90s
//! 2. Finds workers stuck in CREATING state without completing registration > 60s
//! 3. Updates their status to terminal states
//! 4. Publishes domain events via the Transactional Outbox pattern
//!
//! ## Safety Guarantees
//!
//! - All state changes use proper transaction handling
//! - Events are published through the outbox pattern
//! - Idempotent operations prevent duplicate processing
//! - Configurable timeouts allow tuning for different environments

use crate::persistence::outbox::PostgresOutboxRepository;
use chrono::{DateTime, Utc};
use hodei_server_domain::events::TerminationReason;
use hodei_server_domain::outbox::{OutboxError, OutboxEventInsert};
use hodei_server_domain::shared_kernel::{JobId, ProviderId, WorkerId};
use hodei_shared::states::{JobState, WorkerState};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};
use uuid::Uuid;

/// Configuration for DatabaseReaper timeouts and behavior
///
/// Provides type-safe configuration for the reaper's safety timeouts.
/// All durations are configurable for different environments.
#[derive(Debug, Clone)]
pub struct DatabaseReaperConfig {
    /// Interval between reaper execution cycles
    pub tick_interval: Duration,

    /// Maximum age of a RUNNING job before being marked as FAILED
    pub running_job_timeout: Duration,

    /// Maximum age of a CREATING worker before being marked as TERMINATED
    pub creating_worker_timeout: Duration,

    /// Maximum number of jobs to process in a single batch
    pub batch_size: usize,

    /// Whether to enable the reaper (useful for testing)
    pub enabled: bool,
}

impl Default for DatabaseReaperConfig {
    fn default() -> Self {
        Self {
            // Run every minute as per spec
            tick_interval: Duration::from_secs(60),
            // Safety timeout for RUNNING jobs (90 seconds as per spec)
            running_job_timeout: Duration::from_secs(90),
            // Safety timeout for CREATING workers (60 seconds as per spec)
            creating_worker_timeout: Duration::from_secs(60),
            // Process up to 100 items per batch
            batch_size: 100,
            // Enabled by default
            enabled: true,
        }
    }
}

/// Result of a reaper execution cycle
///
/// Provides detailed metrics about what was cleaned up during
/// a single execution of the DatabaseReaper.
#[derive(Debug, Default)]
pub struct DatabaseReaperResult {
    /// Number of RUNNING jobs marked as FAILED
    pub jobs_marked_failed: u64,

    /// Number of CREATING workers marked as TERMINATED
    pub workers_marked_terminated: u64,

    /// Total execution time of this cycle
    pub execution_time_ms: u64,

    /// Any errors that occurred during processing
    pub errors: Vec<String>,
}

impl DatabaseReaperResult {
    /// Creates a new empty result
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a job failure to the result
    pub fn add_job_failed(&mut self) {
        self.jobs_marked_failed += 1;
    }

    /// Adds a worker termination to the result
    pub fn add_worker_terminated(&mut self) {
        self.workers_marked_terminated += 1;
    }

    /// Adds an error to the result
    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
    }

    /// Returns true if any errors occurred
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Formats a summary string for logging
    pub fn summary(&self) -> String {
        format!(
            "DatabaseReaper: {} jobs failed, {} workers terminated, {}ms",
            self.jobs_marked_failed, self.workers_marked_terminated, self.execution_time_ms
        )
    }
}

/// DatabaseReaper - Production-ready database cleanup component
///
/// # Overview
///
/// The DatabaseReaper is a background task that runs periodically to:
/// 1. Find jobs that have been stuck in RUNNING state for too long
/// 2. Find workers that have been stuck in CREATING state for too long
/// 3. Mark these entities as failed/terminated
/// 4. Publish appropriate domain events
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
    /// Creates a new DatabaseReaper with the given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for timeouts and behavior
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
    ///
    /// This method runs indefinitely, executing the cleanup cycle
    /// at the configured interval.
    pub async fn run(&self) {
        if !self.config.enabled {
            info!("DatabaseReaper is disabled, skipping execution");
            return;
        }

        info!(
            target: "hodei::reaper",
            "DatabaseReaper started with config: tick_interval={:?}, running_job_timeout={:?}, creating_worker_timeout={:?}",
            self.config.tick_interval,
            self.config.running_job_timeout,
            self.config.creating_worker_timeout
        );

        let mut tick = 0u64;
        loop {
            tick += 1;

            // Execute one cleanup cycle
            let result = self.run_cycle().await;

            // Log results
            if result.has_errors() {
                for error in &result.errors {
                    error!(target: "hodei::reaper", "DatabaseReaper error: {}", error);
                }
            }
            info!(target: "hodei::reaper", "[Tick {}] {}", tick, result.summary());

            // Wait for next tick
            sleep(self.config.tick_interval).await;
        }
    }

    /// Executes a single cleanup cycle
    ///
    /// This method performs one iteration of:
    /// 1. Finding stuck jobs and workers
    /// 2. Updating their status
    /// 3. Publishing domain events
    ///
    /// # Returns
    ///
    /// A result containing metrics about what was cleaned up
    pub async fn run_cycle(&self) -> DatabaseReaperResult {
        let start_time = std::time::Instant::now();
        let mut result = DatabaseReaperResult::new();

        let now = Utc::now();
        let running_job_threshold = now
            - chrono::Duration::from_std(self.config.running_job_timeout)
                .expect("Invalid running_job_timeout duration");
        let creating_worker_threshold = now
            - chrono::Duration::from_std(self.config.creating_worker_timeout)
                .expect("Invalid creating_worker_timeout duration");

        // Process stuck running jobs
        if let Err(e) = self
            .process_stuck_running_jobs(running_job_threshold, &mut result)
            .await
        {
            result.add_error(format!("Failed to process stuck running jobs: {}", e));
        }

        // Process stuck creating workers
        if let Err(e) = self
            .process_stuck_creating_workers(creating_worker_threshold, &mut result)
            .await
        {
            result.add_error(format!("Failed to process stuck creating workers: {}", e));
        }

        result.execution_time_ms = start_time.elapsed().as_millis() as u64;
        result
    }

    /// Processes jobs that have been stuck in RUNNING state
    async fn process_stuck_running_jobs(
        &self,
        older_than: DateTime<Utc>,
        result: &mut DatabaseReaperResult,
    ) -> Result<(), OutboxError> {
        let stuck_jobs = self.find_stuck_running_jobs(older_than).await?;

        if stuck_jobs.is_empty() {
            return Ok(());
        }

        info!(
            target: "hodei::reaper",
            "Found {} stuck RUNNING jobs to mark as FAILED",
            stuck_jobs.len()
        );

        for job_id in stuck_jobs {
            let error_message = format!(
                "Job marked as FAILED by DatabaseReaper: safety timeout exceeded ({}s)",
                self.config.running_job_timeout.as_secs()
            );

            if let Err(e) = self.mark_job_as_failed(&job_id, &error_message).await {
                result.add_error(format!("Failed to mark job {} as failed: {}", job_id, e));
                continue;
            }

            result.add_job_failed();
        }

        Ok(())
    }

    /// Finds jobs stuck in RUNNING state
    async fn find_stuck_running_jobs(
        &self,
        older_than: DateTime<Utc>,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT id FROM jobs
            WHERE status = 'RUNNING'
              AND updated_at < $1
            ORDER BY updated_at ASC
            LIMIT $2
            "#,
            older_than,
            self.config.batch_size as i64
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.id).collect())
    }

    /// Marks a job as failed with proper event publishing
    async fn mark_job_as_failed(
        &self,
        job_id: &Uuid,
        error_message: &str,
    ) -> Result<(), OutboxError> {
        let mut tx = self.pool.begin().await?;

        let affected = sqlx::query!(
            r#"
            UPDATE jobs
            SET status = 'FAILED',
                error_message = $1,
                finished_at = NOW(),
                updated_at = NOW()
            WHERE id = $2 AND status = 'RUNNING'
            "#,
            error_message,
            job_id
        )
        .execute(&mut *tx)
        .await?;

        if affected.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(());
        }

        let job_row = sqlx::query!(
            r#"
            SELECT correlation_id FROM jobs WHERE id = $1
            "#,
            job_id
        )
        .fetch_optional(&mut *tx)
        .await?;

        let correlation_id = job_row
            .and_then(|r| r.correlation_id)
            .or(Some(Uuid::new_v4().to_string()));

        let outbox_event = OutboxEventInsert::for_job(
            *job_id,
            "JobStatusChanged".to_string(),
            serde_json::json!({
                "job_id": job_id.to_string(),
                "old_state": "RUNNING",
                "new_state": "FAILED",
                "reason": error_message,
                "actor": "DatabaseReaper"
            }),
            None,
            correlation_id.clone(),
        );

        self.outbox_repository
            .insert_events_with_tx(&mut tx, &[outbox_event])
            .await?;

        tx.commit().await?;

        Ok(())
    }

    /// Processes workers that have been stuck in CREATING state
    async fn process_stuck_creating_workers(
        &self,
        older_than: DateTime<Utc>,
        result: &mut DatabaseReaperResult,
    ) -> Result<(), OutboxError> {
        let stuck_workers = self.find_stuck_creating_workers(older_than).await?;

        if stuck_workers.is_empty() {
            return Ok(());
        }

        info!(
            target: "hodei::reaper",
            "Found {} stuck CREATING workers to mark as TERMINATED",
            stuck_workers.len()
        );

        for worker_id in stuck_workers {
            if let Err(e) = self.mark_worker_as_terminated(&worker_id).await {
                result.add_error(format!(
                    "Failed to mark worker {} as terminated: {}",
                    worker_id, e
                ));
                continue;
            }

            result.add_worker_terminated();
        }

        Ok(())
    }

    /// Finds workers stuck in CREATING state
    async fn find_stuck_creating_workers(
        &self,
        older_than: DateTime<Utc>,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT id FROM workers
            WHERE status = 'CREATING'
              AND created_at < $1
            ORDER BY created_at ASC
            LIMIT $2
            "#,
            older_than,
            self.config.batch_size as i64
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.id).collect())
    }

    /// Marks a worker as terminated with proper event publishing
    async fn mark_worker_as_terminated(&self, worker_id: &Uuid) -> Result<(), OutboxError> {
        let mut tx = self.pool.begin().await?;

        let affected = sqlx::query!(
            r#"
            UPDATE workers
            SET status = 'TERMINATED',
                updated_at = NOW()
            WHERE id = $1 AND status = 'CREATING'
            "#,
            worker_id
        )
        .execute(&mut *tx)
        .await?;

        if affected.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(());
        }

        let worker_row = sqlx::query!(
            r#"
            SELECT current_job_id FROM workers WHERE id = $1
            "#,
            worker_id
        )
        .fetch_optional(&mut *tx)
        .await?;

        let correlation_id = Some(Uuid::new_v4().to_string());

        let outbox_event = OutboxEventInsert::for_worker(
            *worker_id,
            "WorkerTerminated".to_string(),
            serde_json::json!({
                "worker_id": worker_id.to_string(),
                "reason": "Registration timeout - worker failed to complete registration within safety window",
                "actor": "DatabaseReaper"
            }),
            None,
            correlation_id.clone(),
        );

        self.outbox_repository
            .insert_events_with_tx(&mut tx, &[outbox_event])
            .await?;

        // If worker had a job assigned, update job status for recovery
        if let Some(Some(job_id)) = worker_row.map(|r| r.current_job_id) {
            sqlx::query!(
                r#"
                UPDATE jobs
                SET status = 'PENDING',
                    worker_id = NULL,
                    updated_at = NOW()
                WHERE id = $1 AND status = 'RUNNING'
                "#,
                job_id
            )
            .execute(&mut *tx)
            .await?;

            let job_outbox_event = OutboxEventInsert::for_job(
                job_id,
                "JobStatusChanged".to_string(),
                serde_json::json!({
                    "job_id": job_id.to_string(),
                    "old_state": "RUNNING",
                    "new_state": "PENDING",
                    "reason": "Job marked PENDING for recovery: worker registration timed out",
                    "actor": "DatabaseReaper"
                }),
                None,
                correlation_id.clone(),
            );

            self.outbox_repository
                .insert_events_with_tx(&mut tx, &[job_outbox_event])
                .await?;
        }

        tx.commit().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reaper_config_defaults() {
        let config = DatabaseReaperConfig::default();

        assert_eq!(config.tick_interval, Duration::from_secs(60));
        assert_eq!(config.running_job_timeout, Duration::from_secs(90));
        assert_eq!(config.creating_worker_timeout, Duration::from_secs(60));
        assert_eq!(config.batch_size, 100);
        assert!(config.enabled);
    }

    #[tokio::test]
    async fn test_reaper_result_addition() {
        let mut result = DatabaseReaperResult::new();

        assert_eq!(result.jobs_marked_failed, 0);
        assert_eq!(result.workers_marked_terminated, 0);

        result.add_job_failed();
        result.add_job_failed();
        assert_eq!(result.jobs_marked_failed, 2);

        result.add_worker_terminated();
        assert_eq!(result.workers_marked_terminated, 1);

        assert!(!result.has_errors());
        result.add_error("test error");
        assert!(result.has_errors());
    }

    #[tokio::test]
    async fn test_reaper_result_summary() {
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

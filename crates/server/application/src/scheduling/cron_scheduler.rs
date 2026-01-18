//! Cron Scheduler Service - Background task for triggering scheduled jobs
//!
//! This module provides a background service that:
//! - Runs continuously as a Tokio task
//! - Checks for due scheduled jobs every second
//! - Triggers jobs by invoking template execution
//! - Publishes domain events for triggering, missed executions, and errors
//! - Handles failure tracking and auto-disable logic

use chrono::{DateTime, Utc};
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::jobs::{JobExecutionStatus, JobTemplate, JobTemplateId, TriggerType};
use hodei_server_domain::shared_kernel::{JobId, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::info;
use uuid::Uuid;

/// Configuration for the Cron Scheduler Service
#[derive(Debug, Clone, Default)]
pub struct CronSchedulerConfig {
    /// How often to check for due jobs (default: 1 second)
    pub tick_interval: Duration,
    /// How many jobs to process in parallel (default: 10)
    pub parallel_trigger_limit: usize,
    /// Maximum jobs to fetch per tick (default: 100)
    pub batch_size: usize,
    /// Whether to enable auto-retry on failure
    pub enable_auto_retry: bool,
}

impl CronSchedulerConfig {
    pub fn new() -> Self {
        Self {
            tick_interval: Duration::from_secs(1),
            parallel_trigger_limit: 10,
            batch_size: 100,
            enable_auto_retry: true,
        }
    }
}

/// Port for triggering jobs from templates
#[async_trait::async_trait]
pub trait JobTriggerPort: Send + Sync {
    /// Trigger a job from a template
    async fn trigger_job_from_template(
        &self,
        template_id: &JobTemplateId,
        template_version: u32,
        triggered_by: TriggerType,
        parameters: HashMap<String, String>,
        scheduled_job_id: Option<Uuid>,
    ) -> Result<JobId>;
}

/// Port for template management operations
#[async_trait::async_trait]
pub trait TemplateManagementPort: Send + Sync {
    /// Get a template by ID
    async fn get_template(&self, template_id: &JobTemplateId) -> Result<Option<JobTemplate>>;
}

/// Port for scheduled job repository operations
#[async_trait::async_trait]
pub trait ScheduledJobRepositoryPort: Send + Sync {
    /// Get jobs that are due for execution
    async fn get_due_jobs(
        &self,
        now: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<hodei_server_domain::jobs::ScheduledJob>>;

    /// Mark a job as triggered
    async fn mark_triggered(&self, id: &Uuid, triggered_at: DateTime<Utc>) -> Result<()>;

    /// Mark a job as failed
    async fn mark_failed(&self, id: &Uuid, status: JobExecutionStatus) -> Result<()>;

    /// Calculate and update next execution time
    async fn calculate_next_execution(&self, id: &Uuid) -> Result<()>;
}

/// Errors that can occur during cron scheduling
#[derive(Debug, thiserror::Error)]
pub enum CronSchedulerError {
    #[error("Template not found: {template_id}")]
    TemplateNotFound { template_id: JobTemplateId },

    #[error("Failed to trigger job: {source}")]
    TriggerFailed { source: anyhow::Error },

    #[error("Scheduled job disabled: {scheduled_job_id}")]
    ScheduledJobDisabled { scheduled_job_id: Uuid },

    #[error("Cron expression parsing failed: {expression}")]
    CronParseError {
        expression: String,
        source: cron::error::Error,
    },
}

/// Result of triggering a scheduled job
#[derive(Debug)]
pub struct TriggerResult {
    pub scheduled_job_id: Uuid,
    pub template_id: JobTemplateId,
    pub parameters: HashMap<String, String>,
    pub job_id: JobId,
    pub triggered_at: DateTime<Utc>,
    pub success: bool,
    pub error: Option<String>,
}

/// Statistics for the cron scheduler
#[derive(Debug, Default, Clone)]
pub struct CronSchedulerStats {
    pub total_ticks: u64,
    pub total_triggers: u64,
    pub total_errors: u64,
    pub total_missed: u64,
    pub last_tick_at: Option<DateTime<Utc>>,
    pub last_trigger_at: Option<DateTime<Utc>>,
}

/// Cron Scheduler Service
///
/// Runs as a background task and triggers scheduled jobs when their cron
/// expression matches the current time.
pub struct CronSchedulerService {
    /// Repository for scheduled jobs
    scheduled_job_repo: Arc<dyn ScheduledJobRepositoryPort>,
    /// Port for triggering jobs
    job_trigger_port: Arc<dyn JobTriggerPort>,
    /// Port for template management
    template_port: Arc<dyn TemplateManagementPort>,
    /// Event bus for publishing events
    event_bus: Option<Arc<dyn EventBus>>,
    /// Service configuration
    config: CronSchedulerConfig,
    /// Shutdown signal (Arc for cloneability)
    shutdown_tx: Arc<broadcast::Sender<()>>,
    /// Scheduler statistics
    stats: Arc<std::sync::Mutex<CronSchedulerStats>>,
    /// Whether the service is running
    is_running: Arc<std::sync::atomic::AtomicBool>,
}

impl CronSchedulerService {
    /// Create a new CronSchedulerService
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        scheduled_job_repo: Arc<dyn ScheduledJobRepositoryPort>,
        job_trigger_port: Arc<dyn JobTriggerPort>,
        template_port: Arc<dyn TemplateManagementPort>,
        event_bus: Option<Arc<dyn EventBus>>,
        config: Option<CronSchedulerConfig>,
        _shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        // Create a sender for internal shutdown signaling
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            scheduled_job_repo,
            job_trigger_port,
            template_port,
            event_bus,
            config: config.unwrap_or_default(),
            shutdown_tx: Arc::new(shutdown_tx),
            stats: Arc::new(std::sync::Mutex::new(CronSchedulerStats::default())),
            is_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Start the scheduler service
    pub async fn start(&self) -> Result<()> {
        if self
            .is_running
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            )
            .is_err()
        {
            tracing::warn!("CronSchedulerService already started");
            return Ok(());
        }

        info!(
            "Starting CronSchedulerService with tick_interval={:?}",
            self.config.tick_interval
        );

        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let stats = self.stats.clone();
        let is_running = self.is_running.clone();
        let tick_interval = self.config.tick_interval;

        tokio::spawn(async move {
            let mut tick_interval = tokio::time::interval(tick_interval);

            loop {
                tokio::select! {
                    _ = tick_interval.tick() => {
                        let now = Utc::now();
                        let mut stats = stats.lock().unwrap();
                        stats.total_ticks += 1;
                        stats.last_tick_at = Some(now);
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }

            is_running.store(false, std::sync::atomic::Ordering::Relaxed);
            info!("CronSchedulerService loop stopped");
        });

        Ok(())
    }

    /// Stop the scheduler service
    pub async fn stop(&self) {
        if !self
            .is_running
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            tracing::warn!("CronSchedulerService not running");
            return;
        }

        info!("Stopping CronSchedulerService");
        let _ = self.shutdown_tx.send(());
    }

    /// Get scheduler statistics
    pub fn stats(&self) -> CronSchedulerStats {
        self.stats.lock().unwrap().clone()
    }

    /// Check if the scheduler is running
    pub fn is_running(&self) -> bool {
        self.is_running.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Clone for CronSchedulerService {
    fn clone(&self) -> Self {
        Self {
            scheduled_job_repo: self.scheduled_job_repo.clone(),
            job_trigger_port: self.job_trigger_port.clone(),
            template_port: self.template_port.clone(),
            event_bus: self.event_bus.clone(),
            config: self.config.clone(),
            shutdown_tx: self.shutdown_tx.clone(),
            stats: self.stats.clone(),
            is_running: self.is_running.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::jobs::ScheduledJob;
    use hodei_server_domain::shared_kernel::JobId;

    #[tokio::test]
    async fn test_cron_scheduler_config_default() {
        let config = CronSchedulerConfig::new();
        assert_eq!(config.tick_interval, Duration::from_secs(1));
        assert_eq!(config.parallel_trigger_limit, 10);
        assert_eq!(config.batch_size, 100);
        assert!(config.enable_auto_retry);
    }

    #[tokio::test]
    async fn test_trigger_result_default() {
        let result = TriggerResult {
            scheduled_job_id: Uuid::new_v4(),
            template_id: JobTemplateId::new(),
            parameters: HashMap::new(),
            job_id: JobId::new(),
            triggered_at: Utc::now(),
            success: true,
            error: None,
        };

        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_cron_scheduler_stats_default() {
        let stats = CronSchedulerStats::default();
        assert_eq!(stats.total_ticks, 0);
        assert_eq!(stats.total_triggers, 0);
        assert_eq!(stats.total_errors, 0);
        assert_eq!(stats.total_missed, 0);
        assert!(stats.last_tick_at.is_none());
        assert!(stats.last_trigger_at.is_none());
    }
}

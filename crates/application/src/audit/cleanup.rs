//! Audit Cleanup Service
//!
//! Manages retention policy for audit logs.
//! Story 15.9: Retención y Cleanup de Audit Logs

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use hodei_jobs_domain::audit::AuditRepository;
use tracing::{info, warn};

/// Configuration for audit log retention
#[derive(Debug, Clone)]
pub struct AuditRetentionConfig {
    /// Number of days to retain audit logs (default: 90)
    pub retention_days: u32,
    /// Interval between cleanup runs (default: 24 hours)
    pub cleanup_interval: Duration,
    /// Whether cleanup is enabled
    pub enabled: bool,
}

impl Default for AuditRetentionConfig {
    fn default() -> Self {
        Self {
            retention_days: 90,
            cleanup_interval: Duration::from_secs(24 * 60 * 60), // 24 hours
            enabled: true,
        }
    }
}

impl AuditRetentionConfig {
    /// Create config from environment variables
    pub fn from_env() -> Self {
        let retention_days = std::env::var("HODEI_AUDIT_RETENTION_DAYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(90);

        let cleanup_interval_hours = std::env::var("HODEI_AUDIT_CLEANUP_INTERVAL_HOURS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(24);

        let enabled = std::env::var("HODEI_AUDIT_CLEANUP_ENABLED")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(true);

        Self {
            retention_days,
            cleanup_interval: Duration::from_secs(cleanup_interval_hours * 60 * 60),
            enabled,
        }
    }

    /// Create config with specific retention days
    pub fn with_retention_days(mut self, days: u32) -> Self {
        self.retention_days = days;
        self
    }

    /// Create config with specific cleanup interval
    pub fn with_cleanup_interval(mut self, interval: Duration) -> Self {
        self.cleanup_interval = interval;
        self
    }

    /// Enable or disable cleanup
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Result of a cleanup operation
#[derive(Debug, Clone)]
pub struct CleanupResult {
    /// Number of logs deleted
    pub deleted_count: u64,
    /// Cutoff date used for deletion
    pub cutoff_date: DateTime<Utc>,
    /// When the cleanup was performed
    pub performed_at: DateTime<Utc>,
}

/// Service for managing audit log retention and cleanup
pub struct AuditCleanupService {
    repository: Arc<dyn AuditRepository>,
    config: AuditRetentionConfig,
}

impl AuditCleanupService {
    /// Create a new cleanup service
    pub fn new(repository: Arc<dyn AuditRepository>, config: AuditRetentionConfig) -> Self {
        Self { repository, config }
    }

    /// Get the current configuration
    pub fn config(&self) -> &AuditRetentionConfig {
        &self.config
    }

    /// Calculate the cutoff date based on retention policy
    pub fn calculate_cutoff_date(&self) -> DateTime<Utc> {
        Utc::now() - chrono::Duration::days(self.config.retention_days as i64)
    }

    /// Run cleanup once, deleting logs older than the retention period
    pub async fn run_cleanup(&self) -> Result<CleanupResult, String> {
        if !self.config.enabled {
            return Ok(CleanupResult {
                deleted_count: 0,
                cutoff_date: self.calculate_cutoff_date(),
                performed_at: Utc::now(),
            });
        }

        let cutoff_date = self.calculate_cutoff_date();
        let performed_at = Utc::now();

        info!(
            "Running audit log cleanup. Deleting logs older than {} ({} days retention)",
            cutoff_date.format("%Y-%m-%d %H:%M:%S UTC"),
            self.config.retention_days
        );

        let deleted_count = self
            .repository
            .delete_before(cutoff_date)
            .await
            .map_err(|e| format!("Failed to delete old audit logs: {}", e))?;

        if deleted_count > 0 {
            info!("Audit cleanup completed: {} logs deleted", deleted_count);
        } else {
            info!("Audit cleanup completed: no logs to delete");
        }

        Ok(CleanupResult {
            deleted_count,
            cutoff_date,
            performed_at,
        })
    }

    /// Start the background cleanup loop
    /// This spawns a tokio task that runs cleanup at the configured interval
    pub fn start_background_cleanup(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        
        tokio::spawn(async move {
            if !service.config.enabled {
                warn!("Audit cleanup is disabled. Background task will not run.");
                return;
            }

            info!(
                "Starting audit cleanup background task (interval: {:?}, retention: {} days)",
                service.config.cleanup_interval,
                service.config.retention_days
            );

            // Run initial cleanup on startup
            if let Err(e) = service.run_cleanup().await {
                warn!("Initial audit cleanup failed: {}", e);
            }

            // Then run at configured interval
            let mut interval = tokio::time::interval(service.config.cleanup_interval);
            interval.tick().await; // Skip first immediate tick (we already ran cleanup)

            loop {
                interval.tick().await;
                
                if let Err(e) = service.run_cleanup().await {
                    warn!("Scheduled audit cleanup failed: {}", e);
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use hodei_jobs_domain::audit::{AuditLog, AuditQuery, AuditQueryResult, EventTypeCount};
    use serde_json::json;
    use std::sync::Mutex;
    use uuid::Uuid;

    struct MockAuditRepository {
        logs: Mutex<Vec<AuditLog>>,
        delete_before_calls: Mutex<Vec<DateTime<Utc>>>,
    }

    impl MockAuditRepository {
        fn new() -> Self {
            Self {
                logs: Mutex::new(Vec::new()),
                delete_before_calls: Mutex::new(Vec::new()),
            }
        }

        fn with_logs(logs: Vec<AuditLog>) -> Self {
            Self {
                logs: Mutex::new(logs),
                delete_before_calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl AuditRepository for MockAuditRepository {
        async fn save(&self, log: &AuditLog) -> hodei_jobs_domain::shared_kernel::Result<()> {
            self.logs.lock().unwrap().push(log.clone());
            Ok(())
        }

        async fn find_by_correlation_id(
            &self,
            id: &str,
        ) -> hodei_jobs_domain::shared_kernel::Result<Vec<AuditLog>> {
            let logs = self.logs.lock().unwrap();
            Ok(logs
                .iter()
                .filter(|l| l.correlation_id.as_deref() == Some(id))
                .cloned()
                .collect())
        }

        async fn find_by_event_type(
            &self,
            _event_type: &str,
            _limit: i64,
            _offset: i64,
        ) -> hodei_jobs_domain::shared_kernel::Result<AuditQueryResult> {
            Ok(AuditQueryResult {
                logs: vec![],
                total_count: 0,
                has_more: false,
            })
        }

        async fn find_by_date_range(
            &self,
            _start: DateTime<Utc>,
            _end: DateTime<Utc>,
            _limit: i64,
            _offset: i64,
        ) -> hodei_jobs_domain::shared_kernel::Result<AuditQueryResult> {
            Ok(AuditQueryResult {
                logs: vec![],
                total_count: 0,
                has_more: false,
            })
        }

        async fn find_by_actor(
            &self,
            _actor: &str,
            _limit: i64,
            _offset: i64,
        ) -> hodei_jobs_domain::shared_kernel::Result<AuditQueryResult> {
            Ok(AuditQueryResult {
                logs: vec![],
                total_count: 0,
                has_more: false,
            })
        }

        async fn query(
            &self,
            _query: AuditQuery,
        ) -> hodei_jobs_domain::shared_kernel::Result<AuditQueryResult> {
            Ok(AuditQueryResult {
                logs: vec![],
                total_count: 0,
                has_more: false,
            })
        }

        async fn count_by_event_type(
            &self,
        ) -> hodei_jobs_domain::shared_kernel::Result<Vec<EventTypeCount>> {
            Ok(vec![])
        }

        async fn delete_before(
            &self,
            before: DateTime<Utc>,
        ) -> hodei_jobs_domain::shared_kernel::Result<u64> {
            self.delete_before_calls.lock().unwrap().push(before);
            let mut logs = self.logs.lock().unwrap();
            let original_len = logs.len();
            logs.retain(|l| l.occurred_at >= before);
            Ok((original_len - logs.len()) as u64)
        }
    }

    fn create_test_log(days_ago: i64) -> AuditLog {
        AuditLog {
            id: Uuid::new_v4(),
            correlation_id: Some("test-corr".to_string()),
            event_type: "TestEvent".to_string(),
            payload: json!({"test": true}),
            occurred_at: Utc::now() - ChronoDuration::days(days_ago),
            actor: Some("test-actor".to_string()),
        }
    }

    #[test]
    fn test_config_default() {
        let config = AuditRetentionConfig::default();
        assert_eq!(config.retention_days, 90);
        assert_eq!(config.cleanup_interval, Duration::from_secs(24 * 60 * 60));
        assert!(config.enabled);
    }

    #[test]
    fn test_config_builder() {
        let config = AuditRetentionConfig::default()
            .with_retention_days(30)
            .with_cleanup_interval(Duration::from_secs(3600))
            .with_enabled(false);

        assert_eq!(config.retention_days, 30);
        assert_eq!(config.cleanup_interval, Duration::from_secs(3600));
        assert!(!config.enabled);
    }

    #[test]
    fn test_calculate_cutoff_date() {
        let config = AuditRetentionConfig::default().with_retention_days(30);
        let repo = Arc::new(MockAuditRepository::new());
        let service = AuditCleanupService::new(repo, config);

        let cutoff = service.calculate_cutoff_date();
        let expected = Utc::now() - ChronoDuration::days(30);

        // Allow 1 second tolerance
        assert!((cutoff - expected).num_seconds().abs() < 1);
    }

    #[tokio::test]
    async fn test_run_cleanup_deletes_old_logs() {
        let logs = vec![
            create_test_log(100), // Should be deleted (older than 90 days)
            create_test_log(95),  // Should be deleted
            create_test_log(50),  // Should be kept
            create_test_log(10),  // Should be kept
        ];
        let repo = Arc::new(MockAuditRepository::with_logs(logs));
        let config = AuditRetentionConfig::default().with_retention_days(90);
        let service = AuditCleanupService::new(repo.clone(), config);

        let result = service.run_cleanup().await.unwrap();

        assert_eq!(result.deleted_count, 2);
        assert_eq!(repo.logs.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_run_cleanup_disabled() {
        let logs = vec![create_test_log(100)];
        let repo = Arc::new(MockAuditRepository::with_logs(logs));
        let config = AuditRetentionConfig::default().with_enabled(false);
        let service = AuditCleanupService::new(repo.clone(), config);

        let result = service.run_cleanup().await.unwrap();

        assert_eq!(result.deleted_count, 0);
        // Log should still exist because cleanup is disabled
        assert_eq!(repo.logs.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_run_cleanup_no_old_logs() {
        let logs = vec![
            create_test_log(10),
            create_test_log(5),
        ];
        let repo = Arc::new(MockAuditRepository::with_logs(logs));
        let config = AuditRetentionConfig::default().with_retention_days(90);
        let service = AuditCleanupService::new(repo.clone(), config);

        let result = service.run_cleanup().await.unwrap();

        assert_eq!(result.deleted_count, 0);
        assert_eq!(repo.logs.lock().unwrap().len(), 2);
    }
}

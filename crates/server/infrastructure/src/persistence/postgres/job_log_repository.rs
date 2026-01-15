//! Postgres-based LogRetriever Implementation
//!
//! Provides persistent log storage and retrieval for job execution logs.
//! Implements the LogRetriever trait for production use with PostgreSQL.
//! Includes cleanup functionality for log management (EPIC-85 US-09).

use chrono::{DateTime, Utc};
use hodei_server_domain::jobs::coordination::{JobLogEntry, JobLogLevel, LogRetriever, LogSource};
use hodei_server_domain::shared_kernel::{DomainError, JobId, Result};
use sqlx::postgres::PgPool;
use sqlx::{Row, query, query_as};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Postgres implementation of LogRetriever
///
/// Stores job execution logs in PostgreSQL and provides retrieval,
/// aggregation, and cleanup capabilities.
#[derive(Clone)]
pub struct PostgresLogRetriever {
    /// Database connection pool
    pool: Arc<PgPool>,
}

impl PostgresLogRetriever {
    /// Create a new PostgresLogRetriever
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }

    /// Create a new PostgresLogRetriever from an existing pool reference
    pub fn from_pool(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Helper to convert string to JobLogLevel
    fn parse_log_level(level: &str) -> JobLogLevel {
        match level.to_uppercase().as_str() {
            "DEBUG" => JobLogLevel::Debug,
            "WARNING" | "WARN" => JobLogLevel::Warning,
            "ERROR" => JobLogLevel::Error,
            "CRITICAL" => JobLogLevel::Critical,
            _ => JobLogLevel::Info,
        }
    }

    /// Helper to convert string to LogSource
    fn parse_log_source(source: &str) -> LogSource {
        match source.to_lowercase().as_str() {
            "stderr" => LogSource::Stderr,
            "system" => LogSource::System,
            s if s.starts_with("provider:") => LogSource::Provider(s.to_string()),
            _ => LogSource::Stdout,
        }
    }
}

#[async_trait::async_trait]
impl LogRetriever for PostgresLogRetriever {
    /// Get all logs for a specific job
    async fn get_job_logs(&self, job_id: &JobId) -> Result<Vec<JobLogEntry>> {
        let rows = query(
            r#"
            SELECT id, job_id, timestamp, level, message, source, line_number, created_at
            FROM job_logs
            WHERE job_id = $1
            ORDER BY timestamp ASC
            "#,
        )
        .bind(job_id.0)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to fetch job logs: {}", e),
        })?;

        let logs: Vec<JobLogEntry> = rows
            .into_iter()
            .map(|row| {
                let line_num: Option<i32> = row.get("line_number");
                JobLogEntry {
                    job_id: JobId(row.get("job_id")),
                    timestamp: row.get("timestamp"),
                    level: Self::parse_log_level(&row.get::<String, _>("level")),
                    message: row.get("message"),
                    source: Self::parse_log_source(&row.get::<String, _>("source")),
                    line_number: line_num.map(|n| n as u32),
                }
            })
            .collect();

        debug!("Retrieved {} logs for job {}", logs.len(), job_id);
        Ok(logs)
    }

    /// Get logs for a job since a specific timestamp
    async fn get_logs_since(
        &self,
        job_id: &JobId,
        since: DateTime<Utc>,
    ) -> Result<Vec<JobLogEntry>> {
        let rows = query(
            r#"
            SELECT id, job_id, timestamp, level, message, source, line_number, created_at
            FROM job_logs
            WHERE job_id = $1 AND timestamp >= $2
            ORDER BY timestamp ASC
            "#,
        )
        .bind(job_id.0)
        .bind(since)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to fetch logs since {}: {}", since, e),
        })?;

        let logs: Vec<JobLogEntry> = rows
            .into_iter()
            .map(|row| {
                let line_num: Option<i32> = row.get("line_number");
                JobLogEntry {
                    job_id: JobId(row.get("job_id")),
                    timestamp: row.get("timestamp"),
                    level: Self::parse_log_level(&row.get::<String, _>("level")),
                    message: row.get("message"),
                    source: Self::parse_log_source(&row.get::<String, _>("source")),
                    line_number: line_num.map(|n| n as u32),
                }
            })
            .collect();

        debug!(
            "Retrieved {} logs for job {} since {}",
            logs.len(),
            job_id,
            since
        );
        Ok(logs)
    }

    /// Add a new log entry
    async fn add_log_entry(&self, log_entry: JobLogEntry) -> Result<()> {
        let line_num = log_entry.line_number.map(|n| n as i32);
        let job_id = log_entry.job_id.clone();
        let message_preview = log_entry.message.chars().take(50).collect::<String>();

        let source_str = match log_entry.source {
            LogSource::Stdout => "stdout".to_string(),
            LogSource::Stderr => "stderr".to_string(),
            LogSource::System => "system".to_string(),
            LogSource::Provider(p) => p,
        };

        query(
            r#"
            INSERT INTO job_logs (job_id, timestamp, level, message, source, line_number)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(job_id.0)
        .bind(log_entry.timestamp)
        .bind(format!("{}", log_entry.level))
        .bind(log_entry.message)
        .bind(source_str)
        .bind(line_num)
        .execute(&*self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to insert log entry: {}", e),
        })?;

        debug!("Added log entry for job {}: {}", job_id, message_preview);
        Ok(())
    }

    /// Clear all logs for a specific job
    async fn clear_job_logs(&self, job_id: &JobId) -> Result<()> {
        let result = query("DELETE FROM job_logs WHERE job_id = $1")
            .bind(job_id.0)
            .execute(&*self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to clear job logs: {}", e),
            })?;

        let count = result.rows_affected();
        info!("Cleared {} log entries for job {}", count, job_id);
        Ok(())
    }

    /// Delete logs older than a specified timestamp (EPIC-85 US-09)
    ///
    /// This method provides real cleanup functionality by deleting
    /// log entries that are older than the specified cutoff time.
    ///
    /// # Returns
    ///
    /// The number of log entries deleted
    async fn delete_logs_older_than(
        &self,
        _job_id: &JobId,
        older_than: DateTime<Utc>,
    ) -> Result<u32> {
        // Note: job_id parameter is ignored for global cleanup
        // For job-specific cleanup, we would use a different query

        // Delete old logs directly
        let result = query(
            r#"
            DELETE FROM job_logs WHERE timestamp < $1
            "#,
        )
        .bind(older_than)
        .execute(&*self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to cleanup old logs: {}", e),
        })?;

        let deleted_count = result.rows_affected() as u32;
        info!(
            "Cleaned up {} log entries older than {}",
            deleted_count, older_than
        );
        Ok(deleted_count)
    }

    /// Get aggregated logs for a job (all logs combined)
    async fn get_aggregated_logs(&self, job_id: &JobId) -> Result<Vec<JobLogEntry>> {
        // For now, this is the same as get_job_logs
        // In the future, this could aggregate from multiple sources
        self.get_job_logs(job_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Executor;
    use sqlx::postgres::PgPoolOptions;

    async fn setup_test_db() -> PgPool {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/test".to_string());

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        // Run migrations
        pool.execute(
            r#"
            CREATE TABLE IF NOT EXISTS job_logs (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                level VARCHAR(20) NOT NULL DEFAULT 'INFO',
                message TEXT NOT NULL,
                source VARCHAR(50) NOT NULL DEFAULT 'stdout',
                line_number INTEGER,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE INDEX IF NOT EXISTS idx_job_logs_job_id ON job_logs(job_id);
            CREATE INDEX IF NOT EXISTS idx_job_logs_timestamp ON job_logs(timestamp);
            CREATE INDEX IF NOT EXISTS idx_job_logs_job_timestamp ON job_logs(job_id, timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_job_logs_level ON job_logs(level);

            CREATE OR REPLACE FUNCTION cleanup_job_logs_older_than(cutoff_timestamp TIMESTAMPTZ)
            RETURNS INTEGER AS $$
            DECLARE
                deleted_count INTEGER;
            BEGIN
                DELETE FROM job_logs WHERE timestamp < cutoff_timestamp;
                GET DIAGNOSTICS deleted_count = ROW_COUNT;
                RETURN deleted_count;
            END;
            $$ LANGUAGE plpgsql;
            "#,
        )
        .await
        .expect("Failed to create test tables");

        pool
    }

    #[tokio::test]
    #[ignore = "Requires database connection"]
    async fn test_add_and_get_logs() {
        let pool = setup_test_db().await;
        let retriever = PostgresLogRetriever::new(pool.clone());

        let job_id = JobId(uuid::Uuid::new_v4());

        // Add some log entries
        let log1 = JobLogEntry::new(job_id.clone(), "Job started".to_string(), Utc::now());
        let log2 = JobLogEntry {
            job_id: job_id.clone(),
            timestamp: Utc::now(),
            level: JobLogLevel::Info,
            message: "Processing step 1".to_string(),
            source: LogSource::Stdout,
            line_number: Some(10),
        };

        retriever
            .add_log_entry(log1)
            .await
            .expect("Failed to add log1");
        retriever
            .add_log_entry(log2)
            .await
            .expect("Failed to add log2");

        // Retrieve logs
        let logs = retriever
            .get_job_logs(&job_id)
            .await
            .expect("Failed to get logs");
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].message, "Job started");
        assert_eq!(logs[1].message, "Processing step 1");

        // Cleanup
        retriever
            .clear_job_logs(&job_id)
            .await
            .expect("Failed to cleanup");
        pool.close().await;
    }

    #[tokio::test]
    #[ignore = "Requires database connection"]
    async fn test_cleanup_old_logs() {
        let pool = setup_test_db().await;
        let retriever = PostgresLogRetriever::new(pool.clone());

        let job_id = JobId(uuid::Uuid::new_v4());

        // Add old logs
        let old_log = JobLogEntry {
            job_id: job_id.clone(),
            timestamp: Utc::now() - chrono::Duration::hours(25),
            level: JobLogLevel::Info,
            message: "Old log entry".to_string(),
            source: LogSource::Stdout,
            line_number: None,
        };
        retriever
            .add_log_entry(old_log)
            .await
            .expect("Failed to add old log");

        // Add recent log
        let recent_log = JobLogEntry {
            job_id: job_id.clone(),
            timestamp: Utc::now(),
            level: JobLogLevel::Info,
            message: "Recent log entry".to_string(),
            source: LogSource::Stdout,
            line_number: None,
        };
        retriever
            .add_log_entry(recent_log)
            .await
            .expect("Failed to add recent log");

        // Cleanup logs older than 24 hours
        let cutoff = Utc::now() - chrono::Duration::hours(24);
        let deleted = retriever
            .delete_logs_older_than(&job_id, cutoff)
            .await
            .expect("Failed to cleanup");

        assert_eq!(deleted, 1); // Should have deleted the old log

        // Verify only recent log remains
        let logs = retriever
            .get_job_logs(&job_id)
            .await
            .expect("Failed to get logs");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, "Recent log entry");

        // Cleanup
        retriever
            .clear_job_logs(&job_id)
            .await
            .expect("Failed to cleanup");
        pool.close().await;
    }
}

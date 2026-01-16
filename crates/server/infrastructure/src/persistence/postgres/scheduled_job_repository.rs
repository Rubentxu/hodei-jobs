//! PostgreSQL ScheduledJob Repository
//!
//! Persistent repository implementation for ScheduledJobs based on PostgreSQL
//!
//! # Pool Management
//!
//! This repository expects a `PgPool` to be passed in via `new()`.
//! The pool should be created using `DatabasePool` for consistent configuration.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cron::Schedule;
use hodei_server_domain::jobs::{
    JobExecutionStatus, JobTemplateId, ScheduledJob, ScheduledJobRepository,
};
use hodei_server_domain::shared_kernel::{DomainError, Result};
use sqlx::postgres::PgPool;
use sqlx::types::chrono::{DateTime as SqlxDateTime, Utc as SqlxUtc};
use sqlx::Row;
use uuid::Uuid;

/// PostgreSQL ScheduledJob Repository
#[derive(Clone)]
pub struct PostgresScheduledJobRepository {
    pool: PgPool,
}

impl PostgresScheduledJobRepository {
    /// Create new repository with existing pool
    ///
    /// The pool should be created centrally (e.g., using `DatabasePool`)
    /// to ensure consistent configuration across all repositories.
    #[inline]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ScheduledJobRepository for PostgresScheduledJobRepository {
    async fn save(&self, scheduled_job: &ScheduledJob) -> Result<()> {
        let parameters_json = serde_json::to_value(&scheduled_job.parameters).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to serialize parameters: {}", e),
            }
        })?;

        let last_execution_status = scheduled_job
            .last_execution_status
            .as_ref()
            .map(|s| s.to_string());

        sqlx::query(
            r#"
            INSERT INTO scheduled_jobs
                (id, name, description, template_id, cron_expression, timezone,
                 next_execution_at, last_execution_at, last_execution_status,
                 enabled, max_consecutive_failures, consecutive_failures,
                 pause_on_failure, parameters, created_at, updated_at, created_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                cron_expression = EXCLUDED.cron_expression,
                timezone = EXCLUDED.timezone,
                next_execution_at = EXCLUDED.next_execution_at,
                last_execution_at = EXCLUDED.last_execution_at,
                last_execution_status = EXCLUDED.last_execution_status,
                enabled = EXCLUDED.enabled,
                max_consecutive_failures = EXCLUDED.max_consecutive_failures,
                consecutive_failures = EXCLUDED.consecutive_failures,
                pause_on_failure = EXCLUDED.pause_on_failure,
                parameters = EXCLUDED.parameters,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(scheduled_job.id)
        .bind(&scheduled_job.name)
        .bind(scheduled_job.description.as_ref())
        .bind(scheduled_job.template_id.0)
        .bind(&scheduled_job.cron_expression)
        .bind(&scheduled_job.timezone)
        .bind(scheduled_job.next_execution_at)
        .bind(scheduled_job.last_execution_at)
        .bind(last_execution_status)
        .bind(scheduled_job.enabled)
        .bind(scheduled_job.max_consecutive_failures as i32)
        .bind(scheduled_job.consecutive_failures as i32)
        .bind(scheduled_job.pause_on_failure)
        .bind(parameters_json)
        .bind(scheduled_job.created_at)
        .bind(scheduled_job.updated_at)
        .bind(scheduled_job.created_by.as_ref())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to save scheduled job: {}", e),
        })?;

        Ok(())
    }

    async fn update(&self, scheduled_job: &ScheduledJob) -> Result<()> {
        // Same as save since we use ON CONFLICT
        self.save(scheduled_job).await
    }

    async fn find_by_id(&self, id: &Uuid) -> Result<Option<ScheduledJob>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, description, template_id, cron_expression, timezone,
                   next_execution_at, last_execution_at, last_execution_status,
                   enabled, max_consecutive_failures, consecutive_failures,
                   pause_on_failure, parameters, created_at, updated_at, created_by
            FROM scheduled_jobs
            WHERE id = $1
            "#,
        )
        .bind(*id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find scheduled job: {}", e),
        })?;

        match row {
            Some(row) => Ok(Some(Self::row_to_scheduled_job(row)?)),
            None => Ok(None),
        }
    }

    async fn find_by_template_id(&self, template_id: &JobTemplateId) -> Result<Vec<ScheduledJob>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, template_id, cron_expression, timezone,
                   next_execution_at, last_execution_at, last_execution_status,
                   enabled, max_consecutive_failures, consecutive_failures,
                   pause_on_failure, parameters, created_at, updated_at, created_by
            FROM scheduled_jobs
            WHERE template_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(template_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find scheduled jobs by template: {}", e),
        })?;

        rows.into_iter()
            .map(|row| Self::row_to_scheduled_job(row))
            .collect()
    }

    async fn list_enabled(&self) -> Result<Vec<ScheduledJob>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, template_id, cron_expression, timezone,
                   next_execution_at, last_execution_at, last_execution_status,
                   enabled, max_consecutive_failures, consecutive_failures,
                   pause_on_failure, parameters, created_at, updated_at, created_by
            FROM scheduled_jobs
            WHERE enabled = true
            ORDER BY next_execution_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to list enabled scheduled jobs: {}", e),
        })?;

        rows.into_iter()
            .map(|row| Self::row_to_scheduled_job(row))
            .collect()
    }

    async fn find_all(&self) -> Result<Vec<ScheduledJob>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, template_id, cron_expression, timezone,
                   next_execution_at, last_execution_at, last_execution_status,
                   enabled, max_consecutive_failures, consecutive_failures,
                   pause_on_failure, parameters, created_at, updated_at, created_by
            FROM scheduled_jobs
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find all scheduled jobs: {}", e),
        })?;

        rows.into_iter()
            .map(|row| Self::row_to_scheduled_job(row))
            .collect()
    }

    async fn find_ready_to_run(&self, now: &DateTime<Utc>) -> Result<Vec<ScheduledJob>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, template_id, cron_expression, timezone,
                   next_execution_at, last_execution_at, last_execution_status,
                   enabled, max_consecutive_failures, consecutive_failures,
                   pause_on_failure, parameters, created_at, updated_at, created_by
            FROM scheduled_jobs
            WHERE enabled = true AND next_execution_at <= $1
            ORDER BY next_execution_at ASC
            "#,
        )
        .bind(*now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find ready-to-run scheduled jobs: {}", e),
        })?;

        rows.into_iter()
            .map(|row| Self::row_to_scheduled_job(row))
            .collect()
    }

    async fn get_due_jobs(&self, now: DateTime<Utc>, limit: usize) -> Result<Vec<ScheduledJob>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, template_id, cron_expression, timezone,
                   next_execution_at, last_execution_at, last_execution_status,
                   enabled, max_consecutive_failures, consecutive_failures,
                   pause_on_failure, parameters, created_at, updated_at, created_by
            FROM scheduled_jobs
            WHERE enabled = true AND next_execution_at <= $1
            ORDER BY next_execution_at ASC
            LIMIT $2
            "#,
        )
        .bind(now)
        .bind(limit as i32)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to get due jobs: {}", e),
        })?;

        rows.into_iter()
            .map(|row| Self::row_to_scheduled_job(row))
            .collect()
    }

    async fn mark_triggered(&self, id: &Uuid, triggered_at: DateTime<Utc>) -> Result<()> {
        // First get the cron expression to calculate next execution
        let cron_expression: String =
            sqlx::query_scalar("SELECT cron_expression FROM scheduled_jobs WHERE id = $1")
                .bind(*id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to get cron expression: {}", e),
                })?;

        // Calculate next execution time
        let next_execution = calculate_next_cron_time(&cron_expression, triggered_at);

        sqlx::query(
            r#"
            UPDATE scheduled_jobs
            SET last_execution_at = $1,
                consecutive_failures = 0,
                next_execution_at = $2,
                updated_at = $3
            WHERE id = $4
            "#,
        )
        .bind(triggered_at)
        .bind(next_execution)
        .bind(Utc::now())
        .bind(*id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to mark scheduled job as triggered: {}", e),
        })?;

        Ok(())
    }

    async fn mark_failed(&self, id: &Uuid, status: JobExecutionStatus) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE scheduled_jobs
            SET last_execution_status = $1,
                consecutive_failures = consecutive_failures + 1,
                last_execution_at = $2,
                updated_at = $3,
                enabled = CASE
                    WHEN pause_on_failure = true AND (consecutive_failures + 1) >= max_consecutive_failures
                    THEN false
                    ELSE enabled
                END
            WHERE id = $4
            "#,
        )
        .bind(status.to_string())
        .bind(Utc::now())
        .bind(Utc::now())
        .bind(*id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to mark scheduled job as failed: {}", e),
        })?;

        Ok(())
    }

    async fn calculate_next_execution(&self, id: &Uuid) -> Result<()> {
        // Get the cron expression and current state using query and manual mapping
        let row = sqlx::query(
            r#"
                SELECT cron_expression, last_execution_at
                FROM scheduled_jobs
                WHERE id = $1
                "#,
        )
        .bind(*id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to get scheduled job for next calculation: {}", e),
        })?;

        let cron_expression: String = row.try_get(0)?;
        let last_execution_at: Option<DateTime<Utc>> = row.try_get(1)?;

        let from_time = last_execution_at.unwrap_or_else(Utc::now);
        let next_execution = calculate_next_cron_time(&cron_expression, from_time);

        sqlx::query(
            "UPDATE scheduled_jobs SET next_execution_at = $1, updated_at = $2 WHERE id = $3",
        )
        .bind(next_execution)
        .bind(Utc::now())
        .bind(*id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to update next execution time: {}", e),
        })?;

        Ok(())
    }

    async fn delete(&self, id: &Uuid) -> Result<()> {
        sqlx::query("DELETE FROM scheduled_jobs WHERE id = $1")
            .bind(*id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to delete scheduled job: {}", e),
            })?;

        Ok(())
    }
}

impl PostgresScheduledJobRepository {
    /// Convert a database row to ScheduledJob
    pub fn row_to_scheduled_job(row: sqlx::postgres::PgRow) -> Result<ScheduledJob> {
        let id: Uuid = row.try_get("id")?;
        let name: String = row.try_get("name")?;
        let description: Option<String> = row.try_get("description")?;
        let template_id_uuid: Uuid = row.try_get("template_id")?;
        let cron_expression: String = row.try_get("cron_expression")?;
        let timezone: String = row.try_get("timezone")?;
        let next_execution_at: SqlxDateTime<SqlxUtc> = row.try_get("next_execution_at")?;
        let last_execution_at: Option<SqlxDateTime<SqlxUtc>> = row.try_get("last_execution_at")?;
        let last_execution_status: Option<String> = row.try_get("last_execution_status")?;
        let enabled: bool = row.try_get("enabled")?;
        let max_consecutive_failures: i32 = row.try_get("max_consecutive_failures")?;
        let consecutive_failures: i32 = row.try_get("consecutive_failures")?;
        let pause_on_failure: bool = row.try_get("pause_on_failure")?;
        let parameters_json: serde_json::Value = row.try_get("parameters")?;
        let created_at: SqlxDateTime<SqlxUtc> = row.try_get("created_at")?;
        let updated_at: SqlxDateTime<SqlxUtc> = row.try_get("updated_at")?;
        let created_by: Option<String> = row.try_get("created_by")?;

        // Convert sqlx::types::chrono::Utc to chrono::Utc
        let next_execution_at: DateTime<chrono::Utc> =
            next_execution_at.with_timezone(&chrono::Utc);
        let last_execution_at = last_execution_at.map(|dt| dt.with_timezone(&chrono::Utc));
        let created_at = created_at.with_timezone(&chrono::Utc);
        let updated_at = updated_at.with_timezone(&chrono::Utc);

        // Deserialize parameters
        let parameters: std::collections::HashMap<String, String> =
            serde_json::from_value(parameters_json).expect("Failed to deserialize parameters");

        // Parse last execution status
        let last_execution_status = last_execution_status.as_ref().map(|s| match s.as_str() {
            "Queued" => hodei_server_domain::jobs::JobExecutionStatus::Queued,
            "Running" => hodei_server_domain::jobs::JobExecutionStatus::Running,
            "Succeeded" => hodei_server_domain::jobs::JobExecutionStatus::Succeeded,
            "Failed" => hodei_server_domain::jobs::JobExecutionStatus::Failed,
            "Error" => hodei_server_domain::jobs::JobExecutionStatus::Error,
            _ => hodei_server_domain::jobs::JobExecutionStatus::Queued,
        });

        Ok(ScheduledJob {
            id,
            name,
            description,
            template_id: JobTemplateId(template_id_uuid),
            cron_expression,
            timezone,
            next_execution_at,
            last_execution_at,
            last_execution_status,
            enabled,
            max_consecutive_failures: max_consecutive_failures as u32,
            consecutive_failures: consecutive_failures as u32,
            pause_on_failure,
            parameters,
            created_at,
            updated_at,
            created_by,
        })
    }
}

/// Calculate next execution time from cron expression
fn calculate_next_cron_time(
    cron_expression: &str,
    from_time: chrono::DateTime<chrono::Utc>,
) -> chrono::DateTime<chrono::Utc> {
    let schedule: Schedule = cron_expression
        .parse()
        .map_err(|e| DomainError::InvalidJobSpec {
            field: "cron_expression".to_string(),
            reason: format!("Invalid cron expression: {}", e),
        })
        .expect("Invalid cron expression");

    let next = schedule
        .after(&from_time)
        .next()
        .expect("Could not calculate next execution time");

    next.with_timezone(&chrono::Utc)
}

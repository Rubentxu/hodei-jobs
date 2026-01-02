//! PostgreSQL ScheduledJob Repository
//!
//! Persistent repository implementation for ScheduledJobs based on PostgreSQL

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hodei_server_domain::jobs::{JobTemplateId, ScheduledJob, ScheduledJobRepository};
use hodei_server_domain::shared_kernel::{DomainError, Result};
use sqlx::postgres::PgPool;
use sqlx::{FromRow, Row};
use uuid::Uuid;

/// PostgreSQL ScheduledJob Repository
#[derive(Clone)]
pub struct PostgresScheduledJobRepository {
    pool: PgPool,
}

impl PostgresScheduledJobRepository {
    /// Create new repository with existing pool
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create repository connecting to database
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(10)
            .connect(url)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to connect to database: {}", e),
            })?;

        Ok(Self { pool })
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
    fn row_to_scheduled_job(row: sqlx::postgres::PgRow) -> Result<ScheduledJob> {
        let id: Uuid = row.try_get("id")?;
        let name: String = row.try_get("name")?;
        let description: Option<String> = row.try_get("description")?;
        let template_id_uuid: Uuid = row.try_get("template_id")?;
        let cron_expression: String = row.try_get("cron_expression")?;
        let timezone: String = row.try_get("timezone")?;
        let next_execution_at: DateTime<Utc> = row.try_get("next_execution_at")?;
        let last_execution_at: Option<DateTime<Utc>> = row.try_get("last_execution_at")?;
        let last_execution_status: Option<String> = row.try_get("last_execution_status")?;
        let enabled: bool = row.try_get("enabled")?;
        let max_consecutive_failures: i32 = row.try_get("max_consecutive_failures")?;
        let consecutive_failures: i32 = row.try_get("consecutive_failures")?;
        let pause_on_failure: bool = row.try_get("pause_on_failure")?;
        let parameters_json: serde_json::Value = row.try_get("parameters")?;
        let created_at: DateTime<Utc> = row.try_get("created_at")?;
        let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
        let created_by: Option<String> = row.try_get("created_by")?;

        // Deserialize parameters
        let parameters: std::collections::HashMap<String, String> =
            serde_json::from_value(parameters_json).expect("Failed to deserialize parameters");

        // Parse last execution status
        let last_execution_status = last_execution_status.as_ref().map(|s| match s.as_str() {
            "Queued" => hodei_server_domain::jobs::ExecutionStatus::Queued,
            "Running" => hodei_server_domain::jobs::ExecutionStatus::Running,
            "Succeeded" => hodei_server_domain::jobs::ExecutionStatus::Succeeded,
            "Failed" => hodei_server_domain::jobs::ExecutionStatus::Failed,
            "Error" => hodei_server_domain::jobs::ExecutionStatus::Error,
            _ => hodei_server_domain::jobs::ExecutionStatus::Queued,
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

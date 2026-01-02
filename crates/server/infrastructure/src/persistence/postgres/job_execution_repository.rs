//! PostgreSQL JobExecution Repository
//!
//! Persistent repository implementation for JobExecutions based on PostgreSQL

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hodei_server_domain::jobs::{JobExecution, JobExecutionRepository, JobSpec, JobTemplateId};
use hodei_server_domain::shared_kernel::{DomainError, JobId, Result};
use sqlx::postgres::PgPool;
use sqlx::{FromRow, Row};
use uuid::Uuid;

/// PostgreSQL JobExecution Repository
#[derive(Clone)]
pub struct PostgresJobExecutionRepository {
    pool: PgPool,
}

impl PostgresJobExecutionRepository {
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
impl JobExecutionRepository for PostgresJobExecutionRepository {
    async fn save(&self, execution: &JobExecution) -> Result<()> {
        let spec_json = serde_json::to_value(&execution.job_spec).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to serialize job spec: {}", e),
            }
        })?;

        let result_json = execution
            .result
            .as_ref()
            .map(|r| serde_json::to_value(r).expect("Failed to serialize execution result"));

        let parameters_json = serde_json::to_value(&execution.parameters).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to serialize parameters: {}", e),
            }
        })?;

        let resource_usage_json = execution
            .resource_usage
            .as_ref()
            .map(|r| serde_json::to_value(r).expect("Failed to serialize resource usage"));

        let metadata_json = serde_json::to_value(&execution.metadata).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to serialize metadata: {}", e),
            }
        })?;

        let job_id = execution.job_id.as_ref().map(|j| j.0);
        let scheduled_job_id = execution.scheduled_job_id;

        sqlx::query(
            r#"
            INSERT INTO job_executions
                (id, execution_number, template_id, template_version, job_id, job_name, job_spec,
                 state, result, queued_at, started_at, completed_at, triggered_by,
                 scheduled_job_id, triggered_by_user, parameters, resource_usage, metadata, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
            ON CONFLICT (id) DO UPDATE SET
                job_id = EXCLUDED.job_id,
                state = EXCLUDED.state,
                result = EXCLUDED.result,
                started_at = EXCLUDED.started_at,
                completed_at = EXCLUDED.completed_at,
                triggered_by_user = EXCLUDED.triggered_by_user,
                resource_usage = EXCLUDED.resource_usage
            "#,
        )
        .bind(execution.id)
        .bind(execution.execution_number as i64)
        .bind(execution.template_id.0)
        .bind(execution.template_version as i32)
        .bind(job_id)
        .bind(&execution.job_name)
        .bind(spec_json)
        .bind(execution.state.to_string())
        .bind(result_json)
        .bind(execution.queued_at)
        .bind(execution.started_at)
        .bind(execution.completed_at)
        .bind(execution.triggered_by.to_string())
        .bind(scheduled_job_id)
        .bind(execution.triggered_by_user.as_ref())
        .bind(parameters_json)
        .bind(resource_usage_json)
        .bind(metadata_json)
        .bind(execution.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to save job execution: {}", e),
        })?;

        Ok(())
    }

    async fn update(&self, execution: &JobExecution) -> Result<()> {
        // Same as save since we use ON CONFLICT
        self.save(execution).await
    }

    async fn find_by_id(&self, id: &Uuid) -> Result<Option<JobExecution>> {
        let row = sqlx::query(
            r#"
            SELECT id, execution_number, template_id, template_version, job_id, job_name, job_spec,
                   state, result, queued_at, started_at, completed_at, triggered_by,
                   scheduled_job_id, triggered_by_user, parameters, resource_usage, metadata, created_at
            FROM job_executions
            WHERE id = $1
            "#,
        )
        .bind(*id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find job execution: {}", e),
        })?;

        match row {
            Some(row) => Ok(Some(Self::row_to_execution(row)?)),
            None => Ok(None),
        }
    }

    async fn find_by_template_id(
        &self,
        template_id: &JobTemplateId,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<JobExecution>, usize)> {
        // Get total count
        let count_row = sqlx::query("SELECT COUNT(*) FROM job_executions WHERE template_id = $1")
            .bind(template_id.0)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to count job executions: {}", e),
            })?;

        let total: i64 = count_row.try_get(0)?;

        // Get executions
        let rows = sqlx::query(
            r#"
            SELECT id, execution_number, template_id, template_version, job_id, job_name, job_spec,
                   state, result, queued_at, started_at, completed_at, triggered_by,
                   scheduled_job_id, triggered_by_user, parameters, resource_usage, metadata, created_at
            FROM job_executions
            WHERE template_id = $1
            ORDER BY execution_number DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(template_id.0)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find job executions: {}", e),
        })?;

        let executions: Result<Vec<_>> = rows
            .into_iter()
            .map(|row| Self::row_to_execution(row))
            .collect();

        executions.map(|execs| (execs, total as usize))
    }

    async fn find_by_job_id(&self, job_id: &JobId) -> Result<Option<JobExecution>> {
        let row = sqlx::query(
            r#"
            SELECT id, execution_number, template_id, template_version, job_id, job_name, job_spec,
                   state, result, queued_at, started_at, completed_at, triggered_by,
                   scheduled_job_id, triggered_by_user, parameters, resource_usage, metadata, created_at
            FROM job_executions
            WHERE job_id = $1
            "#,
        )
        .bind(job_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find job execution by job_id: {}", e),
        })?;

        match row {
            Some(row) => Ok(Some(Self::row_to_execution(row)?)),
            None => Ok(None),
        }
    }

    async fn find_by_scheduled_job_id(
        &self,
        scheduled_job_id: &Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<JobExecution>, usize)> {
        let count_row =
            sqlx::query("SELECT COUNT(*) FROM job_executions WHERE scheduled_job_id = $1")
                .bind(*scheduled_job_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to count scheduled job executions: {}", e),
                })?;

        let total: i64 = count_row.try_get(0)?;

        let rows = sqlx::query(
            r#"
            SELECT id, execution_number, template_id, template_version, job_id, job_name, job_spec,
                   state, result, queued_at, started_at, completed_at, triggered_by,
                   scheduled_job_id, triggered_by_user, parameters, resource_usage, metadata, created_at
            FROM job_executions
            WHERE scheduled_job_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(*scheduled_job_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find scheduled job executions: {}", e),
        })?;

        let executions: Result<Vec<_>> = rows
            .into_iter()
            .map(|row| Self::row_to_execution(row))
            .collect();

        executions.map(|execs| (execs, total as usize))
    }

    async fn get_next_execution_number(&self, template_id: &JobTemplateId) -> Result<u64> {
        let row = sqlx::query(
            "SELECT COALESCE(MAX(execution_number), 0) + 1 FROM job_executions WHERE template_id = $1",
        )
        .bind(template_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to get next execution number: {}", e),
        })?;

        let next: i64 = row.try_get(0)?;
        Ok(next as u64)
    }

    async fn delete_older_than(&self, date: &DateTime<Utc>) -> Result<u64> {
        let result = sqlx::query("DELETE FROM job_executions WHERE created_at < $1")
            .bind(*date)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to delete old executions: {}", e),
            })?;

        Ok(result.rows_affected() as u64)
    }
}

impl PostgresJobExecutionRepository {
    /// Convert a database row to JobExecution
    fn row_to_execution(row: sqlx::postgres::PgRow) -> Result<JobExecution> {
        let id: Uuid = row.try_get("id")?;
        let execution_number: i64 = row.try_get("execution_number")?;
        let template_id_uuid: Uuid = row.try_get("template_id")?;
        let template_version: i32 = row.try_get("template_version")?;
        let job_id: Option<Uuid> = row.try_get("job_id")?;
        let job_name: String = row.try_get("job_name")?;
        let spec_json: serde_json::Value = row.try_get("job_spec")?;
        let state: String = row.try_get("state")?;
        let result_json: Option<serde_json::Value> = row.try_get("result")?;
        let queued_at: DateTime<Utc> = row.try_get("queued_at")?;
        let started_at: Option<DateTime<Utc>> = row.try_get("started_at")?;
        let completed_at: Option<DateTime<Utc>> = row.try_get("completed_at")?;
        let triggered_by: String = row.try_get("triggered_by")?;
        let scheduled_job_id: Option<Uuid> = row.try_get("scheduled_job_id")?;
        let triggered_by_user: Option<String> = row.try_get("triggered_by_user")?;
        let parameters_json: serde_json::Value = row.try_get("parameters")?;
        let resource_usage_json: Option<serde_json::Value> = row.try_get("resource_usage")?;
        let metadata_json: serde_json::Value = row.try_get("metadata")?;
        let created_at: DateTime<Utc> = row.try_get("created_at")?;

        // Deserialize job_spec
        let job_spec: JobSpec =
            serde_json::from_value(spec_json).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to deserialize job spec: {}", e),
            })?;

        // Deserialize result
        let result = result_json
            .map(|j| serde_json::from_value(j).expect("Failed to deserialize execution result"));

        // Deserialize parameters
        let parameters: std::collections::HashMap<String, String> =
            serde_json::from_value(parameters_json).expect("Failed to deserialize parameters");

        // Deserialize resource usage
        let resource_usage = resource_usage_json
            .map(|j| serde_json::from_value(j).expect("Failed to deserialize resource usage"));

        // Deserialize metadata
        let metadata: std::collections::HashMap<String, String> =
            serde_json::from_value(metadata_json).expect("Failed to deserialize metadata");

        // Parse triggered_by
        let triggered_by = match triggered_by.as_str() {
            "Manual" => hodei_server_domain::jobs::TriggerType::Manual,
            "Scheduled" => hodei_server_domain::jobs::TriggerType::Scheduled,
            "Api" => hodei_server_domain::jobs::TriggerType::Api,
            "Webhook" => hodei_server_domain::jobs::TriggerType::Webhook,
            "Retry" => hodei_server_domain::jobs::TriggerType::Retry,
            _ => hodei_server_domain::jobs::TriggerType::Manual,
        };

        // Parse state
        let state = match state.as_str() {
            "Queued" => hodei_server_domain::jobs::ExecutionStatus::Queued,
            "Running" => hodei_server_domain::jobs::ExecutionStatus::Running,
            "Succeeded" => hodei_server_domain::jobs::ExecutionStatus::Succeeded,
            "Failed" => hodei_server_domain::jobs::ExecutionStatus::Failed,
            "Error" => hodei_server_domain::jobs::ExecutionStatus::Error,
            _ => hodei_server_domain::jobs::ExecutionStatus::Queued,
        };

        Ok(JobExecution {
            id,
            execution_number: execution_number as u64,
            template_id: JobTemplateId(template_id_uuid),
            template_version: template_version as u32,
            job_id: job_id.map(|uuid| JobId(uuid)),
            job_name,
            job_spec,
            state,
            result,
            queued_at,
            started_at,
            completed_at,
            triggered_by,
            scheduled_job_id,
            triggered_by_user,
            parameters,
            resource_usage,
            metadata,
            created_at,
        })
    }
}

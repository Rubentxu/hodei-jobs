//! PostgreSQL Job Repository
//!
//! Persistent repository implementation for Jobs based on PostgreSQL

use hodei_server_domain::jobs::Job;
use hodei_server_domain::shared_kernel::{DomainError, JobId, Result};
use sqlx::Row;
use sqlx::postgres::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

/// Database configuration
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub connection_timeout: Duration,
}

impl DatabaseConfig {
    pub fn new(url: String, max_connections: u32, connection_timeout: Duration) -> Self {
        Self {
            url,
            max_connections,
            connection_timeout,
        }
    }
}

/// PostgreSQL Job Repository
#[derive(Clone)]
pub struct PostgresJobRepository {
    pool: PgPool,
}

impl PostgresJobRepository {
    /// Create new repository with existing pool
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create repository connecting to database
    pub async fn connect(config: &DatabaseConfig) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.connection_timeout)
            .connect(&config.url)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to connect to database: {}", e),
            })?;

        Ok(Self { pool })
    }

    /// Run migrations to create job tables
    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS jobs (
                id UUID PRIMARY KEY,
                spec JSONB NOT NULL,
                state VARCHAR(50) NOT NULL,
                selected_provider_id UUID,
                execution_context JSONB,
                attempts INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER NOT NULL DEFAULT 3,
                created_at TIMESTAMPTZ NOT NULL,
                started_at TIMESTAMPTZ,
                completed_at TIMESTAMPTZ,
                result JSONB,
                error_message TEXT,
                metadata JSONB NOT NULL DEFAULT '{}'
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create jobs table: {}", e),
        })?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_jobs_state ON jobs(state);")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to create jobs state index: {}", e),
            })?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at);")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to create jobs created_at index: {}", e),
            })?;

        Ok(())
    }

    fn state_to_string(state: &hodei_server_domain::shared_kernel::JobState) -> String {
        match state {
            hodei_server_domain::shared_kernel::JobState::Pending => "PENDING".to_string(),
            hodei_server_domain::shared_kernel::JobState::Scheduled => "SCHEDULED".to_string(),
            hodei_server_domain::shared_kernel::JobState::Running => "RUNNING".to_string(),
            hodei_server_domain::shared_kernel::JobState::Succeeded => "SUCCEEDED".to_string(),
            hodei_server_domain::shared_kernel::JobState::Failed => "FAILED".to_string(),
            hodei_server_domain::shared_kernel::JobState::Cancelled => "CANCELLED".to_string(),
            hodei_server_domain::shared_kernel::JobState::Timeout => "TIMEOUT".to_string(),
        }
    }

    fn string_to_state(s: &str) -> hodei_server_domain::shared_kernel::JobState {
        match s {
            "PENDING" => hodei_server_domain::shared_kernel::JobState::Pending,
            "SCHEDULED" => hodei_server_domain::shared_kernel::JobState::Scheduled,
            "RUNNING" => hodei_server_domain::shared_kernel::JobState::Running,
            "SUCCEEDED" => hodei_server_domain::shared_kernel::JobState::Succeeded,
            "FAILED" => hodei_server_domain::shared_kernel::JobState::Failed,
            "CANCELLED" => hodei_server_domain::shared_kernel::JobState::Cancelled,
            "TIMEOUT" => hodei_server_domain::shared_kernel::JobState::Timeout,
            _ => hodei_server_domain::shared_kernel::JobState::Failed,
        }
    }
}

#[async_trait::async_trait]
impl hodei_server_domain::jobs::JobRepository for PostgresJobRepository {
    async fn save(&self, job: &Job) -> Result<()> {
        let spec_json =
            serde_json::to_value(&job.spec).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to serialize job spec: {}", e),
            })?;

        let context_json = if let Some(ctx) = job.execution_context() {
            Some(
                serde_json::to_value(ctx).map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to serialize execution context: {}", e),
                })?,
            )
        } else {
            None
        };

        let result_json = if let Some(res) = job.result() {
            Some(
                serde_json::to_value(res).map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to serialize result: {}", e),
                })?,
            )
        } else {
            None
        };

        let metadata_json =
            serde_json::to_value(job.metadata()).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to serialize metadata: {}", e),
            })?;

        let provider_id = job.selected_provider().map(|p| *p.as_uuid());

        // Start transaction for atomic Save + Enqueue
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to begin transaction: {}", e),
            })?;

        sqlx::query(
            r#"
            INSERT INTO jobs
                (id, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
                 created_at, started_at, completed_at, result, error_message, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (id) DO UPDATE SET
                spec = EXCLUDED.spec,
                state = EXCLUDED.state,
                selected_provider_id = EXCLUDED.selected_provider_id,
                execution_context = EXCLUDED.execution_context,
                attempts = EXCLUDED.attempts,
                max_attempts = EXCLUDED.max_attempts,
                created_at = EXCLUDED.created_at,
                started_at = EXCLUDED.started_at,
                completed_at = EXCLUDED.completed_at,
                result = EXCLUDED.result,
                error_message = EXCLUDED.error_message,
                metadata = EXCLUDED.metadata
            "#,
        )
        .bind(job.id.0)
        .bind(spec_json)
        .bind(Self::state_to_string(job.state()))
        .bind(provider_id)
        .bind(context_json)
        .bind(job.attempts() as i32)
        .bind(job.max_attempts() as i32)
        .bind(job.created_at())
        .bind(job.started_at())
        .bind(job.completed_at())
        .bind(result_json)
        .bind(job.error_message())
        .bind(metadata_json)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to save job: {}", e),
        })?;

        // Atomic Enqueue if Pending
        if matches!(
            job.state(),
            hodei_server_domain::shared_kernel::JobState::Pending
        ) {
            sqlx::query(
                r#"
                INSERT INTO job_queue (job_id)
                VALUES ($1)
                ON CONFLICT (job_id) DO NOTHING
                "#,
            )
            .bind(job.id.0)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to enqueue job atomically: {}", e),
            })?;
        }

        tx.commit()
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to commit transaction: {}", e),
            })?;

        Ok(())
    }

    async fn find_by_id(&self, job_id: &JobId) -> Result<Option<Job>> {
        let row = sqlx::query(
            r#"
            SELECT id, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
                   created_at, started_at, completed_at, result, error_message, metadata
            FROM jobs
            WHERE id = $1
            "#,
        )
        .bind(job_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find job by id: {}", e),
        })?;

        if let Some(row) = row {
            Ok(Some(map_row_to_job(row)?))
        } else {
            Ok(None)
        }
    }

    async fn find_by_state(
        &self,
        state: &hodei_server_domain::shared_kernel::JobState,
    ) -> Result<Vec<Job>> {
        let rows = sqlx::query(
            r#"
            SELECT id, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
                   created_at, started_at, completed_at, result, error_message, metadata
            FROM jobs
            WHERE state = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(Self::state_to_string(state))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find jobs by state: {}", e),
        })?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(map_row_to_job(row)?);
        }

        Ok(jobs)
    }

    async fn find_pending(&self) -> Result<Vec<Job>> {
        let rows = sqlx::query(
            r#"
            SELECT id, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
                   created_at, started_at, completed_at, result, error_message, metadata
            FROM jobs
            WHERE state = 'PENDING'
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find pending jobs: {}", e),
        })?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(map_row_to_job(row)?);
        }

        Ok(jobs)
    }

    async fn find_all(&self, limit: usize, offset: usize) -> Result<(Vec<Job>, usize)> {
        let rows: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) as total
            FROM jobs
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to count jobs: {}", e),
        })?;

        let total = rows.0 as usize;

        let job_rows = sqlx::query(
            r#"
            SELECT id, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
                   created_at, started_at, completed_at, result, error_message, metadata
            FROM jobs
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find all jobs: {}", e),
        })?;

        let mut jobs = Vec::new();
        for row in job_rows {
            let job = map_row_to_job(row)?;
            jobs.push(job);
        }

        Ok((jobs, total))
    }

    async fn find_by_execution_id(&self, execution_id: &str) -> Result<Option<Job>> {
        let row_opt = sqlx::query(
            r#"
            SELECT id, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
                   created_at, started_at, completed_at, result, error_message, metadata
            FROM jobs
            WHERE execution_context->>'execution_id' = $1
            "#,
        )
        .bind(execution_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find job by execution id: {}", e),
        })?;

        if let Some(row) = row_opt {
            let job = map_row_to_job(row)?;
            Ok(Some(job))
        } else {
            Ok(None)
        }
    }

    async fn update(&self, job: &Job) -> Result<()> {
        self.save(job).await
    }

    async fn delete(&self, job_id: &JobId) -> Result<()> {
        sqlx::query("DELETE FROM jobs WHERE id = $1")
            .bind(job_id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to delete job: {}", e),
            })?;

        Ok(())
    }
}

fn map_row_to_job(row: sqlx::postgres::PgRow) -> Result<Job> {
    let id: uuid::Uuid = row.get("id");
    let spec_json: serde_json::Value = row.get("spec");
    let state_str: String = row.get("state");
    let selected_provider_id: Option<uuid::Uuid> = row.get("selected_provider_id");
    let execution_context_json: Option<serde_json::Value> = row.get("execution_context");
    let attempts: i32 = row.get("attempts");
    let max_attempts: i32 = row.get("max_attempts");
    let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
    let started_at: Option<chrono::DateTime<chrono::Utc>> = row.get("started_at");
    let completed_at: Option<chrono::DateTime<chrono::Utc>> = row.get("completed_at");
    let result_json: Option<serde_json::Value> = row.get("result");
    let error_message: Option<String> = row.get("error_message");
    let metadata_json: serde_json::Value = row.get("metadata");

    let spec: hodei_server_domain::jobs::JobSpec =
        serde_json::from_value(spec_json).map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to deserialize job spec: {}", e),
        })?;

    let execution_context = if let Some(ctx_json) = execution_context_json {
        Some(
            serde_json::from_value(ctx_json).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to deserialize execution context: {}", e),
            })?,
        )
    } else {
        None
    };

    let result = if let Some(res_json) = result_json {
        Some(
            serde_json::from_value(res_json).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to deserialize result: {}", e),
            })?,
        )
    } else {
        None
    };

    let metadata: std::collections::HashMap<String, String> =
        serde_json::from_value(metadata_json).unwrap_or_default();

    Ok(Job::hydrate(
        JobId(id),
        spec,
        PostgresJobRepository::string_to_state(&state_str),
        selected_provider_id.map(hodei_server_domain::shared_kernel::ProviderId),
        execution_context,
        attempts as u32,
        max_attempts as u32,
        created_at,
        started_at,
        completed_at,
        result,
        error_message,
        metadata,
    ))
}

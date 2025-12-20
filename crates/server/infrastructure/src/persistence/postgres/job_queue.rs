//! PostgreSQL Job Queue
//!
//! Queue implementation for job scheduling based on PostgreSQL

use hodei_server_domain::jobs::{Job, JobQueue};
use hodei_server_domain::shared_kernel::{DomainError, JobId, JobState, ProviderId, Result};
use sqlx::{Row, postgres::PgPool};

use super::DatabaseConfig;

/// PostgreSQL Job Queue
#[derive(Clone)]
pub struct PostgresJobQueue {
    pool: PgPool,
}

impl PostgresJobQueue {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn connect(config: &DatabaseConfig) -> Result<Self> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.connection_timeout)
            .connect(&config.url)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to connect to database: {}", e),
            })?;

        Ok(Self { pool })
    }

    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS job_queue (
                id BIGSERIAL PRIMARY KEY,
                job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
                enqueued_at TIMESTAMPTZ NOT NULL DEFAULT now()
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create job_queue table: {}", e),
        })?;

        sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS uq_job_queue_job_id ON job_queue(job_id);")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to create job_queue unique index: {}", e),
            })?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_job_queue_enqueued_at ON job_queue(enqueued_at);",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create job_queue enqueued_at index: {}", e),
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

    let state = match state_str.as_str() {
        "PENDING" => JobState::Pending,
        "SCHEDULED" => JobState::Scheduled,
        "RUNNING" => JobState::Running,
        "SUCCEEDED" => JobState::Succeeded,
        "FAILED" => JobState::Failed,
        "CANCELLED" => JobState::Cancelled,
        "TIMEOUT" => JobState::Timeout,
        _ => JobState::Failed,
    };

    Ok(Job::hydrate(
        JobId(id),
        spec,
        state,
        selected_provider_id.map(ProviderId),
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

#[async_trait::async_trait]
impl JobQueue for PostgresJobQueue {
    async fn enqueue(&self, job: Job) -> Result<()> {
        tracing::info!("PostgresJobQueue::enqueue called for job_id: {}", job.id.0);
        let result = sqlx::query(
            r#"
            INSERT INTO job_queue (job_id)
            VALUES ($1)
            ON CONFLICT (job_id) DO NOTHING
            "#,
        )
        .bind(job.id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to enqueue job: {}", e),
        })?;

        tracing::info!(
            "PostgresJobQueue::enqueue result - rows_affected: {}",
            result.rows_affected()
        );
        Ok(())
    }

    async fn dequeue(&self) -> Result<Option<Job>> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to start transaction for dequeue: {}", e),
            })?;

        // Atomically claim the job by updating its state and removing from queue
        let claim_result: Option<(
            uuid::Uuid,
            serde_json::Value,
            String,
            Option<uuid::Uuid>,
            Option<serde_json::Value>,
            i32,
            i32,
            chrono::DateTime<chrono::Utc>,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<serde_json::Value>,
            Option<String>,
            serde_json::Value,
        )> = sqlx::query_as(
            r#"
            WITH claimed_job AS (
                SELECT jq.job_id
                FROM job_queue jq
                WHERE jq.job_id IN (
                    SELECT id FROM jobs WHERE state = 'PENDING'
                )
                ORDER BY jq.enqueued_at ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            UPDATE jobs j
            SET state = 'ASSIGNED'
            WHERE j.id = (SELECT job_id FROM claimed_job)
            RETURNING j.id, j.spec, j.state, j.selected_provider_id, j.execution_context,
                      j.attempts, j.max_attempts, j.created_at, j.started_at,
                      j.completed_at, j.result, j.error_message, j.metadata
            "#,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to claim job: {}", e),
        })?;

        if let Some(job_data) = claim_result {
            // Remove from queue
            let job_id = job_data.0;
            sqlx::query("DELETE FROM job_queue WHERE job_id = $1")
                .bind(job_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to remove dequeued job from queue: {}", e),
                })?;

            tx.commit()
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to commit dequeue transaction: {}", e),
                })?;

            // Reconstruct job from data
            let (
                id,
                spec_json,
                state_str,
                selected_provider_id,
                execution_context_json,
                attempts,
                max_attempts,
                created_at,
                started_at,
                completed_at,
                result_json,
                error_message,
                metadata_json,
            ) = job_data;

            let spec: hodei_server_domain::jobs::JobSpec = serde_json::from_value(spec_json)
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to deserialize job spec: {}", e),
                })?;

            let execution_context = if let Some(ctx_json) = execution_context_json {
                Some(serde_json::from_value(ctx_json).map_err(|e| {
                    DomainError::InfrastructureError {
                        message: format!("Failed to deserialize execution context: {}", e),
                    }
                })?)
            } else {
                None
            };

            let result = if let Some(res_json) = result_json {
                Some(serde_json::from_value(res_json).map_err(|e| {
                    DomainError::InfrastructureError {
                        message: format!("Failed to deserialize result: {}", e),
                    }
                })?)
            } else {
                None
            };

            let metadata: std::collections::HashMap<String, String> =
                serde_json::from_value(metadata_json).unwrap_or_default();

            let state = match state_str.as_str() {
                "ASSIGNED" => JobState::Assigned,
                "PENDING" => JobState::Pending,
                "SCHEDULED" => JobState::Scheduled,
                "RUNNING" => JobState::Running,
                "SUCCEEDED" => JobState::Succeeded,
                "FAILED" => JobState::Failed,
                "CANCELLED" => JobState::Cancelled,
                "TIMEOUT" => JobState::Timeout,
                _ => JobState::Failed,
            };

            let job = Job::hydrate(
                JobId(id),
                spec,
                state,
                selected_provider_id.map(ProviderId),
                execution_context,
                attempts as u32,
                max_attempts as u32,
                created_at,
                started_at,
                completed_at,
                result,
                error_message,
                metadata,
            );

            Ok(Some(job))
        } else {
            tx.rollback().await.ok();
            Ok(None)
        }
    }

    async fn len(&self) -> Result<usize> {
        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) as count
            FROM job_queue jq
            JOIN jobs j ON jq.job_id = j.id
            WHERE j.state = 'PENDING'
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to get queue length: {}", e),
        })?;

        Ok(row.0 as usize)
    }

    async fn is_empty(&self) -> Result<bool> {
        Ok(self.len().await? == 0)
    }

    async fn clear(&self) -> Result<()> {
        let _ = sqlx::query("DELETE FROM job_queue")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to clear queue: {}", e),
            })?;

        Ok(())
    }
}

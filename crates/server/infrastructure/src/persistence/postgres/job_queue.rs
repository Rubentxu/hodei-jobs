//! PostgreSQL Job Queue
//!
//! Queue implementation for job scheduling based on PostgreSQL
//!
//! # Pool Management
//!
//! This queue expects a `PgPool` to be passed in via `new()`.
//! The pool should be created using `DatabasePool` for consistent configuration.

use hodei_server_domain::jobs::{Job, JobQueue};
use hodei_server_domain::shared_kernel::{DomainError, JobId, JobState, ProviderId, Result};
use sqlx::{Row, postgres::PgPool};

/// PostgreSQL Job Queue
#[derive(Clone)]
pub struct PostgresJobQueue {
    pool: PgPool,
}

impl PostgresJobQueue {
    /// Create new queue with existing pool
    ///
    /// The pool should be created centrally (e.g., using `DatabasePool`)
    /// to ensure consistent configuration across all repositories.
    #[inline]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Run migrations to create job queue tables
    ///
    /// DEPRECATED: Migrations are now handled by the central MigrationService.
    /// This method is kept for backwards compatibility but does nothing.
    pub async fn run_migrations(&self) -> Result<()> {
        // Migrations are now handled by the central MigrationService
        // See: hodei_server_infrastructure::persistence::postgres::migrations::run_migrations
        Ok(())
    }
}

#[allow(dead_code)]
pub(super) fn map_row_to_job(row: sqlx::postgres::PgRow) -> Result<Job> {
    let id: uuid::Uuid = row.get("id");
    let name: String = row.get("name");
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
        name,
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
        None, // template_id
        None, // template_version
        None, // execution_number
        None, // triggered_by
    ))
}

#[async_trait::async_trait]
impl JobQueue for PostgresJobQueue {
    async fn enqueue(&self, job: Job) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to start transaction for enqueue: {}", e),
            })?;

        // Insert into queue
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
            message: format!("Failed to insert into job_queue: {}", e),
        })?;

        // Ensure job state is PENDING so it can be picked up by dequeue
        sqlx::query("UPDATE jobs SET state = 'PENDING' WHERE id = $1")
            .bind(job.id.0)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to update job state to PENDING: {}", e),
            })?;

        tx.commit()
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to commit enqueue transaction: {}", e),
            })?;

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
        // Use query() instead of query_as!() to avoid compile-time schema verification
        let claim_row = sqlx::query(
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
            RETURNING j.id, j.name, j.spec, j.state, j.selected_provider_id, j.execution_context,
                      j.attempts, j.max_attempts, j.created_at, j.started_at,
                      j.completed_at, j.result, j.error_message, j.metadata
            "#,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to claim job: {}", e),
        })?;

        if let Some(row) = claim_row {
            // Remove from queue
            let job_id: uuid::Uuid = row.get("id");
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

            // Reconstruct job from row using map_row_to_job
            let job = map_row_to_job(row)?;
            Ok(Some(job))
        } else {
            tx.rollback().await.ok();
            Ok(None)
        }
    }

    async fn peek(&self) -> Result<Option<Job>> {
        // Get the next job from the queue without dequeuing
        // Use query() instead of query_as!() to avoid compile-time schema verification
        let row = sqlx::query(
            r#"
            SELECT j.id, j.spec
            FROM job_queue jq
            JOIN jobs j ON jq.job_id = j.id
            WHERE j.state = 'PENDING'
            ORDER BY jq.priority DESC, jq.enqueued_at ASC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to peek queue: {}", e),
        })?;

        match row {
            Some(r) => {
                let _id: uuid::Uuid = r.get("id");
                let spec: serde_json::Value = r.get("spec");
                let job: Job =
                    serde_json::from_value(spec).map_err(|e| DomainError::InfrastructureError {
                        message: format!("Failed to deserialize job spec: {}", e),
                    })?;
                Ok(Some(job))
            }
            None => Ok(None),
        }
    }

    async fn len(&self) -> Result<usize> {
        // Use query() instead of query_as!() to avoid compile-time schema verification
        let row = sqlx::query(
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

        let count: i64 = row.try_get("count").unwrap_or(0);
        Ok(count as usize)
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

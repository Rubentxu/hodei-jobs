//! Transactional Job Repository Implementation
//!
//! PostgreSQL implementation of JobRepositoryTx.

use super::PostgresJobRepository;
use async_trait::async_trait;
use hodei_server_domain::jobs::{Job, aggregate::JobRepositoryTx};
use hodei_server_domain::shared_kernel::{DomainError, JobId, Result};
use hodei_server_domain::transaction::PgTransaction;
use sqlx::Row;

#[async_trait]
impl JobRepositoryTx for PostgresJobRepository {
    async fn save_with_tx(&self, tx: &mut PgTransaction<'_>, job: &Job) -> Result<()> {
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

        sqlx::query(
            r#"
            INSERT INTO jobs
                (id, name, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
                 created_at, started_at, completed_at, result, error_message, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
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
                metadata = EXCLUDED.metadata,
                updated_at = NOW()
            "#,
        )
        .bind(job.id.0)
        .bind(job.name.clone())
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
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to save job in transaction: {}", e),
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
            .execute(&mut **tx)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to enqueue job atomically in transaction: {}", e),
            })?;
        }

        Ok(())
    }

    async fn find_by_id_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        job_id: &JobId,
    ) -> Result<Option<Job>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
                   created_at, started_at, completed_at, result, error_message, metadata
            FROM jobs
            WHERE id = $1
            "#,
        )
        .bind(job_id.0)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find job by id in transaction: {}", e),
        })?;

        if let Some(row) = row {
            Ok(Some(super::job_repository::map_row_to_job(row)?))
        } else {
            Ok(None)
        }
    }

    async fn update_with_tx(&self, tx: &mut PgTransaction<'_>, job: &Job) -> Result<()> {
        self.save_with_tx(tx, job).await
    }

    async fn update_status_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        job_id: &JobId,
        new_status: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE jobs SET state = $1, updated_at = NOW() WHERE id = $2")
            .bind(new_status)
            .bind(job_id.0)
            .execute(&mut **tx)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to update job status in transaction: {}", e),
            })?;

        Ok(())
    }

    async fn find_by_state_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        state: &hodei_server_domain::shared_kernel::JobState,
    ) -> Result<Vec<Job>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
                   created_at, started_at, completed_at, result, error_message, metadata
            FROM jobs
            WHERE state = $1
            "#,
        )
        .bind(Self::state_to_string(state))
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find jobs by state in transaction: {}", e),
        })?;

        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(super::job_repository::map_row_to_job(row)?);
        }
        Ok(jobs)
    }

    async fn update_state_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        job_id: &JobId,
        state: hodei_server_domain::shared_kernel::JobState,
    ) -> Result<()> {
        self.update_status_with_tx(tx, job_id, &Self::state_to_string(&state))
            .await
    }
}

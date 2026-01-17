//! Transactional Job Repository Implementation
//!
//! PostgreSQL implementation of JobRepositoryTx for the Transactional Outbox Pattern.
//! These operations are designed to be called within an existing database transaction.

use super::PostgresJobRepository;
use super::job_repository::map_row_to_job as map_job_row;
use hodei_server_domain::jobs::{Job, JobRepositoryTx};
use hodei_server_domain::shared_kernel::{DomainError, JobId, Result};
use sqlx::PgTransaction;

/// PostgreSQL Transaction-aware Job Repository
///
/// Extends PostgresJobRepository with transaction-aware operations
/// for the Transactional Outbox Pattern.
#[async_trait::async_trait]
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
        .bind(PostgresJobRepository::state_to_string(job.state()))
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

        // Atomic Enqueue if Pending (only PENDING jobs go to the queue)
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
                message: format!("Failed to enqueue job atomically: {}", e),
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
            SELECT id, spec, state, selected_provider_id, execution_context, attempts, max_attempts,
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
            Ok(Some(map_job_row(row)?))
        } else {
            Ok(None)
        }
    }

    async fn update_with_tx(&self, tx: &mut PgTransaction<'_>, job: &Job) -> Result<()> {
        // Delegate to save_with_tx since it handles upsert
        self.save_with_tx(tx, job).await
    }

    async fn update_status_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        job_id: &JobId,
        new_status: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE jobs
            SET state = $2
            WHERE id = $1
            "#,
        )
        .bind(job_id.0)
        .bind(new_status)
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to update job status in transaction: {}", e),
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::jobs::JobSpec;
    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

    /// Creates a test database with a unique name
    async fn setup_test_db() -> sqlx::PgPool {
        let connection_string = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://hodei:hodei@localhost:5432/hodei".to_string());

        let db_name = format!("hodei_tx_test_{}", Uuid::new_v4());
        let base_url = connection_string.trim_end_matches(&format!(
            "/{}",
            connection_string.split('/').last().unwrap()
        ));
        let admin_conn_string = format!("{}/postgres", base_url);

        let mut admin_conn = sqlx::postgres::PgPool::connect(&admin_conn_string)
            .await
            .expect("Failed to connect to postgres");

        sqlx::query(&format!("DROP DATABASE IF EXISTS {}", db_name))
            .execute(&admin_conn)
            .await
            .expect("Failed to drop test database");

        sqlx::query(&format!("CREATE DATABASE {}", db_name))
            .execute(&admin_conn)
            .await
            .expect("Failed to create test database");

        let test_conn_string = format!("{}/{}", base_url, db_name);

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&test_conn_string)
            .await
            .expect("Failed to connect to test database");

        // Run migrations
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS jobs (
                id UUID PRIMARY KEY,
                spec JSONB NOT NULL,
                state VARCHAR(20) NOT NULL,
                selected_provider_id UUID,
                execution_context JSONB,
                attempts INTEGER DEFAULT 1,
                max_attempts INTEGER DEFAULT 3,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                started_at TIMESTAMPTZ,
                completed_at TIMESTAMPTZ,
                result JSONB,
                error_message TEXT,
                metadata JSONB DEFAULT '{}',
                UNIQUE(id)
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create jobs table");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS job_queue (
                job_id UUID PRIMARY KEY,
                enqueued_at TIMESTAMPTZ DEFAULT NOW(),
                UNIQUE(job_id)
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create job_queue table");

        pool
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_save_with_tx_commit() {
        let pool = setup_test_db().await;
        let repo = PostgresJobRepository::new(pool.clone());

        let job_id = JobId(Uuid::new_v4());
        let spec = JobSpec::new(vec!["echo".to_string(), "test".to_string()]);
        let job = Job::new(job_id.clone(), "tx-commit-job".to_string(), spec);

        // Begin transaction
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        // Save job within transaction
        repo.save_with_tx(&mut tx, &job)
            .await
            .expect("Failed to save job");

        // Verify job is not visible outside transaction (not committed)
        let job_outside = repo.find_by_id(&job_id).await.expect("Failed to find job");
        assert!(
            job_outside.is_none(),
            "Job should not be visible before commit"
        );

        // Commit transaction
        tx.commit().await.expect("Failed to commit");

        // Verify job is now visible
        let job_after = repo
            .find_by_id(&job_id)
            .await
            .expect("Failed to find job after commit");
        assert!(job_after.is_some(), "Job should be visible after commit");
        assert_eq!(
            job_after.unwrap().state(),
            &hodei_server_domain::shared_kernel::JobState::Pending
        );

        // Cleanup
        sqlx::query("DROP DATABASE hodei_tx_test")
            .execute(&pool)
            .await
            .ok();
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_save_with_tx_rollback() {
        let pool = setup_test_db().await;
        let repo = PostgresJobRepository::new(pool.clone());

        let job_id = JobId(Uuid::new_v4());
        let spec = JobSpec::new(vec!["echo".to_string(), "test".to_string()]);
        let job = Job::new(job_id.clone(), "tx-rollback-job".to_string(), spec);

        // Begin transaction
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        // Save job within transaction
        repo.save_with_tx(&mut tx, &job)
            .await
            .expect("Failed to save job");

        // Rollback transaction
        tx.rollback().await.expect("Failed to rollback");

        // Verify job is NOT visible after rollback
        let job_after = repo.find_by_id(&job_id).await.expect("Failed to find job");
        assert!(
            job_after.is_none(),
            "Job should not be visible after rollback"
        );

        // Cleanup
        sqlx::query("DROP DATABASE hodei_tx_test")
            .execute(&pool)
            .await
            .ok();
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_update_status_with_tx() {
        let pool = setup_test_db().await;
        let repo = PostgresJobRepository::new(pool.clone());

        let job_id = JobId(Uuid::new_v4());
        let spec = JobSpec::new(vec!["echo".to_string(), "test".to_string()]);
        let job = Job::new(job_id.clone(), "tx-update-job".to_string(), spec);

        // First save the job
        repo.save(&job).await.expect("Failed to save job");

        // Begin transaction to update status
        let mut tx = pool.begin().await.expect("Failed to begin transaction");
        repo.update_status_with_tx(&mut tx, &job_id, "RUNNING")
            .await
            .expect("Failed to update status");
        tx.commit().await.expect("Failed to commit");

        // Verify status was updated
        let updated_job = repo.find_by_id(&job_id).await.expect("Failed to find job");
        assert_eq!(
            updated_job.unwrap().state(),
            &hodei_server_domain::shared_kernel::JobState::Running
        );

        // Cleanup
        sqlx::query("DROP DATABASE hodei_tx_test")
            .execute(&pool)
            .await
            .ok();
    }
}

//! PostgreSQL DLQ Repository
//!
//! SQLx-based implementation of DlqRepository for PostgreSQL.

use hodei_server_domain::outbox::{
    OutboxError, dlq_model::DlqEntry, dlq_repository::DlqRepository, dlq_stats::DlqStats,
};
use sqlx::PgPool;
use uuid::Uuid;

/// Error type specific to PostgreSQL DLQ repository
#[derive(Debug, thiserror::Error)]
pub enum PostgresDlqRepositoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("DLQ domain error: {0}")]
    Dlq(#[from] OutboxError),

    #[error("Infrastructure error: {message}")]
    InfrastructureError { message: String },
}

impl std::fmt::Display for PostgresDlqRepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl From<PostgresDlqRepositoryError> for OutboxError {
    fn from(err: PostgresDlqRepositoryError) -> Self {
        match err {
            PostgresDlqRepositoryError::Database(e) => OutboxError::Database(e),
            PostgresDlqRepositoryError::Dlq(e) => e,
            PostgresDlqRepositoryError::InfrastructureError { message } => {
                OutboxError::InfrastructureError { message }
            }
        }
    }
}

/// PostgreSQL implementation of DlqRepository
pub struct PostgresDlqRepository {
    pool: PgPool,
}

impl PostgresDlqRepository {
    /// Create a new PostgreSQL DLQ repository
    ///
    /// # Arguments
    /// * `pool` - PostgreSQL connection pool
    ///
    /// # Returns
    /// * `Self` - New repository instance
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Run database migrations for the DLQ table
    pub async fn run_migrations(&self) -> Result<(), PostgresDlqRepositoryError> {
        // Create table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS outbox_dlq (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                original_event_id UUID NOT NULL,
                aggregate_id UUID NOT NULL,
                aggregate_type VARCHAR(20) NOT NULL CHECK (aggregate_type IN ('JOB', 'WORKER', 'PROVIDER')),
                event_type VARCHAR(50) NOT NULL,
                payload JSONB NOT NULL,
                metadata JSONB,
                error_message TEXT NOT NULL,
                retry_count INTEGER NOT NULL,
                original_created_at TIMESTAMPTZ NOT NULL,
                moved_at TIMESTAMPTZ DEFAULT NOW(),
                resolved_at TIMESTAMPTZ,
                resolution_notes TEXT,
                resolved_by VARCHAR(100),
                UNIQUE(original_event_id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create indexes
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_dlq_status
            ON outbox_dlq(moved_at)
            WHERE resolved_at IS NULL
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_dlq_aggregate
            ON outbox_dlq(aggregate_type, aggregate_id)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_dlq_event_type
            ON outbox_dlq(event_type, moved_at)
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl DlqRepository for PostgresDlqRepository {
    type Error = PostgresDlqRepositoryError;

    async fn insert(&self, entry: &DlqEntry) -> Result<(), Self::Error> {
        sqlx::query(
            r#"
            INSERT INTO outbox_dlq (
                id, original_event_id, aggregate_id, aggregate_type, event_type,
                payload, metadata, error_message, retry_count,
                original_created_at, moved_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
            ON CONFLICT (original_event_id) DO NOTHING
            "#,
        )
        .bind(&entry.id)
        .bind(&entry.original_event_id)
        .bind(&entry.aggregate_id)
        .bind(&entry.aggregate_type)
        .bind(&entry.event_type)
        .bind(&entry.payload)
        .bind(&entry.metadata)
        .bind(&entry.error_message)
        .bind(entry.retry_count)
        .bind(&entry.original_created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<DlqEntry>, Self::Error> {
        let row = sqlx::query(
            r#"
            SELECT id, original_event_id, aggregate_id, aggregate_type, event_type,
                   payload, metadata, error_message, retry_count,
                   original_created_at, moved_at, resolved_at, resolution_notes, resolved_by
            FROM outbox_dlq
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(DlqEntry {
                id: row.id,
                original_event_id: row.original_event_id,
                aggregate_id: row.aggregate_id,
                aggregate_type: row.aggregate_type,
                event_type: row.event_type,
                payload: row.payload,
                metadata: row.metadata,
                error_message: row.error_message,
                retry_count: row.retry_count,
                original_created_at: row.original_created_at,
                moved_at: row.moved_at,
                resolved_at: row.resolved_at,
                resolution_notes: row.resolution_notes,
                resolved_by: row.resolved_by,
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_by_event_id(&self, event_id: Uuid) -> Result<Option<DlqEntry>, Self::Error> {
        let row = sqlx::query(
            r#"
            SELECT id, original_event_id, aggregate_id, aggregate_type, event_type,
                   payload, metadata, error_message, retry_count,
                   original_created_at, moved_at, resolved_at, resolution_notes, resolved_by
            FROM outbox_dlq
            WHERE original_event_id = $1
            "#,
        )
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(DlqEntry {
                id: row.id,
                original_event_id: row.original_event_id,
                aggregate_id: row.aggregate_id,
                aggregate_type: row.aggregate_type,
                event_type: row.event_type,
                payload: row.payload,
                metadata: row.metadata,
                error_message: row.error_message,
                retry_count: row.retry_count,
                original_created_at: row.original_created_at,
                moved_at: row.moved_at,
                resolved_at: row.resolved_at,
                resolution_notes: row.resolution_notes,
                resolved_by: row.resolved_by,
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_pending(&self, limit: usize, offset: usize) -> Result<Vec<DlqEntry>, Self::Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, original_event_id, aggregate_id, aggregate_type, event_type,
                   payload, metadata, error_message, retry_count,
                   original_created_at, moved_at, resolved_at, resolution_notes, resolved_by
            FROM outbox_dlq
            WHERE resolved_at IS NULL
            ORDER BY moved_at ASC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await?;

        let entries = rows
            .into_iter()
            .map(|row| DlqEntry {
                id: row.id,
                original_event_id: row.original_event_id,
                aggregate_id: row.aggregate_id,
                aggregate_type: row.aggregate_type,
                event_type: row.event_type,
                payload: row.payload,
                metadata: row.metadata,
                error_message: row.error_message,
                retry_count: row.retry_count,
                original_created_at: row.original_created_at,
                moved_at: row.moved_at,
                resolved_at: row.resolved_at,
                resolution_notes: row.resolution_notes,
                resolved_by: row.resolved_by,
            })
            .collect();

        Ok(entries)
    }

    async fn resolve(&self, id: Uuid, notes: &str, resolved_by: &str) -> Result<(), Self::Error> {
        sqlx::query(
            r#"
            UPDATE outbox_dlq
            SET resolved_at = NOW(),
                resolution_notes = $2,
                resolved_by = $3
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(notes)
        .bind(resolved_by)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn requeue(&self, id: Uuid) -> Result<Option<Uuid>, Self::Error> {
        // Get the original event ID first
        let row = sqlx::query(
            r#"
            SELECT original_event_id FROM outbox_dlq WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let original_event_id = row.original_event_id;

            // Delete from DLQ
            sqlx::query("DELETE FROM outbox_dlq WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await?;

            // Reset the original event for retry
            sqlx::query(
                r#"
                UPDATE outbox_events
                SET status = 'PENDING',
                    retry_count = 0,
                    last_error = NULL
                WHERE id = $1
                "#,
            )
            .bind(original_event_id)
            .execute(&self.pool)
            .await?;

            Ok(Some(original_event_id))
        } else {
            Ok(None)
        }
    }

    async fn get_stats(&self) -> Result<DlqStats, Self::Error> {
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(CASE WHEN resolved_at IS NULL THEN 1 END) as pending_count,
                COUNT(CASE WHEN resolved_at IS NOT NULL THEN 1 END) as resolved_count,
                CAST(MIN(CASE WHEN resolved_at IS NULL THEN EXTRACT(EPOCH FROM (NOW() - moved_at)) END) AS BIGINT) as oldest_pending_age_seconds
            FROM outbox_dlq
            "#
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(DlqStats {
            pending_count: row.pending_count.unwrap_or(0) as u64,
            resolved_count: row.resolved_count.unwrap_or(0) as u64,
            oldest_pending_age_seconds: row.oldest_pending_age_seconds,
        })
    }

    async fn delete(&self, id: Uuid) -> Result<(), Self::Error> {
        sqlx::query("DELETE FROM outbox_dlq WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn cleanup_resolved(&self, older_than: std::time::Duration) -> Result<u64, Self::Error> {
        let older_than_secs = older_than.as_secs_f64();
        let result = sqlx::query(
            r#"
            DELETE FROM outbox_dlq
            WHERE resolved_at IS NOT NULL
            AND resolved_at < NOW() - make_interval(secs => $1)
            "#,
        )
        .bind(older_than_secs)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::outbox::DlqRepository;
    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

    async fn setup_test_db() -> PgPool {
        let connection_string = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://hodei:hodei@localhost:5432/hodei_test".to_string());

        let db_name = format!("hodei_dlq_test_{}", uuid::Uuid::new_v4());
        let base_url = connection_string.trim_end_matches(&format!(
            "/{}",
            connection_string.split('/').last().unwrap()
        ));
        let admin_conn_string = format!("{}/postgres", base_url);

        let mut admin_conn = sqlx::postgres::PgPool::connect(&admin_conn_string)
            .await
            .expect("Failed to connect to postgres");

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
        let repo = PostgresDlqRepository::new(pool.clone());
        repo.run_migrations()
            .await
            .expect("Failed to run migrations");

        pool
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_insert_and_get() {
        let pool = setup_test_db().await;
        let repo = PostgresDlqRepository::new(pool);

        let entry = DlqEntry {
            id: Uuid::new_v4(),
            original_event_id: Uuid::new_v4(),
            aggregate_id: Uuid::new_v4(),
            aggregate_type: "JOB".to_string(),
            event_type: "TestEvent".to_string(),
            payload: serde_json::json!({"test": "data"}),
            metadata: None,
            error_message: "Test error".to_string(),
            retry_count: 3,
            original_created_at: chrono::Utc::now(),
            moved_at: chrono::Utc::now(),
            resolved_at: None,
            resolution_notes: None,
            resolved_by: None,
        };

        repo.insert(&entry).await.unwrap();

        let retrieved = repo.get_by_id(entry.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().event_type, "TestEvent");
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_resolve() {
        let pool = setup_test_db().await;
        let repo = PostgresDlqRepository::new(pool);

        let entry = DlqEntry {
            id: Uuid::new_v4(),
            original_event_id: Uuid::new_v4(),
            aggregate_id: Uuid::new_v4(),
            aggregate_type: "WORKER".to_string(),
            event_type: "WorkerFailed".to_string(),
            payload: serde_json::json!({}),
            metadata: None,
            error_message: "Connection lost".to_string(),
            retry_count: 5,
            original_created_at: chrono::Utc::now(),
            moved_at: chrono::Utc::now(),
            resolved_at: None,
            resolution_notes: None,
            resolved_by: None,
        };

        repo.insert(&entry).await.unwrap();
        repo.resolve(entry.id, "Fixed upstream", "admin@test.com")
            .await
            .unwrap();

        let retrieved = repo.get_by_id(entry.id).await.unwrap().unwrap();
        assert!(!retrieved.is_pending());
        assert_eq!(
            retrieved.resolution_notes,
            Some("Fixed upstream".to_string())
        );
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_get_stats() {
        let pool = setup_test_db().await;
        let repo = PostgresDlqRepository::new(pool);

        let entry = DlqEntry {
            id: Uuid::new_v4(),
            original_event_id: Uuid::new_v4(),
            aggregate_id: Uuid::new_v4(),
            aggregate_type: "JOB".to_string(),
            event_type: "TestEvent".to_string(),
            payload: serde_json::json!({}),
            metadata: None,
            error_message: "Error".to_string(),
            retry_count: 1,
            original_created_at: chrono::Utc::now(),
            moved_at: chrono::Utc::now(),
            resolved_at: None,
            resolution_notes: None,
            resolved_by: None,
        };

        repo.insert(&entry).await.unwrap();

        let stats = repo.get_stats().await.unwrap();
        assert_eq!(stats.pending_count, 1);
        assert_eq!(stats.resolved_count, 0);
    }
}

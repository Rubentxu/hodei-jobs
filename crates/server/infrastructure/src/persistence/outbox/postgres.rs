//! PostgreSQL Outbox Repository
//!
//! SQLx-based implementation of OutboxRepository for PostgreSQL.

use hodei_server_domain::outbox::{
    AggregateType, OutboxError, OutboxEventInsert, OutboxEventView, OutboxRepository, OutboxStats,
    OutboxStatus,
};
use sqlx::FromRow;
use sqlx::postgres::{PgConnectOptions, PgPool};
use uuid::Uuid;

/// Error type specific to PostgreSQL outbox repository
#[derive(Debug, thiserror::Error)]
pub enum PostgresOutboxRepositoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Outbox domain error: {0}")]
    Outbox(#[from] OutboxError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Infrastructure error: {message}")]
    InfrastructureError { message: String },
}

impl From<PostgresOutboxRepositoryError> for OutboxError {
    fn from(err: PostgresOutboxRepositoryError) -> Self {
        match err {
            PostgresOutboxRepositoryError::Database(e) => OutboxError::Database(e),
            PostgresOutboxRepositoryError::Outbox(e) => e,
            PostgresOutboxRepositoryError::Serialization(e) => OutboxError::Serialization(e),
            PostgresOutboxRepositoryError::InfrastructureError { message } => {
                OutboxError::InfrastructureError { message }
            }
        }
    }
}

/// Row struct for outbox_events query
#[derive(FromRow)]
struct OutboxEventRow {
    id: Uuid,
    aggregate_id: Uuid,
    aggregate_type: String,
    event_type: String,
    event_version: i32,
    payload: sqlx::types::Json<serde_json::Value>,
    metadata: Option<sqlx::types::Json<serde_json::Value>>,
    idempotency_key: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    published_at: Option<chrono::DateTime<chrono::Utc>>,
    status: String,
    retry_count: i32,
    last_error: Option<String>,
}

/// PostgreSQL implementation of OutboxRepository
pub struct PostgresOutboxRepository {
    pool: PgPool,
}

impl PostgresOutboxRepository {
    /// Create a new PostgreSQL outbox repository
    ///
    /// # Arguments
    /// * `pool` - PostgreSQL connection pool
    ///
    /// # Returns
    /// * `Self` - New repository instance
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Run database migrations for the outbox table
    pub async fn run_migrations(&self) -> Result<(), PostgresOutboxRepositoryError> {
        // Create table - use sqlx::query instead of query! to avoid offline requirements
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS outbox_events (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                aggregate_id UUID NOT NULL,
                aggregate_type VARCHAR(20) NOT NULL CHECK (aggregate_type IN ('JOB', 'WORKER', 'PROVIDER')),
                event_type VARCHAR(50) NOT NULL,
                event_version INTEGER DEFAULT 1,
                payload JSONB NOT NULL,
                metadata JSONB,
                idempotency_key VARCHAR(100),
                created_at TIMESTAMPTZ DEFAULT NOW(),
                published_at TIMESTAMPTZ,
                status VARCHAR(20) DEFAULT 'PENDING' CHECK (status IN ('PENDING', 'PUBLISHED', 'FAILED')),
                retry_count INTEGER DEFAULT 0,
                last_error TEXT,
                UNIQUE(idempotency_key)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create indexes
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_outbox_status_created
            ON outbox_events(status, created_at)
            WHERE status = 'PENDING'
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Helper to convert aggregate type to string for database
    pub(crate) fn aggregate_type_to_str(aggregate_type: &AggregateType) -> &'static str {
        match aggregate_type {
            AggregateType::Job => "JOB",
            AggregateType::Worker => "WORKER",
            AggregateType::Provider => "PROVIDER",
        }
    }

    /// Helper to convert database string to aggregate type
    pub(crate) fn str_to_aggregate_type(s: &str) -> Result<AggregateType, OutboxError> {
        match s {
            "JOB" => Ok(AggregateType::Job),
            "WORKER" => Ok(AggregateType::Worker),
            "PROVIDER" => Ok(AggregateType::Provider),
            _ => Err(OutboxError::InfrastructureError {
                message: format!("Invalid aggregate type: {}", s),
            }),
        }
    }
}

#[async_trait::async_trait]
impl OutboxRepository for PostgresOutboxRepository {
    type Error = PostgresOutboxRepositoryError;

    async fn insert_events(&self, events: &[OutboxEventInsert]) -> Result<(), Self::Error> {
        if events.is_empty() {
            return Ok(());
        }

        // Build the INSERT query with all events
        let mut query_builder = sqlx::QueryBuilder::new(
            "INSERT INTO outbox_events (aggregate_id, aggregate_type, event_type, payload, metadata, idempotency_key, created_at) ",
        );

        query_builder.push_values(events, |mut b, event| {
            b.push_bind(event.aggregate_id);
            b.push_bind(Self::aggregate_type_to_str(&event.aggregate_type));
            b.push_bind(&event.event_type);
            b.push_bind(&event.payload);
            b.push_bind(&event.metadata);
            b.push_bind(&event.idempotency_key);
            b.push("NOW()");
        });

        query_builder.push(" ON CONFLICT (idempotency_key) DO NOTHING");

        let query = query_builder.build();
        query.execute(&self.pool).await?;

        Ok(())
    }

    async fn get_pending_events(
        &self,
        limit: usize,
        max_retries: i32,
    ) -> Result<Vec<OutboxEventView>, Self::Error> {
        let rows: Vec<OutboxEventRow> = sqlx::query_as::<_, OutboxEventRow>(
            r#"
            SELECT id, aggregate_id, aggregate_type, event_type, event_version,
                   payload, metadata, idempotency_key, created_at, published_at,
                   status, retry_count, last_error
            FROM outbox_events
            WHERE status = 'PENDING'
            AND (retry_count < $1 OR retry_count IS NULL)
            ORDER BY created_at ASC
            LIMIT $2
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(max_retries)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut events = Vec::with_capacity(rows.len());

        for row in rows {
            let aggregate_type = Self::str_to_aggregate_type(&row.aggregate_type)?;

            let payload: serde_json::Value = row.payload.0;
            let metadata: Option<serde_json::Value> = row.metadata.map(|j| j.0);

            let event = OutboxEventView {
                id: row.id,
                aggregate_id: row.aggregate_id,
                aggregate_type,
                event_type: row.event_type,
                event_version: row.event_version,
                payload,
                metadata,
                idempotency_key: row.idempotency_key,
                created_at: row.created_at,
                published_at: row.published_at,
                status: match row.status.as_str() {
                    "PENDING" => OutboxStatus::Pending,
                    "PUBLISHED" => OutboxStatus::Published,
                    "FAILED" => OutboxStatus::Failed,
                    _ => {
                        return Err(OutboxError::InfrastructureError {
                            message: format!("Invalid status: {}", row.status),
                        }
                        .into());
                    }
                },
                retry_count: row.retry_count,
                last_error: row.last_error,
            };

            events.push(event);
        }

        Ok(events)
    }

    async fn mark_published(&self, event_ids: &[Uuid]) -> Result<(), Self::Error> {
        if event_ids.is_empty() {
            return Ok(());
        }

        let mut query_builder = sqlx::QueryBuilder::new(
            "UPDATE outbox_events SET status = 'PUBLISHED', published_at = NOW() WHERE id IN (",
        );

        {
            let mut separated = query_builder.separated(", ");
            for id in event_ids {
                separated.push_bind(id);
            }
        }

        query_builder.push(")");

        query_builder.build().execute(&self.pool).await?;

        Ok(())
    }

    async fn mark_failed(&self, event_id: &Uuid, error: &str) -> Result<(), Self::Error> {
        sqlx::query!(
            r#"
            UPDATE outbox_events
            SET status = 'FAILED',
                retry_count = COALESCE(retry_count, 0) + 1,
                last_error = $2
            WHERE id = $1
            "#,
            event_id,
            error
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn exists_by_idempotency_key(&self, idempotency_key: &str) -> Result<bool, Self::Error> {
        let result: Option<OutboxEventRow> = sqlx::query_as::<_, OutboxEventRow>(
            r#"
            SELECT id, aggregate_id, aggregate_type, event_type, event_version,
                   payload, metadata, status, created_at, published_at,
                   retry_count, last_error, idempotency_key
            FROM outbox_events
            WHERE idempotency_key = $1
            LIMIT 1
            "#,
        )
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.is_some())
    }

    async fn count_pending(&self) -> Result<u64, Self::Error> {
        #[derive(sqlx::FromRow)]
        struct CountRow {
            count: i64,
        }
        let result: CountRow = sqlx::query_as::<_, CountRow>(
            r#"
            SELECT COUNT(*) as count
            FROM outbox_events
            WHERE status = 'PENDING'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(result.count as u64)
    }

    async fn get_stats(&self) -> Result<OutboxStats, Self::Error> {
        #[derive(sqlx::FromRow)]
        struct StatsRow {
            pending_count: Option<i64>,
            published_count: Option<i64>,
            failed_count: Option<i64>,
            oldest_pending_age_seconds: Option<i64>,
        }
        let result: StatsRow = sqlx::query_as::<_, StatsRow>(
            r#"
            SELECT
                COUNT(CASE WHEN status = 'PENDING' THEN 1 END) as pending_count,
                COUNT(CASE WHEN status = 'PUBLISHED' THEN 1 END) as published_count,
                COUNT(CASE WHEN status = 'FAILED' THEN 1 END) as failed_count,
                CAST(MIN(CASE WHEN status = 'PENDING' THEN EXTRACT(EPOCH FROM (NOW() - created_at)) END) AS BIGINT) as oldest_pending_age_seconds
            FROM outbox_events
            "#
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(OutboxStats {
            pending_count: result.pending_count.unwrap_or(0) as u64,
            published_count: result.published_count.unwrap_or(0) as u64,
            failed_count: result.failed_count.unwrap_or(0) as u64,
            oldest_pending_age_seconds: result.oldest_pending_age_seconds,
        })
    }

    async fn cleanup_published_events(
        &self,
        older_than: std::time::Duration,
    ) -> Result<u64, Self::Error> {
        let older_than_secs = older_than.as_secs_f64();
        let result: sqlx::postgres::PgQueryResult = sqlx::query!(
            r#"
            DELETE FROM outbox_events
            WHERE status = 'PUBLISHED'
            AND published_at < NOW() - make_interval(secs => $1)
            "#,
            older_than_secs
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() as u64)
    }

    async fn cleanup_failed_events(
        &self,
        max_retries: i32,
        older_than: std::time::Duration,
    ) -> Result<u64, Self::Error> {
        let older_than_secs = older_than.as_secs_f64();
        let result: sqlx::postgres::PgQueryResult = sqlx::query!(
            r#"
            DELETE FROM outbox_events
            WHERE status = 'FAILED'
            AND retry_count >= $1
            AND created_at < NOW() - make_interval(secs => $2)
            "#,
            max_retries,
            older_than_secs
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::outbox::OutboxRepository;
    use sqlx::postgres::PgPoolOptions;

    async fn setup_test_db() -> PgPool {
        let connection_string = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://hodei:hodei@localhost:5432/hodei_test".to_string());

        let db_name = format!("hodei_outbox_test_{}", uuid::Uuid::new_v4());
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
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS outbox_events (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                aggregate_id UUID NOT NULL,
                aggregate_type VARCHAR(20) NOT NULL CHECK (aggregate_type IN ('JOB', 'WORKER', 'PROVIDER')),
                event_type VARCHAR(50) NOT NULL,
                event_version INTEGER DEFAULT 1,
                payload JSONB NOT NULL,
                metadata JSONB,
                idempotency_key VARCHAR(100),
                created_at TIMESTAMPTZ DEFAULT NOW(),
                published_at TIMESTAMPTZ,
                status VARCHAR(20) DEFAULT 'PENDING' CHECK (status IN ('PENDING', 'PUBLISHED', 'FAILED')),
                retry_count INTEGER DEFAULT 0,
                last_error TEXT,
                UNIQUE(idempotency_key)
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create outbox_events table");

        pool
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_insert_events() {
        let pool = setup_test_db().await;
        let repo = PostgresOutboxRepository::new(pool);

        let events = vec![OutboxEventInsert::for_job(
            Uuid::new_v4(),
            "JobAssigned".to_string(),
            serde_json::json!({"job_id": "123", "worker_id": "456"}),
            None,
            Some("assign-123-456".to_string()),
        )];

        let result = repo.insert_events(&events).await;
        assert!(result.is_ok());

        let pending = repo.get_pending_events(10, 3).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].event_type, "JobAssigned");
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_idempotency_check() {
        let pool = setup_test_db().await;
        let repo = PostgresOutboxRepository::new(pool);

        let key = "test-key-123";
        assert!(!repo.exists_by_idempotency_key(key).await.unwrap());

        let event = OutboxEventInsert::for_job(
            Uuid::new_v4(),
            "JobCreated".to_string(),
            serde_json::json!({"test": "data"}),
            None,
            Some(key.to_string()),
        );
        repo.insert_events(&[event]).await.unwrap();

        assert!(repo.exists_by_idempotency_key(key).await.unwrap());
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_mark_published() {
        let pool = setup_test_db().await;
        let repo = PostgresOutboxRepository::new(pool);

        let event = OutboxEventInsert::for_job(
            Uuid::new_v4(),
            "JobAssigned".to_string(),
            serde_json::json!({"test": "data"}),
            None,
            None,
        );
        repo.insert_events(&[event]).await.unwrap();

        let pending = repo.get_pending_events(10, 3).await.unwrap();
        let event_id = pending[0].id;

        assert!(pending[0].is_pending());

        repo.mark_published(&[event_id]).await.unwrap();

        let published = repo.get_pending_events(10, 3).await.unwrap();
        assert!(published.is_empty());
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_mark_failed() {
        let pool = setup_test_db().await;
        let repo = PostgresOutboxRepository::new(pool);

        let event = OutboxEventInsert::for_job(
            Uuid::new_v4(),
            "JobAssigned".to_string(),
            serde_json::json!({"test": "data"}),
            None,
            None,
        );
        repo.insert_events(&[event]).await.unwrap();

        let pending = repo.get_pending_events(10, 3).await.unwrap();
        let event_id = pending[0].id;

        repo.mark_failed(&event_id, "Network error").await.unwrap();

        let pending = repo.get_pending_events(10, 3).await.unwrap();
        assert!(pending.is_empty());
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_stats() {
        let pool = setup_test_db().await;
        let repo = PostgresOutboxRepository::new(pool);

        for i in 0..5 {
            let event = OutboxEventInsert::for_job(
                Uuid::new_v4(),
                format!("JobCreated{}", i),
                serde_json::json!({"id": i}),
                None,
                None,
            );
            repo.insert_events(&[event]).await.unwrap();
        }

        let stats = repo.get_stats().await.unwrap();
        assert_eq!(stats.pending_count, 5);
        assert_eq!(stats.published_count, 0);
        assert_eq!(stats.failed_count, 0);
        assert_eq!(stats.total(), 5);
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_duplicate_idempotency_key() {
        let pool = setup_test_db().await;
        let repo = PostgresOutboxRepository::new(pool);

        let key = "duplicate-key";
        let event1 = OutboxEventInsert::for_job(
            Uuid::new_v4(),
            "JobCreated".to_string(),
            serde_json::json!({"test": "data1"}),
            None,
            Some(key.to_string()),
        );
        let event2 = OutboxEventInsert::for_job(
            Uuid::new_v4(),
            "JobCreated".to_string(),
            serde_json::json!({"test": "data2"}),
            None,
            Some(key.to_string()),
        );

        repo.insert_events(&[event1]).await.unwrap();
        repo.insert_events(&[event2]).await.unwrap();

        let stats = repo.get_stats().await.unwrap();
        assert_eq!(stats.pending_count, 1);
    }
}

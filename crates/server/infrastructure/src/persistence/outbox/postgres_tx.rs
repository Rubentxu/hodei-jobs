//! Transactional Outbox Repository Implementation
//!
//! PostgreSQL implementation of transaction-aware outbox operations
//! for the Transactional Outbox Pattern.

use super::PostgresOutboxRepository;
use hodei_server_domain::outbox::{
    AggregateType, OutboxError, OutboxEventInsert, OutboxRepositoryTx,
};
use sqlx::Row;
use sqlx::postgres::PgTransaction;
use uuid::Uuid;

/// Transaction-aware Outbox Repository for PostgreSQL
///
/// Extends PostgresOutboxRepository with transaction-aware operations
/// enabling atomic persistence of entities and events.
#[async_trait::async_trait]
impl OutboxRepositoryTx for PostgresOutboxRepository {
    type Error = OutboxError;

    async fn insert_events_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        events: &[OutboxEventInsert],
    ) -> Result<(), Self::Error> {
        if events.is_empty() {
            return Ok(());
        }

        // Build the INSERT query with all events - designed for transaction
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
        query
            .execute(&mut **tx)
            .await
            .map_err(|e| OutboxError::Database(e))?;

        Ok(())
    }

    async fn exists_by_idempotency_key_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        idempotency_key: &str,
    ) -> Result<bool, Self::Error> {
        let result: Option<(Uuid,)> = sqlx::query_as::<_, (Uuid,)>(
            r#"
            SELECT id FROM outbox_events
            WHERE idempotency_key = $1
            LIMIT 1
            "#,
        )
        .bind(idempotency_key)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| OutboxError::Database(e))?;

        Ok(result.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::outbox::OutboxRepository;
    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

    /// Creates a test database with a unique name
    async fn setup_test_db() -> sqlx::PgPool {
        let connection_string = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://hodei:hodei@localhost:5432/hodei".to_string());

        let db_name = format!("hodei_outbox_tx_test_{}", Uuid::new_v4());
        let base_url = connection_string.trim_end_matches(&format!(
            "/{}",
            connection_string.split('/').last().unwrap()
        ));
        let admin_conn_string = format!("{}/postgres", base_url);

        let mut admin_conn = sqlx::postgres::PgPool::connect(&admin_conn_string)
            .await
            .expect("Failed to connect to postgres");

        sqlx::query(&format!("DROP DATABASE IF EXISTS {}", db_name))
            .execute(&mut admin_conn)
            .await
            .expect("Failed to drop test database");

        sqlx::query(&format!("CREATE DATABASE {}", db_name))
            .execute(&mut admin_conn)
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
    async fn test_insert_events_with_tx_commit() {
        let pool = setup_test_db().await;
        let repo = PostgresOutboxRepository::new(pool.clone());

        let job_id = Uuid::new_v4();
        let event = OutboxEventInsert::for_job(
            job_id,
            "JobQueued".to_string(),
            serde_json::json!({"job_id": job_id.to_string()}),
            None,
            Some(format!("job-queued-{}", job_id)),
        );

        // Begin transaction
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        // Insert event within transaction
        repo.insert_events_with_tx(&mut tx, &[event.clone()])
            .await
            .expect("Failed to insert event");

        // Verify event is not visible outside transaction (not committed)
        let events_outside = repo.get_pending_events(10, 3).await.unwrap();
        assert!(
            events_outside.is_empty(),
            "Event should not be visible before commit"
        );

        // Commit transaction
        tx.commit().await.expect("Failed to commit");

        // Verify event is now visible
        let events_after = repo.get_pending_events(10, 3).await.unwrap();
        assert!(
            !events_after.is_empty(),
            "Event should be visible after commit"
        );
        assert_eq!(events_after[0].event_type, "JobQueued");

        // Cleanup
        sqlx::query("DROP DATABASE hodei_outbox_tx_test")
            .execute(&pool)
            .await
            .ok();
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_insert_events_with_tx_rollback() {
        let pool = setup_test_db().await;
        let repo = PostgresOutboxRepository::new(pool.clone());

        let job_id = Uuid::new_v4();
        let event = OutboxEventInsert::for_job(
            job_id,
            "JobQueued".to_string(),
            serde_json::json!({"job_id": job_id.to_string()}),
            None,
            Some(format!("job-queued-rollback-{}", job_id)),
        );

        // Begin transaction
        let mut tx = pool.begin().await.expect("Failed to begin transaction");

        // Insert event within transaction
        repo.insert_events_with_tx(&mut tx, &[event])
            .await
            .expect("Failed to insert event");

        // Rollback transaction
        tx.rollback().await.expect("Failed to rollback");

        // Verify event is NOT visible after rollback
        let events_after = repo.get_pending_events(10, 3).await.unwrap();
        assert!(
            events_after.is_empty(),
            "Event should not be visible after rollback"
        );

        // Cleanup
        sqlx::query("DROP DATABASE hodei_outbox_tx_test")
            .execute(&pool)
            .await
            .ok();
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_idempotency_key_with_tx() {
        let pool = setup_test_db().await;
        let repo = PostgresOutboxRepository::new(pool.clone());

        let job_id = Uuid::new_v4();
        let idempotency_key = format!("idempotent-{}", job_id);
        let event = OutboxEventInsert::for_job(
            job_id,
            "JobQueued".to_string(),
            serde_json::json!({"job_id": job_id.to_string()}),
            None,
            Some(idempotency_key.clone()),
        );

        // First insert
        let mut tx1 = pool.begin().await.expect("Failed to begin transaction");
        repo.insert_events_with_tx(&mut tx1, &[event.clone()])
            .await
            .expect("Failed to insert event");
        tx1.commit().await.expect("Failed to commit");

        // Second insert with same idempotency key (should be no-op due to DO NOTHING)
        let mut tx2 = pool.begin().await.expect("Failed to begin transaction");
        repo.insert_events_with_tx(&mut tx2, &[event])
            .await
            .expect("Should not error on duplicate");
        tx2.commit().await.expect("Failed to commit");

        // Verify only one event exists
        let events = repo.get_pending_events(10, 3).await.unwrap();
        assert_eq!(events.len(), 1, "Should have exactly one event");

        // Cleanup
        sqlx::query("DROP DATABASE hodei_outbox_tx_test")
            .execute(&pool)
            .await
            .ok();
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_exists_idempotency_key_with_tx() {
        let pool = setup_test_db().await;
        let repo = PostgresOutboxRepository::new(pool.clone());

        let job_id = Uuid::new_v4();
        let idempotency_key = format!("check-key-{}", job_id);

        // Key should not exist initially
        let mut tx1 = pool.begin().await.expect("Failed to begin transaction");
        let exists = repo
            .exists_by_idempotency_key_with_tx(&mut tx1, &idempotency_key)
            .await
            .expect("Failed to check key");
        assert!(!exists, "Key should not exist before insert");
        tx1.rollback().await.ok();

        // Insert event
        let event = OutboxEventInsert::for_job(
            job_id,
            "JobQueued".to_string(),
            serde_json::json!({"job_id": job_id.to_string()}),
            None,
            Some(idempotency_key.clone()),
        );
        let mut tx2 = pool.begin().await.expect("Failed to begin transaction");
        repo.insert_events_with_tx(&mut tx2, &[event])
            .await
            .expect("Failed to insert");
        tx2.commit().await.expect("Failed to commit");

        // Key should now exist
        let mut tx3 = pool.begin().await.expect("Failed to begin transaction");
        let exists = repo
            .exists_by_idempotency_key_with_tx(&mut tx3, &idempotency_key)
            .await
            .expect("Failed to check key");
        assert!(exists, "Key should exist after insert");
        tx3.rollback().await.ok();

        // Cleanup
        sqlx::query("DROP DATABASE hodei_outbox_tx_test")
            .execute(&pool)
            .await
            .ok();
    }
}

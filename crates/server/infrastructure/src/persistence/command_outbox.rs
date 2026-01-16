//! PostgreSQL Command Outbox Repository
//!
//! SQLx-based implementation of CommandOutboxRepository for PostgreSQL.
//! Implements the Transactional Outbox Pattern for commands.
//!
//! EPIC-50: Command Bus Core Infrastructure - GAP 51.2

use hodei_server_domain::command::{
    CommandOutboxError, CommandOutboxInsert, CommandOutboxRecord, CommandOutboxRepository,
    CommandOutboxStats, CommandOutboxStatus, CommandTargetType,
};
use sqlx::FromRow;
use sqlx::PgPool;
use uuid::Uuid;

/// Helper function to create CommandOutboxInsert for tests
fn make_insert(
    command_type: &str,
    target_id: Uuid,
    target_type: CommandTargetType,
    payload: serde_json::Value,
    idempotency_key: Option<&str>,
) -> CommandOutboxInsert {
    CommandOutboxInsert::new(
        target_id,
        target_type,
        command_type.to_string(),
        payload,
        None,
        idempotency_key.map(|s| s.to_string()),
    )
}

/// Row struct for hodei_commands query
#[derive(FromRow, Debug, Clone)]
struct CommandRecordRow {
    id: Uuid,
    command_type: String,
    target_id: String,
    target_type: String,
    payload: sqlx::types::Json<serde_json::Value>,
    metadata: Option<sqlx::types::Json<serde_json::Value>>,
    idempotency_key: Option<String>,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    processed_at: Option<chrono::DateTime<chrono::Utc>>,
    retry_count: i32,
    last_error: Option<String>,
    saga_id: Option<Uuid>,
    saga_type: Option<String>,
    step_order: Option<i32>,
}

/// PostgreSQL implementation of CommandOutboxRepository
///
/// This repository stores commands in the database for reliable processing
/// via the CommandOutboxRelay background worker.
#[derive(Debug, Clone)]
pub struct PostgresCommandOutboxRepository {
    pool: PgPool,
}

impl PostgresCommandOutboxRepository {
    /// Create a new PostgreSQL command outbox repository
    ///
    /// # Arguments
    /// * `pool` - PostgreSQL connection pool
    ///
    /// # Returns
    /// * `Self` - New repository instance
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Run database migrations for the hodei_commands table
    ///
    /// Creates the table if it doesn't exist with all required columns
    /// for command tracking, idempotency, and saga integration.
    pub async fn run_migrations(&self) -> Result<(), CommandOutboxError> {
        // Create table - using IF NOT EXISTS for idempotent migration
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS hodei_commands (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                command_type VARCHAR(100) NOT NULL,
                target_id VARCHAR(100) NOT NULL,
                target_type VARCHAR(50) NOT NULL,
                payload JSONB NOT NULL,
                metadata JSONB,
                idempotency_key VARCHAR(200) UNIQUE,
                status VARCHAR(20) NOT NULL DEFAULT 'PENDING' CHECK (status IN ('PENDING', 'COMPLETED', 'FAILED', 'CANCELLED')),
                created_at TIMESTAMPTZ DEFAULT NOW(),
                processed_at TIMESTAMPTZ,
                retry_count INTEGER DEFAULT 0,
                max_retries INTEGER DEFAULT 3,
                last_error TEXT,
                saga_id UUID,
                saga_type VARCHAR(50),
                step_order INTEGER,
                error_details JSONB
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| CommandOutboxError::Database(e))?;

        // Create indexes for efficient querying
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_hodei_commands_status_created
            ON hodei_commands(status, created_at)
            WHERE status = 'PENDING'
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| CommandOutboxError::Database(e))?;

        // Index for saga-based queries
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_hodei_commands_saga
            ON hodei_commands(saga_id, step_order)
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| CommandOutboxError::Database(e))?;

        // Index for target lookups
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_hodei_commands_target
            ON hodei_commands(target_type, target_id)
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| CommandOutboxError::Database(e))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl CommandOutboxRepository for PostgresCommandOutboxRepository {
    /// Insert a command into the outbox
    async fn insert_command(
        &self,
        command: &CommandOutboxInsert,
    ) -> Result<Uuid, CommandOutboxError> {
        let id = Uuid::new_v4();

        // Convert CommandTargetType to String for database
        let target_type_str = command.target_type.to_string();

        let query = sqlx::query(
            r#"
            INSERT INTO hodei_commands
            (id, command_type, target_id, target_type, payload, idempotency_key, status)
            VALUES ($1, $2, $3, $4, $5, $6, 'PENDING')
            ON CONFLICT (idempotency_key) DO NOTHING
            "#,
        )
        .bind(id)
        .bind(&command.command_type)
        .bind(command.target_id.to_string())
        .bind(target_type_str)
        .bind(&command.payload)
        .bind(command.idempotency_key.as_ref());

        let result = query.execute(&self.pool).await;

        match result {
            Ok(exec_result) => {
                // Check if a row was inserted (not a conflict)
                if exec_result.rows_affected() > 0 {
                    Ok(id)
                } else {
                    // Idempotency conflict - try to get existing ID
                    if let Some(ref key) = command.idempotency_key {
                        let existing: Option<CommandRecordRow> = sqlx::query_as(
                            r#"
                            SELECT id, command_type, target_id, target_type, payload, metadata,
                                   idempotency_key, status, created_at, processed_at,
                                   retry_count, last_error, saga_id, saga_type, step_order
                            FROM hodei_commands
                            WHERE idempotency_key = $1
                            "#,
                        )
                        .bind(key)
                        .fetch_optional(&self.pool)
                        .await
                        .map_err(CommandOutboxError::Database)?;

                        if let Some(row) = existing {
                            return Ok(row.id);
                        }
                    }
                    Ok(id)
                }
            }
            Err(e) => Err(CommandOutboxError::Database(e)),
        }
    }

    /// Get pending commands for processing
    async fn get_pending_commands(
        &self,
        limit: usize,
        _max_retries: i32,
    ) -> Result<Vec<CommandOutboxRecord>, CommandOutboxError> {
        let rows: Vec<CommandRecordRow> = sqlx::query_as::<_, CommandRecordRow>(
            r#"
            SELECT id, command_type, target_id, target_type, payload, metadata,
                   idempotency_key, status, created_at, processed_at,
                   retry_count, last_error, saga_id, saga_type, step_order
            FROM hodei_commands
            WHERE status = 'PENDING'
            AND retry_count < max_retries
            ORDER BY created_at ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(CommandOutboxError::Database)?;

        let mut commands = Vec::with_capacity(rows.len());

        for row in rows {
            // Note: CommandOutboxStatus doesn't have Dispatched variant
            // Use Failed as processing status marker in this implementation
            let status = match row.status.as_str() {
                "PENDING" => CommandOutboxStatus::Pending,
                "COMPLETED" => CommandOutboxStatus::Completed,
                "FAILED" => CommandOutboxStatus::Failed,
                "CANCELLED" => CommandOutboxStatus::Cancelled,
                _ => CommandOutboxStatus::Pending,
            };

            let payload: serde_json::Value = row.payload.0;
            let metadata: Option<serde_json::Value> = row.metadata.map(|j| j.0);

            let record = CommandOutboxRecord {
                id: row.id,
                command_type: row.command_type,
                target_id: Uuid::parse_str(&row.target_id).unwrap_or_default(),
                target_type: row.target_type.parse().unwrap_or(CommandTargetType::Saga),
                payload,
                metadata,
                idempotency_key: row.idempotency_key,
                status,
                created_at: row.created_at,
                processed_at: row.processed_at,
                retry_count: row.retry_count,
                last_error: row.last_error,
            };

            commands.push(record);
        }

        Ok(commands)
    }

    /// Mark a command as completed
    async fn mark_completed(&self, command_id: &Uuid) -> Result<(), CommandOutboxError> {
        sqlx::query(
            r#"
            UPDATE hodei_commands
            SET status = 'COMPLETED',
                processed_at = NOW(),
                last_error = NULL
            WHERE id = $1
            "#,
        )
        .bind(command_id)
        .execute(&self.pool)
        .await
        .map_err(CommandOutboxError::Database)?;

        Ok(())
    }

    /// Mark a command as failed
    async fn mark_failed(&self, command_id: &Uuid, error: &str) -> Result<(), CommandOutboxError> {
        sqlx::query(
            r#"
            UPDATE hodei_commands
            SET status = 'FAILED',
                retry_count = retry_count + 1,
                last_error = $1
            WHERE id = $2
            "#,
        )
        .bind(error)
        .bind(command_id)
        .execute(&self.pool)
        .await
        .map_err(CommandOutboxError::Database)?;

        Ok(())
    }

    /// Check if an idempotency key exists
    async fn exists_by_idempotency_key(&self, key: &str) -> Result<bool, CommandOutboxError> {
        let result: Option<CommandRecordRow> = sqlx::query_as::<_, CommandRecordRow>(
            r#"
            SELECT id, command_type, target_id, target_type, payload, metadata,
                   idempotency_key, status, created_at, processed_at,
                   retry_count, last_error, saga_id, saga_type, step_order
            FROM hodei_commands
            WHERE idempotency_key = $1
            AND status != 'CANCELLED'
            LIMIT 1
            "#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(CommandOutboxError::Database)?;

        Ok(result.is_some())
    }

    /// Get command statistics
    async fn get_stats(&self) -> Result<CommandOutboxStats, CommandOutboxError> {
        #[derive(sqlx::FromRow)]
        struct StatsRow {
            pending_count: Option<i64>,
            completed_count: Option<i64>,
            failed_count: Option<i64>,
            oldest_pending_age_seconds: Option<i64>,
        }

        let result: StatsRow = sqlx::query_as::<_, StatsRow>(
            r#"
            SELECT
                COUNT(CASE WHEN status = 'PENDING' THEN 1 END) as pending_count,
                COUNT(CASE WHEN status = 'COMPLETED' THEN 1 END) as completed_count,
                COUNT(CASE WHEN status = 'FAILED' THEN 1 END) as failed_count,
                CAST(MIN(CASE WHEN status = 'PENDING' THEN EXTRACT(EPOCH FROM (NOW() - created_at)) END) AS BIGINT) as oldest_pending_age_seconds
            FROM hodei_commands
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(CommandOutboxError::Database)?;

        Ok(CommandOutboxStats {
            pending_count: result.pending_count.unwrap_or(0) as u64,
            completed_count: result.completed_count.unwrap_or(0) as u64,
            failed_count: result.failed_count.unwrap_or(0) as u64,
            oldest_pending_age_seconds: result.oldest_pending_age_seconds,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{postgres::PgPoolOptions, PgPool};

    async fn setup_test_db() -> PgPool {
        let connection_string = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://hodei:hodei@localhost:5432/hodei_test".to_string());

        let db_name = format!("hodei_commands_test_{}", uuid::Uuid::new_v4());
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

        pool
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_insert_command() {
        let pool = setup_test_db().await;
        let repo = PostgresCommandOutboxRepository::new(pool);

        let id = repo
            .insert_command(&make_insert(
                "MarkJobFailed",
                Uuid::new_v4(),
                CommandTargetType::Job,
                serde_json::json!({"job_id": "job-123", "reason": "Test failure"}),
                Some("mark-failed-job-123"),
            ))
            .await
            .unwrap();

        assert!(!id.is_nil());
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_idempotency_check() {
        let pool = setup_test_db().await;
        let repo = PostgresCommandOutboxRepository::new(pool);

        let key = "test-idempotency-key";
        assert!(!repo.exists_by_idempotency_key(key).await.unwrap());

        repo.insert_command(&make_insert(
            "TestCommand",
            Uuid::new_v4(),
            CommandTargetType::Saga,
            serde_json::json!({"data": "test"}),
            Some(key),
        ))
        .await
        .unwrap();

        assert!(repo.exists_by_idempotency_key(key).await.unwrap());
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_get_pending_commands() {
        let pool = setup_test_db().await;
        let repo = PostgresCommandOutboxRepository::new(pool);

        // Insert some commands
        for i in 0..3 {
            repo.insert_command(&make_insert(
                &format!("Command{}", i),
                Uuid::new_v4(),
                CommandTargetType::Saga,
                serde_json::json!({"index": i}),
                None,
            ))
            .await
            .unwrap();
        }

        let pending = repo.get_pending_commands(10, 3).await.unwrap();
        assert_eq!(pending.len(), 3);
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_mark_completed() {
        let pool = setup_test_db().await;
        let repo = PostgresCommandOutboxRepository::new(pool);

        let id = repo
            .insert_command(&make_insert(
                "TestCommand",
                Uuid::new_v4(),
                CommandTargetType::Saga,
                serde_json::json!({"data": "test"}),
                None,
            ))
            .await
            .unwrap();

        repo.mark_completed(&id).await.unwrap();

        let stats = repo.get_stats().await.unwrap();
        assert_eq!(stats.completed_count, 1);
        assert_eq!(stats.pending_count, 0);
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_mark_failed() {
        let pool = setup_test_db().await;
        let repo = PostgresCommandOutboxRepository::new(pool);

        let id = repo
            .insert_command(&make_insert(
                "TestCommand",
                Uuid::new_v4(),
                CommandTargetType::Saga,
                serde_json::json!({"data": "test"}),
                None,
            ))
            .await
            .unwrap();

        repo.mark_failed(&id, "Handler error").await.unwrap();

        let stats = repo.get_stats().await.unwrap();
        assert_eq!(stats.failed_count, 1);
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_duplicate_insert_with_idempotency() {
        let pool = setup_test_db().await;
        let repo = PostgresCommandOutboxRepository::new(pool);

        let key = "unique-key-123";
        let payload = serde_json::json!({"test": "data1"});

        let id1 = repo
            .insert_command(&make_insert(
                "TestCommand",
                Uuid::new_v4(),
                CommandTargetType::Saga,
                payload.clone(),
                Some(key),
            ))
            .await
            .unwrap();

        // Try to insert again with same idempotency key
        let payload2 = serde_json::json!({"test": "data2"});
        let id2 = repo
            .insert_command(&make_insert(
                "TestCommand",
                Uuid::new_v4(),
                CommandTargetType::Saga,
                payload2,
                Some(key),
            ))
            .await
            .unwrap();

        // Should return the same ID (idempotency)
        assert_eq!(id1, id2);

        let stats = repo.get_stats().await.unwrap();
        assert_eq!(stats.pending_count, 1);
    }
}

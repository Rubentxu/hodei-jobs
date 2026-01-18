//! PostgreSQL Command Outbox Repository Implementation
//!
//! Implements the CommandOutboxRepository trait using PostgreSQL.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hodei_server_domain::command::{
    CommandTargetType,
    outbox::{
        CommandOutboxError, CommandOutboxInsert, CommandOutboxRecord, CommandOutboxRepository,
        CommandOutboxStats, CommandOutboxStatus,
    },
};
use sqlx::{PgPool, PgTransaction, Postgres, Row};
use std::sync::Arc;
use uuid::Uuid;

/// PostgreSQL implementation of CommandOutboxRepository
pub struct PostgresCommandOutboxRepository {
    pool: PgPool,
}

impl PostgresCommandOutboxRepository {
    /// Create a new PostgresCommandOutboxRepository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CommandOutboxRepository for PostgresCommandOutboxRepository {
    async fn insert_command(
        &self,
        command: &CommandOutboxInsert,
    ) -> Result<Uuid, CommandOutboxError> {
        let mut tx = self.pool.begin().await?;
        let id = self.insert_command_with_tx(&mut tx, command).await?;
        tx.commit().await?;
        Ok(id)
    }

    async fn insert_command_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        command: &CommandOutboxInsert,
    ) -> Result<Uuid, CommandOutboxError> {
        let id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO hodei_commands (
                id, target_id, target_type, command_type, payload, metadata, idempotency_key, status
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(id)
        .bind(command.target_id)
        .bind(command.target_type.to_string())
        .bind(&command.command_type)
        .bind(&command.payload)
        .bind(&command.metadata)
        .bind(&command.idempotency_key)
        .bind("PENDING")
        .execute(&mut **tx)
        .await?;

        Ok(id)
    }

    async fn get_pending_commands(
        &self,
        limit: usize,
        max_retries: i32,
    ) -> Result<Vec<CommandOutboxRecord>, CommandOutboxError> {
        let rows = sqlx::query(
            r#"
            SELECT id, target_id, target_type, command_type, payload, metadata, idempotency_key, 
                   status, created_at, processed_at, retry_count, last_error
            FROM hodei_commands
            WHERE status = 'PENDING' OR (status = 'FAILED' AND retry_count < $1)
            ORDER BY created_at ASC
            LIMIT $2
            "#,
        )
        .bind(max_retries)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut commands = Vec::new();
        for row in rows {
            commands.push(row_to_record(row)?);
        }

        Ok(commands)
    }

    async fn mark_completed(&self, command_id: &Uuid) -> Result<(), CommandOutboxError> {
        sqlx::query(
            r#"
            UPDATE hodei_commands
            SET status = 'COMPLETED', processed_at = NOW(), updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(command_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn mark_failed(&self, command_id: &Uuid, error: &str) -> Result<(), CommandOutboxError> {
        sqlx::query(
            r#"
            UPDATE hodei_commands
            SET status = 'FAILED', retry_count = retry_count + 1, last_error = $2, updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(command_id)
        .bind(error)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn exists_by_idempotency_key(&self, key: &str) -> Result<bool, CommandOutboxError> {
        let row =
            sqlx::query("SELECT EXISTS(SELECT 1 FROM hodei_commands WHERE idempotency_key = $1)")
                .bind(key)
                .fetch_one(&self.pool)
                .await?;

        Ok(row.get(0))
    }

    async fn get_stats(&self) -> Result<CommandOutboxStats, CommandOutboxError> {
        let row = sqlx::query(
            r#"
            SELECT 
                COUNT(*) FILTER (WHERE status = 'PENDING') as pending,
                COUNT(*) FILTER (WHERE status = 'COMPLETED') as completed,
                COUNT(*) FILTER (WHERE status = 'FAILED') as failed,
                EXTRACT(EPOCH FROM (NOW() - MIN(created_at) FILTER (WHERE status = 'PENDING'))) as oldest_pending
            FROM hodei_commands
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(CommandOutboxStats {
            pending_count: row.get::<i64, _>("pending") as u64,
            completed_count: row.get::<i64, _>("completed") as u64,
            failed_count: row.get::<i64, _>("failed") as u64,
            oldest_pending_age_seconds: row
                .get::<Option<f64>, _>("oldest_pending")
                .map(|f| f as i64),
        })
    }
}

fn row_to_record(row: sqlx::postgres::PgRow) -> Result<CommandOutboxRecord, CommandOutboxError> {
    let target_type_str: String = row.get("target_type");
    let target_type = target_type_str.parse().map_err(|_| {
        CommandOutboxError::HandlerError(format!("Invalid target_type: {}", target_type_str))
    })?;

    let status_str: String = row.get("status");
    let status = match status_str.as_str() {
        "PENDING" => CommandOutboxStatus::Pending,
        "COMPLETED" => CommandOutboxStatus::Completed,
        "FAILED" => CommandOutboxStatus::Failed,
        "CANCELLED" => CommandOutboxStatus::Cancelled,
        _ => {
            return Err(CommandOutboxError::HandlerError(format!(
                "Invalid status: {}",
                status_str
            )));
        }
    };

    Ok(CommandOutboxRecord {
        id: row.get("id"),
        target_id: row.get("target_id"),
        target_type,
        command_type: row.get("command_type"),
        payload: row.get("payload"),
        metadata: row.get("metadata"),
        idempotency_key: row.get("idempotency_key"),
        status,
        created_at: row.get("created_at"),
        processed_at: row.get("processed_at"),
        retry_count: row.get("retry_count"),
        last_error: row.get("last_error"),
    })
}

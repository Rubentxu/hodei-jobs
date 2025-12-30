//! PostgreSQL Audit Repository
//!
//! Audit log persistence using PostgreSQL

use chrono::{DateTime, Utc};
use hodei_server_domain::audit::{
    AuditLog, AuditQuery, AuditQueryResult, AuditRepository, EventTypeCount,
};
use hodei_server_domain::shared_kernel::{DomainError, Result};
use sqlx::Row;
use sqlx::postgres::PgPool;

#[derive(Clone)]
pub struct PostgresAuditRepository {
    pool: PgPool,
}

impl PostgresAuditRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Run migrations for audit tables
    pub async fn run_migrations(&self) -> Result<()> {
        Ok(())
    }

    fn row_to_audit_log(row: &sqlx::postgres::PgRow) -> AuditLog {
        AuditLog {
            id: row.get("id"),
            correlation_id: row.try_get("correlation_id").ok().flatten(),
            event_type: row.get("event_type"),
            payload: row.get("payload"),
            occurred_at: row.get("occurred_at"),
            actor: row.try_get("actor").ok().flatten(),
        }
    }
}

#[async_trait::async_trait]
impl AuditRepository for PostgresAuditRepository {
    async fn save(&self, log: &AuditLog) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO audit_logs (id, correlation_id, event_type, payload, occurred_at, actor)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(log.id)
        .bind(log.correlation_id.as_ref())
        .bind(&log.event_type)
        .bind(&log.payload)
        .bind(log.occurred_at)
        .bind(log.actor.as_ref())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to save audit log: {}", e),
        })?;

        Ok(())
    }

    async fn find_by_correlation_id(&self, id: &str) -> Result<Vec<AuditLog>> {
        let rows = sqlx::query(
            r#"
            SELECT id, correlation_id, event_type, payload, occurred_at, actor
            FROM audit_logs
            WHERE correlation_id = $1
            ORDER BY occurred_at DESC
            "#,
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find audit logs: {}", e),
        })?;

        Ok(rows.iter().map(|r| Self::row_to_audit_log(&r)).collect())
    }

    async fn find_by_event_type(
        &self,
        event_type: &str,
        limit: i64,
        offset: i64,
    ) -> Result<AuditQueryResult> {
        let rows = sqlx::query(
            r#"
            SELECT id, correlation_id, event_type, payload, occurred_at, actor
            FROM audit_logs
            WHERE event_type = $1
            ORDER BY occurred_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(event_type)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find audit logs: {}", e),
        })?;

        let logs: Vec<AuditLog> = rows.iter().map(|r| Self::row_to_audit_log(&r)).collect();

        let count: i64 =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_logs WHERE event_type = $1")
                .bind(event_type)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to count audit logs: {}", e),
                })?;

        let log_count = logs.len() as i64;
        Ok(AuditQueryResult {
            logs,
            total_count: count,
            has_more: log_count == limit,
        })
    }

    async fn find_by_date_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: i64,
        offset: i64,
    ) -> Result<AuditQueryResult> {
        let rows = sqlx::query(
            r#"
            SELECT id, correlation_id, event_type, payload, occurred_at, actor
            FROM audit_logs
            WHERE occurred_at >= $1 AND occurred_at <= $2
            ORDER BY occurred_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(start)
        .bind(end)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find audit logs: {}", e),
        })?;

        let logs: Vec<AuditLog> = rows.iter().map(|r| Self::row_to_audit_log(&r)).collect();

        let count: i64 = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM audit_logs WHERE occurred_at >= $1 AND occurred_at <= $2",
        )
        .bind(start)
        .bind(end)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to count audit logs: {}", e),
        })?;

        let log_count = logs.len() as i64;
        Ok(AuditQueryResult {
            logs,
            total_count: count,
            has_more: log_count == limit,
        })
    }

    async fn find_by_actor(
        &self,
        actor: &str,
        limit: i64,
        offset: i64,
    ) -> Result<AuditQueryResult> {
        let rows = sqlx::query(
            r#"
            SELECT id, correlation_id, event_type, payload, occurred_at, actor
            FROM audit_logs
            WHERE actor = $1
            ORDER BY occurred_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(actor)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find audit logs: {}", e),
        })?;

        let logs: Vec<AuditLog> = rows.iter().map(|r| Self::row_to_audit_log(&r)).collect();

        let count: i64 =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_logs WHERE actor = $1")
                .bind(actor)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to count audit logs: {}", e),
                })?;

        let log_count = logs.len() as i64;
        Ok(AuditQueryResult {
            logs,
            total_count: count,
            has_more: log_count == limit,
        })
    }

    async fn query(&self, query: AuditQuery) -> Result<AuditQueryResult> {
        // Simplified query - use the first available filter
        let limit = query.limit.unwrap_or(100);

        let rows = if let Some(ref event_type) = query.event_type {
            sqlx::query(
                "SELECT id, correlation_id, event_type, payload, occurred_at, actor FROM audit_logs WHERE event_type = $1 ORDER BY occurred_at DESC LIMIT $2",
            )
            .bind(event_type)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
        } else if let Some(ref actor) = query.actor {
            sqlx::query(
                "SELECT id, correlation_id, event_type, payload, occurred_at, actor FROM audit_logs WHERE actor = $1 ORDER BY occurred_at DESC LIMIT $2",
            )
            .bind(actor)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query(
                "SELECT id, correlation_id, event_type, payload, occurred_at, actor FROM audit_logs ORDER BY occurred_at DESC LIMIT $1",
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await
        }.map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to query audit logs: {}", e),
        })?;

        let logs: Vec<AuditLog> = rows.iter().map(|r| Self::row_to_audit_log(&r)).collect();
        let log_count = logs.len() as i64;

        Ok(AuditQueryResult {
            logs,
            total_count: log_count,
            has_more: log_count == limit,
        })
    }

    async fn count_by_event_type(&self) -> Result<Vec<EventTypeCount>> {
        #[derive(sqlx::FromRow)]
        struct RawCount {
            event_type: String,
            count: i64,
        }

        let rows = sqlx::query_as::<_, RawCount>(
            r#"
            SELECT event_type, COUNT(*) as count
            FROM audit_logs
            GROUP BY event_type
            ORDER BY count DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to count audit logs: {}", e),
        })?;

        Ok(rows
            .into_iter()
            .map(|r| EventTypeCount {
                event_type: r.event_type,
                count: r.count,
            })
            .collect())
    }

    async fn delete_before(&self, before: DateTime<Utc>) -> Result<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM audit_logs
            WHERE occurred_at < $1
            "#,
        )
        .bind(before)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to delete audit logs: {}", e),
        })?;

        Ok(result.rows_affected() as u64)
    }
}

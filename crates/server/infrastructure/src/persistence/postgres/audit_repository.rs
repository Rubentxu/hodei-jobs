//! PostgreSQL Audit Repository
//!
//! Audit log persistence using PostgreSQL

use chrono::{DateTime, Utc};
use hodei_server_domain::audit::{
    AuditLog, AuditQuery, AuditQueryResult, AuditRepository, EventTypeCount,
};
use hodei_server_domain::shared_kernel::{DomainError, Result};
use sqlx::{Row, postgres::PgPool};

#[derive(Clone)]
pub struct PostgresAuditRepository {
    pool: PgPool,
}

impl PostgresAuditRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Run migrations for audit tables
    ///
    /// DEPRECATED: Migrations are now handled by the central MigrationService.
    /// This method is kept for backwards compatibility but does nothing.
    pub async fn run_migrations(&self) -> Result<()> {
        // Migrations are now handled by the central MigrationService
        // See: hodei_server_infrastructure::persistence::postgres::migrations::run_migrations
        Ok(())
    }

    fn row_to_audit_log(row: &sqlx::postgres::PgRow) -> AuditLog {
        AuditLog {
            id: row.get("id"),
            correlation_id: row.get("correlation_id"),
            event_type: row.get("event_type"),
            payload: row.get("payload"),
            occurred_at: row.get("occurred_at"),
            actor: row.get("actor"),
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
        .bind(log.correlation_id)
        .bind(log.event_type)
        .bind(log.payload)
        .bind(log.occurred_at)
        .bind(log.actor)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to save audit log: {}", e),
        })?;

        Ok(())
    }

    async fn query(&self, query: AuditQuery) -> Result<AuditQueryResult> {
        // Build query based on which parameters are present
        let sql = match (&query.event_type, &query.actor, &query.correlation_id) {
            (None, None, None) => {
                "SELECT id, correlation_id, event_type, payload, occurred_at, actor FROM audit_logs ORDER BY occurred_at DESC".to_string()
            }
            (Some(e), None, None) => {
                "SELECT id, correlation_id, event_type, payload, occurred_at, actor FROM audit_logs WHERE event_type = $1 ORDER BY occurred_at DESC".to_string()
            }
            (Some(e), Some(a), None) => {
                "SELECT id, correlation_id, event_type, payload, occurred_at, actor FROM audit_logs WHERE event_type = $1 AND actor = $2 ORDER BY occurred_at DESC".to_string()
            }
            (Some(e), Some(a), Some(c)) => {
                "SELECT id, correlation_id, event_type, payload, occurred_at, actor FROM audit_logs WHERE event_type = $1 AND actor = $2 AND correlation_id = $3 ORDER BY occurred_at DESC".to_string()
            }
            (None, Some(a), Some(c)) => {
                "SELECT id, correlation_id, event_type, payload, occurred_at, actor FROM audit_logs WHERE actor = $1 AND correlation_id = $2 ORDER BY occurred_at DESC".to_string()
            }
            (None, Some(a), None) => {
                "SELECT id, correlation_id, event_type, payload, occurred_at, actor FROM audit_logs WHERE actor = $1 ORDER BY occurred_at DESC".to_string()
            }
            (None, None, Some(c)) => {
                "SELECT id, correlation_id, event_type, payload, occurred_at, actor FROM audit_logs WHERE correlation_id = $1 ORDER BY occurred_at DESC".to_string()
            }
        };

        // Add limit if specified
        let sql = if let Some(limit) = query.limit {
            format!("{} LIMIT {}", sql, limit)
        } else {
            sql
        };

        // Execute query based on which parameters are present
        let rows = match (&query.event_type, &query.actor, &query.correlation_id) {
            (None, None, None) => {
                sqlx::query_as::<_, sqlx::postgres::PgRow>(&sql).fetch_all(&self.pool)
            }
            (Some(e), None, None) => sqlx::query_as::<_, sqlx::postgres::PgRow>(&sql)
                .bind(e)
                .fetch_all(&self.pool),
            (Some(e), Some(a), None) => sqlx::query_as::<_, sqlx::postgres::PgRow>(&sql)
                .bind(e)
                .bind(a)
                .fetch_all(&self.pool),
            (Some(e), Some(a), Some(c)) => sqlx::query_as::<_, sqlx::postgres::PgRow>(&sql)
                .bind(e)
                .bind(a)
                .bind(c)
                .fetch_all(&self.pool),
            (None, Some(a), Some(c)) => sqlx::query_as::<_, sqlx::postgres::PgRow>(&sql)
                .bind(a)
                .bind(c)
                .fetch_all(&self.pool),
            (None, Some(a), None) => sqlx::query_as::<_, sqlx::postgres::PgRow>(&sql)
                .bind(a)
                .fetch_all(&self.pool),
            (None, None, Some(c)) => sqlx::query_as::<_, sqlx::postgres::PgRow>(&sql)
                .bind(c)
                .fetch_all(&self.pool),
        }
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to query audit logs: {}", e),
        })?;

        let logs: Vec<AuditLog> = rows.iter().map(|r| Self::row_to_audit_log(r)).collect();
        let count = logs.len() as u64;

        Ok(AuditQueryResult { logs, count })
    }

    async fn count_by_event_type(&self) -> Result<Vec<EventTypeCount>> {
        sqlx::query_as::<_, EventTypeCount>(
            r#"
            SELECT event_type as "event_type!", COUNT(*) as "count!"
            FROM audit_logs
            GROUP BY event_type
            ORDER BY count DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to count audit logs by event type: {}", e),
        })
    }

    async fn delete_older_than(&self, before: DateTime<Utc>) -> Result<u64> {
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
            message: format!("Failed to delete old audit logs: {}", e),
        })?;

        Ok(result.rows_affected() as u64)
    }
}

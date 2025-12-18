use chrono::{DateTime, Utc};
use hodei_jobs_domain::audit::{
    AuditLog, AuditQuery, AuditQueryResult, AuditRepository, EventTypeCount,
};
use hodei_jobs_domain::shared_kernel::{DomainError, Result};
use sqlx::{Row, postgres::PgPool};

#[derive(Clone)]
pub struct PostgresAuditRepository {
    pool: PgPool,
}

impl PostgresAuditRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS audit_logs (
                id UUID PRIMARY KEY,
                correlation_id VARCHAR(255),
                event_type VARCHAR(255) NOT NULL,
                payload JSONB NOT NULL,
                occurred_at TIMESTAMPTZ NOT NULL,
                actor VARCHAR(255)
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create audit_logs table: {}", e),
        })?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_audit_correlation_id ON audit_logs(correlation_id);",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create audit_logs correlation_id index: {}", e),
        })?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_event_type ON audit_logs(event_type);")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to create audit_logs event_type index: {}", e),
            })?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_occurred_at ON audit_logs(occurred_at);")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to create audit_logs occurred_at index: {}", e),
            })?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_logs(actor);")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to create audit_logs actor index: {}", e),
            })?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_event_date ON audit_logs(event_type, occurred_at);")
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to create audit_logs event_date index: {}", e),
            })?;

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
        .bind(&log.correlation_id)
        .bind(&log.event_type)
        .bind(&log.payload)
        .bind(log.occurred_at)
        .bind(&log.actor)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to save audit log: {}", e),
        })?;

        Ok(())
    }

    async fn find_by_correlation_id(&self, id: &str) -> Result<Vec<AuditLog>> {
        let rows = sqlx::query(
            "SELECT * FROM audit_logs WHERE correlation_id = $1 ORDER BY occurred_at DESC",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find audit logs: {}", e),
        })?;

        Ok(rows.iter().map(Self::row_to_audit_log).collect())
    }

    async fn find_by_event_type(
        &self,
        event_type: &str,
        limit: i64,
        offset: i64,
    ) -> Result<AuditQueryResult> {
        let count_row =
            sqlx::query("SELECT COUNT(*) as count FROM audit_logs WHERE event_type = $1")
                .bind(event_type)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::InfrastructureError {
                    message: format!("Failed to count audit logs: {}", e),
                })?;
        let total_count: i64 = count_row.get("count");

        let rows = sqlx::query(
            "SELECT * FROM audit_logs WHERE event_type = $1 ORDER BY occurred_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(event_type)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find audit logs by event type: {}", e),
        })?;

        let logs: Vec<AuditLog> = rows.iter().map(Self::row_to_audit_log).collect();
        let has_more = (offset + logs.len() as i64) < total_count;

        Ok(AuditQueryResult {
            logs,
            total_count,
            has_more,
        })
    }

    async fn find_by_date_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        limit: i64,
        offset: i64,
    ) -> Result<AuditQueryResult> {
        let count_row = sqlx::query(
            "SELECT COUNT(*) as count FROM audit_logs WHERE occurred_at >= $1 AND occurred_at <= $2",
        )
        .bind(start)
        .bind(end)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to count audit logs: {}", e),
        })?;
        let total_count: i64 = count_row.get("count");

        let rows = sqlx::query(
            "SELECT * FROM audit_logs WHERE occurred_at >= $1 AND occurred_at <= $2 ORDER BY occurred_at DESC LIMIT $3 OFFSET $4",
        )
        .bind(start)
        .bind(end)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find audit logs by date range: {}", e),
        })?;

        let logs: Vec<AuditLog> = rows.iter().map(Self::row_to_audit_log).collect();
        let has_more = (offset + logs.len() as i64) < total_count;

        Ok(AuditQueryResult {
            logs,
            total_count,
            has_more,
        })
    }

    async fn find_by_actor(
        &self,
        actor: &str,
        limit: i64,
        offset: i64,
    ) -> Result<AuditQueryResult> {
        let count_row = sqlx::query("SELECT COUNT(*) as count FROM audit_logs WHERE actor = $1")
            .bind(actor)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to count audit logs: {}", e),
            })?;
        let total_count: i64 = count_row.get("count");

        let rows = sqlx::query(
            "SELECT * FROM audit_logs WHERE actor = $1 ORDER BY occurred_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(actor)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find audit logs by actor: {}", e),
        })?;

        let logs: Vec<AuditLog> = rows.iter().map(Self::row_to_audit_log).collect();
        let has_more = (offset + logs.len() as i64) < total_count;

        Ok(AuditQueryResult {
            logs,
            total_count,
            has_more,
        })
    }

    async fn query(&self, query: AuditQuery) -> Result<AuditQueryResult> {
        let limit = query.limit.unwrap_or(100);
        let offset = query.offset.unwrap_or(0);

        let mut count_qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) as count FROM audit_logs ");
        let mut select_qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT * FROM audit_logs ");

        let mut has_where = false;

        if let Some(ref et) = query.event_type {
            if !has_where {
                count_qb.push(" WHERE ");
                select_qb.push(" WHERE ");
                has_where = true;
            } else {
                count_qb.push(" AND ");
                select_qb.push(" AND ");
            }
            count_qb.push("event_type = ");
            select_qb.push("event_type = ");
            count_qb.push_bind(et);
            select_qb.push_bind(et);
        }

        if let Some(ref a) = query.actor {
            if !has_where {
                count_qb.push(" WHERE ");
                select_qb.push(" WHERE ");
                has_where = true;
            } else {
                count_qb.push(" AND ");
                select_qb.push(" AND ");
            }
            count_qb.push("actor = ");
            select_qb.push("actor = ");
            count_qb.push_bind(a);
            select_qb.push_bind(a);
        }

        if let Some(st) = query.start_time {
            if !has_where {
                count_qb.push(" WHERE ");
                select_qb.push(" WHERE ");
                has_where = true;
            } else {
                count_qb.push(" AND ");
                select_qb.push(" AND ");
            }
            count_qb.push("occurred_at >= ");
            select_qb.push("occurred_at >= ");
            count_qb.push_bind(st);
            select_qb.push_bind(st);
        }

        if let Some(et) = query.end_time {
            if !has_where {
                count_qb.push(" WHERE ");
                select_qb.push(" WHERE ");
                has_where = true;
                let _ = has_where; // suppress warning
            } else {
                count_qb.push(" AND ");
                select_qb.push(" AND ");
            }
            count_qb.push("occurred_at <= ");
            select_qb.push("occurred_at <= ");
            count_qb.push_bind(et);
            select_qb.push_bind(et);
        }

        // Execute Count Query
        let count_row = count_qb.build().fetch_one(&self.pool).await.map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to count audit logs: {}", e),
            }
        })?;
        let total_count: i64 = count_row.get("count");

        // Finish Select Query with Order and Pagination
        select_qb.push(" ORDER BY occurred_at DESC LIMIT ");
        select_qb.push_bind(limit);
        select_qb.push(" OFFSET ");
        select_qb.push_bind(offset);

        let rows = select_qb.build().fetch_all(&self.pool).await.map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to query audit logs: {}", e),
            }
        })?;

        let logs: Vec<AuditLog> = rows.iter().map(Self::row_to_audit_log).collect();
        let has_more = (offset + logs.len() as i64) < total_count;

        Ok(AuditQueryResult {
            logs,
            total_count,
            has_more,
        })
    }

    async fn count_by_event_type(&self) -> Result<Vec<EventTypeCount>> {
        let rows = sqlx::query(
            "SELECT event_type, COUNT(*) as count FROM audit_logs GROUP BY event_type ORDER BY count DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to count audit logs by event type: {}", e),
        })?;

        Ok(rows
            .iter()
            .map(|row| EventTypeCount {
                event_type: row.get("event_type"),
                count: row.get("count"),
            })
            .collect())
    }

    async fn delete_before(&self, before: DateTime<Utc>) -> Result<u64> {
        let result = sqlx::query("DELETE FROM audit_logs WHERE occurred_at < $1")
            .bind(before)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to delete old audit logs: {}", e),
            })?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_query_builder() {
        let query = AuditQuery::new()
            .with_event_type("JobCreated")
            .with_actor("user@example.com")
            .with_pagination(50, 0);

        assert_eq!(query.event_type, Some("JobCreated".to_string()));
        assert_eq!(query.actor, Some("user@example.com".to_string()));
        assert_eq!(query.limit, Some(50));
        assert_eq!(query.offset, Some(0));
    }
}

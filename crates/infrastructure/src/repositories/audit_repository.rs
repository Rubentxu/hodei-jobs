use hodei_jobs_domain::audit::{AuditLog, AuditRepository};
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
            
            CREATE INDEX IF NOT EXISTS idx_audit_correlation_id ON audit_logs(correlation_id);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to create audit_logs table: {}", e),
        })?;

        Ok(())
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

        let mut logs = Vec::new();
        for row in rows {
            logs.push(AuditLog {
                id: row.get("id"),
                correlation_id: row.get("correlation_id"),
                event_type: row.get("event_type"),
                payload: row.get("payload"),
                occurred_at: row.get("occurred_at"),
                actor: row.get("actor"),
            });
        }
        Ok(logs)
    }
}

#[cfg(test)]
mod tests {
    // Helper to generic test setup (assumes DB available via env or local)
    // For unit tests we usually mock, but for repository we need integration usually.
    // However, without a running DB in this environment, I can only check compilation.
}

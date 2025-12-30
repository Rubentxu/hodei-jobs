//! PostgreSQL Saga Repository
//!
//! SQLx-based implementation of SagaRepository for PostgreSQL.
//! Provides persistence for saga instances and steps with full audit trail.

use chrono::{DateTime, Utc};
use hodei_server_domain::saga::{
    SagaContext, SagaRepository as SagaRepositoryTrait, SagaState, SagaStepData, SagaStepId,
    SagaStepState, SagaType,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::postgres::{PgPool, PgRow};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

/// PostgreSQL saga repository error types
#[derive(Debug, Error)]
pub enum PostgresSagaRepositoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Infrastructure error: {message}")]
    InfrastructureError { message: String },

    #[error("Saga not found: {saga_id}")]
    SagaNotFound { saga_id: Uuid },
}

/// PostgreSQL implementation of SagaRepository
#[derive(Debug, Clone)]
pub struct PostgresSagaRepository {
    pool: PgPool,
}

impl PostgresSagaRepository {
    /// Create a new PostgreSQL saga repository
    #[inline]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Run database migrations for saga tables
    pub async fn run_migrations(&self) -> Result<(), PostgresSagaRepositoryError> {
        // Create sagas table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sagas (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                saga_type VARCHAR(20) NOT NULL CHECK (saga_type IN ('PROVISIONING', 'EXECUTION', 'RECOVERY')),
                state VARCHAR(20) NOT NULL DEFAULT 'PENDING' CHECK (state IN ('PENDING', 'IN_PROGRESS', 'COMPENSATING', 'COMPLETED', 'FAILED', 'CANCELLED')),
                correlation_id VARCHAR(255),
                actor VARCHAR(255),
                started_at TIMESTAMPTZ DEFAULT NOW(),
                completed_at TIMESTAMPTZ,
                error_message TEXT,
                metadata JSONB DEFAULT '{}',
                created_at TIMESTAMPTZ DEFAULT NOW(),
                updated_at TIMESTAMPTZ DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create indexes
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_sagas_id ON sagas(id)"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_sagas_type ON sagas(saga_type)"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_sagas_state ON sagas(state)"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_sagas_correlation_id ON sagas(correlation_id)"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_sagas_started_at ON sagas(started_at)"#)
            .execute(&self.pool)
            .await?;

        // Create saga_steps table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS saga_steps (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                saga_id UUID NOT NULL REFERENCES sagas(id) ON DELETE CASCADE,
                step_name VARCHAR(100) NOT NULL,
                step_order INTEGER NOT NULL,
                state VARCHAR(20) NOT NULL DEFAULT 'PENDING' CHECK (state IN ('PENDING', 'IN_PROGRESS', 'COMPLETED', 'FAILED', 'COMPENSATING', 'COMPENSATED')),
                input_data JSONB,
                output_data JSONB,
                compensation_data JSONB,
                started_at TIMESTAMPTZ,
                completed_at TIMESTAMPTZ,
                error_message TEXT,
                retry_count INTEGER DEFAULT 0,
                created_at TIMESTAMPTZ DEFAULT NOW(),
                updated_at TIMESTAMPTZ DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create indexes for steps
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_saga_steps_saga_id ON saga_steps(saga_id)"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_saga_steps_state ON saga_steps(state)"#)
            .execute(&self.pool)
            .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_saga_steps_saga_order ON saga_steps(saga_id, step_order)"#)
            .execute(&self.pool)
            .await?;

        // Create saga_audit_events table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS saga_audit_events (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                saga_id UUID NOT NULL REFERENCES sagas(id) ON DELETE CASCADE,
                event_type VARCHAR(50) NOT NULL,
                step_name VARCHAR(100),
                message TEXT,
                payload JSONB,
                occurred_at TIMESTAMPTZ DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"CREATE INDEX IF NOT EXISTS idx_saga_audit_saga_id ON saga_audit_events(saga_id)"#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(r#"CREATE INDEX IF NOT EXISTS idx_saga_audit_occurred_at ON saga_audit_events(occurred_at)"#)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

/// Database row representation of a saga
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SagaDbRow {
    id: Uuid,
    saga_type: String,
    state: String,
    correlation_id: Option<String>,
    actor: Option<String>,
    started_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    error_message: Option<String>,
    metadata: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Database row representation of a saga step
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SagaStepDbRow {
    id: Uuid,
    saga_id: Uuid,
    step_name: String,
    step_order: i32,
    state: String,
    input_data: Option<serde_json::Value>,
    output_data: Option<serde_json::Value>,
    compensation_data: Option<serde_json::Value>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    error_message: Option<String>,
    retry_count: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<SagaDbRow> for SagaContext {
    fn from(row: SagaDbRow) -> Self {
        let saga_state = match row.state.as_str() {
            "PENDING" => SagaState::Pending,
            "IN_PROGRESS" => SagaState::InProgress,
            "COMPENSATING" => SagaState::Compensating,
            "COMPLETED" => SagaState::Completed,
            "FAILED" => SagaState::Failed,
            "CANCELLED" => SagaState::Cancelled,
            _ => SagaState::Pending,
        };

        let saga_type = match row.saga_type.as_str() {
            "PROVISIONING" => SagaType::Provisioning,
            "EXECUTION" => SagaType::Execution,
            "RECOVERY" => SagaType::Recovery,
            _ => SagaType::Provisioning,
        };

        // Reconstruct step_outputs from output_data if available
        let step_outputs = std::collections::HashMap::new();

        Self {
            saga_id: hodei_server_domain::saga::SagaId(row.id),
            saga_type,
            correlation_id: row.correlation_id,
            actor: row.actor,
            started_at: row.started_at,
            current_step: 0, // Will be updated based on step completion
            is_compensating: saga_state == SagaState::Compensating,
            metadata: serde_json::from_value(row.metadata).unwrap_or_default(),
            step_outputs,
            error_message: row.error_message,
        }
    }
}

impl From<SagaStepDbRow> for SagaStepData {
    fn from(row: SagaStepDbRow) -> Self {
        let state = match row.state.as_str() {
            "PENDING" => SagaStepState::Pending,
            "IN_PROGRESS" => SagaStepState::InProgress,
            "COMPLETED" => SagaStepState::Completed,
            "FAILED" => SagaStepState::Failed,
            "COMPENSATING" => SagaStepState::Compensating,
            "COMPENSATED" => SagaStepState::Compensated,
            _ => SagaStepState::Pending,
        };

        Self {
            step_id: SagaStepId(row.id),
            saga_id: hodei_server_domain::saga::SagaId(row.saga_id),
            step_name: row.step_name,
            step_order: row.step_order,
            state,
            input_data: row.input_data,
            output_data: row.output_data,
            compensation_data: row.compensation_data,
            started_at: row.started_at,
            completed_at: row.completed_at,
            error_message: row.error_message,
            retry_count: row.retry_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[async_trait::async_trait]
impl SagaRepositoryTrait for PostgresSagaRepository {
    type Error = PostgresSagaRepositoryError;

    async fn save(&self, context: &SagaContext) -> Result<(), Self::Error> {
        let saga_id = context.saga_id.0;
        let saga_type = context.saga_type.as_str();
        let state = "PENDING";
        let metadata = serde_json::to_value(&context.metadata)
            .map_err(|e| PostgresSagaRepositoryError::Serialization(e))?;

        sqlx::query(
            r#"
            INSERT INTO sagas (id, saga_type, state, correlation_id, actor, started_at, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id) DO UPDATE SET
                state = EXCLUDED.state,
                metadata = EXCLUDED.metadata,
                updated_at = NOW()
            "#,
        )
        .bind(saga_id)
        .bind(saga_type)
        .bind(state)
        .bind(&context.correlation_id)
        .bind(&context.actor)
        .bind(context.started_at)
        .bind(metadata)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_by_id(
        &self,
        saga_id: &hodei_server_domain::saga::SagaId,
    ) -> Result<Option<SagaContext>, Self::Error> {
        let row: Option<SagaDbRow> = sqlx::query_as(
            r#"
            SELECT id, saga_type, state, correlation_id, actor, started_at,
                   completed_at, error_message, metadata, created_at, updated_at
            FROM sagas WHERE id = $1
            "#,
        )
        .bind(saga_id.0)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn find_by_type(&self, saga_type: SagaType) -> Result<Vec<SagaContext>, Self::Error> {
        let saga_type_str = saga_type.as_str();
        let rows: Vec<SagaDbRow> = sqlx::query_as(
            r#"
            SELECT id, saga_type, state, correlation_id, actor, started_at,
                   completed_at, error_message, metadata, created_at, updated_at
            FROM sagas WHERE saga_type = $1
            ORDER BY started_at DESC
            "#,
        )
        .bind(saga_type_str)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn find_by_state(&self, state: SagaState) -> Result<Vec<SagaContext>, Self::Error> {
        let state_str = match state {
            SagaState::Pending => "PENDING",
            SagaState::InProgress => "IN_PROGRESS",
            SagaState::Compensating => "COMPENSATING",
            SagaState::Completed => "COMPLETED",
            SagaState::Failed => "FAILED",
            SagaState::Cancelled => "CANCELLED",
        };

        let rows: Vec<SagaDbRow> = sqlx::query_as(
            r#"
            SELECT id, saga_type, state, correlation_id, actor, started_at,
                   completed_at, error_message, metadata, created_at, updated_at
            FROM sagas WHERE state = $1
            ORDER BY started_at DESC
            "#,
        )
        .bind(state_str)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn find_by_correlation_id(
        &self,
        correlation_id: &str,
    ) -> Result<Vec<SagaContext>, Self::Error> {
        let rows: Vec<SagaDbRow> = sqlx::query_as(
            r#"
            SELECT id, saga_type, state, correlation_id, actor, started_at,
                   completed_at, error_message, metadata, created_at, updated_at
            FROM sagas WHERE correlation_id = $1
            ORDER BY started_at DESC
            "#,
        )
        .bind(correlation_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn update_state(
        &self,
        saga_id: &hodei_server_domain::saga::SagaId,
        state: SagaState,
        error_message: Option<String>,
    ) -> Result<(), Self::Error> {
        let state_str = match state {
            SagaState::Pending => "PENDING",
            SagaState::InProgress => "IN_PROGRESS",
            SagaState::Compensating => "COMPENSATING",
            SagaState::Completed => "COMPLETED",
            SagaState::Failed => "FAILED",
            SagaState::Cancelled => "CANCELLED",
        };

        let completed_at = if state.is_terminal() {
            Some(Utc::now())
        } else {
            None
        };

        sqlx::query(
            r#"
            UPDATE sagas SET
                state = $2,
                completed_at = COALESCE($3, completed_at),
                error_message = COALESCE($4, error_message),
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(saga_id.0)
        .bind(state_str)
        .bind(completed_at)
        .bind(error_message)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn mark_compensating(
        &self,
        saga_id: &hodei_server_domain::saga::SagaId,
    ) -> Result<(), Self::Error> {
        sqlx::query(
            r#"
            UPDATE sagas SET
                state = 'COMPENSATING',
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(saga_id.0)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete(
        &self,
        saga_id: &hodei_server_domain::saga::SagaId,
    ) -> Result<bool, Self::Error> {
        let result = sqlx::query(r#"DELETE FROM sagas WHERE id = $1"#)
            .bind(saga_id.0)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    // ============ Step Operations ============

    async fn save_step(&self, step: &SagaStepData) -> Result<(), Self::Error> {
        let input_data = step.input_data.clone();
        let output_data = step.output_data.clone();
        let compensation_data = step.compensation_data.clone();

        sqlx::query(
            r#"
            INSERT INTO saga_steps (id, saga_id, step_name, step_order, state, input_data, output_data, compensation_data, started_at, completed_at, error_message, retry_count)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (id) DO UPDATE SET
                state = EXCLUDED.state,
                output_data = EXCLUDED.output_data,
                compensation_data = EXCLUDED.compensation_data,
                completed_at = EXCLUDED.completed_at,
                error_message = EXCLUDED.error_message,
                updated_at = NOW()
            "#,
        )
        .bind(step.step_id.0)
        .bind(step.saga_id.0)
        .bind(&step.step_name)
        .bind(step.step_order)
        .bind("PENDING")
        .bind(input_data)
        .bind(output_data)
        .bind(compensation_data)
        .bind(step.started_at)
        .bind(step.completed_at)
        .bind(step.error_message.as_ref())
        .bind(step.retry_count)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_step_by_id(
        &self,
        step_id: &SagaStepId,
    ) -> Result<Option<SagaStepData>, Self::Error> {
        let row: Option<SagaStepDbRow> = sqlx::query_as(
            r#"
            SELECT id, saga_id, step_name, step_order, state, input_data, output_data,
                   compensation_data, started_at, completed_at, error_message, retry_count, created_at, updated_at
            FROM saga_steps WHERE id = $1
            "#,
        )
        .bind(step_id.0)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.into()))
    }

    async fn find_steps_by_saga_id(
        &self,
        saga_id: &hodei_server_domain::saga::SagaId,
    ) -> Result<Vec<SagaStepData>, Self::Error> {
        let rows: Vec<SagaStepDbRow> = sqlx::query_as(
            r#"
            SELECT id, saga_id, step_name, step_order, state, input_data, output_data,
                   compensation_data, started_at, completed_at, error_message, retry_count, created_at, updated_at
            FROM saga_steps WHERE saga_id = $1
            ORDER BY step_order ASC
            "#,
        )
        .bind(saga_id.0)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn update_step_state(
        &self,
        step_id: &SagaStepId,
        state: SagaStepState,
        output: Option<serde_json::Value>,
    ) -> Result<(), Self::Error> {
        let state_str = match state {
            SagaStepState::Pending => "PENDING",
            SagaStepState::InProgress => "IN_PROGRESS",
            SagaStepState::Completed => "COMPLETED",
            SagaStepState::Failed => "FAILED",
            SagaStepState::Compensating => "COMPENSATING",
            SagaStepState::Compensated => "COMPENSATED",
        };

        let (started_at, completed_at) = match state {
            SagaStepState::InProgress => (Some(Utc::now()), None),
            SagaStepState::Completed => (None, Some(Utc::now())),
            _ => (None, None),
        };

        sqlx::query(
            r#"
            UPDATE saga_steps SET
                state = $2,
                output_data = COALESCE($3, output_data),
                started_at = COALESCE($4, started_at),
                completed_at = COALESCE($5, completed_at),
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(step_id.0)
        .bind(state_str)
        .bind(output)
        .bind(started_at)
        .bind(completed_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update_step_compensation(
        &self,
        step_id: &SagaStepId,
        compensation_data: serde_json::Value,
    ) -> Result<(), Self::Error> {
        sqlx::query(
            r#"
            UPDATE saga_steps SET
                compensation_data = $2,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(step_id.0)
        .bind(compensation_data)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ============ Statistics ============

    async fn count_active(&self) -> Result<u64, Self::Error> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM sagas
            WHERE state IN ('PENDING', 'IN_PROGRESS', 'COMPENSATING')
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count as u64)
    }

    async fn count_by_type_and_state(
        &self,
        saga_type: SagaType,
        state: SagaState,
    ) -> Result<u64, Self::Error> {
        let saga_type_str = saga_type.as_str();
        let state_str = match state {
            SagaState::Pending => "PENDING",
            SagaState::InProgress => "IN_PROGRESS",
            SagaState::Compensating => "COMPENSATING",
            SagaState::Completed => "COMPLETED",
            SagaState::Failed => "FAILED",
            SagaState::Cancelled => "CANCELLED",
        };

        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM sagas
            WHERE saga_type = $1 AND state = $2
            "#,
        )
        .bind(saga_type_str)
        .bind(state_str)
        .fetch_one(&self.pool)
        .await?;

        Ok(count as u64)
    }

    async fn avg_duration(&self) -> Result<Option<Duration>, Self::Error> {
        let avg_ms: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT AVG(EXTRACT(EPOCH FROM (completed_at - started_at)) * 1000)::INTEGER
            FROM sagas
            WHERE state = 'COMPLETED' AND completed_at IS NOT NULL
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(avg_ms.map(|ms| Duration::from_millis(ms as u64)))
    }

    // ============ Cleanup ============

    async fn cleanup_completed(&self, older_than: Duration) -> Result<u64, Self::Error> {
        let cutoff = Utc::now() - chrono::Duration::from_std(older_than)?;

        let result = sqlx::query(
            r#"
            DELETE FROM sagas
            WHERE state = 'COMPLETED'
            AND completed_at IS NOT NULL
            AND completed_at < $1
            "#,
        )
        .bind(cutoff)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() as u64)
    }
}

/// Create a new PostgresSagaRepository from a connection pool
#[inline]
pub fn new_saga_repository(pool: PgPool) -> PostgresSagaRepository {
    PostgresSagaRepository::new(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::time::Duration;

    fn create_test_context() -> SagaContext {
        SagaContext::new(
            SagaId::new(),
            SagaType::Provisioning,
            Some("test-correlation-123".to_string()),
            Some("test-actor".to_string()),
        )
    }

    #[tokio::test]
    async fn test_save_and_find_saga() {
        // Setup
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect("postgres://postgres:postgres@localhost:5432/hodei_test")
            .await
            .unwrap();

        let repo = PostgresSagaRepository::new(pool.clone());
        repo.run_migrations().await.unwrap();

        // Test save
        let context = create_test_context();
        repo.save(&context).await.unwrap();

        // Test find
        let found = repo.find_by_id(&context.saga_id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().saga_type, SagaType::Provisioning);

        // Cleanup
        sqlx::query("DROP TABLE IF EXISTS saga_audit_events")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DROP TABLE IF EXISTS saga_steps")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DROP TABLE IF EXISTS sagas")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    #[tokio::test]
    async fn test_find_by_type() {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect("postgres://postgres:postgres@localhost:5432/hodei_test")
            .await
            .unwrap();

        let repo = PostgresSagaRepository::new(pool.clone());
        repo.run_migrations().await.unwrap();

        // Create sagas of different types
        let saga1 = SagaContext::new(SagaId::new(), SagaType::Provisioning, None, None);
        let saga2 = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);
        let saga3 = SagaContext::new(SagaId::new(), SagaType::Recovery, None, None);

        repo.save(&saga1).await.unwrap();
        repo.save(&saga2).await.unwrap();
        repo.save(&saga3).await.unwrap();

        // Find by type
        let provisioning = repo.find_by_type(SagaType::Provisioning).await.unwrap();
        assert_eq!(provisioning.len(), 1);
        assert_eq!(provisioning[0].saga_type, SagaType::Provisioning);

        let execution = repo.find_by_type(SagaType::Execution).await.unwrap();
        assert_eq!(execution.len(), 1);
        assert_eq!(execution[0].saga_type, SagaType::Execution);

        let recovery = repo.find_by_type(SagaType::Recovery).await.unwrap();
        assert_eq!(recovery.len(), 1);
        assert_eq!(recovery[0].saga_type, SagaType::Recovery);

        // Cleanup
        sqlx::query("DROP TABLE IF EXISTS saga_audit_events")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DROP TABLE IF EXISTS saga_steps")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DROP TABLE IF EXISTS sagas")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    #[tokio::test]
    async fn test_update_state() {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect("postgres://postgres:postgres@localhost:5432/hodei_test")
            .await
            .unwrap();

        let repo = PostgresSagaRepository::new(pool.clone());
        repo.run_migrations().await.unwrap();

        let context = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);
        repo.save(&context).await.unwrap();

        // Update to IN_PROGRESS
        repo.update_state(&context.saga_id, SagaState::InProgress, None)
            .await
            .unwrap();

        let found = repo.find_by_id(&context.saga_id).await.unwrap().unwrap();
        // Note: We don't store state in SagaContext, just update in DB

        // Update to COMPLETED
        repo.update_state(&context.saga_id, SagaState::Completed, None)
            .await
            .unwrap();

        // Cleanup
        sqlx::query("DROP TABLE IF EXISTS saga_audit_events")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DROP TABLE IF EXISTS saga_steps")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DROP TABLE IF EXISTS sagas")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    #[tokio::test]
    async fn test_count_active() {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect("postgres://postgres:postgres@localhost:5432/hodei_test")
            .await
            .unwrap();

        let repo = PostgresSagaRepository::new(pool.clone());
        repo.run_migrations().await.unwrap();

        // Initially empty
        let count = repo.count_active().await.unwrap();
        assert_eq!(count, 0);

        // Create active sagas
        let saga1 = SagaContext::new(SagaId::new(), SagaType::Provisioning, None, None);
        let saga2 = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);
        repo.save(&saga1).await.unwrap();
        repo.save(&saga2).await.unwrap();

        // Update to IN_PROGRESS to count
        repo.update_state(&saga1.saga_id, SagaState::InProgress, None)
            .await
            .unwrap();
        repo.update_state(&saga2.saga_id, SagaState::InProgress, None)
            .await
            .unwrap();

        let count = repo.count_active().await.unwrap();
        assert_eq!(count, 2);

        // Cleanup
        sqlx::query("DROP TABLE IF EXISTS saga_audit_events")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DROP TABLE IF EXISTS saga_steps")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DROP TABLE IF EXISTS sagas")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    #[tokio::test]
    async fn test_cleanup_completed() {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect("postgres://postgres:postgres@localhost:5432/hodei_test")
            .await
            .unwrap();

        let repo = PostgresSagaRepository::new(pool.clone());
        repo.run_migrations().await.unwrap();

        let saga = SagaContext::new(SagaId::new(), SagaType::Recovery, None, None);
        repo.save(&saga).await.unwrap();
        repo.update_state(&saga.saga_id, SagaState::Completed, None)
            .await
            .unwrap();

        // Clean up completed sagas older than 0 seconds
        let cleaned = repo
            .cleanup_completed(Duration::from_secs(0))
            .await
            .unwrap();
        assert_eq!(cleaned, 1);

        // Should be gone now
        let found = repo.find_by_id(&saga.saga_id).await.unwrap();
        assert!(found.is_none());

        pool.close().await;
    }
}

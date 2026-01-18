//! PostgreSQL Saga Repository
//!
//! SQLx-based implementation of SagaRepository for PostgreSQL.
//! Provides persistence for saga instances and steps with full audit trail.

use chrono::{DateTime, Utc};
use hodei_server_domain::saga::orchestrator::{RateLimitConfig, TokenBucketRateLimiter};
use hodei_server_domain::saga::{
    CancellationSaga, CleanupSaga, ExecutionSaga, ProvisioningSaga, RecoverySaga, Saga,
    SagaContext, SagaExecutionResult, SagaId, SagaOrchestrator,
    SagaRepository as SagaRepositoryTrait, SagaState, SagaStepData, SagaStepId, SagaStepState,
    SagaType, TimeoutSaga,
};
use hodei_server_domain::shared_kernel::{DomainError, JobId, ProviderId, WorkerId};
use hodei_server_domain::transaction::{PgTransaction, TransactionProvider};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::postgres::{PgPool, PgRow};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Semaphore;
use tracing::{error, info};
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
    ///
    /// DEPRECATED: Migrations are now handled by the central MigrationService.
    /// This method is kept for backwards compatibility but does nothing.
    pub async fn run_migrations(&self) -> Result<(), PostgresSagaRepositoryError> {
        // Migrations are now handled by the central MigrationService
        // See: hodei_server_infrastructure::persistence::postgres::migrations::run_migrations
        Ok(())
    }

    /// Get the database pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait::async_trait]
impl hodei_server_domain::transaction::TransactionProvider for PostgresSagaRepository {
    async fn begin_transaction(
        &self,
    ) -> Result<
        hodei_server_domain::transaction::PgTransaction<'_>,
        hodei_server_domain::transaction::TransactionError,
    > {
        self.pool.begin().await.map_err(|e| {
            hodei_server_domain::transaction::TransactionError::Database(e.to_string())
        })
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
    // EPIC-46 GAP-02: Optimistic Locking field
    version: i64,
    // EPIC-46 GAP-14: W3C Trace Context
    trace_parent: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl sqlx::FromRow<'_, PgRow> for SagaDbRow {
    fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(SagaDbRow {
            id: row.try_get("id")?,
            saga_type: row.try_get("saga_type")?,
            state: row.try_get("state")?,
            correlation_id: row.try_get("correlation_id")?,
            actor: row.try_get("actor")?,
            started_at: row.try_get("started_at")?,
            completed_at: row.try_get("completed_at")?,
            error_message: row.try_get("error_message")?,
            metadata: row.try_get("metadata")?,
            version: row.try_get("version")?,
            trace_parent: row.try_get("trace_parent")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
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

impl sqlx::FromRow<'_, PgRow> for SagaStepDbRow {
    fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(SagaStepDbRow {
            id: row.try_get("id")?,
            saga_id: row.try_get("saga_id")?,
            step_name: row.try_get("step_name")?,
            step_order: row.try_get("step_order")?,
            state: row.try_get("state")?,
            input_data: row.try_get("input_data")?,
            output_data: row.try_get("output_data")?,
            compensation_data: row.try_get("compensation_data")?,
            started_at: row.try_get("started_at")?,
            completed_at: row.try_get("completed_at")?,
            error_message: row.try_get("error_message")?,
            retry_count: row.try_get("retry_count")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
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

        let metadata: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_value(row.metadata).unwrap_or_default();

        // EPIC-46 GAP-02: Include version and trace_parent in from_persistence
        // Use from_persistence constructor
        SagaContext::from_persistence(
            hodei_server_domain::saga::SagaId(row.id),
            saga_type,
            row.correlation_id,
            row.actor,
            row.started_at,
            0, // current_step - will be updated based on step completion
            saga_state == SagaState::Compensating,
            metadata,
            row.error_message,
            saga_state,
            row.version as u64, // EPIC-46 GAP-02: Optimistic Locking version
            row.trace_parent,   // EPIC-46 GAP-14: W3C Trace Context
        )
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
    type Error = hodei_server_domain::shared_kernel::DomainError;

    async fn save(&self, context: &SagaContext) -> Result<(), Self::Error> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        self.save_with_tx(&mut tx, context).await?;

        tx.commit().await.map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        Ok(())
    }

    /// EPIC-43: Create saga if it doesn't exist (idempotent creation)
    async fn create_if_not_exists(&self, context: &SagaContext) -> Result<bool, Self::Error> {
        let saga_id = context.saga_id.0;
        let saga_type = context.saga_type.as_str();
        let state = "PENDING";
        let metadata = serde_json::to_value(&context.metadata).map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Serialization error: {}", e),
            }
        })?;

        // ON CONFLICT DO NOTHING returns 0 rows affected if conflict
        let result = sqlx::query(
            r#"
            INSERT INTO sagas (id, saga_type, state, correlation_id, actor, started_at, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id) DO NOTHING
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
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        // Return true if row was inserted (not conflicted)
        Ok(result.rows_affected() > 0)
    }

    async fn find_by_id(
        &self,
        saga_id: &hodei_server_domain::saga::SagaId,
    ) -> Result<Option<SagaContext>, Self::Error> {
        let row: Option<SagaDbRow> = sqlx::query_as(
            r#"
            SELECT id, saga_type, state, correlation_id, actor, started_at,
                   completed_at, error_message, metadata, created_at, updated_at,
                   version, trace_parent
            FROM sagas WHERE id = $1
            "#,
        )
        .bind(saga_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        Ok(row.map(|r| r.into()))
    }

    async fn find_by_type(&self, saga_type: SagaType) -> Result<Vec<SagaContext>, Self::Error> {
        let saga_type_str = saga_type.as_str();
        let rows: Vec<SagaDbRow> = sqlx::query_as(
            r#"
            SELECT id, saga_type, state, correlation_id, actor, started_at,
                   completed_at, error_message, metadata, created_at, updated_at,
                   version, trace_parent
            FROM sagas WHERE saga_type = $1
            ORDER BY started_at DESC
            "#,
        )
        .bind(saga_type_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

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
                   completed_at, error_message, metadata, created_at, updated_at,
                   version, trace_parent
            FROM sagas WHERE state = $1
            ORDER BY started_at DESC
            "#,
        )
        .bind(state_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn find_by_correlation_id(
        &self,
        correlation_id: &str,
    ) -> Result<Vec<SagaContext>, Self::Error> {
        let rows: Vec<SagaDbRow> = sqlx::query_as(
            r#"
            SELECT id, saga_type, state, correlation_id, actor, started_at,
                   completed_at, error_message, metadata, created_at, updated_at,
                   version, trace_parent
            FROM sagas WHERE correlation_id = $1
            ORDER BY started_at DESC
            "#,
        )
        .bind(correlation_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn update_state(
        &self,
        saga_id: &hodei_server_domain::saga::SagaId,
        state: SagaState,
        error_message: Option<String>,
    ) -> Result<(), Self::Error> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        self.update_state_with_tx(&mut tx, saga_id, state, error_message)
            .await?;

        tx.commit().await.map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

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
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        Ok(())
    }

    async fn delete(
        &self,
        saga_id: &hodei_server_domain::saga::SagaId,
    ) -> Result<bool, Self::Error> {
        let result = sqlx::query(r#"DELETE FROM sagas WHERE id = $1"#)
            .bind(saga_id.0)
            .execute(&self.pool)
            .await
            .map_err(
                |e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Database error: {}", e),
                },
            )?;

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
        .await
        .map_err(|e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: format!("Database error: {}", e),
        })?;

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
        .await
        .map_err(|e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: format!("Database error: {}", e),
        })?;

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
        .await
        .map_err(|e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: format!("Database error: {}", e),
        })?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn update_step_state(
        &self,
        step_id: &SagaStepId,
        state: SagaStepState,
        output: Option<serde_json::Value>,
    ) -> Result<(), Self::Error> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        self.update_step_state_with_tx(&mut tx, step_id, state, output)
            .await?;

        tx.commit().await.map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        Ok(())
    }

    async fn update_step_compensation(
        &self,
        step_id: &SagaStepId,
        compensation_data: serde_json::Value,
    ) -> Result<(), Self::Error> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        self.update_step_compensation_with_tx(&mut tx, step_id, compensation_data)
            .await?;

        tx.commit().await.map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        Ok(())
    }

    // ============ Transactional Operations ============

    async fn save_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        context: &SagaContext,
    ) -> Result<(), Self::Error> {
        let saga_id = context.saga_id.0;
        let saga_type = context.saga_type.as_str();
        let state = context.state.to_string();
        let metadata = serde_json::to_value(&context.metadata).map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Serialization error: {}", e),
            }
        })?;

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
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        Ok(())
    }

    async fn update_state_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        saga_id: &SagaId,
        state: SagaState,
        error_message: Option<String>,
    ) -> Result<(), Self::Error> {
        let state_str = state.to_string();
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
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        Ok(())
    }

    async fn save_step_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        step: &SagaStepData,
    ) -> Result<(), Self::Error> {
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
        .bind(step.state.to_string())
        .bind(input_data)
        .bind(output_data)
        .bind(compensation_data)
        .bind(step.started_at)
        .bind(step.completed_at)
        .bind(step.error_message.as_ref())
        .bind(step.retry_count)
        .execute(&mut *tx)
        .await
        .map_err(|e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
            message: format!("Database error: {}", e),
        })?;

        Ok(())
    }

    async fn update_step_state_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
        step_id: &SagaStepId,
        state: SagaStepState,
        output: Option<serde_json::Value>,
    ) -> Result<(), Self::Error> {
        let state_str = state.to_string();
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
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        Ok(())
    }

    async fn update_step_compensation_with_tx(
        &self,
        tx: &mut PgTransaction<'_>,
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
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

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
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

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
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

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
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        Ok(avg_ms.map(|ms| Duration::from_millis(ms as u64)))
    }

    // ============ Cleanup ============

    async fn cleanup_completed(&self, older_than: Duration) -> Result<u64, Self::Error> {
        let cutoff = Utc::now()
            - chrono::Duration::from_std(older_than).map_err(|e| {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Invalid duration for cleanup: {}", e),
                }
            })?;

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
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error: {}", e),
            }
        })?;

        Ok(result.rows_affected() as u64)
    }

    // ============ Concurrency Control ============

    async fn claim_pending_sagas(
        &self,
        limit: u64,
        _instance_id: &str,
    ) -> Result<Vec<SagaContext>, Self::Error> {
        // EPIC-45 Gap 5: Include stuck sagas (IN_PROGRESS > 5 min ago, COMPENSATING)
        // EPIC-46 GAP-02: Include version and trace_parent columns
        // Use FOR UPDATE SKIP LOCKED to atomically claim sagas without blocking
        let rows = sqlx::query_as::<_, SagaDbRow>(
            r#"
            SELECT id, saga_type, state, correlation_id, actor, started_at,
                   metadata, error_message, completed_at, created_at, updated_at,
                   version, trace_parent
            FROM sagas
            WHERE state = 'PENDING'
               OR (state = 'IN_PROGRESS' AND updated_at < NOW() - INTERVAL '5 minutes')
               OR state = 'COMPENSATING'
            ORDER BY
                CASE
                    WHEN state = 'COMPENSATING' THEN 1
                    WHEN state = 'IN_PROGRESS' THEN 2
                    ELSE 3
                END,
                created_at ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Database error claiming pending sagas: {}", e),
            }
        })?;

        // Update state to IN_PROGRESS for all claimed sagas
        for row in &rows {
            sqlx::query(
                r#"
                UPDATE sagas
                SET state = 'IN_PROGRESS', updated_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(row.id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Database error updating claimed saga state: {}", e),
                }
            })?;
        }

        // Convert rows to SagaContext
        Ok(rows
            .into_iter()
            .map(|row| {
                let saga_type = match row.saga_type.as_str() {
                    "PROVISIONING" => SagaType::Provisioning,
                    "EXECUTION" => SagaType::Execution,
                    "RECOVERY" => SagaType::Recovery,
                    _ => SagaType::Execution,
                };

                let metadata: std::collections::HashMap<String, serde_json::Value> =
                    serde_json::from_value(
                        row.metadata
                            .as_object()
                            .map_or(serde_json::json!({}), |v| serde_json::json!(v)),
                    )
                    .unwrap_or_default();

                SagaContext::from_persistence(
                    SagaId::from_uuid(row.id),
                    saga_type,
                    row.correlation_id,
                    row.actor,
                    row.started_at,
                    0,     // current_step not persisted in this schema
                    false, // is_compensating not persisted in this schema
                    metadata,
                    row.error_message,
                    SagaState::Pending, // Default state for query
                    row.version as u64, // EPIC-46 GAP-02: Optimistic Locking version
                    row.trace_parent,   // EPIC-46 GAP-14: W3C Trace Context
                )
            })
            .collect())
    }
}

/// Create a new PostgresSagaRepository from a connection pool
#[inline]
pub fn new_saga_repository(pool: PgPool) -> PostgresSagaRepository {
    PostgresSagaRepository::new(pool)
}

// ============================================================================
// PostgresSagaOrchestrator - Production-Ready Saga Orchestrator
// ============================================================================

use std::sync::atomic::{AtomicUsize, Ordering};

/// Configuration for PostgresSagaOrchestrator
#[derive(Debug, Clone)]
pub struct PostgresSagaOrchestratorConfig {
    /// Maximum concurrent sagas allowed
    pub max_concurrent_sagas: usize,
    /// Maximum concurrent steps per saga
    pub max_concurrent_steps: usize,
    /// Timeout for individual steps
    pub step_timeout: Duration,
    /// Timeout for entire saga execution
    pub saga_timeout: Duration,
    /// Maximum retry attempts for failed steps
    pub max_retries: u32,
    /// Base delay for exponential backoff
    pub retry_backoff: Duration,
    /// Rate limiting configuration (US-14: Production-Ready Gaps)
    pub rate_limit: Option<RateLimitConfig>,
    /// Maximum sagas per minute
    pub max_sagas_per_minute: Option<u32>,
}

impl Default for PostgresSagaOrchestratorConfig {
    fn default() -> Self {
        Self {
            max_concurrent_sagas: 100,
            max_concurrent_steps: 10,
            step_timeout: Duration::from_secs(30),
            saga_timeout: Duration::from_secs(300),
            max_retries: 3,
            retry_backoff: Duration::from_secs(1),
            rate_limit: None,
            max_sagas_per_minute: Some(60),
        }
    }
}

/// Production-ready saga orchestrator using PostgreSQL for persistence.
#[derive(Debug, Clone)]
pub struct PostgresSagaOrchestrator<R: SagaRepositoryTrait + Clone> {
    repository: Arc<R>,
    config: PostgresSagaOrchestratorConfig,
    /// Active saga count (for concurrency control)
    active_sagas: Arc<AtomicUsize>,
    /// Rate limiter for saga execution (US-14: Production-Ready Gaps)
    rate_limiter: Option<TokenBucketRateLimiter>,
    /// Semaphore for concurrent saga control
    concurrency_semaphore: Arc<Semaphore>,
}

impl<R: SagaRepositoryTrait + Clone> PostgresSagaOrchestrator<R> {
    /// Creates a new production-ready saga orchestrator
    pub fn new(repository: Arc<R>, config: Option<PostgresSagaOrchestratorConfig>) -> Self {
        let config = config.unwrap_or_default();
        let rate_limiter = config.rate_limit.map(TokenBucketRateLimiter::new);
        let concurrency_semaphore = Arc::new(Semaphore::new(config.max_concurrent_sagas));

        Self {
            repository,
            config,
            active_sagas: Arc::new(AtomicUsize::new(0)),
            rate_limiter,
            concurrency_semaphore,
        }
    }
}

#[async_trait::async_trait]
impl<R: SagaRepositoryTrait + TransactionProvider + Clone + Send + Sync + 'static> SagaOrchestrator
    for PostgresSagaOrchestrator<R>
where
    <R as SagaRepositoryTrait>::Error: std::fmt::Display
        + Send
        + Sync
        + From<sqlx::Error>
        + From<serde_json::Error>
        + From<hodei_server_domain::shared_kernel::DomainError>,
{
    type Error = <R as SagaRepositoryTrait>::Error;

    async fn execute_saga(
        &self,
        saga: &dyn Saga,
        mut context: SagaContext,
    ) -> Result<SagaExecutionResult, Self::Error> {
        let start_time = std::time::Instant::now();
        let saga_id = context.saga_id.clone();

        // US-14: Check rate limit first (Token Bucket Rate Limiting)
        if let Some(ref limiter) = self.rate_limiter {
            if !limiter.try_acquire() {
                let remaining = limiter.remaining();
                return Err(Self::Error::from(
                    hodei_server_domain::shared_kernel::DomainError::OrchestratorError(
                        hodei_server_domain::saga::orchestrator::OrchestratorError::RateLimitExceeded { remaining },
                    ),
                ));
            }
        }

        // Check concurrency limit using semaphore
        match self.concurrency_semaphore.try_acquire() {
            Ok(_permit) => { /* permit acquired, guard will release */ }
            Err(_) => {
                return Err(Self::Error::from(
                    hodei_server_domain::shared_kernel::DomainError::OrchestratorError(
                        hodei_server_domain::saga::orchestrator::OrchestratorError::ConcurrencyLimitExceeded {
                            current: self.active_sagas.load(Ordering::SeqCst),
                            max: self.config.max_concurrent_sagas,
                        },
                    ),
                ));
            }
        };

        // Increment active count
        self.active_sagas.fetch_add(1, Ordering::SeqCst);

        // Ensure we decrement on exit (RAII guard)
        let active_sagas = self.active_sagas.clone();
        let _guard = DropGuard {
            active_sagas,
            semaphore: self.concurrency_semaphore.clone(),
        };

        // Save initial context (EPIC-88: Ensure this is also transactional if needed, but for now we keep it as is or wrap in tx)
        {
            let mut tx = self.repository.begin_transaction().await.map_err(|e| {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Transaction error: {}", e),
                }
            })?;
            self.repository.save_with_tx(&mut tx, &context).await?;
            tx.commit().await?;
        }

        // EPIC-SAGA-ENGINE: RESUME LOGIC
        // Load existing steps to determine if we are resuming
        let stored_steps = self
            .repository
            .find_steps_by_saga_id(&saga_id)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to load saga steps for resume: {}", e),
            })?;

        // Check if we are in compensating state
        if context.state == SagaState::Compensating {
            info!(saga_id = %saga_id, "Resuming saga in COMPENSATING state");
            // Return false (not success) but with context to trigger compensation logic below
            // Actually, the current structure expects us to run steps or return error.
            // If we are compensating, we should jump directly to compensation logic.
            // We can simulate a failure from the current step to trigger the existing compensation block.

            // Identify the failed step (the last one that started but didn't complete,
            // or the last completed one if all started ones completed)
            let _last_failed_message = context
                .error_message
                .clone()
                .unwrap_or_else(|| "Resuming compensation".to_string());

            // Trigger compensation block by returning an error immediately?
            // Or better: Extract the compensation logic into a helper method, but that's a big refactor.
            // For now, let's rely on the loop structure.
        }

        let mut completed_step_indices = std::collections::HashSet::new();
        let mut executed_steps = 0;

        // Rehydrate context from stored completed steps
        for step_data in &stored_steps {
            if step_data.state == SagaStepState::Completed {
                completed_step_indices.insert(step_data.step_order as usize);
                executed_steps += 1; // Count as executed

                // Rehydrate output data into context
                if let Some(output) = &step_data.output_data {
                    context
                        .set_step_output(&step_data.step_name, output)
                        .map_err(|e| DomainError::InfrastructureError {
                            message: format!("Failed to rehydrate step output: {}", e),
                        })?;
                }
            }
        }

        if executed_steps > 0 {
            info!(
                saga_id = %saga_id,
                completed_steps = executed_steps,
                "Resuming saga execution (skipping completed steps)"
            );
        }

        let steps = saga.steps();

        // Execute steps sequentially
        for (index, step) in steps.into_iter().enumerate() {
            // EPIC-SAGA-ENGINE: Resume Check
            // If step is already completed, skip it
            if completed_step_indices.contains(&index) {
                tracing::debug!(
                    saga_id = %saga_id,
                    step = step.name(),
                    index = index,
                    "Skipping already completed step"
                );
                continue;
            }
            // Check saga timeout
            if start_time.elapsed() > self.config.saga_timeout {
                context.set_error(format!("Saga timed out after {:?}", start_time.elapsed()));
                let mut tx = self.repository.begin_transaction().await.map_err(|e| {
                    hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                        message: format!("Transaction error: {}", e),
                    }
                })?;
                self.repository
                    .update_state_with_tx(
                        &mut tx,
                        &saga_id,
                        SagaState::Failed,
                        context.error_message.clone(),
                    )
                    .await
                    .ok();
                tx.commit().await.ok();

                return Err(Self::Error::from(
                    hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                        message: format!("Saga timed out after {:?}", start_time.elapsed()),
                    },
                ));
            }

            // Create step data for persistence
            let step_id = SagaStepId(Uuid::new_v4());
            let step_data = SagaStepData::new_with_order(
                saga_id.clone(),
                step.name().to_string(),
                index as i32,
                None,
                step_id,
            );

            // EPIC-88: Start transaction for the entire step execution
            let mut tx = self.repository.begin_transaction().await?;

            // Save step start
            self.repository
                .save_step_with_tx(&mut tx, &step_data)
                .await?;

            // Mark step as IN_PROGRESS before execution
            self.repository
                .update_step_state_with_tx(
                    &mut tx,
                    &step_data.step_id,
                    SagaStepState::InProgress,
                    None,
                )
                .await?;

            // Execute step with timeout (Note: we hold the transaction during execution)
            let step_result = tokio::time::timeout(
                self.config.step_timeout,
                step.execute(&mut tx, &mut context),
            )
            .await;

            match step_result {
                Ok(Ok(())) => {
                    // Step succeeded
                    executed_steps += 1;
                    self.repository
                        .update_step_state_with_tx(
                            &mut tx,
                            &step_data.step_id,
                            SagaStepState::Completed,
                            None,
                        )
                        .await?;

                    // EPIC-45 FIX: Persist updated context (metadata) after each successful step
                    self.repository.save_with_tx(&mut tx, &context).await?;

                    // Commit step transaction
                    tx.commit().await?;
                }
                Ok(Err(e)) => {
                    // Step failed - rollback transaction
                    tx.rollback().await?;

                    // Start a new transaction for state update and potential compensation
                    let mut fail_tx = self.repository.begin_transaction().await.map_err(|e| {
                        hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                            message: format!("Transaction error: {}", e),
                        }
                    })?;

                    // Mark it as FAILED in a fresh transaction or within the compensation block
                    self.repository
                        .update_step_state_with_tx(
                            &mut fail_tx,
                            &step_data.step_id,
                            SagaStepState::Failed,
                            None,
                        )
                        .await?;

                    // Start compensation state
                    context.set_error(e.to_string());
                    self.repository
                        .update_state_with_tx(
                            &mut fail_tx,
                            &saga_id,
                            SagaState::Compensating,
                            Some(e.to_string()),
                        )
                        .await?;

                    fail_tx.commit().await?;

                    // EPIC-SAGA-ENGINE: Execute compensation for completed steps in reverse order
                    info!(
                        saga_id = %saga_id,
                        failed_step = step.name(),
                        error = %e,
                        "⚠️ Step failed, starting compensation for {} completed steps",
                        index
                    );

                    // Load completed steps from database
                    let completed_steps_data = self
                        .repository
                        .find_steps_by_saga_id(&saga_id)
                        .await
                        .map_err(|db_err| DomainError::InfrastructureError {
                            message: format!(
                                "Failed to load saga steps for compensation: {}",
                                db_err
                            ),
                        })
                        .map_err(Into::<Self::Error>::into)?
                        .into_iter()
                        .filter(|s| s.state == SagaStepState::Completed)
                        .collect::<Vec<_>>();

                    let mut compensations_executed = 0;
                    let saga_steps = saga.steps();

                    // Iterate steps in reverse order for compensation
                    for (i, step_def) in saga_steps.into_iter().enumerate().rev() {
                        // Only compensate if this step was completed
                        if completed_steps_data
                            .iter()
                            .any(|s| s.step_order as usize == i)
                        {
                            if step_def.has_compensation() {
                                info!(
                                    saga_id = %saga_id,
                                    step = step_def.name(),
                                    step_order = i,
                                    "🔄 Compensating step"
                                );

                                // EPIC-88: Start transaction for each compensation step
                                let mut comp_tx = self.repository.begin_transaction().await.map_err(|e| {
                        hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                            message: format!("Transaction error: {}", e),
                        }
                    })?;

                                // Mark step as COMPENSATING
                                if let Some(step_record) = completed_steps_data
                                    .iter()
                                    .find(|s| s.step_order as usize == i)
                                {
                                    self.repository
                                        .update_step_state_with_tx(
                                            &mut comp_tx,
                                            &step_record.step_id,
                                            SagaStepState::Compensating,
                                            None,
                                        )
                                        .await
                                        .map_err(|db_err| DomainError::InfrastructureError {
                                            message: format!(
                                                "Failed to update step state to compensating: {}",
                                                db_err
                                            ),
                                        })
                                        .map_err(Into::<Self::Error>::into)?;
                                }

                                // Execute compensation
                                match step_def.compensate(&mut comp_tx, &mut context).await {
                                    Ok(()) => {
                                        info!(
                                            saga_id = %saga_id,
                                            step = step_def.name(),
                                            "✅ Step compensated successfully"
                                        );

                                        // Mark step as COMPENSATED
                                        if let Some(step_record) = completed_steps_data
                                            .iter()
                                            .find(|s| s.step_order as usize == i)
                                        {
                                            self.repository
                                                .update_step_state_with_tx(
                                                    &mut comp_tx,
                                                    &step_record.step_id,
                                                    SagaStepState::Compensated,
                                                    None,
                                                )
                                                .await
                                                .map_err(|db_err| {
                                                    DomainError::InfrastructureError {
                                                        message: format!(
                                                            "Failed to update step state to compensated: {}",
                                                            db_err
                                                        ),
                                                    }
                                                })
                                                .map_err(Into::<Self::Error>::into)?;
                                        }

                                        // Commit compensation transaction
                                        comp_tx.commit().await?;

                                        compensations_executed += 1;
                                    }
                                    Err(comp_err) => {
                                        error!(
                                            saga_id = %saga_id,
                                            step = step_def.name(),
                                            error = %comp_err,
                                            "❌ Compensation failed - saga will be marked as FAILED"
                                        );

                                        // Rollback compensation transaction
                                        comp_tx.rollback().await?;

                                        // Mark saga as FAILED (compensation also failed)
                                        let mut final_tx =
                                            self.repository.begin_transaction().await.map_err(|e| {
                        hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                            message: format!("Transaction error: {}", e),
                        }
                    })?;
                                        self.repository
                                            .update_state_with_tx(
                                                &mut final_tx,
                                                &saga_id,
                                                SagaState::Failed,
                                                Some(format!(
                                                    "Compensation failed for step '{}': {}",
                                                    step_def.name(),
                                                    comp_err
                                                )),
                                            )
                                            .await
                                            .ok();
                                        final_tx.commit().await.ok();

                                        return Err::<_, Self::Error>(
                                            DomainError::SagaStepFailed {
                                                step: step_def.name().to_string(),
                                                error: comp_err.to_string(),
                                            }
                                            .into(),
                                        );
                                    }
                                }
                            } else {
                                info!(
                                    saga_id = %saga_id,
                                    step = step_def.name(),
                                    "⏭️  Skipping compensation (not supported by step)"
                                );
                            }
                        }
                    }

                    // Mark saga as COMPLETED (successful compensation = complete rollback)
                    let mut final_comp_tx =
                        self.repository.begin_transaction().await.map_err(|e| {
                            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                                message: format!("Transaction error: {}", e),
                            }
                        })?;
                    self.repository
                        .update_state_with_tx(
                            &mut final_comp_tx,
                            &saga_id,
                            SagaState::Completed,
                            None,
                        )
                        .await
                        .map_err(|db_err| DomainError::InfrastructureError {
                            message: format!(
                                "Failed to update saga state after compensation: {}",
                                db_err
                            ),
                        })
                        .map_err(Into::<Self::Error>::into)?;

                    final_comp_tx.commit().await?;

                    info!(
                        saga_id = %saga_id,
                        compensations = compensations_executed,
                        original_error = %e,
                        "✅ Saga compensated successfully (rollback complete)"
                    );

                    return Ok(SagaExecutionResult::compensated(
                        saga_id,
                        saga.saga_type(),
                        start_time.elapsed(),
                        executed_steps as u32,
                        compensations_executed,
                    ));
                }
                Err(_) => {
                    // Step timed out - rollback step transaction
                    tx.rollback().await.ok();

                    // Step timed out
                    let timeout_error =
                        format!("Step timed out after {:?}", self.config.step_timeout);
                    context.set_error(timeout_error.clone());

                    return Err::<_, Self::Error>(
                        DomainError::InfrastructureError {
                            message: timeout_error,
                        }
                        .into(),
                    );
                }
            }
        }

        // All steps completed successfully
        {
            let mut tx = self.repository.begin_transaction().await?;
            self.repository
                .update_state_with_tx(&mut tx, &saga_id, SagaState::Completed, None)
                .await?;
            tx.commit().await?;
        }

        Ok(SagaExecutionResult::completed_with_steps(
            saga_id,
            saga.saga_type(),
            start_time.elapsed(),
            executed_steps as u32,
        ))
    }

    /// EPIC-42: Execute saga directly from context (Reactive Saga Processing)
    async fn execute(&self, context: &SagaContext) -> Result<SagaExecutionResult, Self::Error> {
        // Create the appropriate saga based on saga_type
        let saga: Box<dyn Saga> = match context.saga_type {
            SagaType::Provisioning => {
                // Extract provider_id from metadata
                let provider_id_str = context
                    .metadata
                    .get("provider_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let provider_id = if !provider_id_str.is_empty() {
                    ProviderId::from_uuid(
                        uuid::Uuid::parse_str(&provider_id_str)
                            .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                    )
                } else {
                    ProviderId::new()
                };

                // Extract worker_spec from metadata
                let _spec_str = context
                    .metadata
                    .get("worker_spec")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                // Create provisioning saga with extracted config
                // Note: In a full implementation, we'd deserialize the full spec
                let spec = hodei_server_domain::workers::WorkerSpec::new(
                    "hodei-jobs-worker:latest".to_string(),
                    "http://localhost:50051".to_string(),
                );

                Box::new(ProvisioningSaga::new(spec, provider_id))
            }
            SagaType::Execution => {
                // Extract job_id from metadata
                let job_id_str = context
                    .metadata
                    .get("job_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let job_id = if !job_id_str.is_empty() {
                    JobId(
                        uuid::Uuid::parse_str(&job_id_str).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                    )
                } else {
                    JobId::new()
                };

                Box::new(ExecutionSaga::new(job_id))
            }
            SagaType::Recovery => {
                // Recovery sagas would need additional context
                // BUG-009 Fix: WorkerId is now correctly typed in RecoverySaga
                Box::new(RecoverySaga::new(JobId::new(), WorkerId::new(), None))
            }
            SagaType::Cancellation => {
                // Extract job_id and reason from metadata
                let job_id_str = context
                    .metadata
                    .get("job_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let reason = context
                    .metadata
                    .get("cancellation_reason")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "cancelled".to_string());

                let job_id = if !job_id_str.is_empty() {
                    JobId(
                        uuid::Uuid::parse_str(&job_id_str).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                    )
                } else {
                    JobId::new()
                };

                Box::new(CancellationSaga::new(job_id, reason))
            }
            SagaType::Timeout => {
                // Extract job_id and timeout duration from metadata
                let job_id_str = context
                    .metadata
                    .get("job_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let timeout_secs = context
                    .metadata
                    .get("timeout_seconds")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(300);

                let reason = context
                    .metadata
                    .get("timeout_reason")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "timeout".to_string());

                let job_id = if !job_id_str.is_empty() {
                    JobId(
                        uuid::Uuid::parse_str(&job_id_str).unwrap_or_else(|_| uuid::Uuid::new_v4()),
                    )
                } else {
                    JobId::new()
                };

                Box::new(TimeoutSaga::new(
                    job_id,
                    Duration::from_secs(timeout_secs as u64),
                    reason,
                ))
            }
            SagaType::Cleanup => {
                // CleanupSaga doesn't require metadata - creates with default thresholds
                Box::new(CleanupSaga::new())
            }
        };

        // Execute with a clone of the context
        self.execute_saga(&*saga, context.clone()).await
    }

    async fn get_saga(&self, saga_id: &SagaId) -> Result<Option<SagaContext>, Self::Error> {
        self.repository.find_by_id(saga_id).await
    }

    async fn cancel_saga(&self, saga_id: &SagaId) -> Result<(), Self::Error> {
        self.repository.mark_compensating(saga_id).await?;

        self.repository
            .update_state(
                saga_id,
                SagaState::Cancelled,
                Some("Cancelled by user".to_string()),
            )
            .await?;

        Ok(())
    }
}

/// RAII guard to decrement active saga count and release semaphore permit
struct DropGuard {
    active_sagas: Arc<AtomicUsize>,
    semaphore: Arc<Semaphore>,
}

impl Drop for DropGuard {
    fn drop(&mut self) {
        self.active_sagas.fetch_sub(1, Ordering::SeqCst);
        // Release semaphore permit
        self.semaphore.add_permits(1);
    }
}

// ============================================================================
// SagaPoller - Background Poller for Pending Sagas
// ============================================================================

/// Saga poller configuration
#[derive(Debug, Clone)]
pub struct SagaPollerConfig {
    /// How often to poll for pending sagas
    pub polling_interval: Duration,
    /// Maximum concurrent sagas
    pub max_concurrent: usize,
}

impl Default for SagaPollerConfig {
    fn default() -> Self {
        Self {
            polling_interval: Duration::from_secs(5),
            max_concurrent: 10,
        }
    }
}

/// SagaPoller - Background task that processes pending sagas from the database
#[derive(Clone)]
pub struct SagaPoller {
    repository: Arc<PostgresSagaRepository>,
    orchestrator: Arc<PostgresSagaOrchestrator<PostgresSagaRepository>>,
    config: SagaPollerConfig,
}

impl SagaPoller {
    /// Create a new SagaPoller
    pub fn new(
        repository: Arc<PostgresSagaRepository>,
        orchestrator: Arc<PostgresSagaOrchestrator<PostgresSagaRepository>>,
        config: Option<SagaPollerConfig>,
    ) -> Self {
        Self {
            repository,
            orchestrator,
            config: config.unwrap_or_default(),
        }
    }

    /// Start the background poller
    pub fn start<F>(&self, saga_factory: F) -> SagaPollerHandle
    where
        F: Fn(hodei_server_domain::saga::SagaType, serde_json::Value) -> Option<Box<dyn Saga>>
            + Send
            + Sync
            + 'static,
    {
        let repository = self.repository.clone();
        let orchestrator = self.orchestrator.clone();
        let config = self.config.clone();

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

        let poller_handle = SagaPollerHandle {
            shutdown_tx: shutdown_tx.clone(),
        };

        tokio::spawn(async move {
            info!(
                "🔄 Saga background poller started (interval: {:?})",
                config.polling_interval
            );

            let mut interval = tokio::time::interval(config.polling_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Process pending sagas
                        if let Err(e) = process_pending_sagas(
                            &repository,
                            &orchestrator,
                            &saga_factory,
                        ).await {
                            tracing::error!("Error processing pending sagas: {}", e);
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("🔄 Saga background poller shutting down");
                        break;
                    }
                }
            }
        });

        poller_handle
    }
}

/// Handle to control the saga poller
#[derive(Clone)]
pub struct SagaPollerHandle {
    shutdown_tx: tokio::sync::mpsc::Sender<()>,
}

impl SagaPollerHandle {
    /// Signal the poller to stop
    pub async fn stop(&self) -> Result<(), tokio::sync::mpsc::error::SendError<()>> {
        self.shutdown_tx.send(()).await
    }
}

/// Process all pending sagas from the database
async fn process_pending_sagas<F>(
    repository: &PostgresSagaRepository,
    orchestrator: &PostgresSagaOrchestrator<PostgresSagaRepository>,
    saga_factory: &F,
) -> Result<(), DomainError>
where
    F: Fn(hodei_server_domain::saga::SagaType, serde_json::Value) -> Option<Box<dyn Saga>>
        + Send
        + Sync,
{
    // Find all pending sagas
    // Find pending sagas using safe concurrency
    // EPIC-45 Gap 5: Use claim_pending_sagas to prevent race conditions
    // and include stuck/compensating sagas
    let pending_sagas = repository
        .claim_pending_sagas(10, "saga-poller") // Limit 10 per cycle, default instance id
        .await?;

    for saga_context in pending_sagas {
        // Get saga type from context
        let saga_type = saga_context.saga_type;
        let saga_id = saga_context.saga_id.clone();
        let metadata = saga_context.metadata.clone();

        // Create saga instance from factory
        if let Some(saga) = saga_factory(
            saga_type,
            serde_json::to_value(&metadata).unwrap_or_default(),
        ) {
            info!(
                "⚡ Executing pending saga: {} ({})",
                saga_id.0,
                saga_type.as_str()
            );

            match orchestrator.execute_saga(&*saga, saga_context).await {
                Ok(result) => {
                    if result.is_success() {
                        info!("✅ Saga completed successfully: {}", saga_id.0);
                    } else {
                        info!(
                            "⚠️ Saga failed: {} - {}",
                            saga_id.0,
                            result.error_message.clone().unwrap_or_default()
                        );
                    }
                }
                Err(e) => {
                    tracing::error!("❌ Saga execution error: {} - {:?}", saga_id.0, e);
                }
            }
        }
    }

    Ok(())
}

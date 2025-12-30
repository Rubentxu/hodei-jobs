//! PostgreSQL Saga Repository
//!
//! SQLx-based implementation of SagaRepository for PostgreSQL.
//! Uses the saga tables defined in migrations/20251230120000_saga_pattern/

use hodei_server_domain::saga::{
    SagaContext, SagaId, SagaRepository, SagaState, SagaStepData, SagaStepId, SagaStepState,
    SagaType,
};
use sqlx::FromRow;
use sqlx::postgres::PgPool;
use uuid::Uuid;

/// Error type specific to PostgreSQL saga repository
#[derive(Debug, thiserror::Error)]
pub enum PostgresSagaRepositoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Saga domain error: {0}")]
    Saga(#[from] hodei_server_domain::saga::SagaError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Infrastructure error: {message}")]
    InfrastructureError { message: String },
}

/// Row struct for sagas query (matches 20251230120000_saga_pattern migration)
#[derive(FromRow, Debug, Clone)]
struct SagaRow {
    id: Uuid,
    saga_type: String,
    state: String,
    correlation_id: Option<String>,
    actor: Option<String>,
    started_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
    error_message: Option<String>,
    metadata: Option<sqlx::types::Json<serde_json::Value>>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

/// Row struct for saga_steps query (matches 20251230120000_saga_pattern migration)
#[derive(FromRow, Debug, Clone)]
struct SagaStepRow {
    id: Uuid,
    saga_id: Uuid,
    step_name: String,
    step_order: i32,
    state: String,
    input_data: Option<sqlx::types::Json<serde_json::Value>>,
    output_data: Option<sqlx::types::Json<serde_json::Value>>,
    compensation_data: Option<sqlx::types::Json<serde_json::Value>>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
    error_message: Option<String>,
    retry_count: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

/// PostgreSQL implementation of SagaRepository
pub struct PostgresSagaRepository {
    pool: PgPool,
}

impl PostgresSagaRepository {
    /// Create a new PostgreSQL saga repository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Run database migrations for saga tables
    ///
    /// DEPRECATED: Migrations are now handled by the central MigrationService.
    pub async fn run_migrations(&self) -> Result<(), PostgresSagaRepositoryError> {
        Ok(())
    }

    /// Convert database string to SagaState
    fn str_to_state(&self, s: &str) -> Result<SagaState, PostgresSagaRepositoryError> {
        match s {
            "PENDING" => Ok(SagaState::Pending),
            "IN_PROGRESS" => Ok(SagaState::InProgress),
            "COMPENSATING" => Ok(SagaState::Compensating),
            "COMPLETED" => Ok(SagaState::Completed),
            "FAILED" => Ok(SagaState::Failed),
            "CANCELLED" => Ok(SagaState::Cancelled),
            _ => Err(PostgresSagaRepositoryError::InfrastructureError {
                message: format!("Invalid saga state: {}", s),
            }),
        }
    }

    /// Convert SagaState to database string
    fn state_to_str(&self, state: SagaState) -> &'static str {
        match state {
            SagaState::Pending => "PENDING",
            SagaState::InProgress => "IN_PROGRESS",
            SagaState::Compensating => "COMPENSATING",
            SagaState::Completed => "COMPLETED",
            SagaState::Failed => "FAILED",
            SagaState::Cancelled => "CANCELLED",
        }
    }

    /// Convert database string to SagaType
    fn str_to_saga_type(&self, s: &str) -> Result<SagaType, PostgresSagaRepositoryError> {
        match s {
            "PROVISIONING" => Ok(SagaType::Provisioning),
            "EXECUTION" => Ok(SagaType::Execution),
            "RECOVERY" => Ok(SagaType::Recovery),
            _ => Err(PostgresSagaRepositoryError::InfrastructureError {
                message: format!("Invalid saga type: {}", s),
            }),
        }
    }

    /// Convert SagaType to database string
    fn saga_type_to_str(&self, saga_type: SagaType) -> &'static str {
        match saga_type {
            SagaType::Provisioning => "PROVISIONING",
            SagaType::Execution => "EXECUTION",
            SagaType::Recovery => "RECOVERY",
        }
    }

    /// Convert row to SagaContext
    fn row_to_context(&self, row: SagaRow) -> Result<SagaContext, PostgresSagaRepositoryError> {
        let saga_id = SagaId(row.id);
        let saga_type = self.str_to_saga_type(&row.saga_type)?;

        // Convert JSON metadata to HashMap
        let metadata: std::collections::HashMap<String, serde_json::Value> = row
            .metadata
            .map(|j| {
                serde_json::from_value(j.0).unwrap_or_else(|_| std::collections::HashMap::new())
            })
            .unwrap_or_default();

        // Determine current_step from completed_at (simplified)
        let current_step = if row.completed_at.is_some() {
            usize::MAX
        } else {
            0
        };
        let is_compensating = row.state == "COMPENSATING";

        Ok(SagaContext::from_persistence(
            saga_id,
            saga_type,
            row.correlation_id,
            row.actor,
            row.started_at,
            current_step,
            is_compensating,
            metadata,
            row.error_message,
        ))
    }

    /// Convert row to SagaStepData
    fn row_to_step(&self, row: SagaStepRow) -> Result<SagaStepData, PostgresSagaRepositoryError> {
        let step_id = SagaStepId(row.id);
        let saga_id = SagaId(row.saga_id);

        let state = match row.state.as_str() {
            "PENDING" => SagaStepState::Pending,
            "IN_PROGRESS" => SagaStepState::InProgress,
            "COMPLETED" => SagaStepState::Completed,
            "FAILED" => SagaStepState::Failed,
            "COMPENSATING" => SagaStepState::Compensating,
            "COMPENSATED" => SagaStepState::Compensated,
            _ => {
                return Err(PostgresSagaRepositoryError::InfrastructureError {
                    message: format!("Invalid step state: {}", row.state),
                });
            }
        };

        Ok(SagaStepData {
            step_id,
            saga_id,
            step_name: row.step_name,
            step_order: row.step_order,
            state,
            input_data: row.input_data.map(|j| j.0),
            output_data: row.output_data.map(|j| j.0),
            compensation_data: row.compensation_data.map(|j| j.0),
            started_at: row.started_at,
            completed_at: row.completed_at,
            error_message: row.error_message,
            retry_count: row.retry_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[async_trait::async_trait]
impl SagaRepository for PostgresSagaRepository {
    type Error = PostgresSagaRepositoryError;

    async fn save(&self, context: &SagaContext) -> Result<(), Self::Error> {
        let saga_type_str = self.saga_type_to_str(context.saga_type);
        let metadata_json = serde_json::to_value(&context.metadata)
            .map_err(PostgresSagaRepositoryError::Serialization)?;

        // Determine state from context
        let state_str = if context.error_message.is_some() {
            "FAILED"
        } else if context.is_compensating {
            "COMPENSATING"
        } else if context.current_step > 0 {
            "IN_PROGRESS"
        } else {
            "PENDING"
        };

        sqlx::query!(
            r#"
            INSERT INTO sagas (id, saga_type, state, correlation_id, actor, metadata, started_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id) DO UPDATE SET
                state = EXCLUDED.state,
                correlation_id = EXCLUDED.correlation_id,
                actor = EXCLUDED.actor,
                metadata = EXCLUDED.metadata,
                updated_at = NOW()
            "#,
            context.saga_id.0,
            saga_type_str,
            state_str,
            context.correlation_id,
            context.actor,
            metadata_json,
            context.started_at,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        Ok(())
    }

    async fn find_by_id(&self, saga_id: &SagaId) -> Result<Option<SagaContext>, Self::Error> {
        let row = sqlx::query_as::<_, SagaRow>(
            r#"
            SELECT id, saga_type, state, correlation_id, actor, started_at,
                   completed_at, error_message, metadata, created_at, updated_at
            FROM sagas
            WHERE id = $1
            "#,
        )
        .bind(saga_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        match row {
            Some(r) => Ok(Some(self.row_to_context(r)?)),
            None => Ok(None),
        }
    }

    async fn find_by_type(&self, saga_type: SagaType) -> Result<Vec<SagaContext>, Self::Error> {
        let saga_type_str = self.saga_type_to_str(saga_type);

        let rows = sqlx::query_as::<_, SagaRow>(
            r#"
            SELECT id, saga_type, state, correlation_id, actor, started_at,
                   completed_at, error_message, metadata, created_at, updated_at
            FROM sagas
            WHERE saga_type = $1
            ORDER BY started_at DESC
            "#,
        )
        .bind(saga_type_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        rows.into_iter().map(|r| self.row_to_context(r)).collect()
    }

    async fn find_by_state(&self, state: SagaState) -> Result<Vec<SagaContext>, Self::Error> {
        let state_str = self.state_to_str(state);

        let rows = sqlx::query_as::<_, SagaRow>(
            r#"
            SELECT id, saga_type, state, correlation_id, actor, started_at,
                   completed_at, error_message, metadata, created_at, updated_at
            FROM sagas
            WHERE state = $1
            ORDER BY started_at DESC
            "#,
        )
        .bind(state_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        rows.into_iter().map(|r| self.row_to_context(r)).collect()
    }

    async fn find_by_correlation_id(
        &self,
        correlation_id: &str,
    ) -> Result<Vec<SagaContext>, Self::Error> {
        let rows = sqlx::query_as::<_, SagaRow>(
            r#"
            SELECT id, saga_type, state, correlation_id, actor, started_at,
                   completed_at, error_message, metadata, created_at, updated_at
            FROM sagas
            WHERE correlation_id = $1
            ORDER BY started_at DESC
            "#,
        )
        .bind(correlation_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        rows.into_iter().map(|r| self.row_to_context(r)).collect()
    }

    async fn update_state(
        &self,
        saga_id: &SagaId,
        state: SagaState,
        error_message: Option<String>,
    ) -> Result<(), Self::Error> {
        let state_str = self.state_to_str(state);
        let completed_at = match state {
            SagaState::Completed | SagaState::Failed | SagaState::Cancelled => {
                Some(chrono::Utc::now())
            }
            _ => None,
        };

        sqlx::query!(
            r#"
            UPDATE sagas
            SET state = $1,
                error_message = $2,
                completed_at = $3,
                updated_at = NOW()
            WHERE id = $4
            "#,
            state_str,
            error_message,
            completed_at,
            saga_id.0,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        Ok(())
    }

    async fn mark_compensating(&self, saga_id: &SagaId) -> Result<(), Self::Error> {
        sqlx::query!(
            r#"
            UPDATE sagas
            SET state = 'COMPENSATING',
                updated_at = NOW()
            WHERE id = $1
            "#,
            saga_id.0,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        Ok(())
    }

    async fn delete(&self, saga_id: &SagaId) -> Result<bool, Self::Error> {
        let result = sqlx::query!("DELETE FROM sagas WHERE id = $1", saga_id.0,)
            .execute(&self.pool)
            .await
            .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        Ok(result.rows_affected() > 0)
    }

    async fn save_step(&self, step: &SagaStepData) -> Result<(), Self::Error> {
        let state_str = match step.state {
            SagaStepState::Pending => "PENDING",
            SagaStepState::InProgress => "IN_PROGRESS",
            SagaStepState::Completed => "COMPLETED",
            SagaStepState::Failed => "FAILED",
            SagaStepState::Compensating => "COMPENSATING",
            SagaStepState::Compensated => "COMPENSATED",
        };

        let input_data_json = step
            .input_data
            .as_ref()
            .map(|v| serde_json::to_value(v))
            .transpose()
            .map_err(PostgresSagaRepositoryError::Serialization)?;

        let output_data_json = step
            .output_data
            .as_ref()
            .map(|v| serde_json::to_value(v))
            .transpose()
            .map_err(PostgresSagaRepositoryError::Serialization)?;

        let compensation_data_json = step
            .compensation_data
            .as_ref()
            .map(|v| serde_json::to_value(v))
            .transpose()
            .map_err(PostgresSagaRepositoryError::Serialization)?;

        sqlx::query!(
            r#"
            INSERT INTO saga_steps (id, saga_id, step_name, step_order, state, input_data, output_data, compensation_data, started_at, completed_at, error_message, retry_count)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (id) DO UPDATE SET
                state = EXCLUDED.state,
                output_data = EXCLUDED.output_data,
                completed_at = EXCLUDED.completed_at,
                error_message = EXCLUDED.error_message,
                retry_count = EXCLUDED.retry_count,
                updated_at = NOW()
            "#,
            step.step_id.0,
            step.saga_id.0,
            step.step_name,
            step.step_order,
            state_str,
            input_data_json,
            output_data_json,
            compensation_data_json,
            step.started_at,
            step.completed_at,
            step.error_message,
            step.retry_count,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        Ok(())
    }

    async fn find_step_by_id(
        &self,
        step_id: &SagaStepId,
    ) -> Result<Option<SagaStepData>, Self::Error> {
        let row = sqlx::query_as::<_, SagaStepRow>(
            r#"
            SELECT id, saga_id, step_name, step_order, state, input_data, output_data,
                   compensation_data, started_at, completed_at, error_message, retry_count,
                   created_at, updated_at
            FROM saga_steps
            WHERE id = $1
            "#,
        )
        .bind(step_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        match row {
            Some(r) => Ok(Some(self.row_to_step(r)?)),
            None => Ok(None),
        }
    }

    async fn find_steps_by_saga_id(
        &self,
        saga_id: &SagaId,
    ) -> Result<Vec<SagaStepData>, Self::Error> {
        let rows = sqlx::query_as::<_, SagaStepRow>(
            r#"
            SELECT id, saga_id, step_name, step_order, state, input_data, output_data,
                   compensation_data, started_at, completed_at, error_message, retry_count,
                   created_at, updated_at
            FROM saga_steps
            WHERE saga_id = $1
            ORDER BY step_order ASC
            "#,
        )
        .bind(saga_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        rows.into_iter().map(|r| self.row_to_step(r)).collect()
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

        let completed_at = match state {
            SagaStepState::Completed | SagaStepState::Failed | SagaStepState::Compensated => {
                Some(chrono::Utc::now())
            }
            _ => None,
        };

        sqlx::query!(
            r#"
            UPDATE saga_steps
            SET state = $1,
                output_data = $2,
                completed_at = $3,
                updated_at = NOW()
            WHERE id = $4
            "#,
            state_str,
            output,
            completed_at,
            step_id.0,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        Ok(())
    }

    async fn update_step_compensation(
        &self,
        step_id: &SagaStepId,
        compensation_data: serde_json::Value,
    ) -> Result<(), Self::Error> {
        sqlx::query!(
            r#"
            UPDATE saga_steps
            SET compensation_data = $1,
                updated_at = NOW()
            WHERE id = $2
            "#,
            compensation_data,
            step_id.0,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        Ok(())
    }

    async fn count_active(&self) -> Result<u64, Self::Error> {
        let result = sqlx::query!(
            "SELECT COUNT(*) as count FROM sagas WHERE state IN ('PENDING', 'IN_PROGRESS')",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        Ok(result.count.unwrap_or(0) as u64)
    }

    async fn count_by_type_and_state(
        &self,
        saga_type: SagaType,
        state: SagaState,
    ) -> Result<u64, Self::Error> {
        let saga_type_str = self.saga_type_to_str(saga_type);
        let state_str = self.state_to_str(state);

        #[derive(sqlx::FromRow)]
        struct CountRow {
            count: Option<i64>,
        }

        let result = sqlx::query_as::<_, CountRow>(
            "SELECT COUNT(*) as count FROM sagas WHERE saga_type = $1 AND state = $2",
        )
        .bind(saga_type_str)
        .bind(state_str)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        Ok(result.count.unwrap_or(0) as u64)
    }

    async fn avg_duration(&self) -> Result<Option<std::time::Duration>, Self::Error> {
        // Use text to avoid bigdecimal feature requirement
        let result: Option<(f64,)> = sqlx::query_as(
            r#"
            SELECT AVG(EXTRACT(EPOCH FROM (completed_at - started_at))) as avg_seconds
            FROM sagas
            WHERE state = 'COMPLETED' AND completed_at IS NOT NULL
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .ok();

        Ok(result.and_then(|(s,)| Some(std::time::Duration::from_secs_f64(s))))
    }

    async fn cleanup_completed(&self, older_than: std::time::Duration) -> Result<u64, Self::Error> {
        let cutoff = chrono::Utc::now()
            - chrono::Duration::from_std(older_than).map_err(|e| {
                PostgresSagaRepositoryError::InfrastructureError {
                    message: format!("Invalid duration: {}", e),
                }
            })?;

        // Convert to NaiveDateTime for sqlx compatibility
        let cutoff_naive = chrono::NaiveDateTime::from_timestamp_opt(
            cutoff.timestamp(),
            cutoff.timestamp_subsec_nanos(),
        )
        .ok_or_else(|| PostgresSagaRepositoryError::InfrastructureError {
            message: "Invalid cutoff timestamp".to_string(),
        })?;

        // Use query_as without ! macro to avoid DateTime type issues
        #[derive(sqlx::FromRow)]
        struct DeleteResult {
            rows_affected: i64,
        }

        let result: DeleteResult = sqlx::query_as(
            r#"
            DELETE FROM sagas
            WHERE state IN ('COMPLETED', 'FAILED', 'CANCELLED')
            AND completed_at IS NOT NULL
            AND completed_at < $1
            "#,
        )
        .bind(cutoff_naive)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| PostgresSagaRepositoryError::Database(e))?;

        Ok(result.rows_affected as u64)
    }
}

// Unit tests
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn setup_test_db() -> PgPool {
        let base_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:postgres@localhost:5432/hodei_jobs".to_string()
        });

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&base_url)
            .await
            .expect("Failed to connect to database");

        pool
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_save_and_find_saga() {
        let pool = setup_test_db().await;
        let repo = PostgresSagaRepository::new(pool);

        let context = SagaContext::new(
            SagaId::new(),
            SagaType::Provisioning,
            Some("corr-123".to_string()),
            Some("job-dispatcher".to_string()),
        );

        repo.save(&context).await.unwrap();
        let found = repo.find_by_id(&context.saga_id).await.unwrap();

        assert!(found.is_some());
        assert_eq!(found.unwrap().saga_type, SagaType::Provisioning);
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_find_by_type() {
        let pool = setup_test_db().await;
        let repo = PostgresSagaRepository::new(pool);

        let saga1 = SagaContext::new(SagaId::new(), SagaType::Provisioning, None, None);
        let saga2 = SagaContext::new(SagaId::new(), SagaType::Execution, None, None);

        repo.save(&saga1).await.unwrap();
        repo.save(&saga2).await.unwrap();

        let provisioning = repo.find_by_type(SagaType::Provisioning).await.unwrap();
        assert_eq!(provisioning.len(), 1);
        assert_eq!(provisioning[0].saga_type, SagaType::Provisioning);
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_save_and_find_steps() {
        let pool = setup_test_db().await;
        let repo = PostgresSagaRepository::new(pool);

        let saga_id = SagaId::new();
        let context = SagaContext::new(saga_id.clone(), SagaType::Provisioning, None, None);
        repo.save(&context).await.unwrap();

        let step = SagaStepData::new(
            saga_id.clone(),
            "CreateInfrastructure".to_string(),
            1,
            Some(serde_json::json!({"resource_id": "pod-123"})),
        );

        repo.save_step(&step).await.unwrap();
        let found = repo.find_step_by_id(&step.step_id).await.unwrap().unwrap();

        assert_eq!(found.step_name, "CreateInfrastructure");
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL"]
    async fn test_count_active() {
        let pool = setup_test_db().await;
        let repo = PostgresSagaRepository::new(pool);

        let saga = SagaContext::new(SagaId::new(), SagaType::Provisioning, None, None);
        repo.save(&saga).await.unwrap();

        let count = repo.count_active().await.unwrap();
        assert!(count > 0);
    }
}

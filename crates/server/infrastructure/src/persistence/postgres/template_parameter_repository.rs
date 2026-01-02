//! PostgreSQL JobTemplateParameter Repository
//!
//! Persistent repository implementation for JobTemplateParameters based on PostgreSQL

use async_trait::async_trait;
use hodei_server_domain::jobs::{
    JobTemplateId, JobTemplateParameter, JobTemplateParameterRepository,
};
use hodei_server_domain::shared_kernel::{DomainError, Result};
use sqlx::postgres::PgPool;
use sqlx::{FromRow, Row};
use uuid::Uuid;

/// PostgreSQL JobTemplateParameter Repository
#[derive(Clone)]
pub struct PostgresJobTemplateParameterRepository {
    pool: PgPool,
}

impl PostgresJobTemplateParameterRepository {
    /// Create new repository with existing pool
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create repository connecting to database
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(10)
            .connect(url)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to connect to database: {}", e),
            })?;

        Ok(Self { pool })
    }
}

#[async_trait]
impl JobTemplateParameterRepository for PostgresJobTemplateParameterRepository {
    async fn save(&self, parameter: &JobTemplateParameter) -> Result<()> {
        let choices_json = serde_json::to_value(&parameter.choices).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to serialize choices: {}", e),
            }
        })?;

        sqlx::query(
            r#"
            INSERT INTO job_template_parameters
                (id, template_id, name, parameter_type, description, required,
                 default_value, validation_pattern, min_value, max_value,
                 choices, display_order, secret, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                parameter_type = EXCLUDED.parameter_type,
                description = EXCLUDED.description,
                required = EXCLUDED.required,
                default_value = EXCLUDED.default_value,
                validation_pattern = EXCLUDED.validation_pattern,
                min_value = EXCLUDED.min_value,
                max_value = EXCLUDED.max_value,
                choices = EXCLUDED.choices,
                display_order = EXCLUDED.display_order,
                secret = EXCLUDED.secret,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(parameter.id)
        .bind(parameter.template_id.0)
        .bind(&parameter.name)
        .bind(parameter.parameter_type.to_string())
        .bind(&parameter.description)
        .bind(parameter.required)
        .bind(parameter.default_value.as_ref())
        .bind(parameter.validation_pattern.as_ref())
        .bind(parameter.min_value)
        .bind(parameter.max_value)
        .bind(choices_json)
        .bind(parameter.display_order as i32)
        .bind(parameter.secret)
        .bind(parameter.created_at)
        .bind(parameter.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to save template parameter: {}", e),
        })?;

        Ok(())
    }

    async fn update(&self, parameter: &JobTemplateParameter) -> Result<()> {
        // Same as save since we use ON CONFLICT
        self.save(parameter).await
    }

    async fn find_by_id(&self, id: &Uuid) -> Result<Option<JobTemplateParameter>> {
        let row = sqlx::query(
            r#"
            SELECT id, template_id, name, parameter_type, description, required,
                   default_value, validation_pattern, min_value, max_value,
                   choices, display_order, secret, created_at, updated_at
            FROM job_template_parameters
            WHERE id = $1
            "#,
        )
        .bind(*id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find template parameter: {}", e),
        })?;

        match row {
            Some(row) => Ok(Some(Self::row_to_parameter(row)?)),
            None => Ok(None),
        }
    }

    async fn find_by_template_id(
        &self,
        template_id: &JobTemplateId,
    ) -> Result<Vec<JobTemplateParameter>> {
        let rows = sqlx::query(
            r#"
            SELECT id, template_id, name, parameter_type, description, required,
                   default_value, validation_pattern, min_value, max_value,
                   choices, display_order, secret, created_at, updated_at
            FROM job_template_parameters
            WHERE template_id = $1
            ORDER BY display_order ASC
            "#,
        )
        .bind(template_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find template parameters: {}", e),
        })?;

        rows.into_iter()
            .map(|row| Self::row_to_parameter(row))
            .collect()
    }

    async fn delete(&self, id: &Uuid) -> Result<()> {
        sqlx::query("DELETE FROM job_template_parameters WHERE id = $1")
            .bind(*id)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to delete template parameter: {}", e),
            })?;

        Ok(())
    }

    async fn delete_by_template_id(&self, template_id: &JobTemplateId) -> Result<u64> {
        let result = sqlx::query("DELETE FROM job_template_parameters WHERE template_id = $1")
            .bind(template_id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to delete template parameters: {}", e),
            })?;

        Ok(result.rows_affected() as u64)
    }
}

impl PostgresJobTemplateParameterRepository {
    /// Convert a database row to JobTemplateParameter
    fn row_to_parameter(row: sqlx::postgres::PgRow) -> Result<JobTemplateParameter> {
        let id: Uuid = row.try_get("id")?;
        let template_id_uuid: Uuid = row.try_get("template_id")?;
        let name: String = row.try_get("name")?;
        let parameter_type: String = row.try_get("parameter_type")?;
        let description: String = row.try_get("description")?;
        let required: bool = row.try_get("required")?;
        let default_value: Option<String> = row.try_get("default_value")?;
        let validation_pattern: Option<String> = row.try_get("validation_pattern")?;
        let min_value: Option<f64> = row.try_get("min_value")?;
        let max_value: Option<f64> = row.try_get("max_value")?;
        let choices_json: serde_json::Value = row.try_get("choices")?;
        let display_order: i32 = row.try_get("display_order")?;
        let secret: bool = row.try_get("secret")?;
        let created_at: chrono::DateTime<Utc> = row.try_get("created_at")?;
        let updated_at: chrono::DateTime<Utc> = row.try_get("updated_at")?;

        // Deserialize choices
        let choices: Vec<String> =
            serde_json::from_value(choices_json).expect("Failed to deserialize choices");

        // Parse parameter type
        let parameter_type = match parameter_type.as_str() {
            "String" => hodei_server_domain::jobs::ParameterType::String,
            "Number" => hodei_server_domain::jobs::ParameterType::Number,
            "Boolean" => hodei_server_domain::jobs::ParameterType::Boolean,
            "Choice" => hodei_server_domain::jobs::ParameterType::Choice,
            "Secret" => hodei_server_domain::jobs::ParameterType::Secret,
            _ => hodei_server_domain::jobs::ParameterType::String,
        };

        Ok(JobTemplateParameter {
            id,
            template_id: JobTemplateId(template_id_uuid),
            name,
            parameter_type,
            description,
            required,
            default_value,
            validation_pattern,
            min_value,
            max_value,
            choices,
            display_order: display_order as u32,
            secret,
            created_at,
            updated_at,
        })
    }
}

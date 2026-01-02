//! PostgreSQL JobTemplate Repository
//!
//! Persistent repository implementation for JobTemplates based on PostgreSQL

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hodei_server_domain::jobs::{JobSpec, JobTemplate, JobTemplateId, JobTemplateRepository};
use hodei_server_domain::shared_kernel::{DomainError, Result};
use sqlx::postgres::PgPool;
use sqlx::{FromRow, Row};
use std::collections::HashMap;
use uuid::Uuid;

/// PostgreSQL JobTemplate Repository
#[derive(Clone)]
pub struct PostgresJobTemplateRepository {
    pool: PgPool,
}

impl PostgresJobTemplateRepository {
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
impl JobTemplateRepository for PostgresJobTemplateRepository {
    async fn save(&self, template: &JobTemplate) -> Result<()> {
        let spec_json =
            serde_json::to_value(&template.spec).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to serialize job spec: {}", e),
            })?;

        let labels_json = serde_json::to_value(&template.labels).map_err(|e| {
            DomainError::InfrastructureError {
                message: format!("Failed to serialize labels: {}", e),
            }
        })?;

        sqlx::query(
            r#"
            INSERT INTO job_templates
                (id, name, description, spec, status, version, labels,
                 created_at, updated_at, created_by, run_count, success_count, failure_count)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                spec = EXCLUDED.spec,
                status = EXCLUDED.status,
                version = EXCLUDED.version,
                labels = EXCLUDED.labels,
                updated_at = EXCLUDED.updated_at,
                run_count = EXCLUDED.run_count,
                success_count = EXCLUDED.success_count,
                failure_count = EXCLUDED.failure_count
            "#,
        )
        .bind(template.id.0)
        .bind(&template.name)
        .bind(template.description.as_ref())
        .bind(spec_json)
        .bind(template.status.to_string())
        .bind(template.version as i32)
        .bind(labels_json)
        .bind(template.created_at)
        .bind(template.updated_at)
        .bind(template.created_by.as_ref())
        .bind(template.run_count as i64)
        .bind(template.success_count as i64)
        .bind(template.failure_count as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to save job template: {}", e),
        })?;

        Ok(())
    }

    async fn update(&self, template: &JobTemplate) -> Result<()> {
        // Same as save since we use ON CONFLICT
        self.save(template).await
    }

    async fn find_by_id(&self, id: &JobTemplateId) -> Result<Option<JobTemplate>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, description, spec, status, version, labels,
                   created_at, updated_at, created_by, run_count, success_count, failure_count
            FROM job_templates
            WHERE id = $1
            "#,
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find job template: {}", e),
        })?;

        match row {
            Some(row) => Ok(Some(Self::row_to_template(row)?)),
            None => Ok(None),
        }
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<JobTemplate>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, description, spec, status, version, labels,
                   created_at, updated_at, created_by, run_count, success_count, failure_count
            FROM job_templates
            WHERE name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find job template by name: {}", e),
        })?;

        match row {
            Some(row) => Ok(Some(Self::row_to_template(row)?)),
            None => Ok(None),
        }
    }

    async fn list_active(&self) -> Result<Vec<JobTemplate>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, spec, status, version, labels,
                   created_at, updated_at, created_by, run_count, success_count, failure_count
            FROM job_templates
            WHERE status != 'Archived'
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to list active job templates: {}", e),
        })?;

        rows.into_iter()
            .map(|row| Self::row_to_template(row))
            .collect()
    }

    async fn find_by_label(&self, key: &str, value: &str) -> Result<Vec<JobTemplate>> {
        // Use JSONB containment query for labels
        let rows = sqlx::query(
            r#"
            SELECT id, name, description, spec, status, version, labels,
                   created_at, updated_at, created_by, run_count, success_count, failure_count
            FROM job_templates
            WHERE labels->>$1 = $2 AND status != 'Archived'
            ORDER BY created_at DESC
            "#,
        )
        .bind(key)
        .bind(value)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InfrastructureError {
            message: format!("Failed to find job templates by label: {}", e),
        })?;

        rows.into_iter()
            .map(|row| Self::row_to_template(row))
            .collect()
    }

    async fn delete(&self, id: &JobTemplateId) -> Result<()> {
        sqlx::query("DELETE FROM job_templates WHERE id = $1")
            .bind(id.0)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to delete job template: {}", e),
            })?;

        Ok(())
    }
}

impl PostgresJobTemplateRepository {
    /// Convert a database row to JobTemplate
    fn row_to_template(row: sqlx::postgres::PgRow) -> Result<JobTemplate> {
        let id_uuid: Uuid = row.try_get("id")?;
        let name: String = row.try_get("name")?;
        let description: Option<String> = row.try_get("description")?;
        let spec_json: serde_json::Value = row.try_get("spec")?;
        let status: String = row.try_get("status")?;
        let version: i32 = row.try_get("version")?;
        let labels_json: serde_json::Value = row.try_get("labels")?;
        let created_at: DateTime<Utc> = row.try_get("created_at")?;
        let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
        let created_by: Option<String> = row.try_get("created_by")?;
        let run_count: i64 = row.try_get("run_count")?;
        let success_count: i64 = row.try_get("success_count")?;
        let failure_count: i64 = row.try_get("failure_count")?;

        // Deserialize spec
        let spec: JobSpec =
            serde_json::from_value(spec_json).map_err(|e| DomainError::InfrastructureError {
                message: format!("Failed to deserialize job spec: {}", e),
            })?;

        // Deserialize labels
        let labels: HashMap<String, String> =
            serde_json::from_value(labels_json).expect("Failed to deserialize labels");

        // Parse status
        let status = match status.as_str() {
            "Active" => hodei_server_domain::jobs::JobTemplateStatus::Active,
            "Disabled" => hodei_server_domain::jobs::JobTemplateStatus::Disabled,
            "Archived" => hodei_server_domain::jobs::JobTemplateStatus::Archived,
            _ => hodei_server_domain::jobs::JobTemplateStatus::Active,
        };

        Ok(JobTemplate {
            id: JobTemplateId(id_uuid),
            name,
            description,
            spec,
            status,
            version: version as u32,
            labels,
            created_at,
            updated_at,
            created_by,
            run_count: run_count as u64,
            success_count: success_count as u64,
            failure_count: failure_count as u64,
        })
    }
}

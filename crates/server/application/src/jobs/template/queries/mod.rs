//! Template Queries for CQRS Read Path
//!
//! This module defines all queries for reading template data.
//! Queries are executed via QueryBus and return read-optimized results.

use crate::core::query::{PaginatedResult, Pagination};
use hodei_server_domain::jobs::templates::{
    JobExecution, JobTemplate, JobTemplateId, ScheduledJob,
};
use hodei_server_domain::shared_kernel::{JobId, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Query to get a single template by ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTemplateQuery {
    pub template_id: JobTemplateId,
}

impl crate::core::query::Query for GetTemplateQuery {
    const NAME: &'static str = "GetTemplate";
    type Result = Option<TemplateSummary>;
}

/// Query to list templates with filters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTemplatesQuery {
    /// Filter by status (Active, Disabled, Archived)
    pub status: Option<String>,
    /// Filter by label selector (key=value)
    pub label_selector: Option<HashMap<String, String>>,
    /// Pagination parameters
    pub pagination: Option<Pagination>,
}

impl crate::core::query::Query for ListTemplatesQuery {
    const NAME: &'static str = "ListTemplates";
    type Result = PaginatedResult<TemplateSummary>;
}

impl crate::core::query::ValidatableQuery for ListTemplatesQuery {
    fn validate(&self) -> Result<()> {
        if let Some(pagination) = &self.pagination {
            pagination.validate()?;
        }
        Ok(())
    }
}

/// Query to get a template by name
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTemplateByNameQuery {
    pub name: String,
}

impl crate::core::query::Query for GetTemplateByNameQuery {
    const NAME: &'static str = "GetTemplateByName";
    type Result = Option<TemplateSummary>;
}

/// Query to get an execution by ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetExecutionQuery {
    pub execution_id: Uuid,
}

impl crate::core::query::Query for GetExecutionQuery {
    const NAME: &'static str = "GetExecution";
    type Result = Option<ExecutionSummary>;
}

/// Query to list executions for a template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListExecutionsQuery {
    pub template_id: JobTemplateId,
    /// Filter by state
    pub state: Option<String>,
    /// Filter by date range
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Pagination
    pub pagination: Option<Pagination>,
}

impl crate::core::query::Query for ListExecutionsQuery {
    const NAME: &'static str = "ListExecutions";
    type Result = PaginatedResult<ExecutionSummary>;
}

impl crate::core::query::ValidatableQuery for ListExecutionsQuery {
    fn validate(&self) -> Result<()> {
        if let Some(pagination) = &self.pagination {
            pagination.validate()?;
        }
        Ok(())
    }
}

/// Query to get executions by job ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetExecutionsByJobQuery {
    pub job_id: JobId,
}

impl crate::core::query::Query for GetExecutionsByJobQuery {
    const NAME: &'static str = "GetExecutionsByJob";
    type Result = Vec<ExecutionSummary>;
}

/// Summary representation of a template (for read models)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSummary {
    pub id: JobTemplateId,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub version: u32,
    pub run_count: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub success_rate: f64,
    pub labels: HashMap<String, String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub created_by: Option<String>,
}

impl From<JobTemplate> for TemplateSummary {
    fn from(template: JobTemplate) -> Self {
        let success_rate = template.success_rate();
        let labels = template.labels.clone();
        let description = template.description.clone();
        let name = template.name.clone();
        let created_by = template.created_by.clone();
        let status = template.status.to_string();
        let id = template.id;
        let created_at = template.created_at;
        let updated_at = template.updated_at;
        Self {
            id,
            name,
            description,
            status,
            version: template.version,
            run_count: template.run_count,
            success_count: template.success_count,
            failure_count: template.failure_count,
            success_rate,
            labels,
            created_at,
            updated_at,
            created_by,
        }
    }
}

/// Summary of an execution (for read models)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    pub id: Uuid,
    pub execution_number: u64,
    pub template_id: JobTemplateId,
    pub template_version: u32,
    pub job_id: Option<JobId>,
    pub job_name: String,
    pub state: String,
    pub triggered_by: String,
    pub triggered_by_user: Option<String>,
    pub parameters: HashMap<String, String>,
    pub queued_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub duration_ms: Option<u64>,
}

impl From<JobExecution> for ExecutionSummary {
    fn from(execution: JobExecution) -> Self {
        let duration_ms = execution.duration_ms();
        let parameters = execution.parameters.clone();
        let triggered_by_user = execution.triggered_by_user.clone();
        let job_id = execution.job_id;
        let job_name = execution.job_name.clone();
        let state = execution.state.to_string();
        let triggered_by = execution.triggered_by.to_string();
        let id = execution.id;
        let template_id = execution.template_id;
        let queued_at = execution.queued_at;
        Self {
            id,
            execution_number: execution.execution_number,
            template_id,
            template_version: execution.template_version,
            job_id,
            job_name,
            state,
            triggered_by,
            triggered_by_user,
            parameters,
            queued_at,
            started_at: execution.started_at,
            completed_at: execution.completed_at,
            duration_ms,
        }
    }
}

/// Query to get a scheduled job by ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetScheduledJobQuery {
    pub scheduled_job_id: Uuid,
}

impl crate::core::query::Query for GetScheduledJobQuery {
    const NAME: &'static str = "GetScheduledJob";
    type Result = Option<ScheduledJobSummary>;
}

/// Query to list scheduled jobs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListScheduledJobsQuery {
    pub template_id: Option<JobTemplateId>,
    pub enabled: Option<bool>,
    pub pagination: Option<Pagination>,
}

impl crate::core::query::Query for ListScheduledJobsQuery {
    const NAME: &'static str = "ListScheduledJobs";
    type Result = PaginatedResult<ScheduledJobSummary>;
}

/// Summary of a scheduled job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJobSummary {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub template_id: JobTemplateId,
    pub cron_expression: String,
    pub timezone: String,
    pub next_execution_at: chrono::DateTime<chrono::Utc>,
    pub last_execution_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_execution_status: Option<String>,
    pub enabled: bool,
    pub consecutive_failures: u32,
    pub max_consecutive_failures: u32,
    pub parameters: HashMap<String, String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub created_by: Option<String>,
}

impl From<ScheduledJob> for ScheduledJobSummary {
    fn from(scheduled: ScheduledJob) -> Self {
        Self {
            id: scheduled.id,
            name: scheduled.name,
            description: scheduled.description,
            template_id: scheduled.template_id,
            cron_expression: scheduled.cron_expression,
            timezone: scheduled.timezone,
            next_execution_at: scheduled.next_execution_at,
            last_execution_at: scheduled.last_execution_at,
            last_execution_status: scheduled.last_execution_status.map(|s| s.to_string()),
            enabled: scheduled.enabled,
            consecutive_failures: scheduled.consecutive_failures,
            max_consecutive_failures: scheduled.max_consecutive_failures,
            parameters: scheduled.parameters,
            created_at: scheduled.created_at,
            created_by: scheduled.created_by,
        }
    }
}

/// Query to validate a cron expression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateCronQuery {
    pub cron_expression: String,
    pub timezone: Option<String>,
}

impl crate::core::query::Query for ValidateCronQuery {
    const NAME: &'static str = "ValidateCron";
    type Result = CronValidationResult;
}

/// Result of cron validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronValidationResult {
    pub valid: bool,
    pub error_message: Option<String>,
    pub next_execution: Option<chrono::DateTime<chrono::Utc>>,
    pub next_10_executions: Vec<chrono::DateTime<chrono::Utc>>,
}

/// Query to get upcoming scheduled executions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUpcomingExecutionsQuery {
    pub from_time: chrono::DateTime<chrono::Utc>,
    pub to_time: chrono::DateTime<chrono::Utc>,
    pub template_id: Option<JobTemplateId>,
    pub limit: u32,
}

impl crate::core::query::Query for GetUpcomingExecutionsQuery {
    const NAME: &'static str = "GetUpcomingExecutions";
    type Result = Vec<UpcomingExecutionSummary>;
}

/// Summary of an upcoming execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpcomingExecutionSummary {
    pub scheduled_job_id: Uuid,
    pub scheduled_job_name: String,
    pub template_id: JobTemplateId,
    pub scheduled_for: chrono::DateTime<chrono::Utc>,
    pub parameters: HashMap<String, String>,
}

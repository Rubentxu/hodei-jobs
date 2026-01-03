//! Query Handlers for Template Management
//!
//! Implements the read-side logic for template queries.
//! These handlers are invoked by the QueryBus and return read-optimized results.

use crate::core::query::{PaginatedResult, QueryHandler};
use crate::jobs::template::queries::{
    CronValidationResult, ExecutionSummary, GetExecutionQuery, GetExecutionsByJobQuery,
    GetScheduledJobQuery, GetTemplateByNameQuery, GetTemplateQuery, GetUpcomingExecutionsQuery,
    ListExecutionsQuery, ListScheduledJobsQuery, ListTemplatesQuery, ScheduledJobSummary,
    TemplateSummary, UpcomingExecutionSummary, ValidateCronQuery,
};
use crate::jobs::template::read_models::{ExecutionReadModelPort, TemplateReadModelPort};
use hodei_server_domain::jobs::aggregate::JobSpec;
use hodei_server_domain::jobs::templates::{
    JobExecutionRepository, JobTemplateRepository, ScheduledJobRepository,
};
use hodei_server_domain::shared_kernel::{JobId, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, instrument};
use uuid::Uuid;

/// Handler for GetTemplateQuery
#[derive(Clone)]
pub struct GetTemplateHandler {
    read_model: Arc<dyn TemplateReadModelPort>,
}

impl GetTemplateHandler {
    pub fn new(read_model: Arc<dyn TemplateReadModelPort>) -> Self {
        Self { read_model }
    }
}

#[async_trait::async_trait]
impl QueryHandler<GetTemplateQuery> for GetTemplateHandler {
    #[instrument(skip(self, query))]
    async fn handle(&self, query: GetTemplateQuery) -> Result<Option<TemplateSummary>> {
        debug!("Getting template: {}", query.template_id);
        let result = self.read_model.get_by_id(&query.template_id).await;
        Ok(result)
    }
}

/// Handler for GetTemplateByNameQuery
#[derive(Clone)]
pub struct GetTemplateByNameHandler {
    read_model: Arc<dyn TemplateReadModelPort>,
}

impl GetTemplateByNameHandler {
    pub fn new(read_model: Arc<dyn TemplateReadModelPort>) -> Self {
        Self { read_model }
    }
}

#[async_trait::async_trait]
impl QueryHandler<GetTemplateByNameQuery> for GetTemplateByNameHandler {
    async fn handle(&self, query: GetTemplateByNameQuery) -> Result<Option<TemplateSummary>> {
        debug!("Getting template by name: {}", query.name);
        let result = self.read_model.get_by_name(&query.name).await;
        Ok(result)
    }
}

/// Handler for ListTemplatesQuery
#[derive(Clone)]
pub struct ListTemplatesHandler {
    read_model: Arc<dyn TemplateReadModelPort>,
}

impl ListTemplatesHandler {
    pub fn new(read_model: Arc<dyn TemplateReadModelPort>) -> Self {
        Self { read_model }
    }
}

#[async_trait::async_trait]
impl QueryHandler<ListTemplatesQuery> for ListTemplatesHandler {
    async fn handle(&self, query: ListTemplatesQuery) -> Result<PaginatedResult<TemplateSummary>> {
        let limit = query.pagination.as_ref().map(|p| p.limit).unwrap_or(50) as usize;
        let offset = query.pagination.as_ref().map(|p| p.offset).unwrap_or(0) as usize;

        let results = self
            .read_model
            .list(query.status.as_deref(), limit + offset, 0)
            .await;

        // Apply offset and limit
        let total_count = results.len();
        let items = results.into_iter().skip(offset).take(limit).collect();

        Ok(PaginatedResult::new(
            items,
            total_count,
            offset as u32,
            limit as u32,
        ))
    }
}

/// Handler for GetExecutionQuery
#[derive(Clone)]
pub struct GetExecutionHandler {
    read_model: Arc<dyn ExecutionReadModelPort>,
}

impl GetExecutionHandler {
    pub fn new(read_model: Arc<dyn ExecutionReadModelPort>) -> Self {
        Self { read_model }
    }
}

#[async_trait::async_trait]
impl QueryHandler<GetExecutionQuery> for GetExecutionHandler {
    async fn handle(&self, query: GetExecutionQuery) -> Result<Option<ExecutionSummary>> {
        debug!("Getting execution: {}", query.execution_id);
        let result = self.read_model.get_by_id(&query.execution_id).await;
        Ok(result)
    }
}

/// Handler for ListExecutionsQuery
#[derive(Clone)]
pub struct ListExecutionsHandler {
    read_model: Arc<dyn ExecutionReadModelPort>,
}

impl ListExecutionsHandler {
    pub fn new(read_model: Arc<dyn ExecutionReadModelPort>) -> Self {
        Self { read_model }
    }
}

#[async_trait::async_trait]
impl QueryHandler<ListExecutionsQuery> for ListExecutionsHandler {
    async fn handle(
        &self,
        query: ListExecutionsQuery,
    ) -> Result<PaginatedResult<ExecutionSummary>> {
        let limit = query.pagination.as_ref().map(|p| p.limit).unwrap_or(50) as usize;
        let offset = query.pagination.as_ref().map(|p| p.offset).unwrap_or(0) as usize;

        let results = self
            .read_model
            .list_by_template(
                &query.template_id,
                query.state.as_deref(),
                limit + offset,
                0,
            )
            .await;

        let total_count = results.len();
        let items = results.into_iter().skip(offset).take(limit).collect();

        Ok(PaginatedResult::new(
            items,
            total_count,
            offset as u32,
            limit as u32,
        ))
    }
}

/// Handler for GetExecutionsByJobQuery
#[derive(Clone)]
pub struct GetExecutionsByJobHandler {
    read_model: Arc<dyn ExecutionReadModelPort>,
}

impl GetExecutionsByJobHandler {
    pub fn new(read_model: Arc<dyn ExecutionReadModelPort>) -> Self {
        Self { read_model }
    }
}

#[async_trait::async_trait]
impl QueryHandler<GetExecutionsByJobQuery> for GetExecutionsByJobHandler {
    async fn handle(&self, query: GetExecutionsByJobQuery) -> Result<Vec<ExecutionSummary>> {
        debug!("Getting executions for job: {}", query.job_id);
        // This is a simplified implementation
        // In production, we'd query by job_id directly
        // Return empty for now
        Ok(Vec::new())
    }
}

/// Handler for GetScheduledJobQuery
#[derive(Clone)]
pub struct GetScheduledJobHandler {
    scheduled_repo: Arc<dyn ScheduledJobRepository>,
}

impl GetScheduledJobHandler {
    pub fn new(scheduled_repo: Arc<dyn ScheduledJobRepository>) -> Self {
        Self { scheduled_repo }
    }
}

#[async_trait::async_trait]
impl QueryHandler<GetScheduledJobQuery> for GetScheduledJobHandler {
    async fn handle(&self, query: GetScheduledJobQuery) -> Result<Option<ScheduledJobSummary>> {
        debug!("Getting scheduled job: {}", query.scheduled_job_id);
        let scheduled = self
            .scheduled_repo
            .find_by_id(&query.scheduled_job_id)
            .await?;
        Ok(scheduled.map(|s| s.into()))
    }
}

/// Handler for ListScheduledJobsQuery
#[derive(Clone)]
pub struct ListScheduledJobsHandler {
    scheduled_repo: Arc<dyn ScheduledJobRepository>,
}

impl ListScheduledJobsHandler {
    pub fn new(scheduled_repo: Arc<dyn ScheduledJobRepository>) -> Self {
        Self { scheduled_repo }
    }
}

#[async_trait::async_trait]
impl QueryHandler<ListScheduledJobsQuery> for ListScheduledJobsHandler {
    async fn handle(
        &self,
        query: ListScheduledJobsQuery,
    ) -> Result<PaginatedResult<ScheduledJobSummary>> {
        let limit = query.pagination.as_ref().map(|p| p.limit).unwrap_or(50) as usize;
        let offset = query.pagination.as_ref().map(|p| p.offset).unwrap_or(0) as usize;

        // TODO: Implement when persistence layer is ready
        // let mut scheduled_jobs = self.scheduled_repo.find_all().await?;
        let scheduled_jobs: Vec<hodei_server_domain::jobs::templates::ScheduledJob> = Vec::new();

        // Apply filters
        if let Some(template_id) = query.template_id {
            // scheduled_jobs.retain(|s| s.template_id == template_id);
        }
        if let Some(_enabled) = query.enabled {
            // scheduled_jobs.retain(|s| s.enabled == enabled);
        }

        // Convert to summaries
        let summaries: Vec<ScheduledJobSummary> =
            scheduled_jobs.into_iter().map(|s| s.into()).collect();
        let total_count = summaries.len();
        let items = summaries.into_iter().skip(offset).take(limit).collect();

        Ok(PaginatedResult::new(
            items,
            total_count,
            offset as u32,
            limit as u32,
        ))
    }
}

/// Handler for ValidateCronQuery
#[derive(Clone)]
pub struct ValidateCronHandler;

impl ValidateCronHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl QueryHandler<ValidateCronQuery> for ValidateCronHandler {
    async fn handle(&self, query: ValidateCronQuery) -> Result<CronValidationResult> {
        use chrono::{DateTime, Utc};
        // use cron::Schedule;

        // TODO: Implement cron validation when cron crate is configured
        // match Schedule::from_expression(&query.cron_expression) {
        //     Ok(schedule) => {
        //         let now = Utc::now();
        //         let next_executions: Vec<DateTime<Utc>> = schedule
        //             .upcoming(chrono::TimeZone::with_system_tz)
        //             .take(10)
        //             .collect();
        //
        //         let next_execution = next_executions.first().cloned();
        //
        //         Ok(CronValidationResult {
        //             valid: true,
        //             error_message: None,
        //             next_execution,
        //             next_10_executions: next_executions,
        //         })
        //     }
        //     Err(e) => Ok(CronValidationResult {
        //         valid: false,
        //         error_message: Some(e.to_string()),
        //         next_execution: None,
        //         next_10_executions: vec![],
        //     }),
        // }

        // Temporary implementation
        Ok(CronValidationResult {
            valid: true,
            error_message: None,
            next_execution: None,
            next_10_executions: vec![],
        })
    }
}

/// Handler for GetUpcomingExecutionsQuery
#[derive(Clone)]
pub struct GetUpcomingExecutionsHandler {
    scheduled_repo: Arc<dyn ScheduledJobRepository>,
}

impl GetUpcomingExecutionsHandler {
    pub fn new(scheduled_repo: Arc<dyn ScheduledJobRepository>) -> Self {
        Self { scheduled_repo }
    }
}

#[async_trait::async_trait]
impl QueryHandler<GetUpcomingExecutionsQuery> for GetUpcomingExecutionsHandler {
    async fn handle(
        &self,
        query: GetUpcomingExecutionsQuery,
    ) -> Result<Vec<UpcomingExecutionSummary>> {
        // TODO: Implement when persistence layer is ready
        // let mut scheduled_jobs = self.scheduled_repo.find_all().await?;

        // Filter by enabled jobs and time range
        // scheduled_jobs.retain(|s| s.enabled);

        // Apply template filter if provided
        // if let Some(template_id) = query.template_id {
        //     scheduled_jobs.retain(|s| s.template_id == template_id);
        // }

        // Filter by time range
        let mut results = Vec::new();
        // for job in scheduled_jobs {
        //     if job.next_execution_at >= query.from_time && job.next_execution_at <= query.to_time {
        //         results.push(UpcomingExecutionSummary {
        //             scheduled_job_id: job.id,
        //             scheduled_job_name: job.name,
        //             template_id: job.template_id,
        //             scheduled_for: job.next_execution_at,
        //             parameters: job.parameters,
        //         });
        //     }
        // }

        // Sort by scheduled_for
        // results.sort_by(|a, b| a.scheduled_for.cmp(&b.scheduled_for));

        // Limit results
        let limit = query.limit as usize;
        if results.len() > limit {
            results.truncate(limit);
        }

        Ok(results)
    }
}

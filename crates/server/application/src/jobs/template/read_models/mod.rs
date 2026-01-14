//! Template Read Models for CQRS
//!
//! Read models are optimized views for querying template data.
//! They are updated asynchronously by subscribing to domain events.

use crate::jobs::template::queries::{ExecutionSummary, ScheduledJobSummary, TemplateSummary};
use hodei_server_domain::jobs::templates::{
    JobExecution, JobTemplate, JobTemplateId, ScheduledJob,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Read Model for Templates - in-memory optimized view
///
/// In production, this would be backed by a materialized view
/// in PostgreSQL or a separate read model database.
#[derive(Clone, Default)]
pub struct TemplateReadModel {
    /// In-memory storage (would be replaced by DB in production)
    templates: Arc<RwLock<HashMap<JobTemplateId, TemplateSummary>>>,
}

impl TemplateReadModel {
    /// Create a new TemplateReadModel
    pub fn new() -> Self {
        Self {
            templates: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a template in the read model
    pub async fn create(&self, template: &JobTemplate) {
        let summary = TemplateSummary::from(template.clone());
        let mut templates = self.templates.write().await;
        templates.insert(template.id.clone(), summary);
    }

    /// Update a template in the read model
    pub async fn update(&self, template: &JobTemplate) {
        let summary = TemplateSummary::from(template.clone());
        let mut templates = self.templates.write().await;
        if let Some(existing) = templates.get_mut(&template.id) {
            *existing = summary;
        }
    }

    /// Delete a template from the read model
    pub async fn delete(&self, template_id: &JobTemplateId) {
        let mut templates = self.templates.write().await;
        templates.remove(template_id);
    }

    /// Get a template by ID
    pub async fn get_by_id(&self, template_id: &JobTemplateId) -> Option<TemplateSummary> {
        let templates = self.templates.read().await;
        templates.get(template_id).cloned()
    }

    /// Get a template by name
    pub async fn get_by_name(&self, name: &str) -> Option<TemplateSummary> {
        let templates = self.templates.read().await;
        templates.values().find(|t| t.name == name).cloned()
    }

    /// Increment run count for a template
    pub async fn increment_run_count(&self, template_id: &JobTemplateId) {
        let mut templates = self.templates.write().await;
        if let Some(template) = templates.get_mut(template_id) {
            template.run_count += 1;
        }
    }

    /// List templates with optional status filter
    pub async fn list(
        &self,
        status: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Vec<TemplateSummary> {
        let templates = self.templates.read().await;
        let mut results: Vec<TemplateSummary> = templates
            .values()
            .filter(|t| {
                if let Some(status_filter) = status {
                    t.status == status_filter
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        results.into_iter().skip(offset).take(limit).collect()
    }

    /// Increment success count for a template
    pub async fn increment_success_count(&self, template_id: &JobTemplateId) {
        let mut templates = self.templates.write().await;
        if let Some(template) = templates.get_mut(template_id) {
            template.success_count += 1;
            template.success_rate =
                (template.success_count as f64 / template.run_count as f64) * 100.0;
        }
    }

    /// Increment failure count for a template
    pub async fn increment_failure_count(&self, template_id: &JobTemplateId) {
        let mut templates = self.templates.write().await;
        if let Some(template) = templates.get_mut(template_id) {
            template.failure_count += 1;
            template.success_rate =
                (template.success_count as f64 / template.run_count as f64) * 100.0;
        }
    }

    /// Clear all data (for testing)
    pub async fn clear(&self) {
        let mut templates = self.templates.write().await;
        templates.clear();
    }
}

/// Port for read model persistence (allows swapping implementations)
#[async_trait::async_trait]
pub trait TemplateReadModelPort: Send + Sync {
    async fn create(&self, template: &JobTemplate);
    async fn update(&self, template: &JobTemplate);
    async fn delete(&self, template_id: &JobTemplateId);
    async fn get_by_id(&self, template_id: &JobTemplateId) -> Option<TemplateSummary>;
    async fn get_by_name(&self, name: &str) -> Option<TemplateSummary>;
    async fn list(&self, status: Option<&str>, limit: usize, offset: usize)
    -> Vec<TemplateSummary>;
    async fn increment_run_count(&self, template_id: &JobTemplateId);
}

#[async_trait::async_trait]
impl TemplateReadModelPort for TemplateReadModel {
    async fn create(&self, template: &JobTemplate) {
        self.create(template).await;
    }

    async fn update(&self, template: &JobTemplate) {
        self.update(template).await;
    }

    async fn delete(&self, template_id: &JobTemplateId) {
        self.delete(template_id).await;
    }

    async fn get_by_id(&self, template_id: &JobTemplateId) -> Option<TemplateSummary> {
        self.get_by_id(template_id).await
    }

    async fn get_by_name(&self, name: &str) -> Option<TemplateSummary> {
        self.get_by_name(name).await
    }

    async fn list(
        &self,
        status: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Vec<TemplateSummary> {
        self.list(status, limit, offset).await
    }

    async fn increment_run_count(&self, template_id: &JobTemplateId) {
        self.increment_run_count(template_id).await;
    }
}

/// Read Model for Executions
#[derive(Clone, Default)]
pub struct ExecutionReadModel {
    executions: Arc<RwLock<HashMap<uuid::Uuid, ExecutionSummary>>>,
}

impl ExecutionReadModel {
    pub fn new() -> Self {
        Self {
            executions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create(&self, execution: &JobExecution) {
        let summary = ExecutionSummary::from(execution.clone());
        let mut executions = self.executions.write().await;
        executions.insert(execution.id, summary);
    }

    pub async fn update(&self, execution: &JobExecution) {
        let summary = ExecutionSummary::from(execution.clone());
        let mut executions = self.executions.write().await;
        if let Some(existing) = executions.get_mut(&execution.id) {
            *existing = summary;
        }
    }

    pub async fn get_by_id(&self, execution_id: &uuid::Uuid) -> Option<ExecutionSummary> {
        let executions = self.executions.read().await;
        executions.get(execution_id).cloned()
    }

    pub async fn list_by_template(
        &self,
        template_id: &JobTemplateId,
        state: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Vec<ExecutionSummary> {
        let executions = self.executions.read().await;
        let mut results: Vec<ExecutionSummary> = executions
            .values()
            .filter(|e| e.template_id == *template_id)
            .filter(|e| {
                if let Some(state_filter) = state {
                    e.state == state_filter
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        results.sort_by(|a, b| b.queued_at.cmp(&a.queued_at));
        results.into_iter().skip(offset).take(limit).collect()
    }

    pub async fn clear(&self) {
        let mut executions = self.executions.write().await;
        executions.clear();
    }

    /// Create a new execution
    pub async fn create_execution(&self, execution: &JobExecution) {
        self.create(execution).await;
    }
}

/// Port for execution read model
#[async_trait::async_trait]
pub trait ExecutionReadModelPort: Send + Sync {
    async fn create(&self, execution: &JobExecution);
    async fn create_execution(&self, execution: &JobExecution);
    async fn update(&self, execution: &JobExecution);
    async fn get_by_id(&self, execution_id: &uuid::Uuid) -> Option<ExecutionSummary>;
    async fn list_by_template(
        &self,
        template_id: &JobTemplateId,
        state: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Vec<ExecutionSummary>;
}

#[async_trait::async_trait]
impl ExecutionReadModelPort for ExecutionReadModel {
    async fn create(&self, execution: &JobExecution) {
        self.create(execution).await;
    }

    async fn create_execution(&self, execution: &JobExecution) {
        self.create_execution(execution).await;
    }

    async fn update(&self, execution: &JobExecution) {
        self.update(execution).await;
    }

    async fn get_by_id(&self, execution_id: &uuid::Uuid) -> Option<ExecutionSummary> {
        self.get_by_id(execution_id).await
    }

    async fn list_by_template(
        &self,
        template_id: &JobTemplateId,
        state: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Vec<ExecutionSummary> {
        self.list_by_template(template_id, state, limit, offset)
            .await
    }
}

/// Read Model for Scheduled Jobs - in-memory optimized view for scheduled job queries
#[derive(Clone, Default)]
pub struct ScheduledJobReadModel {
    /// In-memory storage (would be replaced by DB in production)
    scheduled_jobs: Arc<RwLock<HashMap<Uuid, ScheduledJobSummary>>>,
}

impl ScheduledJobReadModel {
    /// Create a new ScheduledJobReadModel
    pub fn new() -> Self {
        Self {
            scheduled_jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a scheduled job in the read model
    pub async fn create(&self, scheduled_job: &ScheduledJob) {
        let summary = ScheduledJobSummary::from(scheduled_job.clone());
        let mut jobs = self.scheduled_jobs.write().await;
        jobs.insert(scheduled_job.id, summary);
    }

    /// Update a scheduled job in the read model
    pub async fn update(&self, scheduled_job: &ScheduledJob) {
        let summary = ScheduledJobSummary::from(scheduled_job.clone());
        let mut jobs = self.scheduled_jobs.write().await;
        if let Some(existing) = jobs.get_mut(&scheduled_job.id) {
            *existing = summary;
        }
    }

    /// Delete a scheduled job from the read model
    pub async fn delete(&self, scheduled_job_id: &Uuid) {
        let mut jobs = self.scheduled_jobs.write().await;
        jobs.remove(scheduled_job_id);
    }

    /// Get a scheduled job by ID
    pub async fn get_by_id(&self, scheduled_job_id: &Uuid) -> Option<ScheduledJobSummary> {
        let jobs = self.scheduled_jobs.read().await;
        jobs.get(scheduled_job_id).cloned()
    }

    /// List scheduled jobs with optional filters
    pub async fn list(
        &self,
        template_id: Option<&JobTemplateId>,
        enabled: Option<bool>,
        limit: usize,
        offset: usize,
    ) -> Vec<ScheduledJobSummary> {
        let jobs = self.scheduled_jobs.read().await;
        let mut results: Vec<ScheduledJobSummary> = jobs
            .values()
            .filter(|j| {
                if let Some(template_filter) = template_id {
                    &j.template_id == template_filter
                } else {
                    true
                }
            })
            .filter(|j| {
                if let Some(enabled_filter) = enabled {
                    j.enabled == enabled_filter
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        results.into_iter().skip(offset).take(limit).collect()
    }

    /// Get scheduled jobs that are due for execution
    pub async fn get_due_jobs(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Vec<ScheduledJobSummary> {
        let jobs = self.scheduled_jobs.read().await;
        jobs.values()
            .filter(|j| j.enabled && j.next_execution_at <= now)
            .cloned()
            .collect()
    }

    /// Get upcoming scheduled executions within a time range
    pub async fn get_upcoming_executions(
        &self,
        from_time: chrono::DateTime<chrono::Utc>,
        to_time: chrono::DateTime<chrono::Utc>,
        template_id: Option<&JobTemplateId>,
        limit: usize,
    ) -> Vec<(ScheduledJobSummary, chrono::DateTime<chrono::Utc>)> {
        let jobs = self.scheduled_jobs.read().await;
        let mut results: Vec<(ScheduledJobSummary, chrono::DateTime<chrono::Utc>)> = jobs
            .values()
            .filter(|j| {
                if let Some(template_filter) = template_id {
                    &j.template_id == template_filter
                } else {
                    true
                }
            })
            .filter(|j| j.next_execution_at >= from_time && j.next_execution_at <= to_time)
            .filter(|j| j.enabled)
            .map(|j| (j.clone(), j.next_execution_at))
            .collect();

        results.sort_by(|a, b| a.1.cmp(&b.1));
        results.into_iter().take(limit).collect()
    }

    /// Clear all data (for testing)
    pub async fn clear(&self) {
        let mut jobs = self.scheduled_jobs.write().await;
        jobs.clear();
    }
}

/// Port for scheduled job read model persistence
#[async_trait::async_trait]
pub trait ScheduledJobReadModelPort: Send + Sync {
    async fn create(&self, scheduled_job: &ScheduledJob);
    async fn update(&self, scheduled_job: &ScheduledJob);
    async fn delete(&self, scheduled_job_id: &Uuid);
    async fn get_by_id(&self, scheduled_job_id: &Uuid) -> Option<ScheduledJobSummary>;
    async fn list(
        &self,
        template_id: Option<&JobTemplateId>,
        enabled: Option<bool>,
        limit: usize,
        offset: usize,
    ) -> Vec<ScheduledJobSummary>;
    async fn get_upcoming_executions(
        &self,
        from_time: chrono::DateTime<chrono::Utc>,
        to_time: chrono::DateTime<chrono::Utc>,
        template_id: Option<&JobTemplateId>,
        limit: usize,
    ) -> Vec<(ScheduledJobSummary, chrono::DateTime<chrono::Utc>)>;
}

#[async_trait::async_trait]
impl ScheduledJobReadModelPort for ScheduledJobReadModel {
    async fn create(&self, scheduled_job: &ScheduledJob) {
        self.create(scheduled_job).await;
    }

    async fn update(&self, scheduled_job: &ScheduledJob) {
        self.update(scheduled_job).await;
    }

    async fn delete(&self, scheduled_job_id: &Uuid) {
        self.delete(scheduled_job_id).await;
    }

    async fn get_by_id(&self, scheduled_job_id: &Uuid) -> Option<ScheduledJobSummary> {
        self.get_by_id(scheduled_job_id).await
    }

    async fn list(
        &self,
        template_id: Option<&JobTemplateId>,
        enabled: Option<bool>,
        limit: usize,
        offset: usize,
    ) -> Vec<ScheduledJobSummary> {
        self.list(template_id, enabled, limit, offset).await
    }

    async fn get_upcoming_executions(
        &self,
        from_time: chrono::DateTime<chrono::Utc>,
        to_time: chrono::DateTime<chrono::Utc>,
        template_id: Option<&JobTemplateId>,
        limit: usize,
    ) -> Vec<(ScheduledJobSummary, chrono::DateTime<chrono::Utc>)> {
        self.get_upcoming_executions(from_time, to_time, template_id, limit)
            .await
    }
}

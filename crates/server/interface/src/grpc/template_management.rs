//! Template Management gRPC Service Implementation

use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info};

use crate::mappers::{
    error_to_status, map_job_spec_to_proto, map_proto_to_job_spec, now_timestamp, to_timestamp,
};
use hodei_jobs::{
    CreateTemplateRequest, CreateTemplateResponse, DeleteTemplateRequest, DeleteTemplateResponse,
    GetExecutionRequest, GetExecutionResponse, GetTemplateRequest, GetTemplateResponse,
    JobExecution, JobTemplate, ListExecutionsRequest, ListExecutionsResponse, ListTemplatesRequest,
    ListTemplatesResponse, TriggerRunRequest, TriggerRunResponse, UpdateTemplateRequest,
    UpdateTemplateResponse, template_management_service_server::TemplateManagementService,
};

use hodei_server_domain::jobs::{
    JobExecution, JobSpec, JobTemplate, JobTemplateId, JobTemplateParameter,
};
use hodei_server_domain::shared_kernel::{JobId, Result};

/// Template Management Service Implementation
#[derive(Clone)]
pub struct TemplateManagementServiceImpl {
    /// Repository for job templates
    template_repository: Arc<dyn hodei_server_domain::jobs::templates::JobTemplateRepository>,
    /// Job repository for creating jobs from templates
    job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
    /// Event bus for publishing domain events
    event_bus: Option<Arc<dyn hodei_server_domain::event_bus::EventBus>>,
}

impl TemplateManagementServiceImpl {
    /// Create a new template management service
    pub fn new(
        template_repository: Arc<dyn hodei_server_domain::jobs::templates::JobTemplateRepository>,
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
    ) -> Self {
        Self {
            template_repository,
            job_repository,
            event_bus: None,
        }
    }

    /// Create with event bus support
    pub fn with_event_bus(
        template_repository: Arc<dyn hodei_server_domain::jobs::templates::JobTemplateRepository>,
        job_repository: Arc<dyn hodei_server_domain::jobs::JobRepository>,
        event_bus: Arc<dyn hodei_server_domain::event_bus::EventBus>,
    ) -> Self {
        Self {
            template_repository,
            job_repository,
            event_bus: Some(event_bus),
        }
    }
}

#[tonic::async_trait]
impl TemplateManagementService for TemplateManagementServiceImpl {
    async fn create_template(
        &self,
        request: Request<CreateTemplateRequest>,
    ) -> Result<Response<CreateTemplateResponse>, Status> {
        debug!("Create template request received");

        let req = request.into_inner();
        let correlation_id = request
            .metadata()
            .get("correlation-id")
            .map(|s| s.to_string());

        // Convert JobSpec from proto to domain
        let spec = map_proto_to_job_spec(&req.spec).map_err(|e| error_to_status(e))?;

        // Validate the spec
        spec.validate().map_err(|e| error_to_status(e))?;

        // Create template
        let mut template = JobTemplate::new(req.name, spec);
        if let Some(desc) = req.description {
            template = template.with_description(desc);
        }

        if let Some(user) = req.created_by {
            template = template.with_created_by(user);
        }

        // Add labels
        for (key, value) in req.labels {
            template = template.with_label(key, value);
        }

        // Save template
        self.template_repository
            .save(&template)
            .await
            .map_err(|e| error_to_status(e))?;

        // Publish event if event bus is available
        if let Some(event_bus) = &self.event_bus {
            let event = hodei_server_domain::events::DomainEvent::TemplateCreated {
                template_id: template.id.to_string(),
                template_name: template.name.clone(),
                version: template.version,
                created_by: template.created_by.clone(),
                spec_summary: format!("command={:?}", template.spec.command),
                occurred_at: chrono::Utc::now(),
                correlation_id,
                actor: template.created_by.clone(),
            };

            if let Err(e) = event_bus.publish(&event).await {
                error!("Failed to publish TemplateCreated event: {:?}", e);
            }
        }

        info!("Template created: {}", template.id);

        // Convert to proto
        let proto_template = JobTemplate {
            template_id: template.id.to_string(),
            name: template.name,
            description: template.description,
            spec: map_job_spec_to_proto(&template.spec),
            status: match template.status {
                hodei_server_domain::jobs::JobTemplateStatus::Active => "ACTIVE".to_string(),
                hodei_server_domain::jobs::JobTemplateStatus::Disabled => "DISABLED".to_string(),
                hodei_server_domain::jobs::JobTemplateStatus::Archived => "ARCHIVED".to_string(),
            },
            version: template.version,
            labels: template.labels,
            created_at: Some(now_timestamp()),
            updated_at: Some(to_timestamp(template.updated_at)),
            created_by: template.created_by,
            run_count: template.run_count,
            success_count: template.success_count,
            failure_count: template.failure_count,
        };

        Ok(Response::new(CreateTemplateResponse {
            template: Some(proto_template),
        }))
    }

    async fn update_template(
        &self,
        request: Request<UpdateTemplateRequest>,
    ) -> Result<Response<UpdateTemplateResponse>, Status> {
        debug!("Update template request received");

        let req = request.into_inner();

        // Parse template ID
        let template_id: JobTemplateId = req
            .template_id
            .parse()
            .map_err(|_| Status::invalid_argument("Invalid template ID"))?;

        // Find template
        let mut template = self
            .template_repository
            .find_by_id(&template_id)
            .await
            .map_err(|e| error_to_status(e))?
            .ok_or_else(|| Status::not_found("Template not found"))?;

        // Update description if provided
        if let Some(description) = req.description {
            template.description = Some(description);
        }

        // Update spec if provided
        if let Some(spec) = req.spec {
            let domain_spec = map_proto_to_job_spec(&spec).map_err(|e| error_to_status(e))?;

            domain_spec.validate().map_err(|e| error_to_status(e))?;

            template.update_spec(domain_spec);
        }

        // Update labels
        if !req.labels.is_empty() {
            template.labels = req.labels;
        }

        // Save updated template
        self.template_repository
            .update(&template)
            .await
            .map_err(|e| error_to_status(e))?;

        // Publish event
        if let Some(event_bus) = &self.event_bus {
            let event = hodei_server_domain::events::DomainEvent::TemplateUpdated {
                template_id: template.id.to_string(),
                template_name: template.name.clone(),
                old_version: template.version - 1,
                new_version: template.version,
                changes: Some("Updated spec".to_string()),
                occurred_at: chrono::Utc::now(),
                correlation_id: None,
                actor: None,
            };

            if let Err(e) = event_bus.publish(&event).await {
                error!("Failed to publish TemplateUpdated event: {:?}", e);
            }
        }

        // Convert to proto
        let proto_template = JobTemplate {
            template_id: template.id.to_string(),
            name: template.name,
            description: template.description,
            spec: map_job_spec_to_proto(&template.spec),
            status: match template.status {
                hodei_server_domain::jobs::JobTemplateStatus::Active => "ACTIVE".to_string(),
                hodei_server_domain::jobs::JobTemplateStatus::Disabled => "DISABLED".to_string(),
                hodei_server_domain::jobs::JobTemplateStatus::Archived => "ARCHIVED".to_string(),
            },
            version: template.version,
            labels: template.labels,
            created_at: Some(to_timestamp(template.created_at)),
            updated_at: Some(to_timestamp(template.updated_at)),
            created_by: template.created_by,
            run_count: template.run_count,
            success_count: template.success_count,
            failure_count: template.failure_count,
        };

        Ok(Response::new(UpdateTemplateResponse {
            template: Some(proto_template),
        }))
    }

    async fn get_template(
        &self,
        request: Request<GetTemplateRequest>,
    ) -> Result<Response<GetTemplateResponse>, Status> {
        debug!("Get template request received");

        let req = request.into_inner();

        let template_id: JobTemplateId = req
            .template_id
            .parse()
            .map_err(|_| Status::invalid_argument("Invalid template ID"))?;

        let template = self
            .template_repository
            .find_by_id(&template_id)
            .await
            .map_err(|e| error_to_status(e))?
            .ok_or_else(|| Status::not_found("Template not found"))?;

        let proto_template = JobTemplate {
            template_id: template.id.to_string(),
            name: template.name,
            description: template.description,
            spec: map_job_spec_to_proto(&template.spec),
            status: match template.status {
                hodei_server_domain::jobs::JobTemplateStatus::Active => "ACTIVE".to_string(),
                hodei_server_domain::jobs::JobTemplateStatus::Disabled => "DISABLED".to_string(),
                hodei_server_domain::jobs::JobTemplateStatus::Archived => "ARCHIVED".to_string(),
            },
            version: template.version,
            labels: template.labels,
            created_at: Some(to_timestamp(template.created_at)),
            updated_at: Some(to_timestamp(template.updated_at)),
            created_by: template.created_by,
            run_count: template.run_count,
            success_count: template.success_count,
            failure_count: template.failure_count,
        };

        Ok(Response::new(GetTemplateResponse {
            template: Some(proto_template),
        }))
    }

    async fn list_templates(
        &self,
        _request: Request<ListTemplatesRequest>,
    ) -> Result<Response<ListTemplatesResponse>, Status> {
        debug!("List templates request received");

        let templates = self
            .template_repository
            .list_active()
            .await
            .map_err(|e| error_to_status(e))?;

        let proto_templates: Vec<JobTemplate> = templates
            .into_iter()
            .map(|template| JobTemplate {
                template_id: template.id.to_string(),
                name: template.name,
                description: template.description,
                spec: map_job_spec_to_proto(&template.spec),
                status: match template.status {
                    hodei_server_domain::jobs::JobTemplateStatus::Active => "ACTIVE".to_string(),
                    hodei_server_domain::jobs::JobTemplateStatus::Disabled => {
                        "DISABLED".to_string()
                    }
                    hodei_server_domain::jobs::JobTemplateStatus::Archived => {
                        "ARCHIVED".to_string()
                    }
                },
                version: template.version,
                labels: template.labels,
                created_at: Some(to_timestamp(template.created_at)),
                updated_at: Some(to_timestamp(template.updated_at)),
                created_by: template.created_by,
                run_count: template.run_count,
                success_count: template.success_count,
                failure_count: template.failure_count,
            })
            .collect();

        Ok(Response::new(ListTemplatesResponse {
            templates: proto_templates,
            next_page_token: "".to_string(),
        }))
    }

    async fn delete_template(
        &self,
        request: Request<DeleteTemplateRequest>,
    ) -> Result<Response<DeleteTemplateResponse>, Status> {
        debug!("Delete template request received");

        let req = request.into_inner();

        let template_id: JobTemplateId = req
            .template_id
            .parse()
            .map_err(|_| Status::invalid_argument("Invalid template ID"))?;

        self.template_repository
            .delete(&template_id)
            .await
            .map_err(|e| error_to_status(e))?;

        Ok(Response::new(DeleteTemplateResponse { success: true }))
    }

    async fn trigger_run(
        &self,
        request: Request<TriggerRunRequest>,
    ) -> Result<Response<TriggerRunResponse>, Status> {
        debug!("Trigger run request received");

        let req = request.into_inner();

        let template_id: JobTemplateId = req
            .template_id
            .parse()
            .map_err(|_| Status::invalid_argument("Invalid template ID"))?;

        // Find template
        let mut template = self
            .template_repository
            .find_by_id(&template_id)
            .await
            .map_err(|e| error_to_status(e))?
            .ok_or_else(|| Status::not_found("Template not found"))?;

        if !template.can_create_run() {
            return Err(Status::failed_precondition("Template is not active"));
        }

        // Create job from template
        let job_name = req.job_name.unwrap_or_else(|| template.name.clone());
        let mut execution = JobExecution::new(
            &template,
            template.run_count + 1,
            job_name,
            hodei_server_domain::jobs::TriggerType::Manual,
        );

        // Set parameters
        execution.parameters = req.parameters;

        // Create job
        let job = template.create_run().map_err(|e| error_to_status(e))?;

        // Save job
        self.job_repository
            .save(&job)
            .await
            .map_err(|e| error_to_status(e))?;

        // Update execution with job ID
        execution.job_id = Some(job.id().clone());

        // Update template
        self.template_repository
            .update(&template)
            .await
            .map_err(|e| error_to_status(e))?;

        // Publish event
        if let Some(event_bus) = &self.event_bus {
            let event = hodei_server_domain::events::DomainEvent::TemplateRunCreated {
                template_id: template.id.to_string(),
                template_name: template.name.clone(),
                execution_id: execution.id.to_string(),
                job_id: execution.job_id.clone(),
                job_name: execution.job_name.clone(),
                template_version: execution.template_version,
                execution_number: execution.execution_number,
                triggered_by: "MANUAL".to_string(),
                triggered_by_user: req.triggered_by_user,
                occurred_at: chrono::Utc::now(),
                correlation_id: None,
                actor: req.triggered_by_user,
            };

            if let Err(e) = event_bus.publish(&event).await {
                error!("Failed to publish TemplateRunCreated event: {:?}", e);
            }
        }

        let proto_execution = JobExecution {
            execution_id: execution.id.to_string(),
            execution_number: execution.execution_number,
            template_id: execution.template_id.to_string(),
            template_version: execution.template_version,
            job_id: execution.job_id.map(|id| id.to_string()),
            job_name: execution.job_name,
            job_spec: map_job_spec_to_proto(&execution.job_spec),
            state: match execution.state {
                hodei_server_domain::jobs::ExecutionStatus::Queued => "QUEUED".to_string(),
                hodei_server_domain::jobs::ExecutionStatus::Running => "RUNNING".to_string(),
                hodei_server_domain::jobs::ExecutionStatus::Succeeded => "SUCCEEDED".to_string(),
                hodei_server_domain::jobs::ExecutionStatus::Failed => "FAILED".to_string(),
                hodei_server_domain::jobs::ExecutionStatus::Error => "ERROR".to_string(),
            },
            result: execution.result.map(|r| hodei_jobs::ExecutionResult {
                exit_code: r.exit_code,
                output_summary: r.output_summary,
                error_output: r.error_output,
                duration_ms: r.duration_ms,
            }),
            queued_at: Some(to_timestamp(execution.queued_at)),
            started_at: execution.started_at.map(to_timestamp),
            completed_at: execution.completed_at.map(to_timestamp),
            triggered_by: match execution.triggered_by {
                hodei_server_domain::jobs::TriggerType::Manual => "MANUAL".to_string(),
                hodei_server_domain::jobs::TriggerType::Scheduled => "SCHEDULED".to_string(),
                hodei_server_domain::jobs::TriggerType::Api => "API".to_string(),
                hodei_server_domain::jobs::TriggerType::Webhook => "WEBHOOK".to_string(),
                hodei_server_domain::jobs::TriggerType::Retry => "RETRY".to_string(),
            },
            scheduled_job_id: execution.scheduled_job_id.map(|id| id.to_string()),
            triggered_by_user: execution.triggered_by_user,
            parameters: execution.parameters,
            resource_usage: execution
                .resource_usage
                .map(|r| hodei_jobs::ResourceUsageSnapshot {
                    peak_cpu_percent: r.peak_cpu_percent,
                    peak_memory_mb: r.peak_memory_mb,
                    avg_cpu_percent: r.avg_cpu_percent,
                    avg_memory_mb: r.avg_memory_mb,
                    disk_io_bytes: r.disk_io_bytes,
                    network_io_bytes: r.network_io_bytes,
                }),
            metadata: execution.metadata,
            created_at: Some(to_timestamp(execution.created_at)),
        };

        Ok(Response::new(TriggerRunResponse {
            execution: Some(proto_execution),
        }))
    }

    async fn get_execution(
        &self,
        _request: Request<GetExecutionRequest>,
    ) -> Result<Response<GetExecutionResponse>, Status> {
        // TODO: Implement when we have execution repository
        Err(Status::unimplemented("Get execution not yet implemented"))
    }

    async fn list_executions(
        &self,
        _request: Request<ListExecutionsRequest>,
    ) -> Result<Response<ListExecutionsResponse>, Status> {
        // TODO: Implement when we have execution repository
        Err(Status::unimplemented("List executions not yet implemented"))
    }
}

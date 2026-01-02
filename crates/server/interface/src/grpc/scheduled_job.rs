//! Scheduled Job gRPC Service Implementation

use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info};

use hodei_jobs::{
    CreateScheduledJobRequest, CreateScheduledJobResponse, DeleteScheduledJobRequest,
    DeleteScheduledJobResponse, GetScheduledJobRequest, GetScheduledJobResponse,
    GetUpcomingExecutionsRequest, GetUpcomingExecutionsResponse, ListScheduledJobsRequest,
    ListScheduledJobsResponse, SetScheduledJobStatusRequest, SetScheduledJobStatusResponse,
    TriggerScheduledJobNowRequest, TriggerScheduledJobNowResponse, UpcomingExecution,
    ValidateCronExpressionRequest, ValidateCronExpressionResponse,
    scheduled_job_service_server::ScheduledJobService,
};

use hodei_server_domain::jobs::{JobTemplateId, ScheduledJob};
use hodei_server_domain::shared_kernel::Result;

/// Scheduled Job Service Implementation
#[derive(Clone)]
pub struct ScheduledJobServiceImpl {
    /// Repository for scheduled jobs
    scheduled_job_repository:
        Arc<dyn hodei_server_domain::jobs::scheduled_job::ScheduledJobRepository>,
    /// Template repository for validation
    template_repository: Arc<dyn hodei_server_domain::jobs::templates::JobTemplateRepository>,
    /// Event bus for publishing domain events
    event_bus: Option<Arc<dyn hodei_server_domain::event_bus::EventBus>>,
}

impl ScheduledJobServiceImpl {
    /// Create a new scheduled job service
    pub fn new(
        scheduled_job_repository: Arc<
            dyn hodei_server_domain::jobs::scheduled_job::ScheduledJobRepository,
        >,
        template_repository: Arc<dyn hodei_server_domain::jobs::templates::JobTemplateRepository>,
    ) -> Self {
        Self {
            scheduled_job_repository,
            template_repository,
            event_bus: None,
        }
    }

    /// Create with event bus support
    pub fn with_event_bus(
        scheduled_job_repository: Arc<
            dyn hodei_server_domain::jobs::scheduled_job::ScheduledJobRepository,
        >,
        template_repository: Arc<dyn hodei_server_domain::jobs::templates::JobTemplateRepository>,
        event_bus: Arc<dyn hodei_server_domain::event_bus::EventBus>,
    ) -> Self {
        Self {
            scheduled_job_repository,
            template_repository,
            event_bus: Some(event_bus),
        }
    }
}

#[tonic::async_trait]
impl ScheduledJobService for ScheduledJobServiceImpl {
    async fn create_scheduled_job(
        &self,
        request: Request<CreateScheduledJobRequest>,
    ) -> Result<Response<CreateScheduledJobResponse>, Status> {
        debug!("Create scheduled job request received");

        let req = request.into_inner();

        // Validate template exists
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

        // Create scheduled job
        let mut scheduled_job = ScheduledJob::new(
            req.name,
            template_id,
            req.cron_expression,
            req.timezone.unwrap_or_else(|| "UTC".to_string()),
        );

        if let Some(description) = req.description {
            scheduled_job = scheduled_job.with_description(description);
        }

        if let Some(user) = req.created_by {
            scheduled_job = scheduled_job.with_created_by(user);
        }

        // Add parameters
        for (key, value) in req.parameters {
            scheduled_job = scheduled_job.with_parameter(key, value);
        }

        if let Some(max_failures) = req.max_consecutive_failures {
            scheduled_job = scheduled_job.with_max_consecutive_failures(max_failures);
        }

        if req.pause_on_failure {
            scheduled_job = scheduled_job.with_pause_on_failure(true);
        }

        // Save scheduled job
        self.scheduled_job_repository
            .save(&scheduled_job)
            .await
            .map_err(|e| error_to_status(e))?;

        // Publish event
        if let Some(event_bus) = &self.event_bus {
            let event = hodei_server_domain::events::DomainEvent::ScheduledJobCreated {
                scheduled_job_id: scheduled_job.id.to_string(),
                name: scheduled_job.name.clone(),
                template_id: template_id.to_string(),
                cron_expression: scheduled_job.cron_expression.clone(),
                timezone: scheduled_job.timezone.clone(),
                created_by: scheduled_job.created_by.clone(),
                occurred_at: chrono::Utc::now(),
                correlation_id: None,
                actor: scheduled_job.created_by.clone(),
            };

            if let Err(e) = event_bus.publish(&event).await {
                error!("Failed to publish ScheduledJobCreated event: {:?}", e);
            }
        }

        info!("Scheduled job created: {}", scheduled_job.id);

        // Convert to proto
        let proto_scheduled_job = hodei_jobs::ScheduledJob {
            scheduled_job_id: scheduled_job.id.to_string(),
            name: scheduled_job.name,
            description: scheduled_job.description,
            template_id: scheduled_job.template_id.to_string(),
            cron_expression: scheduled_job.cron_expression,
            timezone: scheduled_job.timezone,
            next_execution_at: Some(to_timestamp(scheduled_job.next_execution_at)),
            last_execution_at: scheduled_job.last_execution_at.map(to_timestamp),
            last_execution_status: scheduled_job.last_execution_status.map(|s| match s {
                hodei_server_domain::jobs::ExecutionStatus::Queued => "QUEUED".to_string(),
                hodei_server_domain::jobs::ExecutionStatus::Running => "RUNNING".to_string(),
                hodei_server_domain::jobs::ExecutionStatus::Succeeded => "SUCCEEDED".to_string(),
                hodei_server_domain::jobs::ExecutionStatus::Failed => "FAILED".to_string(),
                hodei_server_domain::jobs::ExecutionStatus::Error => "ERROR".to_string(),
            }),
            enabled: scheduled_job.enabled,
            max_consecutive_failures: scheduled_job.max_consecutive_failures,
            consecutive_failures: scheduled_job.consecutive_failures,
            pause_on_failure: scheduled_job.pause_on_failure,
            parameters: scheduled_job.parameters,
            created_at: Some(to_timestamp(scheduled_job.created_at)),
            updated_at: Some(to_timestamp(scheduled_job.updated_at)),
            created_by: scheduled_job.created_by,
        };

        Ok(Response::new(CreateScheduledJobResponse {
            scheduled_job: Some(proto_scheduled_job),
        }))
    }

    async fn update_scheduled_job(
        &self,
        request: Request<UpdateScheduledJobRequest>,
    ) -> Result<Response<UpdateScheduledJobResponse>, Status> {
        debug!("Update scheduled job request received");

        let req = request.into_inner();

        // TODO: Implement when we have update method
        Err(Status::unimplemented(
            "Update scheduled job not yet implemented",
        ))
    }

    async fn get_scheduled_job(
        &self,
        request: Request<GetScheduledJobRequest>,
    ) -> Result<Response<GetScheduledJobResponse>, Status> {
        debug!("Get scheduled job request received");

        let req = request.into_inner();

        // TODO: Implement when we have repository method
        Err(Status::unimplemented(
            "Get scheduled job not yet implemented",
        ))
    }

    async fn list_scheduled_jobs(
        &self,
        _request: Request<ListScheduledJobsRequest>,
    ) -> Result<Response<ListScheduledJobsResponse>, Status> {
        debug!("List scheduled jobs request received");

        // TODO: Implement when we have repository method
        Err(Status::unimplemented(
            "List scheduled jobs not yet implemented",
        ))
    }

    async fn delete_scheduled_job(
        &self,
        request: Request<DeleteScheduledJobRequest>,
    ) -> Result<Response<DeleteScheduledJobResponse>, Status> {
        debug!("Delete scheduled job request received");

        let req = request.into_inner();

        // TODO: Implement when we have repository method
        Err(Status::unimplemented(
            "Delete scheduled job not yet implemented",
        ))
    }

    async fn set_scheduled_job_status(
        &self,
        request: Request<SetScheduledJobStatusRequest>,
    ) -> Result<Response<SetScheduledJobStatusResponse>, Status> {
        debug!("Set scheduled job status request received");

        let req = request.into_inner();

        // TODO: Implement when we have repository method
        Err(Status::unimplemented(
            "Set scheduled job status not yet implemented",
        ))
    }

    async fn trigger_scheduled_job_now(
        &self,
        request: Request<TriggerScheduledJobNowRequest>,
    ) -> Result<Response<TriggerScheduledJobNowResponse>, Status> {
        debug!("Trigger scheduled job now request received");

        let req = request.into_inner();

        // TODO: Implement when we have repository method
        Err(Status::unimplemented(
            "Trigger scheduled job now not yet implemented",
        ))
    }

    async fn get_upcoming_executions(
        &self,
        request: Request<GetUpcomingExecutionsRequest>,
    ) -> Result<Response<GetUpcomingExecutionsResponse>, Status> {
        debug!("Get upcoming executions request received");

        let req = request.into_inner();

        // TODO: Implement with cron expression parsing
        Err(Status::unimplemented(
            "Get upcoming executions not yet implemented",
        ))
    }

    async fn validate_cron_expression(
        &self,
        request: Request<ValidateCronExpressionRequest>,
    ) -> Result<Response<ValidateCronExpressionResponse>, Status> {
        debug!("Validate cron expression request received");

        let req = request.into_inner();

        // TODO: Implement cron validation
        Err(Status::unimplemented(
            "Validate cron expression not yet implemented",
        ))
    }
}

/// Helper functions (these should be in a mapper module)
fn to_timestamp(dt: chrono::DateTime<chrono::Utc>) -> prost_types::Timestamp {
    prost_types::Timestamp::date_time(dt.timestamp(), dt.timestamp_subsec_nanos())
        .expect("Failed to convert timestamp")
}

//! Template Command Handlers
//!
//! Implements the business logic for template management commands.
//! Each handler is invoked by the CommandBus and publishes domain events.

use crate::core::command::CommandHandler;
use crate::jobs::template::commands::{
    CreateTemplateCommand, DeleteTemplateCommand, DisableTemplateCommand, EnableTemplateCommand,
    ExecutionResult, TemplateResult, TriggerRunCommand, UpdateTemplateCommand,
};
use crate::jobs::template::read_models::{ExecutionReadModelPort, TemplateReadModelPort};
use hodei_server_domain::event_bus::EventBus;
use hodei_server_domain::events::{DomainEvent, JobCreated};
use hodei_server_domain::jobs::aggregate::JobSpec;
use hodei_server_domain::jobs::templates::{
    JobExecution, JobExecutionRepository, JobTemplate, JobTemplateRepository,
    ScheduledJobRepository,
};
use hodei_server_domain::shared_kernel::Result;
use std::sync::Arc;
use tracing::{debug, info, instrument};

/// Handler for CreateTemplateCommand
#[derive(Clone)]
pub struct CreateTemplateHandler {
    template_repo: Arc<dyn JobTemplateRepository>,
    event_bus: Arc<dyn EventBus>,
    read_model: Arc<dyn TemplateReadModelPort>,
}

impl CreateTemplateHandler {
    pub fn new(
        template_repo: Arc<dyn JobTemplateRepository>,
        event_bus: Arc<dyn EventBus>,
        read_model: Arc<dyn TemplateReadModelPort>,
    ) -> Self {
        Self {
            template_repo,
            event_bus,
            read_model,
        }
    }
}

#[async_trait::async_trait]
impl CommandHandler<CreateTemplateCommand> for CreateTemplateHandler {
    #[instrument(skip(self, command))]
    async fn handle(&self, command: CreateTemplateCommand) -> Result<TemplateResult> {
        info!("Creating template: {}", command.name);

        // Check if template with same name exists
        if let Some(_existing) = self.template_repo.find_by_name(&command.name).await? {
            return Err(
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Template with name '{}' already exists", command.name),
                },
            );
        }

        // Parse and validate the spec
        let spec: JobSpec = serde_json::from_value(command.spec.clone()).map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::TemplateParameterValidationError {
                message: format!("Invalid job spec: {}", e),
            }
        })?;

        // Create the template
        let mut template = JobTemplate::new(command.name.clone(), spec)
            .with_description(command.description.clone().unwrap_or_default())
            .with_created_by(command.created_by.clone());

        // Add labels
        for (key, value) in &command.labels {
            template = template.with_label(key.clone(), value.clone());
        }

        // Save the template
        self.template_repo.save(&template).await?;

        // Publish domain event
        let event = DomainEvent::TemplateCreated {
            template_id: template.id.to_string(),
            template_name: template.name.clone(),
            version: template.version,
            created_by: template.created_by.clone(),
            spec_summary: "Template created".to_string(),
            occurred_at: template.created_at,
            correlation_id: None,
            actor: Some(command.created_by),
        };
        self.event_bus.publish(&event).await?;

        // Update read model
        self.read_model.create(&template).await;

        debug!("Template created successfully: {}", template.id);

        Ok(TemplateResult {
            template_id: template.id,
            name: template.name,
            version: template.version,
            status: template.status.to_string(),
            created_at: template.created_at,
        })
    }
}

// Query handlers
mod query_handlers;

pub use query_handlers::{
    GetExecutionHandler, GetExecutionsByJobHandler, GetScheduledJobHandler,
    GetTemplateByNameHandler, GetTemplateHandler, GetUpcomingExecutionsHandler,
    ListExecutionsHandler, ListScheduledJobsHandler, ListTemplatesHandler, ValidateCronHandler,
};

/// Handler for UpdateTemplateCommand
#[derive(Clone)]
pub struct UpdateTemplateHandler {
    template_repo: Arc<dyn JobTemplateRepository>,
    event_bus: Arc<dyn EventBus>,
    read_model: Arc<dyn TemplateReadModelPort>,
}

impl UpdateTemplateHandler {
    pub fn new(
        template_repo: Arc<dyn JobTemplateRepository>,
        event_bus: Arc<dyn EventBus>,
        read_model: Arc<dyn TemplateReadModelPort>,
    ) -> Self {
        Self {
            template_repo,
            event_bus,
            read_model,
        }
    }
}

#[async_trait::async_trait]
impl CommandHandler<UpdateTemplateCommand> for UpdateTemplateHandler {
    #[instrument(skip(self, command))]
    async fn handle(&self, command: UpdateTemplateCommand) -> Result<TemplateResult> {
        info!("Updating template: {}", command.template_id);

        // Load existing template
        let mut template = self
            .template_repo
            .find_by_id(&command.template_id)
            .await?
            .ok_or_else(|| {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Template not found: {}", command.template_id),
                }
            })?;

        // Track old version
        let old_version = template.version;

        // Update fields if provided
        if let Some(description) = command.description {
            template.description = Some(description);
        }
        if let Some(spec) = command.spec {
            let new_spec: JobSpec = serde_json::from_value(spec).map_err(|e| {
                hodei_server_domain::shared_kernel::DomainError::TemplateParameterValidationError {
                    message: format!("Invalid job spec: {}", e),
                }
            })?;
            template.update_spec(new_spec);
        }
        if let Some(labels) = command.labels {
            template.labels = labels;
        }

        // Save updated template
        self.template_repo.update(&template).await?;

        // Publish domain event
        let event = DomainEvent::TemplateUpdated {
            template_id: template.id.to_string(),
            template_name: template.name.clone(),
            old_version,
            new_version: template.version,
            changes: Some("spec/description/labels updated".to_string()),
            occurred_at: template.updated_at,
            correlation_id: None,
            actor: Some(command.updated_by),
        };
        self.event_bus.publish(&event).await?;

        // Update read model
        self.read_model.update(&template).await;

        debug!(
            "Template updated: {} from v{} to v{}",
            template.id, old_version, template.version
        );

        Ok(TemplateResult {
            template_id: template.id,
            name: template.name,
            version: template.version,
            status: template.status.to_string(),
            created_at: template.created_at,
        })
    }
}

/// Handler for DeleteTemplateCommand
#[derive(Clone)]
pub struct DeleteTemplateHandler {
    template_repo: Arc<dyn JobTemplateRepository>,
    execution_repo: Arc<dyn JobExecutionRepository>,
    scheduled_job_repo: Arc<dyn ScheduledJobRepository>,
    event_bus: Arc<dyn EventBus>,
    read_model: Arc<dyn TemplateReadModelPort>,
}

impl DeleteTemplateHandler {
    pub fn new(
        template_repo: Arc<dyn JobTemplateRepository>,
        execution_repo: Arc<dyn JobExecutionRepository>,
        scheduled_job_repo: Arc<dyn ScheduledJobRepository>,
        event_bus: Arc<dyn EventBus>,
        read_model: Arc<dyn TemplateReadModelPort>,
    ) -> Self {
        Self {
            template_repo,
            execution_repo,
            scheduled_job_repo,
            event_bus,
            read_model,
        }
    }
}

#[async_trait::async_trait]
impl CommandHandler<DeleteTemplateCommand> for DeleteTemplateHandler {
    #[instrument(skip(self, command))]
    async fn handle(&self, command: DeleteTemplateCommand) -> Result<bool> {
        info!("Deleting template: {}", command.template_id);

        // Load template
        let template = self
            .template_repo
            .find_by_id(&command.template_id)
            .await?
            .ok_or_else(|| {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Template not found: {}", command.template_id),
                }
            })?;

        // Check for dependencies if not forced
        if !command.force {
            // Check for scheduled jobs
            let scheduled_jobs = self
                .scheduled_job_repo
                .find_by_template_id(&command.template_id)
                .await?;

            if !scheduled_jobs.is_empty() {
                return Err(
                    hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                        message: format!(
                            "Cannot delete template: {} scheduled jobs still exist",
                            scheduled_jobs.len()
                        ),
                    },
                );
            }
        }

        // Delete template
        self.template_repo.delete(&command.template_id).await?;

        // Publish domain event
        let event = DomainEvent::TemplateDisabled {
            template_id: template.id.to_string(),
            template_name: template.name.clone(),
            version: template.version,
            occurred_at: chrono::Utc::now(),
            correlation_id: None,
            actor: Some(command.deleted_by),
        };
        self.event_bus.publish(&event).await?;

        // Update read model
        self.read_model.delete(&command.template_id).await;

        debug!("Template deleted: {}", command.template_id);
        Ok(true)
    }
}

/// Handler for TriggerRunCommand (CRITICAL - creates jobs from templates)
#[derive(Clone)]
pub struct TriggerRunHandler {
    template_repo: Arc<dyn JobTemplateRepository>,
    execution_repo: Arc<dyn JobExecutionRepository>,
    event_bus: Arc<dyn EventBus>,
    read_model: Arc<dyn TemplateReadModelPort>,
    execution_read_model: Arc<dyn ExecutionReadModelPort>,
}

impl TriggerRunHandler {
    pub fn new(
        template_repo: Arc<dyn JobTemplateRepository>,
        execution_repo: Arc<dyn JobExecutionRepository>,
        event_bus: Arc<dyn EventBus>,
        read_model: Arc<dyn TemplateReadModelPort>,
        execution_read_model: Arc<dyn ExecutionReadModelPort>,
    ) -> Self {
        Self {
            template_repo,
            execution_repo,
            event_bus,
            read_model,
            execution_read_model,
        }
    }
}

#[async_trait::async_trait]
impl CommandHandler<TriggerRunCommand> for TriggerRunHandler {
    #[instrument(skip(self, command))]
    async fn handle(&self, command: TriggerRunCommand) -> Result<ExecutionResult> {
        info!("Triggering run for template: {}", command.template_id);

        // Load template
        let mut template = self
            .template_repo
            .find_by_id(&command.template_id)
            .await?
            .ok_or_else(|| {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Template not found: {}", command.template_id),
                }
            })?;

        // Validate template is active
        if !template.can_create_run() {
            return Err(
                hodei_server_domain::shared_kernel::DomainError::TemplateParameterValidationError {
                    message: format!("Template is not active: status is {}", template.status),
                },
            );
        }

        // Get next execution number
        let execution_number = self
            .execution_repo
            .get_next_execution_number(&command.template_id)
            .await?;

        // Generate job name
        let job_name = command
            .job_name
            .clone()
            .unwrap_or_else(|| format!("{}-run-{}", template.name, execution_number));

        // Create job execution
        let mut execution = JobExecution::new(
            &template,
            execution_number,
            job_name.clone(),
            command.triggered_by.clone(),
        );

        execution.triggered_by_user = Some(command.triggered_by_user.clone());
        execution.parameters = command.parameters.clone();

        // Save execution
        self.execution_repo.save(&execution).await?;

        // Create the actual job from template
        let job = template.create_run().map_err(|e| {
            hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                message: format!("Failed to create job from template: {}", e),
            }
        })?;

        // Extract job_id before job is consumed and before execution is moved
        let job_id = job.id.clone();
        let job_id_for_result = job_id.clone();
        let job_id_for_event = job_id_for_result.clone();

        // Link execution to job
        execution.job_id = Some(job_id);

        // Update execution with job_id
        self.execution_repo.update(&execution).await?;

        // Publish TemplateRunCreated event
        let event = DomainEvent::TemplateRunCreated {
            template_id: template.id.to_string(),
            template_name: template.name.clone(),
            execution_id: execution.id.to_string(),
            job_id: Some(job_id_for_event),
            job_name: job_name.clone(),
            template_version: template.version,
            execution_number,
            triggered_by: command.triggered_by.to_string(),
            triggered_by_user: Some(command.triggered_by_user),
            occurred_at: execution.queued_at,
            correlation_id: None,
            actor: None,
        };
        self.event_bus.publish(&event).await?;

        // Publish JobCreated event (for job queue processing)
        // The job is created from the template and ready for processing
        let job_spec = job.spec.clone();
        let job_created_event = DomainEvent::JobCreated(JobCreated {
            job_id: job_id_for_result.clone(),
            spec: job_spec,
            occurred_at: execution.queued_at,
            correlation_id: Some(execution.id.to_string()),
            actor: execution.triggered_by_user.clone(),
        });
        self.event_bus
            .publish(&job_created_event)
            .await
            .map_err(
                |e| hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Failed to publish JobCreated event: {}", e),
                },
            )?;

        // Update read models
        self.read_model
            .increment_run_count(&command.template_id)
            .await;
        self.execution_read_model.create_execution(&execution).await;

        debug!(
            "Triggered run: execution={}, job={}",
            execution.id, job_id_for_result
        );

        Ok(ExecutionResult {
            execution_id: execution.id,
            job_id: Some(job_id_for_result),
            job_name,
            state: execution.state,
            template_id: template.id,
            template_version: template.version,
            triggered_by: command.triggered_by,
            queued_at: execution.queued_at,
        })
    }
}

/// Handler for DisableTemplateCommand
#[derive(Clone)]
pub struct DisableTemplateHandler {
    template_repo: Arc<dyn JobTemplateRepository>,
    event_bus: Arc<dyn EventBus>,
    read_model: Arc<dyn TemplateReadModelPort>,
}

impl DisableTemplateHandler {
    pub fn new(
        template_repo: Arc<dyn JobTemplateRepository>,
        event_bus: Arc<dyn EventBus>,
        read_model: Arc<dyn TemplateReadModelPort>,
    ) -> Self {
        Self {
            template_repo,
            event_bus,
            read_model,
        }
    }
}

#[async_trait::async_trait]
impl CommandHandler<DisableTemplateCommand> for DisableTemplateHandler {
    async fn handle(&self, command: DisableTemplateCommand) -> Result<TemplateResult> {
        let mut template = self
            .template_repo
            .find_by_id(&command.template_id)
            .await?
            .ok_or_else(|| {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Template not found: {}", command.template_id),
                }
            })?;

        template.disable();
        self.template_repo.update(&template).await?;

        let event = DomainEvent::TemplateDisabled {
            template_id: template.id.to_string(),
            template_name: template.name.clone(),
            version: template.version,
            occurred_at: template.updated_at,
            correlation_id: None,
            actor: Some(command.disabled_by),
        };
        self.event_bus.publish(&event).await?;
        self.read_model.update(&template).await;

        Ok(TemplateResult {
            template_id: template.id,
            name: template.name,
            version: template.version,
            status: template.status.to_string(),
            created_at: template.created_at,
        })
    }
}

/// Handler for EnableTemplateCommand
#[derive(Clone)]
pub struct EnableTemplateHandler {
    template_repo: Arc<dyn JobTemplateRepository>,
    event_bus: Arc<dyn EventBus>,
    read_model: Arc<dyn TemplateReadModelPort>,
}

impl EnableTemplateHandler {
    pub fn new(
        template_repo: Arc<dyn JobTemplateRepository>,
        event_bus: Arc<dyn EventBus>,
        read_model: Arc<dyn TemplateReadModelPort>,
    ) -> Self {
        Self {
            template_repo,
            event_bus,
            read_model,
        }
    }
}

#[async_trait::async_trait]
impl CommandHandler<EnableTemplateCommand> for EnableTemplateHandler {
    async fn handle(&self, command: EnableTemplateCommand) -> Result<TemplateResult> {
        let mut template = self
            .template_repo
            .find_by_id(&command.template_id)
            .await?
            .ok_or_else(|| {
                hodei_server_domain::shared_kernel::DomainError::InfrastructureError {
                    message: format!("Template not found: {}", command.template_id),
                }
            })?;

        template.enable();
        self.template_repo.update(&template).await?;

        // Publish re-enablement event (using TemplateCreated as proxy)
        let enabled_by = command.enabled_by.clone();
        let event = DomainEvent::TemplateCreated {
            template_id: template.id.to_string(),
            template_name: template.name.clone(),
            version: template.version,
            created_by: Some(enabled_by.clone()),
            spec_summary: format!("Re-enabled v{}", template.version),
            occurred_at: template.updated_at,
            correlation_id: None,
            actor: Some(enabled_by),
        };
        self.event_bus.publish(&event).await?;
        self.read_model.update(&template).await;

        Ok(TemplateResult {
            template_id: template.id,
            name: template.name,
            version: template.version,
            status: template.status.to_string(),
            created_at: template.created_at,
        })
    }
}

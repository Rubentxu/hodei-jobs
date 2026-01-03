//! Template Commands for Event-Driven Architecture
//!
//! This module defines all commands for template management operations.
//! Commands represent intent to change state and are dispatched via CommandBus.

use hodei_server_domain::jobs::templates::{
    JobExecutionStatus, JobTemplateId, JobTemplateParameter, TriggerType,
};
use hodei_server_domain::shared_kernel::{JobId, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Command to create a new job template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTemplateCommand {
    /// Unique template name
    pub name: String,
    /// Template description
    pub description: Option<String>,
    /// Job specification
    pub spec: serde_json::Value,
    /// Labels for categorization
    pub labels: HashMap<String, String>,
    /// User creating the template
    pub created_by: String,
    /// Template parameters
    pub parameters: Vec<JobTemplateParameter>,
}

impl crate::core::command::Command for CreateTemplateCommand {
    const NAME: &'static str = "CreateTemplate";
    type Result = TemplateResult;
}

impl crate::core::command::ValidatableCommand for CreateTemplateCommand {
    fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(
                hodei_server_domain::shared_kernel::DomainError::TemplateParameterValidationError {
                    message: "Template name cannot be empty".to_string(),
                },
            );
        }
        if self.spec.is_null() || self.spec.as_object().is_none() {
            return Err(
                hodei_server_domain::shared_kernel::DomainError::TemplateParameterValidationError {
                    message: "Template spec cannot be empty".to_string(),
                },
            );
        }
        Ok(())
    }
}

/// Command to update an existing template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTemplateCommand {
    /// Template ID
    pub template_id: JobTemplateId,
    /// New description (optional)
    pub description: Option<String>,
    /// New spec (optional)
    pub spec: Option<serde_json::Value>,
    /// New labels (optional)
    pub labels: Option<HashMap<String, String>>,
    /// Actor making the change
    pub updated_by: String,
}

impl crate::core::command::Command for UpdateTemplateCommand {
    const NAME: &'static str = "UpdateTemplate";
    type Result = TemplateResult;
}

impl crate::core::command::ValidatableCommand for UpdateTemplateCommand {
    fn validate(&self) -> Result<()> {
        if self.template_id.to_string().is_empty() {
            return Err(
                hodei_server_domain::shared_kernel::DomainError::TemplateParameterValidationError {
                    message: "Template ID cannot be empty".to_string(),
                },
            );
        }
        Ok(())
    }
}

/// Command to delete a template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTemplateCommand {
    /// Template ID to delete
    pub template_id: JobTemplateId,
    /// Force delete even with dependencies
    pub force: bool,
    /// Actor making the change
    pub deleted_by: String,
}

impl crate::core::command::Command for DeleteTemplateCommand {
    const NAME: &'static str = "DeleteTemplate";
    type Result = bool;
}

impl crate::core::command::ValidatableCommand for DeleteTemplateCommand {
    fn validate(&self) -> Result<()> {
        if self.template_id.to_string().is_empty() {
            return Err(
                hodei_server_domain::shared_kernel::DomainError::TemplateParameterValidationError {
                    message: "Template ID cannot be empty".to_string(),
                },
            );
        }
        Ok(())
    }
}

/// Command to trigger a job run from a template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerRunCommand {
    /// Template to execute
    pub template_id: JobTemplateId,
    /// Custom job name (optional, auto-generated if None)
    pub job_name: Option<String>,
    /// User triggering the run
    pub triggered_by_user: String,
    /// Parameters to substitute in the template
    pub parameters: HashMap<String, String>,
    /// Trigger source
    pub triggered_by: TriggerType,
}

impl crate::core::command::Command for TriggerRunCommand {
    const NAME: &'static str = "TriggerRun";
    type Result = ExecutionResult;
}

impl crate::core::command::ValidatableCommand for TriggerRunCommand {
    fn validate(&self) -> Result<()> {
        if self.template_id.to_string().is_empty() {
            return Err(
                hodei_server_domain::shared_kernel::DomainError::TemplateParameterValidationError {
                    message: "Template ID cannot be empty".to_string(),
                },
            );
        }
        Ok(())
    }
}

/// Command to disable a template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisableTemplateCommand {
    pub template_id: JobTemplateId,
    pub reason: String,
    pub disabled_by: String,
}

impl crate::core::command::Command for DisableTemplateCommand {
    const NAME: &'static str = "DisableTemplate";
    type Result = TemplateResult;
}

/// Command to enable a template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnableTemplateCommand {
    pub template_id: JobTemplateId,
    pub enabled_by: String,
}

impl crate::core::command::Command for EnableTemplateCommand {
    const NAME: &'static str = "EnableTemplate";
    type Result = TemplateResult;
}

/// Result of a template operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateResult {
    pub template_id: JobTemplateId,
    pub name: String,
    pub version: u32,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Result of a trigger run operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub execution_id: Uuid,
    pub job_id: Option<JobId>,
    pub job_name: String,
    pub state: JobExecutionStatus,
    pub template_id: JobTemplateId,
    pub template_version: u32,
    pub triggered_by: TriggerType,
    pub queued_at: chrono::DateTime<chrono::Utc>,
}

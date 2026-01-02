//! Job Template Domain Model
//!
//! JobTemplate represents a reusable job definition (similar to Jenkins Job).
//! JobRun represents an execution instance of a template (similar to Jenkins Build).
//!
//! This follows the PRD v6.0 pattern where:
//! - JobTemplate: persistent, versioned definition
//! - JobRun (Job): ephemeral execution instance with unique ID

use crate::jobs::{Job, JobSpec};
use crate::shared_kernel::{JobId, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Unique identifier for a JobTemplate
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobTemplateId(pub Uuid);

impl JobTemplateId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for JobTemplateId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for JobTemplateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for JobTemplateId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Status of a JobTemplate
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobTemplateStatus {
    /// Template is active and can be used to create runs
    Active,
    /// Template is disabled and cannot create new runs
    Disabled,
    /// Template is archived (soft delete)
    Archived,
}

impl Default for JobTemplateStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// A reusable job definition
///
/// JobTemplate stores the specification that can be used to create
/// multiple JobRun instances. Similar to a Jenkins Job definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobTemplate {
    /// Unique identifier
    pub id: JobTemplateId,
    /// Human-readable name
    pub name: String,
    /// Description of what this template does
    pub description: Option<String>,
    /// The job specification (command, env, resources, etc.)
    pub spec: JobSpec,
    /// Current status
    pub status: JobTemplateStatus,
    /// Version number (incremented on updates)
    pub version: u32,
    /// Labels for categorization
    pub labels: HashMap<String, String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// User who created the template
    pub created_by: Option<String>,
    /// Total number of runs created from this template
    pub run_count: u64,
    /// Number of successful runs
    pub success_count: u64,
    /// Number of failed runs
    pub failure_count: u64,
}

impl JobTemplate {
    /// Create a new JobTemplate
    pub fn new(name: String, spec: JobSpec) -> Self {
        let now = Utc::now();
        Self {
            id: JobTemplateId::new(),
            name,
            description: None,
            spec,
            status: JobTemplateStatus::Active,
            version: 1,
            labels: HashMap::new(),
            created_at: now,
            updated_at: now,
            created_by: None,
            run_count: 0,
            success_count: 0,
            failure_count: 0,
        }
    }

    /// Create a new JobTemplate with a specific ID
    pub fn with_id(id: JobTemplateId, name: String, spec: JobSpec) -> Self {
        let mut template = Self::new(name, spec);
        template.id = id;
        template
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add a label
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Set created_by
    pub fn with_created_by(mut self, user: impl Into<String>) -> Self {
        self.created_by = Some(user.into());
        self
    }

    /// Check if template can create new runs
    pub fn can_create_run(&self) -> bool {
        matches!(self.status, JobTemplateStatus::Active)
    }

    /// Create a new JobRun (Job) from this template
    pub fn create_run(&mut self) -> Result<Job> {
        if !self.can_create_run() {
            return Err(crate::shared_kernel::DomainError::InvalidProviderConfig {
                message: format!("Template {} is not active", self.id),
            });
        }

        let job_id = JobId::new();
        let mut job = Job::new(job_id, self.spec.clone());

        // Link job to template
        job.metadata_mut()
            .insert("template_id".to_string(), self.id.to_string());
        job.metadata_mut()
            .insert("template_name".to_string(), self.name.clone());
        job.metadata_mut()
            .insert("template_version".to_string(), self.version.to_string());

        self.run_count += 1;
        self.updated_at = Utc::now();

        Ok(job)
    }

    /// Record a successful run
    pub fn record_success(&mut self) {
        self.success_count += 1;
        self.updated_at = Utc::now();
    }

    /// Record a failed run
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.updated_at = Utc::now();
    }

    /// Disable the template
    pub fn disable(&mut self) {
        self.status = JobTemplateStatus::Disabled;
        self.updated_at = Utc::now();
    }

    /// Enable the template
    pub fn enable(&mut self) {
        self.status = JobTemplateStatus::Active;
        self.updated_at = Utc::now();
    }

    /// Archive the template (soft delete)
    pub fn archive(&mut self) {
        self.status = JobTemplateStatus::Archived;
        self.updated_at = Utc::now();
    }

    /// Update the spec (increments version)
    pub fn update_spec(&mut self, spec: JobSpec) {
        self.spec = spec;
        self.version += 1;
        self.updated_at = Utc::now();
    }

    /// Get success rate as percentage (0.0 - 100.0)
    pub fn success_rate(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            return 0.0;
        }
        (self.success_count as f64 / total as f64) * 100.0
    }
}

/// Repository trait for JobTemplate persistence
#[async_trait]
pub trait JobTemplateRepository: Send + Sync {
    /// Save a new template
    async fn save(&self, template: &JobTemplate) -> Result<()>;

    /// Update an existing template
    async fn update(&self, template: &JobTemplate) -> Result<()>;

    /// Find template by ID
    async fn find_by_id(&self, id: &JobTemplateId) -> Result<Option<JobTemplate>>;

    /// Find template by name
    async fn find_by_name(&self, name: &str) -> Result<Option<JobTemplate>>;

    /// List all active templates
    async fn list_active(&self) -> Result<Vec<JobTemplate>>;

    /// List templates by label
    async fn find_by_label(&self, key: &str, value: &str) -> Result<Vec<JobTemplate>>;

    /// Delete template (hard delete)
    async fn delete(&self, id: &JobTemplateId) -> Result<()>;
}

/// Type of trigger for a job execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerType {
    /// Manual trigger from UI/API
    Manual,
    /// Scheduled trigger via cron
    Scheduled,
    /// Triggered via API
    Api,
    /// Triggered via webhook
    Webhook,
    /// Triggered as a retry
    Retry,
}

/// Result of a job execution
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Exit code from the job
    pub exit_code: i32,
    /// Summary of output
    pub output_summary: String,
    /// Error output if any
    pub error_output: String,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

/// Snapshot of resource usage during execution
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceUsageSnapshot {
    /// Peak CPU usage percentage
    pub peak_cpu_percent: f64,
    /// Peak memory usage in MB
    pub peak_memory_mb: u64,
    /// Average CPU usage percentage
    pub avg_cpu_percent: f64,
    /// Average memory usage in MB
    pub avg_memory_mb: u64,
    /// Disk I/O in bytes
    pub disk_io_bytes: u64,
    /// Network I/O in bytes
    pub network_io_bytes: u64,
}

/// Represents an execution instance of a job template
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobExecution {
    /// Unique identifier for this execution
    pub id: Uuid,
    /// Sequential execution number for this template
    pub execution_number: u64,
    /// Template that was executed
    pub template_id: JobTemplateId,
    /// Version of the template at execution time
    pub template_version: u32,
    /// Job created from this execution
    pub job_id: Option<JobId>,
    /// Human-readable job name
    pub job_name: String,
    /// Job specification (potentially with parameters substituted)
    pub job_spec: JobSpec,
    /// Current state of execution
    pub state: JobExecutionStatus,
    /// Result of execution (if completed)
    pub result: Option<ExecutionResult>,
    /// When execution was queued
    pub queued_at: DateTime<Utc>,
    /// When execution started
    pub started_at: Option<DateTime<Utc>>,
    /// When execution completed
    pub completed_at: Option<DateTime<Utc>>,
    /// What triggered this execution
    pub triggered_by: TriggerType,
    /// ID of scheduled job if triggered by scheduler
    pub scheduled_job_id: Option<Uuid>,
    /// User who triggered (if manual/API)
    pub triggered_by_user: Option<String>,
    /// Parameters used for this execution
    pub parameters: HashMap<String, String>,
    /// Resource usage snapshot
    pub resource_usage: Option<ResourceUsageSnapshot>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

impl JobExecution {
    /// Create a new execution from a template
    pub fn new(
        template: &JobTemplate,
        execution_number: u64,
        job_name: String,
        triggered_by: TriggerType,
    ) -> Self {
        let job_spec = template.spec.clone();
        Self {
            id: Uuid::new_v4(),
            execution_number,
            template_id: template.id.clone(),
            template_version: template.version,
            job_id: None,
            job_name,
            job_spec,
            state: JobExecutionStatus::Queued,
            result: None,
            queued_at: Utc::now(),
            started_at: None,
            completed_at: None,
            triggered_by,
            scheduled_job_id: None,
            triggered_by_user: None,
            parameters: HashMap::new(),
            resource_usage: None,
            metadata: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// Create a JobExecution from an existing Job
    pub fn from_job(
        job: &Job,
        template_id: JobTemplateId,
        template_version: u32,
        execution_number: u64,
    ) -> Self {
        let job_name = job
            .metadata()
            .get("job_name")
            .cloned()
            .unwrap_or_else(|| format!("job-{}", &job.id));

        let triggered_by = if job.metadata().contains_key("scheduled_job_id") {
            TriggerType::Scheduled
        } else if job.metadata().contains_key("retry_of") {
            TriggerType::Retry
        } else {
            TriggerType::Manual
        };

        let scheduled_job_id = job
            .metadata()
            .get("scheduled_job_id")
            .and_then(|s| Uuid::parse_str(s).ok());

        let triggered_by_user = job.metadata().get("triggered_by_user").cloned();

        Self {
            id: Uuid::new_v4(),
            execution_number,
            template_id,
            template_version,
            job_id: Some(job.id.clone()),
            job_name,
            job_spec: job.spec.clone(),
            state: map_job_state_to_execution_status(job.state()),
            result: job.result().map(|r| match r {
                crate::shared_kernel::JobResult::Success {
                    exit_code,
                    output,
                    error_output,
                } => ExecutionResult {
                    exit_code: *exit_code,
                    output_summary: output.clone(),
                    error_output: error_output.clone(),
                    duration_ms: job
                        .execution_duration()
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0),
                },
                crate::shared_kernel::JobResult::Failed {
                    exit_code,
                    error_message,
                    error_output,
                } => ExecutionResult {
                    exit_code: *exit_code,
                    output_summary: error_message.clone(),
                    error_output: error_output.clone(),
                    duration_ms: job
                        .execution_duration()
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0),
                },
                crate::shared_kernel::JobResult::Cancelled => ExecutionResult {
                    exit_code: -1,
                    output_summary: "Cancelled".to_string(),
                    error_output: "".to_string(),
                    duration_ms: job
                        .execution_duration()
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0),
                },
                crate::shared_kernel::JobResult::Timeout => ExecutionResult {
                    exit_code: -2,
                    output_summary: "Timeout".to_string(),
                    error_output: "".to_string(),
                    duration_ms: job
                        .execution_duration()
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0),
                },
            }),
            queued_at: *job.created_at(),
            started_at: job.started_at().cloned(),
            completed_at: job.completed_at().cloned(),
            triggered_by,
            scheduled_job_id,
            triggered_by_user,
            parameters: HashMap::new(),
            resource_usage: None,
            metadata: job.metadata().clone(),
            created_at: Utc::now(),
        }
    }

    /// Mark execution as started
    pub fn start(&mut self) {
        self.state = JobExecutionStatus::Running;
        self.started_at = Some(Utc::now());
    }

    /// Complete execution with result
    pub fn complete(&mut self, result: ExecutionResult) {
        self.state = match result.exit_code {
            0 => JobExecutionStatus::Succeeded,
            _ => JobExecutionStatus::Failed,
        };
        self.result = Some(result);
        self.completed_at = Some(Utc::now());
    }

    /// Mark execution as failed
    pub fn fail(&mut self, error_output: String, duration_ms: u64) {
        self.state = JobExecutionStatus::Failed;
        self.result = Some(ExecutionResult {
            exit_code: -1,
            output_summary: "Failed".to_string(),
            error_output,
            duration_ms,
        });
        self.completed_at = Some(Utc::now());
    }

    /// Get duration of execution
    pub fn duration_ms(&self) -> Option<u64> {
        if let (Some(start), Some(end)) = (self.started_at, self.completed_at) {
            let duration = end.signed_duration_since(start);
            Some(duration.num_milliseconds() as u64)
        } else {
            None
        }
    }
}

/// Status of a scheduled job
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduledJobStatus {
    /// Scheduled job is active and will run
    Enabled,
    /// Scheduled job is paused
    Disabled,
    /// Scheduled job has encountered errors
    Error,
    /// Scheduled job is temporarily paused due to failures
    Paused,
}

/// Represents a cron-scheduled job
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduledJob {
    /// Unique identifier
    pub id: Uuid,
    /// Human-readable name
    pub name: String,
    /// Description of the scheduled job
    pub description: Option<String>,
    /// Template to execute
    pub template_id: JobTemplateId,
    /// Cron expression for scheduling
    pub cron_expression: String,
    /// Timezone for cron expression
    pub timezone: String,
    /// Next execution time (calculated)
    pub next_execution_at: DateTime<Utc>,
    /// Last execution time
    pub last_execution_at: Option<DateTime<Utc>>,
    /// Status of last execution
    pub last_execution_status: Option<JobExecutionStatus>,
    /// Whether the scheduled job is enabled
    pub enabled: bool,
    /// Maximum consecutive failures before pausing
    pub max_consecutive_failures: u32,
    /// Current consecutive failure count
    pub consecutive_failures: u32,
    /// Whether to pause on failure
    pub pause_on_failure: bool,
    /// Default parameters for job executions
    pub parameters: HashMap<String, String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// User who created the scheduled job
    pub created_by: Option<String>,
}

impl ScheduledJob {
    /// Create a new scheduled job
    pub fn new(
        name: String,
        template_id: JobTemplateId,
        cron_expression: String,
        timezone: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            description: None,
            template_id,
            cron_expression,
            timezone,
            next_execution_at: now,
            last_execution_at: None,
            last_execution_status: None,
            enabled: true,
            max_consecutive_failures: 3,
            consecutive_failures: 0,
            pause_on_failure: false,
            parameters: HashMap::new(),
            created_at: now,
            updated_at: now,
            created_by: None,
        }
    }

    /// Check if the job should run now
    pub fn should_run_now(&self, now: DateTime<Utc>) -> bool {
        self.enabled && now >= self.next_execution_at
    }

    /// Mark that the job has been triggered
    pub fn mark_triggered(&mut self) {
        self.last_execution_at = Some(Utc::now());
        self.consecutive_failures = 0;
        // Note: next_execution_at should be calculated by the scheduler service
        // based on the cron expression
    }

    /// Mark execution as successful
    pub fn mark_success(&mut self) {
        self.last_execution_status = Some(JobExecutionStatus::Succeeded);
        self.consecutive_failures = 0;
        self.updated_at = Utc::now();
    }

    /// Mark execution as failed
    pub fn mark_failed(&mut self) {
        self.last_execution_status = Some(JobExecutionStatus::Failed);
        self.consecutive_failures += 1;
        self.updated_at = Utc::now();

        // Pause if max consecutive failures reached and pause_on_failure is true
        if self.pause_on_failure && self.consecutive_failures >= self.max_consecutive_failures {
            self.enabled = false;
        }
    }

    /// Disable the scheduled job
    pub fn disable(&mut self) {
        self.enabled = false;
        self.updated_at = Utc::now();
    }

    /// Enable the scheduled job
    pub fn enable(&mut self) {
        self.enabled = true;
        self.updated_at = Utc::now();
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add default parameter
    pub fn with_parameter(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.parameters.insert(key.into(), value.into());
        self
    }

    /// Set created_by
    pub fn with_created_by(mut self, user: impl Into<String>) -> Self {
        self.created_by = Some(user.into());
        self
    }

    /// Set max consecutive failures
    pub fn with_max_consecutive_failures(mut self, max_failures: u32) -> Self {
        self.max_consecutive_failures = max_failures;
        self
    }

    /// Set pause on failure
    pub fn with_pause_on_failure(mut self, pause: bool) -> Self {
        self.pause_on_failure = pause;
        self
    }
}

/// Status of a job execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobExecutionStatus {
    /// Execution is queued
    Queued,
    /// Execution is running
    Running,
    /// Execution succeeded
    Succeeded,
    /// Execution failed
    Failed,
    /// Execution encountered an error
    Error,
}

/// Type of a template parameter
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParameterType {
    /// String parameter
    String,
    /// Number parameter
    Number,
    /// Boolean parameter
    Boolean,
    /// Choice parameter (enum)
    Choice,
    /// Secret parameter (password, token, etc.)
    Secret,
}

/// Represents a parameter for a job template
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobTemplateParameter {
    /// Unique identifier
    pub id: Uuid,
    /// Template this parameter belongs to
    pub template_id: JobTemplateId,
    /// Parameter name
    pub name: String,
    /// Type of the parameter
    pub parameter_type: ParameterType,
    /// Description of the parameter
    pub description: String,
    /// Whether this parameter is required
    pub required: bool,
    /// Default value (if any)
    pub default_value: Option<String>,
    /// Validation pattern (regex) for string parameters
    pub validation_pattern: Option<String>,
    /// Minimum value for number parameters
    pub min_value: Option<f64>,
    /// Maximum value for number parameters
    pub max_value: Option<f64>,
    /// Allowed choices for choice parameters
    pub choices: Vec<String>,
    /// Display order in UI
    pub display_order: u32,
    /// Whether this is a secret parameter
    pub secret: bool,
}

impl JobTemplateParameter {
    /// Create a new parameter
    pub fn new(template_id: JobTemplateId, name: String, parameter_type: ParameterType) -> Self {
        Self {
            id: Uuid::new_v4(),
            template_id,
            name,
            parameter_type,
            description: String::new(),
            required: false,
            default_value: None,
            validation_pattern: None,
            min_value: None,
            max_value: None,
            choices: Vec::new(),
            display_order: 0,
            secret: false,
        }
    }

    /// Validate a value against this parameter's constraints
    pub fn validate(&self, value: &str) -> crate::shared_kernel::Result<()> {
        // Required check
        if self.required && (value.is_empty() || value.trim().is_empty()) {
            return Err(ValidationError::Required {
                parameter_name: self.name.clone(),
            }
            .into());
        }

        // If value is empty and not required, it's valid
        if value.is_empty() {
            return Ok(());
        }

        // Type-specific validation
        match self.parameter_type {
            ParameterType::String => {
                // Validate pattern if present
                if let Some(pattern) = &self.validation_pattern {
                    let regex = ::regex::Regex::new(pattern).map_err(|_| {
                        ValidationError::InvalidPattern {
                            pattern: pattern.clone(),
                        }
                    })?;

                    if !regex.is_match(value) {
                        return Err(ValidationError::PatternMismatch {
                            parameter_name: self.name.clone(),
                            value: value.to_string(),
                        }
                        .into());
                    }
                }
            }
            ParameterType::Number => {
                // Parse as number
                let num: std::result::Result<f64, _> = value.parse();
                match num {
                    Ok(parsed) => {
                        // Range validation
                        if let Some(min) = self.min_value {
                            if parsed < min {
                                return Err(ValidationError::OutOfRange {
                                    parameter_name: self.name.clone(),
                                    value: value.to_string(),
                                    min: Some(min),
                                    max: self.max_value,
                                }
                                .into());
                            }
                        }

                        if let Some(max) = self.max_value {
                            if parsed > max {
                                return Err(ValidationError::OutOfRange {
                                    parameter_name: self.name.clone(),
                                    value: value.to_string(),
                                    min: self.min_value,
                                    max: Some(max),
                                }
                                .into());
                            }
                        }
                    }
                    Err(_) => {
                        return Err(ValidationError::InvalidType {
                            parameter_name: self.name.clone(),
                            expected: "number".to_string(),
                            actual: value.to_string(),
                        }
                        .into());
                    }
                }
            }
            ParameterType::Boolean => {
                // Validate as boolean
                if value != "true" && value != "false" {
                    return Err(ValidationError::InvalidType {
                        parameter_name: self.name.clone(),
                        expected: "boolean".to_string(),
                        actual: value.to_string(),
                    }
                    .into());
                }
            }
            ParameterType::Choice => {
                // Validate against allowed choices
                if !self.choices.contains(&value.to_string()) {
                    return Err(ValidationError::InvalidChoice {
                        parameter_name: self.name.clone(),
                        value: value.to_string(),
                        allowed_choices: self.choices.clone(),
                    }
                    .into());
                }
            }
            ParameterType::Secret => {
                // Secrets just need to be non-empty
                if value.is_empty() {
                    return Err(ValidationError::Required {
                        parameter_name: self.name.clone(),
                    }
                    .into());
                }
            }
        }

        Ok(())
    }
}

/// Validation error for template parameters
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ValidationError {
    /// Required parameter is missing
    Required { parameter_name: String },
    /// Invalid type
    InvalidType {
        parameter_name: String,
        expected: String,
        actual: String,
    },
    /// Value out of range
    OutOfRange {
        parameter_name: String,
        value: String,
        min: Option<f64>,
        max: Option<f64>,
    },
    /// Invalid choice
    InvalidChoice {
        parameter_name: String,
        value: String,
        allowed_choices: Vec<String>,
    },
    /// Pattern mismatch
    PatternMismatch {
        parameter_name: String,
        value: String,
    },
    /// Invalid regex pattern
    InvalidPattern { pattern: String },
}

impl From<ValidationError> for crate::shared_kernel::DomainError {
    fn from(error: ValidationError) -> Self {
        match error {
            ValidationError::Required { parameter_name } => {
                crate::shared_kernel::DomainError::TemplateParameterValidationError {
                    message: format!("Required parameter '{}' is missing", parameter_name),
                }
            }
            ValidationError::InvalidType {
                parameter_name,
                expected,
                actual,
            } => crate::shared_kernel::DomainError::TemplateParameterValidationError {
                message: format!(
                    "Parameter '{}' expects {} but got {}",
                    parameter_name, expected, actual
                ),
            },
            ValidationError::OutOfRange {
                parameter_name,
                value,
                min,
                max,
            } => {
                let range_str = match (min, max) {
                    (Some(min), Some(max)) => format!("between {} and {}", min, max),
                    (Some(min), None) => format!(">= {}", min),
                    (None, Some(max)) => format!("<= {}", max),
                    (None, None) => "no range".to_string(),
                };
                crate::shared_kernel::DomainError::TemplateParameterValidationError {
                    message: format!(
                        "Parameter '{}' value '{}' is out of range {}",
                        parameter_name, value, range_str
                    ),
                }
            }
            ValidationError::InvalidChoice {
                parameter_name,
                value,
                allowed_choices,
            } => crate::shared_kernel::DomainError::TemplateParameterValidationError {
                message: format!(
                    "Parameter '{}' value '{}' is not in allowed choices: {:?}",
                    parameter_name, value, allowed_choices
                ),
            },
            ValidationError::PatternMismatch {
                parameter_name,
                value,
            } => crate::shared_kernel::DomainError::TemplateParameterValidationError {
                message: format!(
                    "Parameter '{}' value '{}' does not match validation pattern",
                    parameter_name, value
                ),
            },
            ValidationError::InvalidPattern { pattern } => {
                crate::shared_kernel::DomainError::TemplateParameterValidationError {
                    message: format!("Invalid validation pattern: {}", pattern),
                }
            }
        }
    }
}

/// Repository trait for JobExecutions
#[async_trait]
pub trait JobExecutionRepository: Send + Sync {
    /// Save a new execution
    async fn save(&self, execution: &JobExecution) -> Result<()>;

    /// Update an existing execution
    async fn update(&self, execution: &JobExecution) -> Result<()>;

    /// Find execution by ID
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<JobExecution>>;

    /// Find executions by template ID
    async fn find_by_template_id(
        &self,
        template_id: &JobTemplateId,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<JobExecution>, usize)>;

    /// Find executions by job ID
    async fn find_by_job_id(&self, job_id: &JobId) -> Result<Option<JobExecution>>;

    /// Find executions by scheduled job ID
    async fn find_by_scheduled_job_id(
        &self,
        scheduled_job_id: &Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<JobExecution>, usize)>;

    /// Get the next execution number for a template
    async fn get_next_execution_number(&self, template_id: &JobTemplateId) -> Result<u64>;

    /// Delete executions older than a date
    async fn delete_older_than(&self, date: &DateTime<Utc>) -> Result<u64>;
}

/// Repository trait for ScheduledJobs
#[async_trait]
pub trait ScheduledJobRepository: Send + Sync {
    /// Save a new scheduled job
    async fn save(&self, scheduled_job: &ScheduledJob) -> Result<()>;

    /// Update an existing scheduled job
    async fn update(&self, scheduled_job: &ScheduledJob) -> Result<()>;

    /// Find scheduled job by ID
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<ScheduledJob>>;

    /// Find scheduled jobs by template ID
    async fn find_by_template_id(&self, template_id: &JobTemplateId) -> Result<Vec<ScheduledJob>>;

    /// List all enabled scheduled jobs
    async fn list_enabled(&self) -> Result<Vec<ScheduledJob>>;

    /// Find scheduled jobs that should run now
    async fn find_ready_to_run(&self, now: &DateTime<Utc>) -> Result<Vec<ScheduledJob>>;

    /// Delete scheduled job
    async fn delete(&self, id: &Uuid) -> Result<()>;
}

/// Repository trait for JobTemplateParameters
#[async_trait]
pub trait JobTemplateParameterRepository: Send + Sync {
    /// Save a new parameter
    async fn save(&self, parameter: &JobTemplateParameter) -> Result<()>;

    /// Update an existing parameter
    async fn update(&self, parameter: &JobTemplateParameter) -> Result<()>;

    /// Find parameter by ID
    async fn find_by_id(&self, id: &Uuid) -> Result<Option<JobTemplateParameter>>;

    /// Find parameters by template ID
    async fn find_by_template_id(
        &self,
        template_id: &JobTemplateId,
    ) -> Result<Vec<JobTemplateParameter>>;

    /// Delete parameter
    async fn delete(&self, id: &Uuid) -> Result<()>;

    /// Delete all parameters for a template
    async fn delete_by_template_id(&self, template_id: &JobTemplateId) -> Result<()>;
}

/// Helper to map JobState to JobExecutionStatus
fn map_job_state_to_execution_status(state: &crate::shared_kernel::JobState) -> JobExecutionStatus {
    match state {
        crate::shared_kernel::JobState::Pending => JobExecutionStatus::Queued,
        crate::shared_kernel::JobState::Assigned => JobExecutionStatus::Queued,
        crate::shared_kernel::JobState::Scheduled => JobExecutionStatus::Queued,
        crate::shared_kernel::JobState::Running => JobExecutionStatus::Running,
        crate::shared_kernel::JobState::Succeeded => JobExecutionStatus::Succeeded,
        crate::shared_kernel::JobState::Failed => JobExecutionStatus::Failed,
        crate::shared_kernel::JobState::Cancelled => JobExecutionStatus::Failed,
        crate::shared_kernel::JobState::Timeout => JobExecutionStatus::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::CommandType;

    fn create_test_spec() -> JobSpec {
        JobSpec::with_command(CommandType::shell("echo hello"))
    }

    #[test]
    fn test_job_template_creation() {
        let spec = create_test_spec();
        let template = JobTemplate::new("test-job".to_string(), spec);

        assert_eq!(template.name, "test-job");
        assert_eq!(template.version, 1);
        assert!(matches!(template.status, JobTemplateStatus::Active));
        assert_eq!(template.run_count, 0);
    }

    #[test]
    fn test_job_template_with_builder() {
        let spec = create_test_spec();
        let template = JobTemplate::new("my-job".to_string(), spec)
            .with_description("A test job")
            .with_label("team", "platform")
            .with_created_by("admin");

        assert_eq!(template.description, Some("A test job".to_string()));
        assert_eq!(template.labels.get("team"), Some(&"platform".to_string()));
        assert_eq!(template.created_by, Some("admin".to_string()));
    }

    #[test]
    fn test_create_run_from_template() {
        let spec = create_test_spec();
        let mut template = JobTemplate::new("test-job".to_string(), spec);

        let job = template.create_run().expect("should create run");

        assert_eq!(template.run_count, 1);
        assert_eq!(
            job.metadata().get("template_id"),
            Some(&template.id.to_string())
        );
        assert_eq!(
            job.metadata().get("template_name"),
            Some(&"test-job".to_string())
        );
    }

    #[test]
    fn test_disabled_template_cannot_create_run() {
        let spec = create_test_spec();
        let mut template = JobTemplate::new("test-job".to_string(), spec);
        template.disable();

        let result = template.create_run();
        assert!(result.is_err());
    }

    #[test]
    fn test_update_spec_increments_version() {
        let spec = create_test_spec();
        let mut template = JobTemplate::new("test-job".to_string(), spec);
        assert_eq!(template.version, 1);

        let new_spec = JobSpec::with_command(CommandType::shell("echo world"));
        template.update_spec(new_spec);

        assert_eq!(template.version, 2);
    }

    #[test]
    fn test_success_rate_calculation() {
        let spec = create_test_spec();
        let mut template = JobTemplate::new("test-job".to_string(), spec);

        // No runs yet
        assert_eq!(template.success_rate(), 0.0);

        // 3 successes, 1 failure = 75%
        template.success_count = 3;
        template.failure_count = 1;
        assert!((template.success_rate() - 75.0).abs() < 0.01);
    }

    #[test]
    fn test_template_lifecycle() {
        let spec = create_test_spec();
        let mut template = JobTemplate::new("test-job".to_string(), spec);

        assert!(template.can_create_run());

        template.disable();
        assert!(!template.can_create_run());

        template.enable();
        assert!(template.can_create_run());

        template.archive();
        assert!(!template.can_create_run());
    }

    #[test]
    fn test_job_template_id_parsing() {
        let id = JobTemplateId::new();
        let id_str = id.to_string();
        let parsed: JobTemplateId = id_str.parse().expect("should parse");
        assert_eq!(id, parsed);
    }
}

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
        job.metadata
            .insert("template_id".to_string(), self.id.to_string());
        job.metadata
            .insert("template_name".to_string(), self.name.clone());
        job.metadata
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
            job.metadata.get("template_id"),
            Some(&template.id.to_string())
        );
        assert_eq!(
            job.metadata.get("template_name"),
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

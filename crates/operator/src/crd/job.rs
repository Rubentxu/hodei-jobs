//! Job CRD - Invokes scheduler.QueueJob() via gRPC

use super::*;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Job CRD - Creates a job by calling gRPC scheduler
#[derive(CustomResource, Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[kube(
    group = "hodei.io",
    version = "v1alpha1",
    kind = "Job",
    namespaced,
    status = "JobStatus",
    shortname = "j"
)]
pub struct JobSpec {
    /// Command to execute (required)
    pub command: String,

    /// Arguments for the command
    #[serde(default)]
    pub arguments: Option<Vec<String>>,

    /// Environment variables
    #[serde(default)]
    pub environment: Option<HashMap<String, String>>,

    /// Container image to use
    #[serde(default)]
    pub image: Option<String>,

    /// Working directory
    #[serde(default)]
    pub working_dir: Option<String>,

    /// Resource requirements
    #[serde(default)]
    pub resources: Option<ResourceRequirements>,

    /// Timeout in milliseconds
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,

    /// Number of retries
    #[serde(default = "default_retries")]
    pub retries: i32,

    /// Template reference (optional)
    #[serde(skip_serializing_if = "Option::is_none", rename = "templateRef")]
    pub template_ref: Option<TemplateRef>,

    /// Parameters for template execution
    #[serde(default)]
    pub parameters: Option<HashMap<String, String>>,

    /// Job preferences
    #[serde(default)]
    pub preferences: Option<JobPreferences>,

    /// Labels to apply to the job
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

fn default_timeout() -> u64 {
    3600000 // 1 hour
}

fn default_retries() -> i32 {
    3
}

/// Template reference
#[derive(Debug, Clone, Deserialize, JsonSchema, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TemplateRef {
    pub name: String,
}

/// JobStatus defines the observed state of Job
#[derive(Debug, Clone, Default, Deserialize, JsonSchema, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    #[serde(default)]
    pub phase: JobPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "executionId")]
    pub execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "startTime")]
    pub start_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "endTime")]
    pub end_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "queuedTime")]
    pub queued_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "durationSeconds")]
    pub duration_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "exitCode")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub observed_generation: i64,
}

/// Job phase enumeration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum JobPhase {
    #[default]
    Pending,
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Timeout,
}

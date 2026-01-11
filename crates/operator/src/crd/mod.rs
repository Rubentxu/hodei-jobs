//! CRD Definitions for Hodei Operator
//!
//! These CRDs provide a Kubernetes-native interface to the Hodei Jobs Platform.
//! When a CRD is created/updated, the operator translates it into gRPC calls
//! to the Hodei Server.

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

mod job;
mod provider_config;
mod worker_pool;

pub use job::{Job, JobPhase, JobSpec, JobStatus};
pub use provider_config::{ProviderConfig, ProviderConfigSpec, ProviderConfigStatus};
pub use worker_pool::{WorkerPool, WorkerPoolSpec};

/// Resource requirements for jobs
#[derive(Debug, Clone, Default, Deserialize, JsonSchema, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRequirements {
    #[serde(default)]
    pub cpu_cores: f64,
    #[serde(default)]
    pub memory_mb: i64,
    #[serde(default)]
    pub storage_mb: i64,
    #[serde(default)]
    pub gpu_required: bool,
    #[serde(default)]
    pub architecture: String,
}

/// Environment variable
#[derive(Debug, Clone, Deserialize, JsonSchema, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// Job preferences
#[derive(Debug, Clone, Default, Deserialize, JsonSchema, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(default)]
    pub allow_retry: bool,
}

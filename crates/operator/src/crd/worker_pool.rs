//! WorkerPool CRD - Template for K8s provider configuration

use super::*;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// WorkerPool CRD
#[derive(CustomResource, Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[kube(
    group = "hodei.io",
    version = "v1alpha1",
    kind = "WorkerPool",
    namespaced,
    status = "WorkerPoolStatus",
    shortname = "wp"
)]
pub struct WorkerPoolSpec {
    #[serde(rename = "provider")]
    pub provider_type: String,

    #[serde(default = "default_min_replicas")]
    pub min_replicas: i32,

    #[serde(default = "default_max_replicas")]
    pub max_replicas: i32,

    #[serde(default = "default_image")]
    pub image: String,

    #[serde(default)]
    pub image_pull_policy: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub kubernetes: Option<KubernetesWorkerConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourceRequirements>,

    #[serde(default)]
    pub node_selector: HashMap<String, String>,

    #[serde(default)]
    pub labels: HashMap<String, String>,
}

fn default_min_replicas() -> i32 {
    0
}

fn default_max_replicas() -> i32 {
    10
}

fn default_image() -> String {
    "hodei/worker:latest".to_string()
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KubernetesWorkerConfig {
    #[serde(default)]
    pub namespace: String,
    #[serde(default)]
    pub service_account: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_pull_secret: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkerPoolStatus {
    #[serde(default)]
    pub available_replicas: i32,
    #[serde(default)]
    pub busy_replicas: i32,
    #[serde(default)]
    pub observed_generation: i64,
}

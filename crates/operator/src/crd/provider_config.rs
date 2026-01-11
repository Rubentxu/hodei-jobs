//! ProviderConfig CRD - Provider configuration for Hodei

use super::*;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ProviderConfig CRD - Cluster-scoped provider configuration
#[derive(CustomResource, Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[kube(
    group = "hodei.io",
    version = "v1alpha1",
    kind = "ProviderConfig",
    namespaced = false,
    status = "ProviderConfigStatus",
    shortname = "pc"
)]
pub struct ProviderConfigSpec {
    #[serde(rename = "type")]
    pub provider_type: String,

    pub name: String,

    #[serde(default = "default_priority")]
    pub priority: i32,

    #[serde(default = "default_max_workers")]
    pub max_workers: u32,

    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default)]
    pub metadata: HashMap<String, String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker: Option<DockerProviderConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub kubernetes: Option<KubernetesProviderConfig>,
}

fn default_priority() -> i32 {
    100
}

fn default_max_workers() -> u32 {
    10
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DockerProviderConfig {
    #[serde(default = "default_docker_socket")]
    pub socket_path: String,
    #[serde(default)]
    pub default_image: String,
}

fn default_docker_socket() -> String {
    "/var/run/docker.sock".to_string()
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KubernetesProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kubeconfig: Option<String>,
    #[serde(default = "default_k8s_namespace")]
    pub namespace: String,
    #[serde(default)]
    pub service_account: String,
}

fn default_k8s_namespace() -> String {
    "hodei-workers".to_string()
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigStatus {
    #[serde(default)]
    pub registered: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub active_workers: u32,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub observed_generation: i64,
}

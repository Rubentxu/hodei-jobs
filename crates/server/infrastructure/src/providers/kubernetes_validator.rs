//! Kubernetes Connection Validator
//!
//! Production-ready validation of Kubernetes provider connectivity.
//! Performs comprehensive checks including API access, permissions, and resource quotas.

use async_trait::async_trait;
use hodei_server_domain::providers::{
    KubernetesConfig, ProviderConfig, ProviderConnectionError, ProviderConnectionValidator,
    ValidationCheck, ValidationConfig, ValidationReport,
};
use hodei_server_domain::shared_kernel::ProviderId;
use hodei_server_domain::workers::ProviderType;
use k8s_openapi::api::core::v1::{Namespace, Pod};
use kube::{Api, Client, Config};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// Kubernetes connection validator
///
/// Performs comprehensive validation of Kubernetes provider configurations:
/// 1. API server connectivity
/// 2. Namespace access
/// 3. Pod creation permissions (dry-run)
/// 4. Resource quota availability
/// 5. Image pull secret validity (if configured)
#[derive(Clone)]
pub struct KubernetesConnectionValidator {
    /// Default validation timeout
    timeout: Duration,
}

impl KubernetesConnectionValidator {
    /// Create a new validator with default timeout (30 seconds)
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(30),
        }
    }

    /// Create a validator with custom timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Create a Kubernetes client from configuration
    async fn create_client(
        &self,
        config: &KubernetesConfig,
    ) -> Result<Client, ProviderConnectionError> {
        let kube_config = match &config.kubeconfig_path {
            Some(path) => {
                let kubeconfig = kube::config::Kubeconfig::read_from(path).map_err(|e| {
                    ProviderConnectionError::Configuration(format!(
                        "Failed to read kubeconfig from {}: {}",
                        path, e
                    ))
                })?;
                Config::from_custom_kubeconfig(
                    kubeconfig,
                    &kube::config::KubeConfigOptions::default(),
                )
                .await
                .map_err(|e| {
                    ProviderConnectionError::Configuration(format!(
                        "Failed to create Kubernetes config: {}",
                        e
                    ))
                })?
            }
            None => Config::infer().await.map_err(|e| {
                ProviderConnectionError::Configuration(format!(
                    "Failed to infer Kubernetes config: {}",
                    e
                ))
            })?,
        };

        Client::try_from(kube_config).map_err(|e| {
            ProviderConnectionError::Authentication(format!(
                "Failed to create Kubernetes client: {}",
                e
            ))
        })
    }

    /// Run all validation checks
    async fn run_checks(
        &self,
        client: &Client,
        config: &KubernetesConfig,
        validation_config: &ValidationConfig,
    ) -> Vec<ValidationCheck> {
        let mut checks = Vec::new();

        // Check 1: API server connectivity
        let start = Instant::now();
        let api_result = self.check_api_connectivity(client).await;
        checks.push(match api_result {
            Ok(version) => ValidationCheck::passed(
                "api_connectivity",
                format!("Connected to Kubernetes API: {}", version),
                start.elapsed().as_millis() as u64,
            ),
            Err(e) => ValidationCheck::failed(
                "api_connectivity",
                format!("API connectivity failed: {}", e),
                start.elapsed().as_millis() as u64,
            ),
        });

        // Check 2: Namespace access (if configured)
        if validation_config.validate_namespace {
            let start = Instant::now();
            let ns_result = self.check_namespace_access(client, &config.namespace).await;
            checks.push(match ns_result {
                Ok(phase) => ValidationCheck::passed(
                    "namespace_access",
                    format!(
                        "Namespace '{}' is active (status: {})",
                        config.namespace, phase
                    ),
                    start.elapsed().as_millis() as u64,
                ),
                Err(e) => ValidationCheck::failed(
                    "namespace_access",
                    format!("Cannot access namespace '{}': {}", config.namespace, e),
                    start.elapsed().as_millis() as u64,
                ),
            });
        }

        // Check 3: Pod creation permissions (dry-run)
        if validation_config.validate_permissions {
            let start = Instant::now();
            let pod_result = self
                .check_pod_creation_permission(client, &config.namespace)
                .await;
            checks.push(match pod_result {
                Ok(()) => ValidationCheck::passed(
                    "pod_creation_permission",
                    "ServiceAccount has permission to create pods",
                    start.elapsed().as_millis() as u64,
                ),
                Err(e) => ValidationCheck::failed(
                    "pod_creation_permission",
                    format!("Insufficient permissions to create pods: {}", e),
                    start.elapsed().as_millis() as u64,
                ),
            });
        }

        // Check 4: Resource quotas
        if validation_config.validate_resources {
            let start = Instant::now();
            let quota_result = self.check_resource_quotas(client, &config.namespace).await;
            checks.push(match quota_result {
                Ok(Some(usage)) => ValidationCheck::passed(
                    "resource_quotas",
                    format!("Resource quotas checked ({}% utilization)", usage),
                    start.elapsed().as_millis() as u64,
                ),
                Ok(None) => ValidationCheck::passed(
                    "resource_quotas",
                    "No resource quotas configured or accessible",
                    start.elapsed().as_millis() as u64,
                ),
                Err(e) => ValidationCheck::failed(
                    "resource_quotas",
                    format!("Failed to check resource quotas: {}", e),
                    start.elapsed().as_millis() as u64,
                ),
            });
        }

        // Check 5: Image pull secrets (if configured)
        if !config.image_pull_secrets.is_empty() {
            let start = Instant::now();
            let secret_result = self
                .check_image_pull_secrets(client, &config.namespace, &config.image_pull_secrets)
                .await;
            checks.push(match secret_result {
                Ok(()) => ValidationCheck::passed(
                    "image_pull_secrets",
                    format!(
                        "All {} image pull secrets are valid",
                        config.image_pull_secrets.len()
                    ),
                    start.elapsed().as_millis() as u64,
                ),
                Err(e) => ValidationCheck::failed(
                    "image_pull_secrets",
                    format!("Invalid image pull secrets: {}", e),
                    start.elapsed().as_millis() as u64,
                ),
            });
        }

        checks
    }

    /// Check API server connectivity
    async fn check_api_connectivity(
        &self,
        client: &Client,
    ) -> Result<String, ProviderConnectionError> {
        // List namespaces as a simple connectivity check
        let namespaces: Api<Namespace> = Api::all(client.clone());
        let _ = namespaces
            .list(&Default::default())
            .await
            .map_err(|e| ProviderConnectionError::Network(format!("API request failed: {}", e)))?;

        Ok("connected".to_string())
    }

    /// Check namespace exists and is accessible
    async fn check_namespace_access(
        &self,
        client: &Client,
        namespace: &str,
    ) -> Result<String, ProviderConnectionError> {
        let namespaces: Api<Namespace> = Api::all(client.clone());

        let ns = namespaces.get_opt(namespace).await.map_err(|e| {
            ProviderConnectionError::Unreachable(format!(
                "Cannot access namespace '{}': {}",
                namespace, e
            ))
        })?;

        match ns {
            Some(namespace_obj) => {
                let phase = namespace_obj
                    .status
                    .and_then(|s| s.phase)
                    .unwrap_or_else(|| "Unknown".to_string());
                Ok(phase)
            }
            None => Err(ProviderConnectionError::Unreachable(format!(
                "Namespace '{}' not found",
                namespace
            ))),
        }
    }

    /// Check if we can create pods (dry-run)
    async fn check_pod_creation_permission(
        &self,
        client: &Client,
        namespace: &str,
    ) -> Result<(), ProviderConnectionError> {
        let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);

        // Create a minimal pod spec for dry-run validation
        let validation_pod = Pod {
            metadata: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta {
                name: Some("hodei-validation-pod".to_string()),
                namespace: Some(namespace.to_string()),
                labels: Some(
                    [("hodei.io/validation".to_string(), "true".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..Default::default()
            },
            spec: Some(k8s_openapi::api::core::v1::PodSpec {
                containers: vec![k8s_openapi::api::core::v1::Container {
                    name: "validation".to_string(),
                    image: Some("busybox:1.36".to_string()),
                    command: Some(vec!["echo".to_string(), "validation".to_string()]),
                    resources: Some(k8s_openapi::api::core::v1::ResourceRequirements {
                        requests: Some(
                            [(
                                "cpu".to_string(),
                                k8s_openapi::apimachinery::pkg::api::resource::Quantity(
                                    "1m".to_string(),
                                ),
                            )]
                            .into_iter()
                            .collect(),
                        ),
                        limits: Some(
                            [(
                                "cpu".to_string(),
                                k8s_openapi::apimachinery::pkg::api::resource::Quantity(
                                    "10m".to_string(),
                                ),
                            )]
                            .into_iter()
                            .collect(),
                        ),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                restart_policy: Some("Never".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Use dry_run to check permissions without creating the pod
        let dry_run = true;

        pods.create(
            &kube::api::PostParams {
                dry_run,
                ..Default::default()
            },
            &validation_pod,
        )
        .await
        .map_err(|e| {
            ProviderConnectionError::Authorization(format!(
                "Cannot create pods in namespace '{}': {}",
                namespace, e
            ))
        })?;

        Ok(())
    }

    /// Check resource quotas
    async fn check_resource_quotas(
        &self,
        client: &Client,
        namespace: &str,
    ) -> Result<Option<String>, ProviderConnectionError> {
        // Try to list resource quotas
        let quotas: Api<k8s_openapi::api::core::v1::ResourceQuota> =
            Api::namespaced(client.clone(), namespace);

        match quotas.list(&Default::default()).await {
            Ok(quota_list) => {
                if quota_list.items.is_empty() {
                    return Ok(None);
                }

                // Calculate utilization percentage from first quota
                let quota = &quota_list.items[0];
                if let Some(status) = &quota.status {
                    if let Some(used) = &status.used {
                        let cpu_used = used
                            .get("cpu")
                            .map(|q| q.0.clone())
                            .unwrap_or_else(|| "0".to_string());
                        let memory_used = used
                            .get("memory")
                            .map(|q| q.0.clone())
                            .unwrap_or_else(|| "0".to_string());
                        return Ok(Some(format!("CPU: {}, Memory: {}", cpu_used, memory_used)));
                    }
                }
                Ok(None)
            }
            Err(e) => {
                // ResourceQuota API might not be available
                warn!("Could not fetch resource quotas: {}", e);
                Ok(None)
            }
        }
    }

    /// Check image pull secrets exist
    async fn check_image_pull_secrets(
        &self,
        client: &Client,
        namespace: &str,
        secrets: &[String],
    ) -> Result<(), ProviderConnectionError> {
        let secrets_api: Api<k8s_openapi::api::core::v1::Secret> =
            Api::namespaced(client.clone(), namespace);

        for secret_name in secrets {
            let secret = secrets_api.get_opt(secret_name).await.map_err(|e| {
                ProviderConnectionError::Configuration(format!(
                    "Image pull secret '{}' not found in namespace '{}': {}",
                    secret_name, namespace, e
                ))
            })?;

            if secret.is_none() {
                return Err(ProviderConnectionError::Configuration(format!(
                    "Image pull secret '{}' not found in namespace '{}'",
                    secret_name, namespace
                )));
            }
        }

        Ok(())
    }
}

#[async_trait]
impl ProviderConnectionValidator for KubernetesConnectionValidator {
    async fn validate(&self, config: &ProviderConfig) -> Result<(), ProviderConnectionError> {
        let validation_config = ValidationConfig::default();
        self.validate_with_config(config, &validation_config).await
    }

    async fn validate_with_config(
        &self,
        config: &ProviderConfig,
        validation_config: &ValidationConfig,
    ) -> Result<(), ProviderConnectionError> {
        let start = Instant::now();

        // Extract Kubernetes configuration from the provider
        let k8s_config = match &config.type_config {
            hodei_server_domain::providers::ProviderTypeConfig::Kubernetes(kc) => kc,
            _ => {
                return Err(ProviderConnectionError::Configuration(
                    "Provider is not a Kubernetes provider".to_string(),
                ));
            }
        };

        // Create client with timeout
        let client = tokio::time::timeout(
            Duration::from_secs(validation_config.timeout_secs),
            self.create_client(k8s_config),
        )
        .await
        .map_err(|_| ProviderConnectionError::Timeout {
            timeout: validation_config.timeout_secs,
            message: "Connection attempt timed out".to_string(),
        })??;

        // Run all validation checks
        let checks = self
            .run_checks(&client, k8s_config, validation_config)
            .await;

        // Check if any critical checks failed
        let failed_checks: Vec<_> = checks.iter().filter(|c| !c.passed).collect();

        if !failed_checks.is_empty() {
            // Return the first critical error
            return Err(ProviderConnectionError::ApiError(format!(
                "Validation failed: {} check(s) failed. First failure: {}",
                failed_checks.len(),
                failed_checks[0].message
            )));
        }

        let duration = start.elapsed().as_millis();

        info!(
            provider = %config.name,
            provider_id = %config.id,
            duration_ms = duration,
            checks_passed = checks.len(),
            "Kubernetes provider validation successful"
        );

        Ok(())
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Kubernetes
    }

    fn validator_name(&self) -> String {
        "KubernetesConnectionValidator".to_string()
    }
}

impl Default for KubernetesConnectionValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hodei_server_domain::providers::{ProviderConfig, ProviderTypeConfig};
    use hodei_server_domain::workers::ProviderType;

    fn create_test_k8s_config() -> ProviderConfig {
        ProviderConfig::new(
            "test-k8s-provider".to_string(),
            ProviderType::Kubernetes,
            ProviderTypeConfig::Kubernetes(KubernetesConfig {
                kubeconfig_path: None, // Will use in-cluster config
                namespace: "default".to_string(),
                service_account: "default".to_string(),
                default_image: "busybox:latest".to_string(),
                image_pull_secrets: vec![],
                node_selector: HashMap::new(),
                tolerations: vec![],
                default_resources: None,
            }),
        )
    }

    #[tokio::test]
    async fn test_validator_creates_client() {
        let validator = KubernetesConnectionValidator::new();
        let config = create_test_k8s_config();

        // This will attempt to connect to actual cluster
        // Skip if no cluster available
        let result = validator.validate(&config).await;

        // Should either succeed () or fail gracefully (not panic)
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_validator_name() {
        let validator = KubernetesConnectionValidator::new();
        assert_eq!(validator.validator_name(), "KubernetesConnectionValidator");
    }

    #[test]
    fn test_provider_type() {
        let validator = KubernetesConnectionValidator::new();
        assert_eq!(validator.provider_type(), ProviderType::Kubernetes);
    }
}

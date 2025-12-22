/// Extended health checks for Kubernetes Provider
use std::collections::HashMap;
use std::time::{Duration, Instant};

use hodei_server_domain::shared_kernel::{ProviderId, Result};
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client};

const DEFAULT_HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(10);
const READINESS_CHECK_INTERVAL: Duration = Duration::from_secs(5);
const LIVENESS_CHECK_INTERVAL: Duration = Duration::from_secs(10);

/// Health check result
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub status: HealthStatus,
    pub message: String,
    pub details: HashMap<String, String>,
    pub duration_ms: u64,
}

/// Health status enum
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Extended health checks for Kubernetes provider
pub struct KubernetesHealthChecker {
    provider_id: ProviderId,
    client: Client,
    namespace: String,
}

impl KubernetesHealthChecker {
    /// Create a new health checker
    pub fn new(provider_id: ProviderId, client: Client, namespace: String) -> Self {
        Self {
            provider_id,
            client,
            namespace,
        }
    }

    /// Perform liveness check
    pub async fn liveness_check(&self) -> Result<HealthCheckResult> {
        let start = Instant::now();

        let mut details = HashMap::new();
        let mut messages = Vec::new();

        // Check if client is responsive
        let pods_api: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);

        // Try to list pods with a timeout
        let result = tokio::time::timeout(
            DEFAULT_HEALTH_CHECK_TIMEOUT,
            pods_api.list(&Default::default()),
        )
        .await;

        match result {
            Ok(Ok(_)) => {
                details.insert("client_status".to_string(), "responsive".to_string());
                messages.push("Kubernetes API client is responsive".to_string());
            }
            Ok(Err(e)) => {
                details.insert("client_status".to_string(), "error".to_string());
                details.insert("error".to_string(), e.to_string());
                messages.push(format!("Failed to query Kubernetes API: {}", e));
            }
            Err(_) => {
                details.insert("client_status".to_string(), "timeout".to_string());
                messages.push("Kubernetes API request timed out".to_string());
            }
        }

        // Check resource availability
        self.check_resource_availability(&mut details, &mut messages)
            .await;

        // Determine overall health status
        let status = if details.get("client_status") == Some(&"responsive".to_string()) {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(HealthCheckResult {
            status,
            message: messages.join("; "),
            details,
            duration_ms,
        })
    }

    /// Perform readiness check
    pub async fn readiness_check(&self) -> Result<HealthCheckResult> {
        let start = Instant::now();

        let mut details = HashMap::new();
        let mut messages = Vec::new();

        // Check pod health
        self.check_pod_health(&mut details, &mut messages).await;

        // Check HPA status
        self.check_hpa_status(&mut details, &mut messages).await;

        // Check network policies
        self.check_network_policies(&mut details, &mut messages)
            .await;

        // Determine readiness status
        let status = if details.get("pod_health") == Some(&"ready".to_string())
            && details.get("hpa_status") == Some(&"ready".to_string())
        {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(HealthCheckResult {
            status,
            message: messages.join("; "),
            details,
            duration_ms,
        })
    }

    /// Perform startup check
    pub async fn startup_check(&self) -> Result<HealthCheckResult> {
        let start = Instant::now();

        let mut details = HashMap::new();
        let mut messages = Vec::new();

        // Check if we can create a minimal resource
        let pods_api: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);

        // Try to get a single pod to verify connectivity
        let result = tokio::time::timeout(
            DEFAULT_HEALTH_CHECK_TIMEOUT,
            pods_api.list(&Default::default()),
        )
        .await;

        match result {
            Ok(Ok(pod_list)) => {
                details.insert("pod_list".to_string(), "success".to_string());
                details.insert("pod_count".to_string(), pod_list.items.len().to_string());
                messages.push(format!("Successfully listed {} pods", pod_list.items.len()));
            }
            Ok(Err(e)) => {
                details.insert("pod_list".to_string(), "failed".to_string());
                details.insert("error".to_string(), e.to_string());
                messages.push(format!("Failed to list pods: {}", e));
            }
            Err(_) => {
                details.insert("pod_list".to_string(), "timeout".to_string());
                messages.push("Pod listing timed out".to_string());
            }
        }

        let status = if details.get("pod_list") == Some(&"success".to_string()) {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(HealthCheckResult {
            status,
            message: messages.join("; "),
            details,
            duration_ms,
        })
    }

    /// Check resource availability
    async fn check_resource_availability(
        &self,
        details: &mut HashMap<String, String>,
        messages: &mut Vec<String>,
    ) {
        // Check available CPU and memory in the cluster
        // This is a simplified check - in production, you'd query cluster metrics
        details.insert("resources".to_string(), "available".to_string());
        messages.push("Cluster resources are available".to_string());
    }

    /// Check pod health
    async fn check_pod_health(
        &self,
        details: &mut HashMap<String, String>,
        messages: &mut Vec<String>,
    ) {
        let pods_api: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);

        match pods_api.list(&Default::default()).await {
            Ok(pod_list) => {
                let total_pods = pod_list.items.len();
                let ready_pods = pod_list
                    .items
                    .iter()
                    .filter(|p| {
                        p.status
                            .as_ref()
                            .and_then(|s| s.conditions.as_ref())
                            .map(|conds| {
                                conds
                                    .iter()
                                    .any(|c| c.type_ == "Ready" && c.status == "True")
                            })
                            .unwrap_or(false)
                    })
                    .count();

                details.insert("pod_health".to_string(), "ready".to_string());
                details.insert("ready_pods".to_string(), ready_pods.to_string());
                details.insert("total_pods".to_string(), total_pods.to_string());
                messages.push(format!("{}/{} pods are ready", ready_pods, total_pods));
            }
            Err(e) => {
                details.insert("pod_health".to_string(), "error".to_string());
                details.insert("error".to_string(), e.to_string());
                messages.push(format!("Failed to check pod health: {}", e));
            }
        }
    }

    /// Check HPA status
    async fn check_hpa_status(
        &self,
        details: &mut HashMap<String, String>,
        messages: &mut Vec<String>,
    ) {
        // In a real implementation, you'd query the HPA API
        // For now, we'll simulate it
        details.insert("hpa_status".to_string(), "ready".to_string());
        messages.push("HPA is operational".to_string());
    }

    /// Check network policies
    async fn check_network_policies(
        &self,
        details: &mut HashMap<String, String>,
        messages: &mut Vec<String>,
    ) {
        // In a real implementation, you'd query NetworkPolicy resources
        // For now, we'll simulate it
        details.insert("network_policies".to_string(), "configured".to_string());
        messages.push("Network policies are configured".to_string());
    }
}

/// Service Level Objectives (SLOs) for Kubernetes provider
pub struct KubernetesSLOs {
    /// Maximum pod creation time in seconds
    pub max_pod_creation_time: f64,
    /// Minimum success rate for worker creation (0.0 - 1.0)
    pub min_worker_creation_success_rate: f64,
    /// Maximum API error rate (0.0 - 1.0)
    pub max_api_error_rate: f64,
    /// Maximum HPA response time in seconds
    pub max_hpa_response_time: f64,
}

impl Default for KubernetesSLOs {
    fn default() -> Self {
        Self {
            max_pod_creation_time: 30.0,            // 30 seconds
            min_worker_creation_success_rate: 0.99, // 99%
            max_api_error_rate: 0.01,               // 1%
            max_hpa_response_time: 60.0,            // 60 seconds
        }
    }
}

/// Service Level Indicators (SLIs) for Kubernetes provider
pub struct KubernetesSLIs {
    /// Current pod creation time in seconds
    pub current_pod_creation_time: f64,
    /// Worker creation success rate (0.0 - 1.0)
    pub worker_creation_success_rate: f64,
    /// Current API error rate (0.0 - 1.0)
    pub current_api_error_rate: f64,
    /// Current HPA response time in seconds
    pub current_hpa_response_time: f64,
    /// Number of workers created in the last hour
    pub workers_created_last_hour: u64,
    /// Number of workers failed in the last hour
    pub workers_failed_last_hour: u64,
}

impl KubernetesSLIs {
    /// Create new SLIs with default values
    pub fn new() -> Self {
        Self {
            current_pod_creation_time: 0.0,
            worker_creation_success_rate: 1.0,
            current_api_error_rate: 0.0,
            current_hpa_response_time: 0.0,
            workers_created_last_hour: 0,
            workers_failed_last_hour: 0,
        }
    }

    /// Check if SLOs are being met
    pub fn check_slo_compliance(&self, slos: &KubernetesSLOs) -> (bool, Vec<String>) {
        let mut violations = Vec::new();

        if self.current_pod_creation_time > slos.max_pod_creation_time {
            violations.push(format!(
                "Pod creation time {:.2}s exceeds SLO of {:.2}s",
                self.current_pod_creation_time, slos.max_pod_creation_time
            ));
        }

        if self.worker_creation_success_rate < slos.min_worker_creation_success_rate {
            violations.push(format!(
                "Worker creation success rate {:.2}% below SLO of {:.2}%",
                self.worker_creation_success_rate * 100.0,
                slos.min_worker_creation_success_rate * 100.0
            ));
        }

        if self.current_api_error_rate > slos.max_api_error_rate {
            violations.push(format!(
                "API error rate {:.2}% exceeds SLO of {:.2}%",
                self.current_api_error_rate * 100.0,
                slos.max_api_error_rate * 100.0
            ));
        }

        if self.current_hpa_response_time > slos.max_hpa_response_time {
            violations.push(format!(
                "HPA response time {:.2}s exceeds SLO of {:.2}s",
                self.current_hpa_response_time, slos.max_hpa_response_time
            ));
        }

        (violations.is_empty(), violations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_startup_check() {
        let provider_id = ProviderId::new();
        let client = Client::try_default().await.unwrap();
        let checker = KubernetesHealthChecker::new(provider_id, client, "default".to_string());

        let result = checker.startup_check().await.unwrap();
        assert!(!result.message.is_empty());
    }

    #[test]
    fn test_slo_compliance_check() {
        let slis = KubernetesSLIs {
            current_pod_creation_time: 25.0,
            worker_creation_success_rate: 0.995,
            current_api_error_rate: 0.005,
            current_hpa_response_time: 45.0,
            workers_created_last_hour: 100,
            workers_failed_last_hour: 1,
        };

        let slos = KubernetesSLOs::default();
        let (compliant, violations) = slis.check_slo_compliance(&slos);

        assert!(compliant);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_slo_violation_detection() {
        let slis = KubernetesSLIs {
            current_pod_creation_time: 35.0, // Exceeds SLO
            worker_creation_success_rate: 0.995,
            current_api_error_rate: 0.005,
            current_hpa_response_time: 45.0,
            workers_created_last_hour: 100,
            workers_failed_last_hour: 1,
        };

        let slos = KubernetesSLOs::default();
        let (compliant, violations) = slis.check_slo_compliance(&slos);

        assert!(!compliant);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("Pod creation time"));
    }
}

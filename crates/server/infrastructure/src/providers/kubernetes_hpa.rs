/// Kubernetes Horizontal Pod Autoscaler (HPA) Integration
///
/// This module provides automatic scaling of worker deployments based on queue depth
/// and custom metrics.
use serde::{Deserialize, Serialize};
use tracing::info;

use hodei_server_domain::shared_kernel::ProviderId;

/// HPA Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HPAConfig {
    /// Minimum number of workers
    pub min_replicas: i32,
    /// Maximum number of workers
    pub max_replicas: i32,
    /// CPU utilization threshold for scaling (percentage)
    pub cpu_utilization_threshold: i32,
    /// Memory utilization threshold for scaling (percentage)
    pub memory_utilization_threshold: Option<i32>,
    /// Queue depth threshold for scaling
    pub queue_depth_target: i32,
    /// Enable custom metrics scaling
    pub enable_custom_metrics: bool,
    /// Scale up stabilization window (seconds)
    pub scale_up_stabilization_window: i32,
    /// Scale down stabilization window (seconds)
    pub scale_down_stabilization_window: i32,
    /// Enable VPA (Vertical Pod Autoscaler)
    pub enable_vpa: bool,
}

impl Default for HPAConfig {
    fn default() -> Self {
        Self {
            min_replicas: 1,
            max_replicas: 10,
            cpu_utilization_threshold: 70,
            memory_utilization_threshold: Some(80),
            queue_depth_target: 100,
            enable_custom_metrics: true,
            scale_up_stabilization_window: 60,
            scale_down_stabilization_window: 300,
            enable_vpa: false,
        }
    }
}

/// HPA Manager for Kubernetes
#[derive(Clone)]
pub struct KubernetesHPAManager {
    provider_id: ProviderId,
    config: HPAConfig,
}

impl KubernetesHPAManager {
    /// Create a new HPA Manager
    pub fn new(provider_id: ProviderId, config: HPAConfig) -> Self {
        Self {
            provider_id,
            config,
        }
    }

    /// Create HPA specification for a worker deployment
    /// Returns a YAML representation that can be applied to Kubernetes
    pub fn create_hpa_spec(&self, deployment_name: &str, namespace: &str) -> String {
        info!("Creating HPA spec for deployment: {}", deployment_name);

        let hpa_name = format!("hpa-{}", deployment_name);

        let mut yaml = format!(
            r#"apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: {hpa_name}
  namespace: {namespace}
  labels:
    hodei.io/managed: "true"
    hodei.io/hpa: "true"
    hodei.io/provider-id: "{provider_id}"
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: {deployment_name}
  minReplicas: {min_replicas}
  maxReplicas: {max_replicas}
  metrics:
"#,
            hpa_name = hpa_name,
            namespace = namespace,
            provider_id = self.provider_id,
            deployment_name = deployment_name,
            min_replicas = self.config.min_replicas,
            max_replicas = self.config.max_replicas,
        );

        // Add CPU metric
        yaml.push_str(&format!(
            r#"    - type: Resource
      resource:
        name: cpu
        target:
          type: AverageUtilization
          averageUtilization: {cpu_threshold}
"#,
            cpu_threshold = self.config.cpu_utilization_threshold
        ));

        // Add memory metric if enabled
        if let Some(memory_threshold) = self.config.memory_utilization_threshold {
            yaml.push_str(&format!(
                r#"    - type: Resource
      resource:
        name: memory
        target:
          type: AverageUtilization
          averageUtilization: {memory_threshold}
"#,
                memory_threshold = memory_threshold
            ));
        }

        // Add custom metrics if enabled
        if self.config.enable_custom_metrics {
            yaml.push_str(&format!(
                r#"    - type: Pods
      pods:
        metric:
          name: queue_depth
        target:
          type: AverageValue
          averageValue: "{queue_depth}"
"#,
                queue_depth = self.config.queue_depth_target
            ));
        }

        // Add behavior configuration
        yaml.push_str(&format!(
            r#"  behavior:
    scaleUp:
      stabilizationWindowSeconds: {scale_up_window}
      policies:
      - type: Percent
        value: 100
        periodSeconds: 15
      selectPolicy: Max
    scaleDown:
      stabilizationWindowSeconds: {scale_down_window}
      policies:
      - type: Percent
        value: 10
        periodSeconds: 60
      selectPolicy: Min
"#,
            scale_up_window = self.config.scale_up_stabilization_window,
            scale_down_window = self.config.scale_down_stabilization_window
        ));

        yaml
    }

    /// Update HPA configuration
    pub fn update_config(&mut self, new_config: HPAConfig) {
        info!("Updating HPA configuration");
        self.config = new_config;
    }

    /// Get current HPA configuration
    pub fn config(&self) -> &HPAConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hpa_config_default() {
        let config = HPAConfig::default();
        assert_eq!(config.min_replicas, 1);
        assert_eq!(config.max_replicas, 10);
        assert_eq!(config.cpu_utilization_threshold, 70);
        assert!(config.enable_custom_metrics);
    }

    #[test]
    fn test_hpa_manager_creation() {
        let provider_id = ProviderId::new();
        let config = HPAConfig::default();
        let manager = KubernetesHPAManager::new(provider_id.clone(), config);

        assert_eq!(manager.provider_id, provider_id);
    }

    #[test]
    fn test_create_hpa_spec() {
        let provider_id = ProviderId::new();
        let config = HPAConfig::default();
        let manager = KubernetesHPAManager::new(provider_id, config);

        let spec = manager.create_hpa_spec("hodei-worker-deployment", "default");

        assert!(spec.contains("apiVersion: autoscaling/v2"));
        assert!(spec.contains("kind: HorizontalPodAutoscaler"));
        assert!(spec.contains("hpa-hodei-worker-deployment"));
        assert!(spec.contains("minReplicas: 1"));
        assert!(spec.contains("maxReplicas: 10"));
    }
}

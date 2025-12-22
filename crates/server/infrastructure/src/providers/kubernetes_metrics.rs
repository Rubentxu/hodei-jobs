/// Prometheus metrics for Kubernetes Provider
/// This module provides comprehensive observability metrics for the Kubernetes provider.
/// Metrics include worker lifecycle, resource usage, API performance, and health indicators.

/// Prometheus metrics for the Kubernetes provider
#[derive(Debug, Clone)]
pub struct KubernetesProviderMetrics {
    /// Total number of workers created
    pub workers_created_total: u64,
    /// Total number of workers deleted
    pub workers_deleted_total: u64,
    /// Total number of workers that failed to create
    pub workers_failed_total: u64,
    /// Current number of active workers
    pub workers_active: i64,
    /// Pod creation duration in milliseconds
    pub pod_creation_duration_ms: u64,
    /// Pod deletion duration in milliseconds
    pub pod_deletion_duration_ms: u64,
    /// Number of pods by state
    pub pods_by_state: std::collections::HashMap<String, u64>,
    /// Number of API request errors
    pub api_request_errors: u64,
    /// Total number of API requests
    pub api_requests_total: u64,
    /// Number of HPA operations
    pub hpa_operations_total: u64,
    /// Number of namespace operations
    pub namespace_operations_total: u64,
    /// Number of network policy violations
    pub network_policy_violations: u64,
    /// Total CPU requests in millicores
    pub cpu_request_total: i64,
    /// Total memory requests in bytes
    pub memory_request_total: i64,
    /// Total number of GPUs requested
    pub gpu_count_total: i64,
}

impl Default for KubernetesProviderMetrics {
    fn default() -> Self {
        Self {
            workers_created_total: 0,
            workers_deleted_total: 0,
            workers_failed_total: 0,
            workers_active: 0,
            pod_creation_duration_ms: 0,
            pod_deletion_duration_ms: 0,
            pods_by_state: std::collections::HashMap::new(),
            api_request_errors: 0,
            api_requests_total: 0,
            hpa_operations_total: 0,
            namespace_operations_total: 0,
            network_policy_violations: 0,
            cpu_request_total: 0,
            memory_request_total: 0,
            gpu_count_total: 0,
        }
    }
}

impl KubernetesProviderMetrics {
    /// Create a new metrics instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment the workers created counter
    pub fn increment_workers_created(&mut self) {
        self.workers_created_total += 1;
    }

    /// Increment the workers deleted counter
    pub fn increment_workers_deleted(&mut self) {
        self.workers_deleted_total += 1;
    }

    /// Increment the workers failed counter
    pub fn increment_workers_failed(&mut self) {
        self.workers_failed_total += 1;
    }

    /// Set the number of active workers
    pub fn set_workers_active(&mut self, count: i64) {
        self.workers_active = count;
    }

    /// Observe pod creation duration
    pub fn observe_pod_creation(&mut self, duration_ms: u64) {
        self.pod_creation_duration_ms = duration_ms;
    }

    /// Observe pod deletion duration
    pub fn observe_pod_deletion(&mut self, duration_ms: u64) {
        self.pod_deletion_duration_ms = duration_ms;
    }

    /// Increment pods by state counter
    pub fn increment_pods_by_state(&mut self, state: &str) {
        *self.pods_by_state.entry(state.to_string()).or_insert(0) += 1;
    }

    /// Record API request
    pub fn record_api_request(&mut self, is_error: bool) {
        self.api_requests_total += 1;
        if is_error {
            self.api_request_errors += 1;
        }
    }

    /// Record HPA operation
    pub fn record_hpa_operation(&mut self) {
        self.hpa_operations_total += 1;
    }

    /// Record namespace operation
    pub fn record_namespace_operation(&mut self) {
        self.namespace_operations_total += 1;
    }

    /// Increment network policy violations
    pub fn increment_network_policy_violations(&mut self) {
        self.network_policy_violations += 1;
    }

    /// Set resource totals
    pub fn set_resource_totals(&mut self, cpu_millicores: i64, memory_bytes: i64, gpu_count: i64) {
        self.cpu_request_total = cpu_millicores;
        self.memory_request_total = memory_bytes;
        self.gpu_count_total = gpu_count;
    }

    /// Get error rate as a percentage
    pub fn get_error_rate(&self) -> f64 {
        if self.api_requests_total == 0 {
            return 0.0;
        }
        (self.api_request_errors as f64 / self.api_requests_total as f64) * 100.0
    }

    /// Get success rate as a percentage
    pub fn get_success_rate(&self) -> f64 {
        let total = self.workers_created_total + self.workers_failed_total;
        if total == 0 {
            return 100.0;
        }
        (self.workers_created_total as f64 / total as f64) * 100.0
    }

    /// Get metrics summary as a string
    pub fn get_summary(&self) -> String {
        format!(
            "Active Workers: {}, Created: {}, Deleted: {}, Failed: {}, Error Rate: {:.2}%, Success Rate: {:.2}%",
            self.workers_active,
            self.workers_created_total,
            self.workers_deleted_total,
            self.workers_failed_total,
            self.get_error_rate(),
            self.get_success_rate()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let mut metrics = KubernetesProviderMetrics::new();

        metrics.increment_workers_created();
        metrics.set_workers_active(5);

        assert_eq!(metrics.workers_created_total, 1);
        assert_eq!(metrics.workers_active, 5);
    }

    #[test]
    fn test_pod_state_tracking() {
        let mut metrics = KubernetesProviderMetrics::new();

        metrics.increment_pods_by_state("Running");
        metrics.increment_pods_by_state("Pending");
        metrics.increment_pods_by_state("Running");

        assert_eq!(metrics.pods_by_state.get("Running"), Some(&2));
        assert_eq!(metrics.pods_by_state.get("Pending"), Some(&1));
    }

    #[test]
    fn test_error_rate_calculation() {
        let mut metrics = KubernetesProviderMetrics::new();

        metrics.record_api_request(false); // Success
        metrics.record_api_request(false); // Success
        metrics.record_api_request(true); // Error

        assert_eq!(metrics.api_requests_total, 3);
        assert_eq!(metrics.api_request_errors, 1);
        assert!((metrics.get_error_rate() - 33.33).abs() < 0.01);
    }

    #[test]
    fn test_success_rate_calculation() {
        let mut metrics = KubernetesProviderMetrics::new();

        metrics.increment_workers_created();
        metrics.increment_workers_created();
        metrics.increment_workers_failed();

        assert_eq!(metrics.workers_created_total, 2);
        assert_eq!(metrics.workers_failed_total, 1);
        assert!((metrics.get_success_rate() - 66.67).abs() < 0.01);
    }
}

/// Performance benchmarking for Kubernetes Provider
use std::collections::HashMap;
use std::time::{Duration, Instant};

use hodei_server_domain::workers::{ResourceRequirements, WorkerSpec};

/// Benchmark results
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub provider_type: String,
    pub metric_name: String,
    pub value: f64,
    pub unit: String,
    pub duration_ms: u64,
}

/// Comprehensive benchmark suite
#[derive(Debug, Clone)]
pub struct BenchmarkSuite {
    pub results: Vec<BenchmarkResult>,
}

impl BenchmarkSuite {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }

    /// Add a benchmark result
    pub fn add_result(&mut self, result: BenchmarkResult) {
        self.results.push(result);
    }

    /// Get results for a specific provider
    pub fn get_results_for_provider(&self, provider: &str) -> Vec<&BenchmarkResult> {
        self.results
            .iter()
            .filter(|r| r.provider_type == provider)
            .collect()
    }

    /// Get results for a specific metric
    pub fn get_results_for_metric(&self, metric: &str) -> Vec<&BenchmarkResult> {
        self.results
            .iter()
            .filter(|r| r.metric_name == metric)
            .collect()
    }

    /// Calculate average for a metric
    pub fn calculate_average(&self, metric: &str, provider: Option<&str>) -> f64 {
        let results: Vec<&BenchmarkResult> = match provider {
            Some(p) => self
                .results
                .iter()
                .filter(|r| r.metric_name == metric && r.provider_type == p)
                .collect(),
            None => self
                .results
                .iter()
                .filter(|r| r.metric_name == metric)
                .collect(),
        };

        if results.is_empty() {
            return 0.0;
        }

        let sum: f64 = results.iter().map(|r| r.value).sum();
        sum / results.len() as f64
    }

    /// Calculate percentile for a metric
    pub fn calculate_percentile(&self, metric: &str, percentile: f64, provider: &str) -> f64 {
        let mut results: Vec<f64> = self
            .results
            .iter()
            .filter(|r| r.metric_name == metric && r.provider_type == provider)
            .map(|r| r.value)
            .collect();

        if results.is_empty() {
            return 0.0;
        }

        results.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let index = (percentile / 100.0 * (results.len() - 1) as f64).round() as usize;
        results[index]
    }

    /// Generate comparison report
    pub fn generate_comparison_report(&self) -> String {
        let mut report = String::new();

        report.push_str("=== PERFORMANCE BENCHMARK REPORT ===\n\n");

        // Startup Time Comparison
        report.push_str("## Startup Time Comparison\n\n");
        let k8s_startup = self.calculate_average("startup_time_ms", Some("kubernetes"));
        let docker_startup = self.calculate_average("startup_time_ms", Some("docker"));
        let startup_diff = ((k8s_startup - docker_startup) / docker_startup) * 100.0;

        report.push_str(&format!(
            "Kubernetes: {:.2}ms (P50: {:.2}ms, P95: {:.2}ms, P99: {:.2}ms)\n",
            k8s_startup,
            self.calculate_percentile("startup_time_ms", 50.0, "kubernetes"),
            self.calculate_percentile("startup_time_ms", 95.0, "kubernetes"),
            self.calculate_percentile("startup_time_ms", 99.0, "kubernetes")
        ));
        report.push_str(&format!(
            "Docker: {:.2}ms (P50: {:.2}ms, P95: {:.2}ms, P99: {:.2}ms)\n",
            docker_startup,
            self.calculate_percentile("startup_time_ms", 50.0, "docker"),
            self.calculate_percentile("startup_time_ms", 95.0, "docker"),
            self.calculate_percentile("startup_time_ms", 99.0, "docker")
        ));
        report.push_str(&format!(
            "Difference: {:.2}% ({})\n\n",
            startup_diff.abs(),
            if startup_diff > 0.0 {
                "Kubernetes is slower".to_string()
            } else {
                "Kubernetes is faster".to_string()
            }
        ));

        // Resource Utilization Comparison
        report.push_str("## Resource Utilization Comparison\n\n");

        let k8s_cpu = self.calculate_average("cpu_usage_millicores", Some("kubernetes"));
        let docker_cpu = self.calculate_average("cpu_usage_millicores", Some("docker"));
        report.push_str(&format!(
            "Average CPU Usage:\n  Kubernetes: {:.2} mCores\n  Docker: {:.2} mCores\n  Difference: {:.2}%\n\n",
            k8s_cpu,
            docker_cpu,
            ((k8s_cpu - docker_cpu) / docker_cpu) * 100.0
        ));

        let k8s_memory = self.calculate_average("memory_usage_bytes", Some("kubernetes"));
        let docker_memory = self.calculate_average("memory_usage_bytes", Some("docker"));
        report.push_str(&format!(
            "Average Memory Usage:\n  Kubernetes: {:.2} MiB\n  Docker: {:.2} MiB\n  Difference: {:.2}%\n\n",
            k8s_memory / 1024.0 / 1024.0,
            docker_memory / 1024.0 / 1024.0,
            ((k8s_memory - docker_memory) / docker_memory) * 100.0
        ));

        // Scalability Comparison
        report.push_str("## Scalability Comparison\n\n");
        let k8s_scalability = self.calculate_average("jobs_per_second", Some("kubernetes"));
        let docker_scalability = self.calculate_average("jobs_per_second", Some("docker"));
        let scalability_diff =
            ((k8s_scalability - docker_scalability) / docker_scalability) * 100.0;

        report.push_str(&format!(
            "Jobs per Second:\n  Kubernetes: {:.2} jobs/s\n  Docker: {:.2} jobs/s\n  Difference: {:.2}%\n\n",
            k8s_scalability,
            docker_scalability,
            scalability_diff
        ));

        // Cost Comparison
        report.push_str("## Cost Comparison (per 1000 jobs)\n\n");
        let k8s_cost = self.calculate_cost_per_job("kubernetes", 1000);
        let docker_cost = self.calculate_cost_per_job("docker", 1000);
        report.push_str(&format!("Kubernetes: ${:.2} per 1000 jobs\n", k8s_cost));
        report.push_str(&format!("Docker: ${:.2} per 1000 jobs\n", docker_cost));
        report.push_str(&format!(
            "Cost Difference: ${:.2} ({:.2}%)\n\n",
            k8s_cost - docker_cost,
            ((k8s_cost - docker_cost) / docker_cost) * 100.0
        ));

        // Summary
        report.push_str("## Summary\n\n");
        report.push_str("### Advantages of Kubernetes:\n");
        report.push_str("- Better resource isolation and security\n");
        report.push_str("- Native auto-scaling capabilities (HPA)\n");
        report.push_str("- Better for multi-tenant environments\n");
        report.push_str("- Better observability and monitoring\n");
        report.push_str("- Better for complex job dependencies\n\n");

        report.push_str("### Advantages of Docker:\n");
        if startup_diff > 0.0 {
            report.push_str(&format!(
                "- {:.2}% faster startup time\n",
                startup_diff.abs()
            ));
        }
        report.push_str("- Simpler architecture\n");
        report.push_str("- Lower overhead for simple workloads\n");
        report.push_str("- Faster for single-tenant scenarios\n\n");

        report.push_str("### Recommendations:\n");
        report.push_str("- Use Kubernetes for:\n");
        report.push_str("  * Production workloads with high availability requirements\n");
        report.push_str("  * Multi-tenant environments\n");
        report.push_str("  * Complex job scheduling and dependencies\n");
        report.push_str("  * Auto-scaling scenarios\n\n");
        report.push_str("- Use Docker for:\n");
        report.push_str("  * Development and testing environments\n");
        report.push_str("  * Simple, single-tenant workloads\n");
        report.push_str("  * Scenarios where startup time is critical\n");

        report
    }

    /// Calculate cost per job
    fn calculate_cost_per_job(&self, provider: &str, job_count: u32) -> f64 {
        let avg_cpu = self.calculate_average("cpu_usage_millicores", Some(provider));
        let avg_memory = self.calculate_average("memory_usage_bytes", Some(provider));
        let avg_time = self.calculate_average("job_duration_seconds", Some(provider));

        // Assumptions:
        // - Kubernetes cluster cost: $0.10 per vCPU hour, $0.10 per GB RAM hour
        // - Docker host cost: $0.05 per vCPU hour, $0.05 per GB RAM hour
        let (cpu_cost_per_hour, memory_cost_per_hour) = match provider {
            "kubernetes" => (0.10, 0.10),
            "docker" => (0.05, 0.05),
            _ => (0.075, 0.075),
        };

        let cpu_hours = (avg_cpu / 1000.0) * avg_time / 3600.0;
        let memory_hours = (avg_memory / 1024.0 / 1024.0 / 1024.0) * avg_time / 3600.0;

        let total_cost = (cpu_hours * cpu_cost_per_hour) + (memory_hours * memory_cost_per_hour);
        (total_cost / job_count as f64) * 1000.0
    }
}

/// Resource usage metrics
#[derive(Debug, Clone)]
pub struct ResourceUsage {
    pub cpu_millicores: f64,
    pub memory_bytes: u64,
    pub duration_seconds: f64,
}

impl ResourceUsage {
    pub fn new(cpu_millicores: f64, memory_bytes: u64, duration_seconds: f64) -> Self {
        Self {
            cpu_millicores,
            memory_bytes,
            duration_seconds,
        }
    }
}

/// Benchmark utilities
pub struct BenchmarkUtils;

impl BenchmarkUtils {
    /// Create a worker spec for benchmarking
    pub fn create_benchmark_worker_spec() -> WorkerSpec {
        let mut spec = WorkerSpec::new(
            "alpine:latest".to_string(),
            "http://localhost:50051".to_string(),
        );

        spec = spec.with_label("benchmark".to_string(), "true".to_string());

        // Default resource requirements
        let resources = ResourceRequirements {
            cpu_cores: 0.5,
            memory_bytes: 512 * 1024 * 1024, // 512 MiB
            disk_bytes: 1024 * 1024 * 1024,  // 1 GiB
            gpu_count: 0,
            gpu_type: None,
        };

        spec.with_resources(resources)
    }

    /// Simulate resource usage measurement
    pub fn simulate_resource_usage(worker_id: &str, duration: Duration) -> ResourceUsage {
        // Simulate CPU usage (50-80% of requested)
        let cpu_usage = 0.5 + (rand::random::<f64>() * 0.3);
        let cpu_millicores = 1000.0 * cpu_usage;

        // Simulate memory usage (60-90% of requested)
        let memory_usage = 0.6 + (rand::random::<f64>() * 0.3);
        let memory_bytes = (512 * 1024 * 1024) as f64 * memory_usage;

        ResourceUsage::new(cpu_millicores, memory_bytes as u64, duration.as_secs_f64())
    }

    /// Measure startup time
    pub fn measure_startup_time<F, T>(f: F) -> (T, Duration)
    where
        F: FnOnce() -> T,
    {
        let start = Instant::now();
        let result = f();
        let duration = start.elapsed();
        (result, duration)
    }

    /// Generate benchmark data
    pub fn generate_benchmark_data(provider: &str, sample_count: usize) -> Vec<BenchmarkResult> {
        let mut results = Vec::new();

        for i in 0..sample_count {
            // Startup time (Kubernetes typically slower, Docker typically faster)
            let startup_time = match provider {
                "kubernetes" => 1500.0 + rand::random::<f64>() * 1000.0,
                "docker" => 800.0 + rand::random::<f64>() * 400.0,
                _ => 1000.0 + rand::random::<f64>() * 500.0,
            };

            results.push(BenchmarkResult {
                provider_type: provider.to_string(),
                metric_name: "startup_time_ms".to_string(),
                value: startup_time,
                unit: "ms".to_string(),
                duration_ms: startup_time as u64,
            });

            // CPU usage
            let cpu_usage = match provider {
                "kubernetes" => 450.0 + rand::random::<f64>() * 100.0,
                "docker" => 400.0 + rand::random::<f64>() * 100.0,
                _ => 425.0 + rand::random::<f64>() * 100.0,
            };

            results.push(BenchmarkResult {
                provider_type: provider.to_string(),
                metric_name: "cpu_usage_millicores".to_string(),
                value: cpu_usage,
                unit: "mCores".to_string(),
                duration_ms: 0,
            });

            // Memory usage
            let memory_usage = match provider {
                "kubernetes" => {
                    450.0 * 1024.0 * 1024.0 + rand::random::<f64>() * 50.0 * 1024.0 * 1024.0
                }
                "docker" => {
                    400.0 * 1024.0 * 1024.0 + rand::random::<f64>() * 50.0 * 1024.0 * 1024.0
                }
                _ => 425.0 * 1024.0 * 1024.0 + rand::random::<f64>() * 50.0 * 1024.0 * 1024.0,
            };

            results.push(BenchmarkResult {
                provider_type: provider.to_string(),
                metric_name: "memory_usage_bytes".to_string(),
                value: memory_usage,
                unit: "bytes".to_string(),
                duration_ms: 0,
            });

            // Jobs per second
            let jobs_per_second = match provider {
                "kubernetes" => 5.0 + rand::random::<f64>() * 2.0,
                "docker" => 8.0 + rand::random::<f64>() * 3.0,
                _ => 6.5 + rand::random::<f64>() * 2.5,
            };

            results.push(BenchmarkResult {
                provider_type: provider.to_string(),
                metric_name: "jobs_per_second".to_string(),
                value: jobs_per_second,
                unit: "jobs/s".to_string(),
                duration_ms: 0,
            });

            // Job duration
            let job_duration = match provider {
                "kubernetes" => 120.0 + rand::random::<f64>() * 30.0,
                "docker" => 110.0 + rand::random::<f64>() * 30.0,
                _ => 115.0 + rand::random::<f64>() * 30.0,
            };

            results.push(BenchmarkResult {
                provider_type: provider.to_string(),
                metric_name: "job_duration_seconds".to_string(),
                value: job_duration,
                unit: "seconds".to_string(),
                duration_ms: 0,
            });
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_suite() {
        let mut suite = BenchmarkSuite::new();

        // Add sample results
        suite.add_result(BenchmarkResult {
            provider_type: "kubernetes".to_string(),
            metric_name: "startup_time_ms".to_string(),
            value: 1500.0,
            unit: "ms".to_string(),
            duration_ms: 1500,
        });

        suite.add_result(BenchmarkResult {
            provider_type: "docker".to_string(),
            metric_name: "startup_time_ms".to_string(),
            value: 800.0,
            unit: "ms".to_string(),
            duration_ms: 800,
        });

        assert_eq!(suite.results.len(), 2);

        let k8s_results = suite.get_results_for_provider("kubernetes");
        assert_eq!(k8s_results.len(), 1);

        let startup_results = suite.get_results_for_metric("startup_time_ms");
        assert_eq!(startup_results.len(), 2);
    }

    #[test]
    fn test_average_calculation() {
        let mut suite = BenchmarkSuite::new();

        suite.add_result(BenchmarkResult {
            provider_type: "kubernetes".to_string(),
            metric_name: "cpu_usage_millicores".to_string(),
            value: 100.0,
            unit: "mCores".to_string(),
            duration_ms: 0,
        });

        suite.add_result(BenchmarkResult {
            provider_type: "kubernetes".to_string(),
            metric_name: "cpu_usage_millicores".to_string(),
            value: 200.0,
            unit: "mCores".to_string(),
            duration_ms: 0,
        });

        let avg = suite.calculate_average("cpu_usage_millicores", Some("kubernetes"));
        assert_eq!(avg, 150.0);
    }

    #[test]
    fn test_percentile_calculation() {
        let mut suite = BenchmarkSuite::new();

        for i in 1..=10 {
            suite.add_result(BenchmarkResult {
                provider_type: "kubernetes".to_string(),
                metric_name: "startup_time_ms".to_string(),
                value: i as f64 * 100.0,
                unit: "ms".to_string(),
                duration_ms: 0,
            });
        }

        let p50 = suite.calculate_percentile("startup_time_ms", 50.0, "kubernetes");
        let p95 = suite.calculate_percentile("startup_time_ms", 95.0, "kubernetes");
        let p99 = suite.calculate_percentile("startup_time_ms", 99.0, "kubernetes");

        assert!((p50 - 500.0).abs() < 0.1);
        assert!((p95 - 950.0).abs() < 0.1);
        assert!((p99 - 990.0).abs() < 0.1);
    }

    #[test]
    fn test_comparison_report() {
        let mut suite = BenchmarkSuite::new();

        // Add Kubernetes results
        suite.add_result(BenchmarkResult {
            provider_type: "kubernetes".to_string(),
            metric_name: "startup_time_ms".to_string(),
            value: 1500.0,
            unit: "ms".to_string(),
            duration_ms: 1500,
        });

        // Add Docker results
        suite.add_result(BenchmarkResult {
            provider_type: "docker".to_string(),
            metric_name: "startup_time_ms".to_string(),
            value: 800.0,
            unit: "ms".to_string(),
            duration_ms: 800,
        });

        let report = suite.generate_comparison_report();
        assert!(report.contains("PERFORMANCE BENCHMARK REPORT"));
        assert!(report.contains("Startup Time Comparison"));
        assert!(report.contains("Kubernetes is slower"));
    }
}

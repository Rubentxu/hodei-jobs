//! Worker Provider Implementations
//!
//! This module contains production-ready implementations of the WorkerProvider trait
//! for different container orchestration platforms.

pub mod docker;
pub mod firecracker;
pub mod kubernetes;
pub mod kubernetes_health;
pub mod kubernetes_hpa;
pub mod kubernetes_metrics;
pub mod kubernetes_validator;

pub mod metrics_collector;

pub use docker::{DockerProvider, DockerProviderBuilder};
pub use firecracker::{
    FirecrackerConfig, FirecrackerConfigBuilder, FirecrackerCreationGuard,
    FirecrackerNetworkConfig, FirecrackerProvider, NetworkResources,
};
pub use kubernetes::{
    KubernetesConfig, KubernetesConfigBuilder, KubernetesProvider, KubernetesProviderBuilder,
    KubernetesToleration,
};
pub use kubernetes_health::{
    HealthCheckResult, HealthStatus, KubernetesHealthChecker, KubernetesSLIs, KubernetesSLOs,
};
pub use kubernetes_hpa::{HPAConfig, KubernetesHPAManager};
pub use kubernetes_metrics::KubernetesProviderMetrics;
pub use kubernetes_validator::KubernetesConnectionValidator;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_worker_provider;

#[cfg(any(test, feature = "test-utils"))]
pub use test_worker_provider::{TestWorkerConfig, TestWorkerProvider, TestWorkerProviderBuilder};

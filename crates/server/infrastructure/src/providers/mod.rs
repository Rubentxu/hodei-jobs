//! Worker Provider Implementations
//!
//! This module contains production-ready implementations of the WorkerProvider trait
//! for different container orchestration platforms.

pub mod docker;
pub mod firecracker;
pub mod kubernetes;
pub mod kubernetes_hpa;

pub use docker::{DockerProvider, DockerProviderBuilder};
pub use firecracker::{
    FirecrackerConfig, FirecrackerConfigBuilder, FirecrackerNetworkConfig, FirecrackerProvider,
};
pub use kubernetes::{
    KubernetesConfig, KubernetesConfigBuilder, KubernetesProvider, KubernetesToleration,
};
pub use kubernetes_hpa::{HPAConfig, KubernetesHPAManager};

#[cfg(any(test, feature = "test-utils"))]
pub mod test_worker_provider;

#[cfg(any(test, feature = "test-utils"))]
pub use test_worker_provider::{TestWorkerConfig, TestWorkerProvider, TestWorkerProviderBuilder};

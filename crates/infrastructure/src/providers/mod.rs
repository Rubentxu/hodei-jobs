//! Worker Provider Implementations
//!
//! This module contains production-ready implementations of the WorkerProvider trait
//! for different container orchestration platforms.

pub mod docker;
pub mod firecracker;
pub mod kubernetes;

pub mod test_worker_provider;

pub use docker::DockerProvider;
pub use firecracker::{
    FirecrackerConfig, FirecrackerConfigBuilder, FirecrackerNetworkConfig, FirecrackerProvider,
    IpPool, MicroVMResources,
};
pub use kubernetes::{
    KubernetesConfig, KubernetesConfigBuilder, KubernetesProvider, KubernetesToleration,
};

pub use test_worker_provider::{TestWorkerConfig, TestWorkerProvider, TestWorkerProviderBuilder};

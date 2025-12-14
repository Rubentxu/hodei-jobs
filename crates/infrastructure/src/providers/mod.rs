//! Worker Provider Implementations
//!
//! This module contains production-ready implementations of the WorkerProvider trait
//! for different container orchestration platforms.

pub mod docker;
pub mod kubernetes;
pub mod firecracker;

pub use docker::DockerProvider;
pub use kubernetes::{KubernetesConfig, KubernetesConfigBuilder, KubernetesProvider, KubernetesToleration};
pub use firecracker::{
    FirecrackerConfig, FirecrackerConfigBuilder, FirecrackerNetworkConfig,
    FirecrackerProvider, IpPool, MicroVMResources,
};

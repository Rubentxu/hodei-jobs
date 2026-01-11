//! Hodei Operator - Kubernetes Operator for Hodei Jobs Platform
//!
//! This operator acts as a bridge between Kubernetes and Hodei Server via gRPC.
//! It watches CRDs (Job, WorkerPool, ProviderConfig) and translates them into
//! gRPC calls to the Hodei server, providing a K8s-native experience.

pub mod crd;
pub mod grpc;
pub mod watcher;
#[cfg(feature = "leptos")]
pub mod web;

pub use grpc::GrpcClient;
pub use watcher::OperatorState;

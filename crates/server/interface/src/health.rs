//! Health Check Service Interface
//!
//! Re-exports health check types from domain layer.

pub use hodei_server_domain::health::{
    ComponentHealth, HealthCheckConfig, HealthCheckError, HealthCheckResponse, HealthChecker,
    HealthStatus, LivenessResponse, ReadinessResponse, SagaHealthChecker, SagaHealthResponse,
};

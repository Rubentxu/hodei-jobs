//! gRPC Services for Hodei Job Platform
//!
//! This crate provides gRPC service implementations following DDD architecture.
//! It acts as an **adapter layer** between gRPC transport and application services.
//!
//! # Architecture
//!
//! ```text
//! gRPC Request → [grpc adapter] → Application Service → Domain → Response
//! ```

pub mod interceptors;
pub mod services;
pub mod worker_command_sender;

#[cfg(test)]
mod tests;

// Re-export proto types for convenience
pub use hodei_jobs::*;

use tonic::Status;

/// Error types for gRPC services
#[derive(Debug, thiserror::Error)]
pub enum GrpcError {
    #[error("Service error: {message}")]
    ServiceError { message: String },

    #[error("Validation error: {message}")]
    ValidationError { message: String },

    #[error("Resource error: {message}")]
    ResourceError { message: String },

    #[error("Not found: {message}")]
    NotFound { message: String },

    #[error("Internal error: {message}")]
    InternalError { message: String },
}

impl From<anyhow::Error> for GrpcError {
    fn from(error: anyhow::Error) -> Self {
        GrpcError::InternalError {
            message: error.to_string(),
        }
    }
}

impl From<GrpcError> for Status {
    fn from(error: GrpcError) -> Self {
        match error {
            GrpcError::ValidationError { message } => Status::invalid_argument(message),
            GrpcError::ResourceError { message } => Status::failed_precondition(message),
            GrpcError::NotFound { message } => Status::not_found(message),
            GrpcError::ServiceError { message } => Status::unavailable(message),
            GrpcError::InternalError { message } => Status::internal(message),
        }
    }
}

/// Result type for gRPC services
pub type GrpcResult<T> = Result<T, GrpcError>;
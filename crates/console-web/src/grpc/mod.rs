//! gRPC Client module for Hodei Jobs Server communication
//!
////! Provides a unified gRPC client wrapper for communicating with Hodei Jobs Server.

use thiserror::Error;

/// Errors that can occur when communicating with the gRPC server
#[derive(Debug, Error)]
pub enum GrpcClientError {
    #[error("Connection error: {0}")]
    Connection(#[from] tonic::transport::Error),

    #[error("Request error: {0}")]
    Request(#[from] tonic::Status),

    #[error("Service unavailable: {service}")]
    ServiceUnavailable { service: &'static str },
}

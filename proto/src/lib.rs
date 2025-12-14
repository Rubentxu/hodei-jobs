//! Generated Protocol Buffer types for Hodei Job Platform
//! 
//! This crate contains the generated Rust types from the Protocol Buffer definitions
//! for all gRPC services and messages used in the Hodei Job Platform.
//! 
//! # Architecture (DDD)
//! 
//! This crate is the **infrastructure layer** for gRPC message types only.
//! It re-exports all generated protobuf types and gRPC service traits.
//! 
//! Service implementations are in `hodei-jobs-grpc`.

// Include the generated types from hodei_all_in_one.proto
include!("generated/hodei.rs");

/// Provider management gRPC types
pub mod providers {
    include!("generated/hodei.providers.rs");
}

/// File descriptor set for gRPC reflection
pub const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!("generated/hodei_descriptor.bin");
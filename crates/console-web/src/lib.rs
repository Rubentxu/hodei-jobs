//! Console-Web Client - Direct gRPC Connection (No Fallback)
//!
//! This module provides direct gRPC client connection to Hodei Server.
//! HTTP fallback has been eliminated to simplify architecture.
//!
//! Now we use gRPC exclusively with automatic reconnection.

#![warn(missing_docs)]

pub mod grpc;
pub mod server;
pub mod types;

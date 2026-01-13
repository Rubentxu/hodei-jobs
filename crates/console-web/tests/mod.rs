//! Integration tests for hodei-console-web
//!
//! This module contains all integration tests including:
//! - Component tests (StatsCard, StatusBadge, DataTable)
//! - gRPC service tests with mocking
//! - Page integration tests

#[cfg(test)]
mod components;

#[cfg(test)]
mod grpc;

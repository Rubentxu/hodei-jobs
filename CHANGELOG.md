# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [v0.9.1] - 2024-12-26

### Added

- **Error Transparency System**: Comprehensive error handling and reporting
  - `JobFailureReason` enum with 12 categories (CommandNotFound, PermissionDenied, FileNotFound, ProcessSpawnFailed, ExecutionTimeout, SignalReceived, NonZeroExitCode, InfrastructureError, ValidationError, ConfigurationError, SecretInjectionError, IoError, Unknown)
  - `suggested_actions()` method providing user guidance for each error type
  - `is_user_error()` and `is_infrastructure_error()` classification methods
  - `DispatchFailureReason`, `ProvisioningFailureReason`, `SchedulingFailureReason`, and `ProviderErrorType` enums

- **New Domain Events** for error transparency:
  - `JobExecutionError`: Published when a job fails during execution
  - `JobDispatchFailed`: Published when job dispatch to worker fails
  - `WorkerProvisioningError`: Published when worker provisioning fails
  - `SchedulingDecisionFailed`: Published when scheduling decision cannot be made
  - `ProviderExecutionError`: Published when provider encounters an error

- **Error Categorizer Module**: `crates/worker/infrastructure/src/error_categorizer.rs`
  - Converts raw error messages into structured `JobFailureReason` types
  - Supports categorization for permission denied, file not found, timeout, signals, and more

- **gRPC API Extensions**:
  - `JobFailureReason` enum in common.proto
  - `ErrorDetails` message for structured error information
  - Updated `JobFailedEvent` with error details
  - Enhanced `FailJobRequest` with error type, suggested actions, and error context
  - Enhanced `FailJobResponse` with processed error details

- **Error Documentation**: `docs/ERRORS.md`
  - Complete error code reference (ERR_1001-ERR_5008)
  - Troubleshooting guide for each error type
  - Recovery procedures and emergency actions
  - Event documentation with JSON examples

### Changed

- All error types now include structured information for better diagnostics
- gRPC responses include error details and suggested actions
- Improved job failure tracking and correlation

## [v0.9.0] - 2024-12-26

### Fixed

- Updated EC2Config test to use correct struct fields

### Refactored

- Removed HTTP module exports from interface crate
- Updated dependencies and removed HTTP dependencies

### Build

- Updated binary and tests for new configuration

## [v0.8.0] - 2024-12-20

### Added

- Multi-provider support (Docker, Kubernetes, Firecracker)
- Worker lifecycle management
- gRPC-based communication
- Event-driven architecture with domain events

[Unreleased]: https://github.com/hodei-platform/hodei-job-platform/compare/v0.9.1...HEAD
[v0.9.1]: https://github.com/hodei-platform/hodei-job-platform/compare/v0.9.0...v0.9.1
[v0.9.0]: https://github.com/hodei-platform/hodei-job-platform/releases/tag/v0.9.0
[v0.8.0]: https://github.com/hodei-platform/hodei-job-platform/releases/tag/v0.8.0

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Hodei Jobs Platform - A distributed HPC-ready job execution platform built in Rust with Zero Trust security. Features automatic worker provisioning on Docker, Kubernetes, or Firecracker microVMs.

## Build Commands

```bash
# Debug build
cargo build --workspace

# Release build
cargo build --workspace --release

# Single crate build
cargo build -p hodei-server-bin
```

## Testing

```bash
# Run all tests
cargo test --workspace

# Run tests for specific crate
cargo test -p hodei-server-infrastructure

# Run a single test
cargo test test_worker_registration_with_otp

# Run ignored tests (require Docker/PostgreSQL/K8s)
cargo test -- --ignored

# Provider-specific integration tests
HODEI_K8S_TEST=1 cargo test -p hodei-server-infrastructure --test kubernetes_integration
HODEI_FC_TEST=1 cargo test -p hodei-server-infrastructure --test firecracker_integration
```

## Code Quality

```bash
# Format code
cargo fmt --all

# Lint with clippy
cargo clippy --workspace -- -D warnings

# Check for dead code
cargo check --all-targets
```

## Architecture

This project follows **Hexagonal Architecture** (Ports & Adapters) with clear separation of concerns:

### Crate Structure

```
crates/
├── server/          # Server-side components
│   ├── domain/      # Domain models, events, business rules (pure Rust)
│   ├── application/ # Use cases, services, orchestrators
│   ├── infrastructure/ # Database, messaging, provider implementations
│   ├── interface/   # gRPC/REST adapters, DTOs, API definitions
│   └── bin/         # Server binary entrypoint
├── worker/          # Worker agent components
│   ├── domain/      # Worker domain models
│   ├── application/ # Worker orchestration
│   └── infrastructure/ # Executor, logging, secret injection
├── shared/          # Shared types, error handling, utilities
└── cli/             # CLI tool
```

### Domain-Driven Design Patterns

- **Domain Events**: Published for significant business events (WorkerRegistered, JobCreated, JobStatusChanged)
- **Value Objects**: ProviderId, JobId, WorkerId with validation
- **Repositories**: Abstraction for persistence (PostgresJobRepository, PostgresWorkerRegistry)
- **Event Bus**: Async event publishing/subscription (PostgresEventBus)
- **Providers**: Pluggable infrastructure (DockerProvider, KubernetesProvider, FirecrackerProvider)

### Key Domain Concepts

- **Job**: Work unit with command, arguments, environment, resource requirements
- **Worker**: Execution agent registered via OTP, streams logs, reports results
- **Provider**: Infrastructure adapter for creating/destroying workers
- **Scheduler**: Smart provider selection based on labels, capabilities, health

### Provider Pattern

Each provider implements the `WorkerProvider` trait:
```rust
trait WorkerProvider {
    fn provider_id(&self) -> &ProviderId;
    async fn create_worker(&self, spec: &WorkerSpec) -> Result<WorkerHandle>;
    async fn get_worker_status(&self, handle: &WorkerHandle) -> Result<WorkerState>;
    async fn destroy_worker(&self, handle: &WorkerHandle) -> Result<>;
    async fn health_check(&self) -> Result<HealthStatus>;
}
```

## gRPC Services

Located in `crates/server/interface/src/grpc/`:
- **JobExecutionService**: Queue jobs, get status, cancel
- **SchedulerService**: Job scheduling, provider management
- **WorkerAgentService**: Worker registration, heartbeat, log streaming
- **LogStreamService**: Batched log streaming (90-99% overhead reduction)

## Event Flow

1. Client → `QueueJob` (gRPC)
2. JobController detects pending job → provisions worker with OTP
3. Worker starts → `RegisterWorker` with OTP
4. Server dispatches job via bidirectional stream
5. Worker executes → streams logs with batching
6. Worker reports result → Job marked completed/failed

## Protocol Buffers

Proto files in `proto/` generate gRPC code. Regenerate with:
```bash
cargo build -p hodei-jobs  # Triggers tonic-build
```

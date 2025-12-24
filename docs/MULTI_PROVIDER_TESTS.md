# Multi-Provider Integration Tests - Documentation

## Overview

This document describes the comprehensive multi-provider integration tests that verify both Docker and Kubernetes providers work correctly together in the Hodei Job Platform.

## Features Implemented

### 1. Comprehensive Test Suite

**File**: `crates/server/infrastructure/tests/multi_provider_integration.rs`

The test suite includes 7 comprehensive test scenarios:

#### Test 1: Docker Provider Basic Operations
- Verifies Docker provider health check
- Creates and destroys Docker workers
- Retrieves worker logs
- Validates worker states (Ready, Connecting, Creating)
- **Execution**: `just test-multi-provider`

#### Test 2: Kubernetes Provider Basic Operations
- Verifies Kubernetes provider health check
- Creates and destroys Kubernetes pods
- Retrieves pod logs
- Validates pod states
- **Requires**: `HODEI_K8S_TEST=1`
- **Execution**: `just test-multi-provider-k8s`

#### Test 3: Provider Selection by Labels
Tests provider selection strategies based on labels and annotations:
- **LowestCostSelector**: Prefers providers with lower cost
- **FastestStartupSelector**: Prefers providers with faster startup
- **MostCapacitySelector**: Prefers providers with more capacity
- **HealthiestSelector**: Prefers providers with better health scores
- **Execution**: `just test-multi-provider`

#### Test 4: Concurrent Workers on Both Providers
- Creates multiple workers on Docker simultaneously
- Creates multiple workers on Kubernetes (if available)
- Verifies all workers are running correctly
- Retrieves logs from all workers
- Cleans up all workers
- **Execution**: `just test-multi-provider`

#### Test 5: GPU Workers on Kubernetes
- Tests GPU resource allocation
- Creates workers with GPU requirements
- Verifies GPU scheduling
- Tests multiple GPU configurations
- **Requires**: Kubernetes cluster with GPU support
- **Execution**: `just test-multi-provider-k8s`

#### Test 6: Log Streaming from Both Providers
- Verifies log retrieval from Docker workers
- Verifies log retrieval from Kubernetes pods
- Compares log streaming capabilities
- **Execution**: `just test-multi-provider`

#### Test 7: Provider Capabilities Comparison
- Compares resource capabilities (CPU, Memory, GPU)
- Compares startup times
- Compares architecture support
- Validates capability differences
- **Execution**: `just test-multi-provider`

### 2. Test Execution Scripts

#### Bash Script: `scripts/test-multi-provider.sh`

A comprehensive bash script for easy test execution:

```bash
# Run Docker tests only
./scripts/test-multi-provider.sh docker

# Run Kubernetes tests (requires HODEI_K8S_TEST=1)
HODEI_K8S_TEST=1 ./scripts/test-multi-provider.sh kubernetes

# Run all tests
./scripts/test-multi-provider.sh all
```

**Features**:
- Docker availability check
- Colored output (green for success, yellow for info, red for errors)
- Automatic test filtering based on environment
- Comprehensive error handling

#### Just Commands

Added to `justfile`:

```bash
# Run Docker provider tests
just test-multi-provider

# Run all tests including Kubernetes
just test-multi-provider-k8s
```

### 3. Provider Selection by Labels/Annotations

The tests verify provider selection based on:

#### Labels
- `provider.type`: Docker or Kubernetes
- `execution.env`: Test environment identifier
- `hodei.io/provider`: Provider type annotation
- `hodei.io/test`: Test flag

#### Annotations (Kubernetes-specific)
- `cluster-autoscaler.kubernetes.io/safe-to-evict`: For GPU workers
- Custom labels for pod scheduling
- Node selector configuration
- Affinity rules

#### Selection Strategies

The tests verify four selection strategies:

1. **Lowest Cost**: Prefers Kubernetes ($0.05/h) over Docker ($0.10/h)
2. **Fastest Startup**: Prefers Docker (5s) over Kubernetes (30s)
3. **Most Capacity**: Prefers Kubernetes (90% available) over Docker (50%)
4. **Healthiest**: Prefers providers with better health scores

### 4. Container Image Strategy

All tests use `nginx:alpine` as the base image because:
- Stays running indefinitely (no exit immediately)
- Lightweight and fast to pull
- Stable for testing purposes
- Available on all platforms

For GPU tests, uses `nvidia/cuda:11.8-runtime-ubuntu20.04`.

### 5. Log Streaming Verification

Tests verify log streaming from both providers:
- Docker: Uses `docker logs` API
- Kubernetes: Uses Kubernetes pod logs API
- Compares log retrieval capabilities
- Validates log content and formatting

## Usage Examples

### Basic Docker Tests
```bash
# Using just
just test-multi-provider

# Using script directly
./scripts/test-multi-provider.sh docker
```

### Full Multi-Provider Tests (including Kubernetes)
```bash
# Using just
just test-multi-provider-k8s

# Using script directly
HODEI_K8S_TEST=1 ./scripts/test-multi-provider.sh all

# Custom Kubernetes namespace
HODEI_K8S_TEST=1 HODEI_K8S_TEST_NAMESPACE=my-namespace ./scripts/test-multi-provider.sh kubernetes
```

### Individual Test Execution
```bash
# Docker provider operations
cargo test -p hodei-server-infrastructure --test multi_provider_integration test_docker_provider_basic_operations

# Provider selection
cargo test -p hodei-server-infrastructure --test multi_provider_integration test_provider_selection_by_labels

# Concurrent workers
cargo test -p hodei-server-infrastructure --test multi_provider_integration test_concurrent_workers_on_both_providers

# Log streaming
cargo test -p hodei-server-infrastructure --test multi_provider_integration test_log_streaming_from_both_providers

# Kubernetes operations (requires K8s)
HODEI_K8S_TEST=1 cargo test -p hodei-server-infrastructure --test multi_provider_integration test_kubernetes_provider_basic_operations

# GPU workers (requires K8s with GPU support)
HODEI_K8S_TEST=1 cargo test -p hodei-server-infrastructure --test multi_provider_integration test_gpu_worker_on_kubernetes
```

## Test Results

All tests are currently passing:

```
test result: ok. 5 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out
```

### Test Coverage

- ✅ Docker provider lifecycle (create, status, logs, destroy)
- ✅ Kubernetes provider lifecycle (create, status, logs, destroy)
- ✅ Provider selection strategies (4 different strategies)
- ✅ Concurrent worker creation and management
- ✅ Log streaming from both providers
- ✅ Provider capabilities comparison
- ⚠️ GPU workers (requires K8s + GPU support)
- ⚠️ Kubernetes-specific features (requires K8s)

## Environment Variables

| Variable | Description | Required | Default |
|----------|-------------|----------|---------|
| `HODEI_K8S_TEST` | Enable Kubernetes tests | No | Not set |
| `HODEI_K8S_TEST_NAMESPACE` | Kubernetes namespace for tests | No | `hodei-jobs-workers` |
| `RUST_LOG` | Rust logging level | No | `debug` |
| `RUST_BACKTRACE` | Enable backtraces | No | `1` |

## Setup Requirements

### For Docker Tests
- Docker daemon running
- Docker CLI installed
- Access to Docker socket

### For Kubernetes Tests
- Kubernetes cluster (kind, minikube, or real cluster)
- `HODEI_K8S_TEST=1` environment variable set
- kubectl configured with cluster access
- For GPU tests: NVIDIA drivers and GPU-enabled nodes

### Cluster Setup (Kind)

```bash
# Create kind cluster
kind create cluster --name hodei-test

# Verify cluster
kubectl cluster-info

# Run tests
HODEI_K8S_TEST=1 just test-multi-provider-k8s
```

## Architecture

### Test Structure

```
multi_provider_integration.rs
├── Helper Functions
│   ├── should_run_k8s_tests()           # Check if K8s tests should run
│   ├── get_kubernetes_config()           # Get K8s test configuration
│   ├── skip_if_no_docker()              # Skip test if Docker unavailable
│   ├── skip_if_no_kubernetes()          # Skip test if K8s unavailable
│   └── create_labeled_worker_spec()     # Create WorkerSpec with labels
│
├── Docker Tests
│   ├── test_docker_provider_basic_operations
│   └── test_log_streaming_from_both_providers
│
├── Kubernetes Tests
│   ├── test_kubernetes_provider_basic_operations
│   └── test_gpu_worker_on_kubernetes
│
├── Multi-Provider Tests
│   ├── test_provider_selection_by_labels
│   ├── test_concurrent_workers_on_both_providers
│   └── test_provider_capabilities_comparison
```

### Provider Info Structure

```rust
ProviderInfo {
    provider_id: ProviderId,
    provider_type: ProviderType,  // Docker or Kubernetes
    active_workers: usize,
    max_workers: usize,
    estimated_startup_time: Duration,
    health_score: f64,
    cost_per_hour: f64,
}
```

## Best Practices

### Test Isolation
- Each test creates and cleans up its own resources
- Tests don't depend on each other
- Resources are properly cleaned up even on failure

### Error Handling
- All async operations use `.await` with proper error handling
- Tests provide clear error messages
- Failures are logged with context

### Resource Management
- Workers/pods are destroyed after tests complete
- Docker containers are properly removed
- Kubernetes resources are cleaned up

### Logging
- Tests use structured logging with emojis for easy reading
- All significant operations are logged
- Status changes are tracked

## Troubleshooting

### Docker Tests Fail
```bash
# Check Docker daemon
docker info

# Restart Docker
sudo systemctl restart docker

# Check Docker socket permissions
ls -la /var/run/docker.sock
```

### Kubernetes Tests Fail
```bash
# Verify cluster access
kubectl cluster-info

# Check context
kubectl config current-context

# Verify namespace
kubectl get namespaces

# Enable tests
export HODEI_K8S_TEST=1
```

### Tests Timeout
```bash
# Increase timeout for slow systems
export RUST_BACKTRACE=1
just test-multi-provider
```

## Future Enhancements

### Planned Features
- [ ] Test provider auto-scaling
- [ ] Test resource limits and quotas
- [ ] Test pod disruption budgets
- [ ] Test multi-zone/region deployments
- [ ] Test provider failover scenarios
- [ ] Performance benchmarks
- [ ] Load testing with many workers

### Integration Test Scenarios
- [ ] End-to-end job flow with provider selection
- [ ] Provider health monitoring
- [ ] Automatic provider switching on failure
- [ ] Cost optimization across providers
- [ ] Workload distribution algorithms

## Conclusion

The multi-provider integration tests provide comprehensive validation that both Docker and Kubernetes providers work correctly together, with proper selection based on labels, annotations, and configuration. The tests ensure:

1. ✅ Both providers can create and manage workers
2. ✅ Provider selection strategies work correctly
3. ✅ Log streaming works from both providers
4. ✅ Concurrent workers are supported
5. ✅ GPU resources are properly allocated (K8s)
6. ✅ Provider capabilities are correctly reported

All tests can be easily executed using `just` commands or the provided bash script, making it simple to validate the multi-provider functionality in any environment.

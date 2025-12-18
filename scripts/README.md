# Hodei Scripts

Scripts for building, deploying, and managing the Hodei Jobs Platform v8.0.

## Quick Start

### 1. Development Environment Setup

```bash
# First time setup (installs all dependencies)
./scripts/Core_Development/setup.sh

# Start development environment (hot reload)
./scripts/Core_Development/dev.sh
```

### 2. Production Start

```bash
# Start production environment
./scripts/Core_Development/start.sh --build-worker
```

### 3. Run Test Jobs

```bash
# Maven job (simple - delegates to maven_job_with_logs.sh)
./scripts/Job_Execution/run_maven_job.sh

# Maven job with live logs (--simple or --complex)
./scripts/Job_Execution/maven_job_with_logs.sh --simple
./scripts/Job_Execution/maven_job_with_logs.sh --complex

# Trace specific job
./scripts/Job_Execution/trace-job.sh <job-id>
```

## Scripts Directory Structure

```
scripts/
├── Core_Development
│   ├── setup.sh              # Initial development environment setup
│   ├── dev.sh                # Rapid development workflow (hot reload)
│   ├── start.sh              # Production quick start
│   └── cleanup.sh            # Clean up Docker resources
│
├── Worker_Management
│   ├── rebuild_worker.sh     # Rebuild worker image with latest code
│   └── generate-certificates.sh # Generate PKI certificates for mTLS
│
├── Job_Execution
│   ├── run_maven_job.sh      # Run Maven build verification job (delegates to maven_job_with_logs.sh)
│   ├── maven_job_with_logs.sh # Maven job with live log streaming (--simple | --complex)
│   └── trace-job.sh          # Trace job from submission to completion
│
├── Monitoring_and_Debugging
│   ├── watch_logs.sh         # Monitor job logs in real-time with LogBatching
│   ├── list-jobs.sh          # List jobs with filters and search
│   └── test_e2e.sh           # Run end-to-end tests
│
└── Firecracker Provider
    └── firecracker/          # Firecracker microVM provider (optional)
```

## Worker Agent v8.0 - HPC Ready

### Optimizations Included

The worker agent v8.0 includes these optimizations **automatically**:

| Optimization | Script Reference | Description |
|--------------|------------------|-------------|
| **LogBatching** | `watch_logs.sh`, `maven_job_with_logs.sh` | 90-99% reduction in gRPC calls |
| **Zero-Copy I/O** | Internal to worker | Memory-efficient streaming |
| **Secret Injection** | Internal to worker | Secure via stdin with JSON |
| **mTLS** | `generate-certificates.sh` | Zero Trust PKI infrastructure |
| **Cached Metrics** | Internal to worker | 35s TTL cache, non-blocking |
| **Write-Execute Pattern** | Internal to worker | Jenkins/K8s-style script execution |
| **Backpressure Handling** | Internal to worker | try_send() for async stability |

### Using New Features

**Generate mTLS Certificates** (for Zero Trust):
```bash
./scripts/Worker_Management/generate-certificates.sh
```

**Monitor Performance** (logs show batching):
```bash
./scripts/Monitoring_and_Debugging/watch_logs.sh
```

## Docker/Kubernetes Providers

Both providers use the same worker image:

```bash
# Build worker image
docker build -f Dockerfile.worker -t hodei-jobs-worker:latest .

# Enable Docker provider
export HODEI_DOCKER_ENABLED=1

# Enable Kubernetes provider
export HODEI_K8S_ENABLED=1
export HODEI_K8S_NAMESPACE=hodei-jobs-workers
```

## Image Building

### Worker Image

Standard multi-stage build with Debian runtime and asdf support:

```bash
# Build for local development
docker build -f Dockerfile.worker -t hodei-jobs-worker:latest .

# Build with specific tag
docker build -f Dockerfile.worker -t hodei-jobs-worker:v8.0 .

# Rebuild worker with latest code
./scripts/rebuild_worker.sh
```

The Dockerfile.worker includes:
- Rust toolchain for building the worker
- asdf for managing runtime versions (Node.js, Python, etc.)
- Debian slim runtime with minimal dependencies
- Non-root user for security

## Common Tasks

### Monitor Jobs

```bash
# Watch all running jobs (with LogBatching)
./scripts/Monitoring_and_Debugging/watch_logs.sh

# Watch specific job
./scripts/Monitoring_and_Debugging/watch_logs.sh <job-id>

# List jobs with filters
./scripts/Monitoring_and_Debugging/list-jobs.sh --running
./scripts/Monitoring_and_Debugging/list-jobs.sh --search maven
./scripts/Monitoring_and_Debugging/list-jobs.sh --status COMPLETED
```

### Run Tests

```bash
# E2E tests (complete job flow)
./scripts/Monitoring_and_Debugging/test_e2e.sh --e2e

# Maven build test only
./scripts/Monitoring_and_Debugging/test_e2e.sh --maven

# All tests (unit, integration, e2e)
./scripts/Monitoring_and_Debugging/test_e2e.sh --all

# Specific test types
./scripts/Monitoring_and_Debugging/test_e2e.sh --unit
./scripts/Monitoring_and_Debugging/test_e2e.sh --integration
```

### Certificate Management

```bash
# Generate new mTLS certificates (Zero Trust)
./scripts/generate-certificates.sh

# Certificates are created in: certs/
# - Root CA (10 years)
# - Intermediate CA (3 years)
# - Worker certificates (90 days)
# - Server certificates (1 year)
```

### Worker Management

```bash
# Rebuild worker with latest code
./scripts/Worker_Management/rebuild_worker.sh

# Rebuild and restart containers
./scripts/Worker_Management/rebuild_worker.sh --restart

# Setup development environment
./scripts/Core_Development/setup.sh
./scripts/Core_Development/setup.sh --minimal
```

## Troubleshooting

### Common Issues

**Docker: Permission denied**
```bash
sudo usermod -aG docker $USER
# Logout and login again
```

**Worker image not found**
```bash
# Build the worker image
docker build -f Dockerfile.worker -t hodei-jobs-worker:latest .

# Or use the rebuild script
./scripts/Worker_Management/rebuild_worker.sh --restart
```

**API not responding**
```bash
# Check system status
docker ps --filter "name=hodei"

# Check job status
./scripts/list-jobs.sh

# Trace specific job without logs
./scripts/trace-job.sh <job-id> --no-logs
```

**Certificate errors (mTLS)**
```bash
# Regenerate mTLS certificates
./scripts/Worker_Management/generate-certificates.sh
```

**Development environment not ready**
```bash
# Run full setup
./scripts/Core_Development/setup.sh

# Or minimal setup
./scripts/Core_Development/setup.sh --minimal

# Start development
./scripts/Core_Development/dev.sh
```

## Performance Tips

### v8.0 Optimizations

1. **LogBatching is automatic** - No configuration needed
   - Logs are batched automatically every 100ms or 100 entries
   - Watch for "batch" messages in logs

2. **Metrics are cached** - Reduces overhead
   - 35-second cache TTL
   - Non-blocking collection

3. **Backpressure handling** - Prevents blocking
   - Uses try_send() instead of await
   - Monitors dropped messages

### Monitoring Performance

```bash
# Watch logs to see batching in action
./scripts/watch_logs.sh

# List jobs to see performance
./scripts/list-jobs.sh --limit 10

# Trace specific job performance
./scripts/trace-job.sh <job-id>
```

## Related Documentation

- [Getting Started Guide](../GETTING_STARTED.md) - Complete setup guide
- [Kubernetes Guide](../GETTING_STARTED_KUBERNETES.md) - K8s provider setup
- [Architecture Documentation](../docs/architecture.md) - System design
- [Workflow Documentation](../docs/workflows.md) - Detailed workflows (v8.0)
- [Security Documentation](../docs/security/) - mTLS and PKI (v8.0)

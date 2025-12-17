# Hodei Scripts

Scripts for building, deploying, and managing the Hodei worker images.

## Directory Structure

```
scripts/
├── rebuild_worker.sh       # Rebuild worker image
├── start.sh               # Quick start script
├── setup.sh               # Development environment setup
├── cleanup.sh             # Clean up Docker resources
├── e2e/                   # E2E test scripts
│   └── run-docker-e2e.sh
├── verification/          # Job verification scripts
│   └── maven_build_job.sh
├── watch_logs.sh          # Monitor job logs
├── run_maven_job.sh       # Run Maven test job
└── submit_optimized_job.sh # Submit optimized job
```

## Quick Start

### Build Worker Image

The worker image is built from the centralized `Dockerfile.worker` in the project root:

```bash
# Using the rebuild script
./scripts/rebuild_worker.sh

# Or directly with Docker
docker build -f Dockerfile.worker -t hodei-worker:latest .
```

### Docker/Kubernetes Providers

Both providers use the same worker image:

```bash
# Build image
docker build -f Dockerfile.worker -t hodei-worker:latest .

# Enable Docker provider
export HODEI_DOCKER_ENABLED=1

# Enable Kubernetes provider
export HODEI_K8S_ENABLED=1
export HODEI_K8S_NAMESPACE=hodei-workers
```

## Image Building

### Worker Image

Standard multi-stage build with Debian runtime and asdf support:

```bash
# Build for local development
docker build -f Dockerfile.worker -t hodei-worker:latest .

# Build with specific tag
docker build -f Dockerfile.worker -t hodei-worker:v1.0.0 .

# Build for E2E tests
docker build -f Dockerfile.worker -t hodei-worker:e2e-test .
```

The Dockerfile.worker includes:
- Rust toolchain for building the worker
- asdf for managing runtime versions (Node.js, Python, etc.)
- Debian slim runtime with minimal dependencies
- Non-root user for security

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
docker build -f Dockerfile.worker -t hodei-worker:latest .
```

## Related Documentation

- [Getting Started Guide](../GETTING_STARTED.md)
- [Kubernetes Guide](../GETTING_STARTED_KUBERNETES.md)
- [Deployment Guide](../docs/DEPLOYMENT.md)

# Docker Scripts

Scripts for building and managing Docker images for Hodei workers.

## Files

- `Dockerfile.worker` - Multi-stage Dockerfile for worker image
- `build-worker-image.sh` - Build script with options

## Quick Start

```bash
# Build default image
./scripts/docker/build-worker-image.sh

# Build with custom tag
./scripts/docker/build-worker-image.sh -t hodei-worker:v1.0.0

# Build and push to registry
./scripts/docker/build-worker-image.sh -r ghcr.io/myorg -t v1.0.0 -p
```

## Build Options

| Option | Description |
|--------|-------------|
| `-t, --tag TAG` | Image tag (default: `hodei-worker:latest`) |
| `-p, --push` | Push to registry after build |
| `-r, --registry REG` | Registry prefix (e.g., `ghcr.io/myorg`) |
| `--no-cache` | Build without Docker cache |

## Running the Worker

```bash
# Basic run
docker run -d \
  -e HODEI_WORKER_ID=$(uuidgen) \
  -e HODEI_SERVER_ADDRESS=http://host.docker.internal:50051 \
  -e HODEI_OTP_TOKEN=your-token \
  hodei-worker:latest

# With volume for logs
docker run -d \
  -e HODEI_WORKER_ID=$(uuidgen) \
  -e HODEI_SERVER_ADDRESS=http://host.docker.internal:50051 \
  -e HODEI_OTP_TOKEN=your-token \
  -v /var/log/hodei:/var/log/hodei \
  hodei-worker:latest
```

## Environment Variables

| Variable | Description | Required |
|----------|-------------|----------|
| `HODEI_WORKER_ID` | Unique worker identifier | Yes |
| `HODEI_SERVER_ADDRESS` | Control plane gRPC address | Yes |
| `HODEI_OTP_TOKEN` | Authentication token | Yes |
| `HODEI_LOG_LEVEL` | Log level (default: `info`) | No |
| `RUST_LOG` | Rust log filter | No |

## Image Details

The worker image is built using a multi-stage build:

1. **Builder stage**: Compiles Rust code with all dependencies
2. **Runtime stage**: Minimal Debian image with only runtime dependencies

Final image size: ~100MB (depending on dependencies)

## Customization

### Using a Different Base Image

Edit `Dockerfile.worker` to change the runtime base:

```dockerfile
# Alpine (smaller but may have compatibility issues)
FROM alpine:3.19 AS runtime

# Ubuntu
FROM ubuntu:24.04 AS runtime
```

### Adding Custom Tools

Add packages in the runtime stage:

```dockerfile
RUN apt-get update && apt-get install -y \
    your-package \
    && rm -rf /var/lib/apt/lists/*
```

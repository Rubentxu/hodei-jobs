# Hodei Scripts

Scripts for building, deploying, and managing Hodei worker images across different providers.

## Directory Structure

```
scripts/
├── docker/                 # Docker provider scripts
│   ├── Dockerfile.worker   # Worker image Dockerfile
│   ├── build-worker-image.sh
│   ├── check-env.sh
│   └── README.md
├── kubernetes/             # Kubernetes provider scripts
│   ├── Dockerfile.worker   # Distroless worker image
│   ├── build-worker-image.sh
│   ├── check-env.sh
│   └── README.md
├── firecracker/            # Firecracker provider scripts
│   ├── build-rootfs.sh     # Alpine rootfs builder
│   ├── check-env.sh
│   └── README.md
└── README.md               # This file
```

## Quick Start

### Docker Provider

```bash
# Check environment
./scripts/docker/check-env.sh

# Build worker image
./scripts/docker/build-worker-image.sh

# Enable provider
export HODEI_DOCKER_ENABLED=1
```

### Kubernetes Provider

```bash
# Check environment
./scripts/kubernetes/check-env.sh

# Build and push worker image
./scripts/kubernetes/build-worker-image.sh -r ghcr.io/myorg -t v1.0.0 -p

# Install K8s manifests
./deploy/kubernetes/install.sh

# Enable provider
export HODEI_K8S_ENABLED=1
export HODEI_K8S_NAMESPACE=hodei-workers
```

### Firecracker Provider

```bash
# Check environment
./scripts/firecracker/check-env.sh

# Build rootfs (requires root)
sudo ./scripts/firecracker/build-rootfs.sh

# Enable provider
export HODEI_FC_ENABLED=1
```

## Environment Verification

Each provider has a `check-env.sh` script that verifies:

| Provider | Checks |
|----------|--------|
| Docker | Docker daemon, socket permissions, worker image |
| Kubernetes | kubectl, cluster connectivity, RBAC, namespaces |
| Firecracker | KVM, binaries, kernel, rootfs |

Run all checks:

```bash
./scripts/docker/check-env.sh
./scripts/kubernetes/check-env.sh
./scripts/firecracker/check-env.sh
```

## Image Building

### Docker

Standard multi-stage build with Debian runtime:

```bash
./scripts/docker/build-worker-image.sh -t hodei-worker:v1.0.0
```

### Kubernetes

Distroless image optimized for security:

```bash
# Single architecture
./scripts/kubernetes/build-worker-image.sh -t hodei-worker:v1.0.0

# Multi-architecture (amd64 + arm64)
./scripts/kubernetes/build-worker-image.sh --multi-arch -r ghcr.io/myorg -p
```

### Firecracker

Alpine-based rootfs with init scripts:

```bash
sudo ./scripts/firecracker/build-rootfs.sh /var/lib/hodei/rootfs.ext4
```

## Provider Comparison

| Feature | Docker | Kubernetes | Firecracker |
|---------|--------|------------|-------------|
| Isolation | Container | Container | Hardware (KVM) |
| Startup Time | ~1s | ~5-15s | ~125ms |
| Resource Overhead | Low | Low | Very Low |
| Security | Good | Good | Excellent |
| GPU Support | Yes | Yes | No |
| Requirements | Docker | K8s Cluster | KVM Host |

## CI/CD Integration

### GitHub Actions

```yaml
jobs:
  build-images:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Build Docker image
        run: ./scripts/docker/build-worker-image.sh -t ${{ github.sha }}
      
      - name: Build K8s image
        run: |
          ./scripts/kubernetes/build-worker-image.sh \
            --multi-arch \
            -r ghcr.io/${{ github.repository }} \
            -t ${{ github.sha }} \
            -p
```

### GitLab CI

```yaml
build-images:
  stage: build
  script:
    - ./scripts/docker/build-worker-image.sh -t $CI_COMMIT_SHA
    - ./scripts/kubernetes/build-worker-image.sh -r $CI_REGISTRY_IMAGE -t $CI_COMMIT_SHA -p
```

## Troubleshooting

### Common Issues

**Docker: Permission denied**
```bash
sudo usermod -aG docker $USER
# Logout and login again
```

**Kubernetes: Cannot connect to cluster**
```bash
kubectl config view
kubectl cluster-info
```

**Firecracker: KVM not available**
```bash
# Check CPU virtualization
grep -E '(vmx|svm)' /proc/cpuinfo

# Load KVM module
sudo modprobe kvm_intel  # or kvm_amd
```

## Related Documentation

- [Docker Provider Design](../docs/providers/docker-provider-design.md)
- [Kubernetes Provider Design](../docs/providers/kubernetes-provider-design.md)
- [Firecracker Provider Design](../docs/providers/firecracker-provider-design.md)
- [Kubernetes Deployment](../deploy/kubernetes/README.md)

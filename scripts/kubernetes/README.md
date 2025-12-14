# Kubernetes Scripts

Scripts for building and deploying Hodei worker images in Kubernetes.

## Files

- `Dockerfile.worker` - Distroless-based Dockerfile optimized for K8s
- `build-worker-image.sh` - Build script with multi-arch support

## Quick Start

```bash
# Build default image
./scripts/kubernetes/build-worker-image.sh

# Build with custom tag
./scripts/kubernetes/build-worker-image.sh -t hodei-worker:v1.0.0

# Build multi-arch and push to registry
./scripts/kubernetes/build-worker-image.sh --multi-arch -r ghcr.io/myorg -t v1.0.0 -p
```

## Build Options

| Option | Description |
|--------|-------------|
| `-t, --tag TAG` | Image tag (default: `hodei-worker:latest`) |
| `-p, --push` | Push to registry after build |
| `-r, --registry REG` | Registry prefix (e.g., `ghcr.io/myorg`) |
| `--platform PLATFORM` | Target platform (default: `linux/amd64`) |
| `--multi-arch` | Build for amd64 and arm64 |
| `--no-cache` | Build without Docker cache |

## Deployment

### 1. Build and Push Image

```bash
# Build and push to your registry
./scripts/kubernetes/build-worker-image.sh \
  -r your-registry.io/hodei \
  -t v1.0.0 \
  -p
```

### 2. Apply Base Manifests

```bash
# Install Hodei Kubernetes resources
./deploy/kubernetes/install.sh
```

### 3. Configure Image Pull Secret (if private registry)

```bash
kubectl create secret docker-registry hodei-registry \
  --docker-server=your-registry.io \
  --docker-username=your-user \
  --docker-password=your-password \
  -n hodei-workers
```

### 4. Update Provider Configuration

Set environment variables for the Hodei server:

```bash
export HODEI_K8S_ENABLED=1
export HODEI_WORKER_IMAGE=your-registry.io/hodei/hodei-worker:v1.0.0
export HODEI_K8S_IMAGE_PULL_SECRET=hodei-registry
```

## Image Details

### Base Image

The Kubernetes image uses Google's Distroless base (`gcr.io/distroless/cc-debian12:nonroot`):

- **Minimal attack surface**: No shell, package manager, or unnecessary binaries
- **Non-root by default**: Runs as `nonroot` user (UID 65532)
- **Small size**: ~30MB base + application

### Multi-Architecture Support

Build for both amd64 and arm64:

```bash
./scripts/kubernetes/build-worker-image.sh --multi-arch -r ghcr.io/myorg -p
```

This creates a manifest list that works on:
- x86_64 nodes (Intel/AMD)
- ARM64 nodes (AWS Graviton, Apple Silicon)

## Environment Variables

| Variable | Description | Required |
|----------|-------------|----------|
| `HODEI_WORKER_ID` | Unique worker identifier | Yes |
| `HODEI_SERVER_ADDRESS` | Control plane gRPC address | Yes |
| `HODEI_OTP_TOKEN` | Authentication token | Yes |
| `HODEI_LOG_LEVEL` | Log level (default: `info`) | No |

## Security Considerations

### Pod Security Standards

The worker image is compatible with Kubernetes Pod Security Standards:

```yaml
securityContext:
  runAsNonRoot: true
  runAsUser: 65532
  readOnlyRootFilesystem: true
  allowPrivilegeEscalation: false
  capabilities:
    drop:
      - ALL
```

### Network Policies

Workers should only communicate with the control plane:

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: hodei-worker-egress
  namespace: hodei-workers
spec:
  podSelector:
    matchLabels:
      app: hodei-worker
  policyTypes:
  - Egress
  egress:
  - to:
    - namespaceSelector:
        matchLabels:
          app.kubernetes.io/name: hodei
    ports:
    - protocol: TCP
      port: 50051
```

## Troubleshooting

### Image Pull Errors

```bash
# Check image pull secret
kubectl get secret hodei-registry -n hodei-workers -o yaml

# Test pulling manually
docker pull your-registry.io/hodei/hodei-worker:v1.0.0
```

### Worker Not Starting

```bash
# Check pod logs
kubectl logs -n hodei-workers -l app=hodei-worker

# Check pod events
kubectl describe pod -n hodei-workers -l app=hodei-worker
```

### Permission Denied

Distroless images run as non-root. Ensure volumes are writable:

```yaml
securityContext:
  fsGroup: 65532
```

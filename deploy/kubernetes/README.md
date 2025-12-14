# Hodei Kubernetes Provider Deployment

This directory contains Kubernetes manifests for deploying the Hodei Kubernetes Provider.

## Prerequisites

- Kubernetes cluster (1.28+)
- `kubectl` configured with cluster access
- Cluster admin permissions (for RBAC setup)

## Quick Start

```bash
# Install all resources
./install.sh

# Or apply manually
kubectl apply -f namespace.yaml
kubectl apply -f rbac.yaml
kubectl apply -f network-policy.yaml
kubectl apply -f configmap.yaml
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `HODEI_K8S_ENABLED` | Enable Kubernetes provider | `0` |
| `HODEI_K8S_NAMESPACE` | Namespace for worker pods | `hodei-workers` |
| `HODEI_K8S_KUBECONFIG` | Path to kubeconfig (optional) | In-cluster |
| `HODEI_K8S_CONTEXT` | Kubeconfig context (optional) | Current |
| `HODEI_K8S_SERVICE_ACCOUNT` | ServiceAccount for workers | `hodei-worker` |
| `HODEI_K8S_IMAGE_PULL_SECRET` | Image pull secret name | - |
| `HODEI_K8S_DEFAULT_CPU_REQUEST` | Default CPU request | `100m` |
| `HODEI_K8S_DEFAULT_MEMORY_REQUEST` | Default memory request | `128Mi` |

### Manifests

| File | Description |
|------|-------------|
| `namespace.yaml` | Creates `hodei-workers` namespace |
| `rbac.yaml` | ServiceAccounts, Role, and RoleBinding |
| `network-policy.yaml` | Network policies for worker isolation |
| `configmap.yaml` | Provider configuration |

## RBAC Permissions

The Kubernetes provider requires the following permissions in the workers namespace:

```yaml
rules:
- apiGroups: [""]
  resources: ["pods"]
  verbs: ["create", "get", "list", "watch", "delete"]
- apiGroups: [""]
  resources: ["pods/log"]
  verbs: ["get"]
- apiGroups: [""]
  resources: ["pods/status"]
  verbs: ["get"]
```

## Network Policies

Worker pods are configured with:
- **Egress**: DNS, Hodei control plane (gRPC:50051), HTTPS (443)
- **Ingress**: Denied by default

## Verification

```bash
# Check namespace
kubectl get ns hodei-workers

# Check RBAC
kubectl get serviceaccount -n hodei-workers
kubectl get role -n hodei-workers
kubectl get rolebinding -n hodei-workers

# Check network policies
kubectl get networkpolicies -n hodei-workers

# Test pod creation (manual)
kubectl run test-worker --image=alpine --rm -it -n hodei-workers -- echo "Hello"
```

## Cleanup

```bash
kubectl delete namespace hodei-workers
kubectl delete namespace hodei-system
```

## Troubleshooting

### Pod stuck in Pending

Check node resources and scheduling constraints:
```bash
kubectl describe pod <pod-name> -n hodei-workers
```

### Permission denied errors

Verify RBAC is correctly applied:
```bash
kubectl auth can-i create pods -n hodei-workers --as=system:serviceaccount:hodei-system:hodei-control-plane
```

### Network connectivity issues

Check network policies:
```bash
kubectl describe networkpolicy -n hodei-workers
```

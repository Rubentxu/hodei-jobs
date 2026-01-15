# Kubernetes Provider Setup Guide

This guide describes how to set up the Kubernetes provider for the Hodei Jobs Platform.

## Prerequisites

- Kubernetes cluster (v1.20+)
- `kubectl` configured with cluster access
- Helm 3.x (optional, for templating)

## Namespace Setup

Create the dedicated namespace for Hodei jobs:

```bash
kubectl create namespace hodei-jobs
```

## RBAC Configuration

### Service Account

```yaml
# hodei-jobs-namespace/hodei-worker-sa.yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: hodei-worker
  namespace: hodei-jobs
```

### Role for Pod Management

```yaml
# hodei-jobs-namespace/hodei-worker-role.yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: hodei-worker
  namespace: hodei-jobs
rules:
- apiGroups: [""]
  resources: ["pods"]
  verbs: ["create", "delete", "get", "list", "watch"]
- apiGroups: [""]
  resources: ["pods/log"]
  verbs: ["get"]
- apiGroups: [""]
  resources: ["pods/status"]
  verbs: ["get"]
- apiGroups: [""]
  resources: ["events"]
  verbs: ["create", "patch"]
```

### RoleBinding

```yaml
# hodei-jobs-namespace/hodei-worker-rolebinding.yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata:
  name: hodei-worker
  namespace: hodei-jobs
subjects:
- kind: ServiceAccount
  name: hodei-worker
  namespace: hodei-jobs
roleRef:
  kind: Role
  name: hodei-worker
  apiGroup: rbac.authorization.k8s.io
```

### Optional: ClusterRole for Node Access

If workers need node-level resources:

```yaml
# cluster-scoped/hodei-worker-clusterrole.yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: hodei-worker
rules:
- apiGroups: [""]
  resources: ["nodes"]
  verbs: ["get", "list"]
```

## Image Pull Secrets

If using private container registry:

```bash
kubectl create secret docker-registry hodei-registry-secret \
  --docker-server=registry.example.com \
  --docker-username=admin \
  --docker-password=password \
  --namespace=hodei-jobs
```

Add to provider config:
```json
{
  "image_pull_secrets": ["hodei-registry-secret"]
}
```

## Node Selector (Optional)

Restrict pods to specific nodes:

```bash
kubectl label nodes worker-node-1 node-type=hodei-worker
```

Provider config:
```json
{
  "node_selector": {
    "node-type": "hodei-worker"
  }
}
```

## Tolerations (Optional)

For tainted nodes:

```json
{
  "tolerations": [
    {
      "key": "dedicated",
      "operator": "Equal",
      "value": "hodei-jobs",
      "effect": "NoSchedule"
    }
  ]
}
```

## In-Cluster vs External Kubeconfig

### In-Cluster (Recommended for GKE/EKS/AKS)

```json
{
  "kubeconfig_path": null
}
```

The provider will use the pod's service account credentials.

### External Kubeconfig

```json
{
  "kubeconfig_path": "/secrets/kubeconfig"
}
```

Mount the kubeconfig as a secret:
```yaml
volumeMounts:
- name: kubeconfig
  mountPath: /secrets
  readOnly: true
volumes:
- name: kubeconfig
  secret:
    secretName: hodei-kubeconfig
```

## Complete Example

```bash
# Apply all resources
kubectl apply -f hodei-jobs-namespace/
```

Provider config in database:
```sql
INSERT INTO provider_configs (
    id, name, provider_type, config, status, priority, max_workers,
    preferred_region, allowed_regions, required_labels, annotations
) VALUES (
    'a1b2c3d4-e5f6-7890-abcd-ef1234567890',
    'GKE Production',
    'kubernetes',
    '{
      "type": "kubernetes",
      "namespace": "hodei-jobs",
      "service_account": "hodei-worker",
      "default_image": "hodei-jobs-worker:v3",
      "node_selector": {"node-type": "hodei-worker"},
      "image_pull_secrets": ["hodei-registry-secret"]
    }'::jsonb,
    'ACTIVE',
    100,
    50,
    'us-central1',
    '["us-central1", "us-east1"]'::jsonb,
    '{"environment": "production"}'::jsonb,
    '{"compliance": "soc2"}'::jsonb
);
```

## Health Check Verification

Test the health check endpoint:

```bash
# From the server pod
kubectl exec -n hodei-jobs deploy/hodei-server -- \
  curl http://localhost:8080/health/providers

# Manual verification
kubectl auth can-i create pods --as=system:serviceaccount:hodei-jobs:hodei-worker -n hodei-jobs
```

## Troubleshooting

### Permission Denied Errors

1. Verify RoleBinding exists
2. Check service account is correct
3. Ensure namespace matches

### Image Pull Errors

1. Verify secret exists: `kubectl get secrets -n hodei-jobs`
2. Check registry credentials
3. Verify image exists and is accessible

### Node Selector Not Working

1. Verify nodes have labels: `kubectl get nodes --show-labels`
2. Check label key/value matches exactly
3. Consider using taints/tolerations instead

### Network Timeouts

1. Check pod network plugin is working
2. Verify service DNS resolution
3. Check network policies

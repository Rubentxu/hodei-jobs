# Hodei Jobs Platform - Production Deployment Guide

This guide covers deploying Hodei Jobs Platform to Kubernetes with GitOps practices.

## Prerequisites

- Kubernetes cluster (1.28+)
- Helm 3.8+
- kubectl configured
- External Secrets Operator (for secret management)

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Hodei Jobs Platform                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│  Server Deployment                                                           │
│  ├── gRPC Service (port 9090)                                                │
│  ├── HTTP Health Checks (port 8080)                                          │
│  ├── NATS JetStream Client                                                   │
│  └── PostgreSQL Client                                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│  Infrastructure                                                              │
│  ├── NATS Cluster (3 replicas)                                               │
│  ├── PostgreSQL (Primary + Read Replicas)                                    │
│  └── Prometheus Stack                                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│  External Dependencies                                                       │
│  ├── External Secrets (AWS/GCP/Azure/Vault)                                  │
│  ├── Ingress Controller (NGINX + Cert-Manager)                               │
│  └── Object Storage (S3/GCS/Azure)                                           │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Quick Start

### 1. Add Helm Repository

```bash
helm repo add hodei-jobs https://rubentxu.github.io/hodei-jobs-platform
helm repo update
```

### 2. Create Namespace

```bash
kubectl create namespace hodei-jobs
```

### 3. Install External Secrets Operator (if not installed)

```bash
helm repo add external-secrets https://charts.external-secrets.io
helm install external-secrets external-secrets/external-secrets \
  --namespace external-secrets \
  --create-namespace
```

### 4. Create Secret Store

**AWS Secrets Manager Example:**

```bash
# Create ClusterSecretStore
kubectl apply -f - <<EOF
apiVersion: external-secrets.io/v1beta1
kind: ClusterSecretStore
metadata:
  name: hodei-secret-store
spec:
  provider:
    aws:
      service: SecretsManager
      region: us-east-1
EOF
```

**Vault Example:**

```bash
kubectl apply -f - <<EOF
apiVersion: external-secrets.io/v1beta1
kind: ClusterSecretStore
metadata:
  name: hodei-secret-store
spec:
  provider:
    vault:
      server: "https://vault.example.com"
      path: "secret/data/hodei"
      version: "v2"
      auth:
        kubernetes:
          mountPath: /var/run/secrets/kubernetes.io/serviceaccount
          role: hodei-jobs
EOF
```

### 5. Create Secrets in Secret Manager

**AWS Secrets Manager:**

```bash
# PostgreSQL credentials
aws secretsmanager create-secret \
  --name hodei-jobs/postgresql \
  --secret-string '{"postgres-password":"your-secure-password","connection-string":"postgresql://..."}'

# NATS credentials
aws secretsmanager create-secret \
  --name hodei-jobs/nats \
  --secret-string '{"user":"hodei","password":"your-nats-password"}'

# Backup credentials
aws secretsmanager create-secret \
  --name hodei-jobs/backup \
  --secret-string '{"bucket":"hodei-backups","access-key":"...","secret-key":"..."}'

# gRPC TLS certificates
aws secretsmanager create-secret \
  --name hodei-jobs/grpc-tls \
  --secret-string '{"tls.crt":"---","tls.key":"---","ca.crt":"---"}'
```

**HashiCorp Vault:**

```bash
# Enable KV v2
vault secrets enable -path=secret kv-v2

# Create secrets
vault kv put secret/hodei/postgresql postgres-password="your-password" connection-string="..."
vault kv put secret/hodei/nats user="hodei" password="nats-password"
vault kv put secret/hodei/backup bucket="hodei-backups" access-key="..." secret-key="..."
vault kv put secret/hodei/grpc-tls tls.crt="..." tls.key="..." ca.crt="..."
```

### 6. Deploy with Helm

```bash
# Create values file
cat > hodei-values.yaml <<EOF
# Cluster configuration
global:
  clusterDomain: "cluster.local"

# Server configuration
image:
  repository: ghcr.io/rubentxu/hodei-jobs-platform
  tag: "v0.27.0"

# NATS configuration
nats:
  enabled: true
  replicaCount: 3
  jetstream:
    enabled: true
    maxMemoryStore: 1Gi
    maxFileStore: 10Gi

# PostgreSQL configuration
postgresql:
  enabled: true
  auth:
    existingSecret: "postgresql"
    postgresPassword: ""  # From External Secrets
  primary:
    persistence:
      size: 20Gi
  readReplicas:
    replicaCount: 0  # Set to 2+ for read scaling

# Kubernetes provider
kubernetesProvider:
  enabled: true
  namespace: "hodei-jobs-workers"
  enableDynamicNamespaces: true
  namespacePrefix: "hodei-tenant"

# High availability
autoscaling:
  enabled: true
  minReplicas: 3
  maxReplicas: 10

# External secrets
externalSecrets:
  enabled: true
  secretStore:
    name: "hodei-secret-store"
    provider: "aws"  # aws, gcp, azure, vault

# Monitoring
serviceMonitor:
  enabled: true
  labels:
    hodei.io/monitor: "true"

# Backup
backup:
  enabled: true
  schedule: "0 2 * * *"
  storage:
    type: "s3"
    bucket: "hodei-backups"
    region: "us-east-1"
    secretName: "backup-credentials"

# Ingress
ingress:
  enabled: true
  hosts:
    - host: hodei.example.com
  tls:
    - secretName: hodei-tls
      hosts:
        - hodei.example.com
EOF

# Install
helm install hodei-jobs hodei-jobs/hodei-jobs-platform \
  --namespace hodei-jobs \
  --values hodei-values.yaml \
  --wait --timeout 10m
```

## GitOps Deployment with ArgoCD

### 1. Install ArgoCD

```bash
kubectl create namespace argocd
kubectl apply -n argocd \
  -f https://raw.githubusercontent.com/argoproj/argo-cd/stable/manifests/install.yaml
```

### 2. Create Application Manifest

```yaml
# hodei-jobs-platform.yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: hodei-jobs-platform
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/rubentxu/hodei-jobs-platform.git
    targetRevision: main
    path: helm/hodei-jobs-platform
    helm:
      valueFiles:
        - values-production.yaml
  destination:
    server: https://kubernetes.default.svc
    namespace: hodei-jobs
  syncPolicy:
    automated:
      prune: true
      selfHeal: true
    syncOptions:
      - CreateNamespace=true
      - PrunePropagationPolicy=foreground
      - PruneLast=true
    retry:
      limit: 5
      backoff:
        duration: 5s
        factor: 2
        maxDuration: 3m
  ignoreDifferences:
  - group: apps
    kind: Deployment
    jsonPointers:
    - /spec/replicas
  revisionHistoryLimit: 10
```

### 3. Create Environment Values

**values-production.yaml:**

```yaml
# Production-specific configuration
global:
  clusterDomain: "cluster.internal"

image:
  tag: "v0.27.0"

replicaCount: 5

autoscaling:
  minReplicas: 5
  maxReplicas: 20
  targetCPUUtilizationPercentage: 65
  targetMemoryUtilizationPercentage: 70

nats:
  replicaCount: 3
  jetstream:
    maxMemoryStore: 2Gi
    maxFileStore: 20Gi

postgresql:
  readReplicas:
    replicaCount: 2

backup:
  schedule: "0 2 * * *"
  retention: "720h"  # 30 days

externalSecrets:
  secretStore:
    provider: "vault"
    vault:
      server: "https://vault.internal"
```

### 4. Apply to ArgoCD

```bash
kubectl apply -f hodei-jobs-platform.yaml
```

## Secret Management with External Secrets

### AWS Secrets Manager

```bash
# Install AWS Secrets Manager integration
helm install aws-secrets-manager \
  external-secrets/external-secrets \
  --set serviceAccount.create=true \
  --set "serviceAccount.annotations.eks.amazonaws.com/role-arn=arn:aws:iam::ACCOUNT:role/hodei-jobs-secrets-role"

# Sync secrets
kubectl apply -f <<EOF
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: postgresql
  namespace: hodei-jobs
spec:
  refreshInterval: 1h
  secretStoreRef:
    name: hodei-secret-store
    kind: ClusterSecretStore
  target:
    name: postgresql
    creationPolicy: Owner
  data:
    - secretKey: POSTGRES_PASSWORD
      remoteRef:
        key: hodei-jobs/postgresql
        property: postgres-password
    - secretKey: connection-string
      remoteRef:
        key: hodei-jobs/postgresql
        property: connection-string
EOF
```

### Vault

```bash
# Install Vault integration
helm install vault-secrets \
  external-secrets/external-secrets \
  --set serviceAccount.create=true \
  --set "serviceAccount.annotations.hashicorp.com/service-account=external-secrets"

# Configure Vault authentication
kubectl apply -f <<EOF
apiVersion: external-secrets.io/v1beta1
kind: ClusterSecretStore
metadata:
  name: hodei-secret-store
spec:
  provider:
    vault:
      server: "https://vault.internal"
      path: "secret/data/hodei"
      version: "v2"
      auth:
        kubernetes:
          mountPath: /var/run/secrets/kubernetes.io/serviceaccount
          role: hodei-jobs
EOF
```

## TLS Configuration

### Using Cert-Manager

```yaml
# certificates.yaml
apiVersion: cert-manager.io/v1
kind: Issuer
metadata:
  name: letsencrypt-prod
  namespace: hodei-jobs
spec:
  acme:
    server: https://acme-v02.api.letsencrypt.org/directory
    email: admin@example.com
    privateKeySecretRef:
      name: letsencrypt-prod-account-key
    solvers:
      - http01:
          ingress:
            class: nginx
---
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: hodei-tls
  namespace: hodei-jobs
spec:
  secretName: hodei-tls
  issuerRef:
    name: letsencrypt-prod
    kind: Issuer
  dnsNames:
    - hodei.example.com
```

### mTLS for NATS

```yaml
# nats-tls-config.yaml
apiVersion: v1
kind: Secret
metadata:
  name: nats-tls
  namespace: hodei-jobs
type: Opaque
stringData:
  ca.crt: |
    -----BEGIN CERTIFICATE-----
    ...
    -----END CERTIFICATE-----
  tls.crt: |
    -----BEGIN CERTIFICATE-----
    ...
    -----END CERTIFICATE-----
  tls.key: |
    -----BEGIN PRIVATE KEY-----
    ...
    -----END PRIVATE KEY-----
---
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: nats-tls
  namespace: hodei-jobs
spec:
  secretStoreRef:
    name: hodei-secret-store
    kind: ClusterSecretStore
  target:
    name: nats-tls
  data:
    - secretKey: ca.crt
      remoteRef:
        key: hodei-jobs/nats-tls
        property: ca.crt
    - secretKey: tls.crt
      remoteRef:
        key: hodei-jobs/nats-tls
        property: tls.crt
    - secretKey: tls.key
      remoteRef:
        key: hodei-jobs/nats-tls
        property: tls.key
```

## Monitoring Setup

### Prometheus ServiceMonitor

The chart includes a ServiceMonitor for Prometheus:

```yaml
# Already configured in values.yaml
serviceMonitor:
  enabled: true
  labels:
    prometheus.io/scrape: "true"
    hodei.io/monitor: "true"
```

### Grafana Dashboards

Import the Hodei Jobs Platform dashboard:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: hodei-jobs-grafana-dashboard
  namespace: monitoring
  labels:
    grafana_dashboard: "1"
data:
  hodei-jobs.json: |
    {
      "dashboard": {
        "title": "Hodei Jobs Platform",
        "panels": [
          {
            "title": "Job Queue Depth",
            "type": "graph",
            "targets": [
              {
                "expr": "hodei_jobs_queue_depth",
                "legendFormat": "Queue Depth"
              }
            ]
          },
          {
            "title": "Active Workers",
            "type": "stat",
            "targets": [
              {
                "expr": "hodei_workers_active",
                "legendFormat": "Active Workers"
              }
            ]
          }
        ]
      }
    }
```

## Scaling Considerations

### Horizontal Pod Autoscaler

The chart includes HPA by default. For custom scaling:

```yaml
autoscaling:
  enabled: true
  minReplicas: 3
  maxReplicas: 20
  targetCPUUtilizationPercentage: 70
  targetMemoryUtilizationPercentage: 80
  behavior:
    scaleDown:
      stabilizationWindowSeconds: 300
      policies:
        - type: Percent
          value: 10
          periodSeconds: 60
    scaleUp:
      stabilizationWindowSeconds: 60
      policies:
        - type: Percent
          value: 100
          periodSeconds: 15
```

### Vertical Pod Autoscaler

For VPA, add:

```yaml
verticalPodAutoscaler:
  enabled: true
  updatePolicy:
    updateMode: "Auto"
  resourcePolicy:
    containerPolicies:
      - containerName: hodei-jobs-platform
        minAllowed:
          cpu: 100m
          memory: 128Mi
        maxAllowed:
          cpu: 4000m
          memory: 4Gi
```

## Backup and Disaster Recovery

### Database Backup

The chart creates a CronJob for PostgreSQL backups:

```yaml
backup:
  enabled: true
  schedule: "0 2 * * *"  # Daily at 2 AM
  retention: "720h"  # 30 days
  targets:
    postgresql:
      enabled: true
```

### Restore Procedure

```bash
# 1. Stop the application
kubectl scale deployment hodei-jobs-platform --replicas=0 -n hodei-jobs

# 2. Restore database
kubectl exec -it hodei-jobs-platform-postgresql-0 -n hodei-jobs -- \
  psql -U postgres -c "DROP DATABASE IF EXISTS hodei_jobs"
kubectl exec -it hodei-jobs-platform-postgresql-0 -n hodei-jobs -- \
  psql -U postgres -c "CREATE DATABASE hodei_jobs"
gunzip -c /path/to/backup.sql.gz | \
  kubectl exec -i hodei-jobs-platform-postgresql-0 -n hodei-jobs -- psql -U postgres -d hodei_jobs

# 3. Restart the application
kubectl scale deployment hodei-jobs-platform --replicas=3 -n hodei-jobs
```

## Troubleshooting

### Check Pod Status

```bash
kubectl get pods -n hodei-jobs -l app.kubernetes.io/name=hodei-jobs-platform
kubectl describe pod <pod-name> -n hodei-jobs
kubectl logs <pod-name> -n hodei-jobs -f
```

### Check Events

```bash
kubectl get events -n hodei-jobs --sort-by='.lastTimestamp'
```

### NATS Connection Issues

```bash
# Check NATS pods
kubectl get pods -n hodei-jobs -l app.kubernetes.io/name=nats

# Check NATS connectivity
kubectl exec -it deploy/hodei-jobs-platform -n hodei-jobs -- \
  nats-box -s nats://hodei-jobs-platform-nats:4222 sub foo
```

### PostgreSQL Connection Issues

```bash
# Check PostgreSQL status
kubectl exec -it hodei-jobs-platform-postgresql-0 -n hodei-jobs -- pg_isready

# Check connections
kubectl exec -it hodei-jobs-platform-postgresql-0 -n hodei-jobs -- \
  psql -U postgres -c "SELECT count(*) FROM pg_stat_activity"
```

## Upgrades

```bash
# Check current version
helm list -n hodei-jobs

# Upgrade to new version
helm upgrade hodei-jobs hodei-jobs/hodei-jobs-platform \
  --namespace hodei-jobs \
  --values hodei-values.yaml \
  --wait --timeout 10m

# Rollback if needed
helm rollback hodei-jobs <revision> -n hodei-jobs
```

## Uninstall

```bash
# Uninstall release
helm uninstall hodei-jobs -n hodei-jobs

# Remove CRDs (if needed)
kubectl delete crd clustersecretstores.external-secrets.io \
  externalsecrets.external-secrets.io \
  secretstores.external-secrets.io
```

## Security Checklist

- [ ] Enable TLS for all services
- [ ] Use External Secrets for all credentials
- [ ] Enable Pod Disruption Budgets
- [ ] Configure Network Policies
- [ ] Enable Pod Security Standards
- [ ] Use Read-Only Root Filesystem
- [ ] Drop All Capabilities
- [ ] Enable Audit Logging
- [ ] Configure Resource Limits
- [ ] Enable Image Scanning

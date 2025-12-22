# Hodei Job Platform Helm Chart

This Helm chart deploys the Hodei Job Platform on Kubernetes with all necessary components for a production-ready deployment.

## Prerequisites

- Kubernetes 1.20+
- Helm 3.0+
- kubectl configured to interact with your cluster

## Installation

### Add the repository (if published)

```bash
helm repo add hodei https://rubentxu.github.io/hodei-job-platform
helm repo update
```

### Install the chart

```bash
# Basic installation
helm install hodei-jobs-platform hodei/hodei-jobs-platform

# Installation with custom values
helm install hodei-jobs-platform hodei/hodei-jobs-platform \
  --values values-production.yaml
```

## Configuration

### Basic Configuration

Edit `values.yaml` to customize your deployment:

```yaml
# Image configuration
image:
  repository: ghcr.io/rubentxu/hodei-jobs-platform
  tag: "v1.0.0"
  pullPolicy: IfNotPresent

# Replica count
replicaCount: 3

# Resource limits
resources:
  limits:
    cpu: 2000m
    memory: 2Gi
  requests:
    cpu: 500m
    memory: 512Mi

# PostgreSQL
postgresql:
  enabled: true
  auth:
    postgresPassword: "your-secure-password"
    database: "hodei_jobs"

# Redis
redis:
  enabled: true
```

### Kubernetes Provider Configuration

Configure the Kubernetes provider for worker pods:

```yaml
kubernetesProvider:
  enabled: true
  namespace: "hodei-jobs-workers"
  serviceAccount: "hodei-jobs-worker"
  defaultCpuRequest: "100m"
  defaultMemoryRequest: "128Mi"
  defaultCpuLimit: "1000m"
  defaultMemoryLimit: "512Mi"
  podSecurityStandard: "Restricted"
  enableHPA: true
  enableDynamicNamespaces: false
  namespacePrefix: "hodei-tenant"
```

### Ingress Configuration

Enable and configure ingress:

```yaml
ingress:
  enabled: true
  className: "nginx"
  annotations:
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
    cert-manager.io/cluster-issuer: "letsencrypt-prod"
  hosts:
    - host: hodei.example.com
      paths:
        - path: /
          pathType: Prefix
  tls:
    - secretName: hodei-tls
      hosts:
        - hodei.example.com
```

### Monitoring with Prometheus

Enable Prometheus monitoring:

```yaml
prometheus:
  enabled: true
  prometheus:
    prometheusSpec:
      serviceMonitorSelector:
        matchLabels:
          hodei.io/monitor: "true"
```

### Network Policies

Enable network policies for security:

```yaml
networkPolicy:
  enabled: true
  ingress:
    enabled: true
    ports:
      - port: 8080
        protocol: TCP
      - port: 9090
        protocol: TCP
  egress:
    enabled: true
```

## Upgrading

### Upgrade the release

```bash
# Upgrade with new values
helm upgrade hodei-jobs-platform hodei/hodei-jobs-platform \
  --values values-production.yaml

# Upgrade to a specific version
helm upgrade hodei-jobs-platform hodei/hodei-jobs-platform \
  --set image.tag=v1.1.0
```

### Check upgrade history

```bash
helm history hodei-jobs-platform
```

### Rollback to a previous version

```bash
helm rollback hodei-jobs-platform 1
```

## Backup and Restore

### Backup

The chart includes a backup script at `/scripts/backup.sh`:

```bash
# Run backup manually
kubectl exec -n hodei-jobs deployment/hodei-jobs-platform -- /scripts/backup.sh

# With S3 configuration
kubectl exec -n hodei-jobs deployment/hodei-jobs-platform -- \
  S3_BUCKET=my-backup-bucket /scripts/backup.sh

# Schedule automatic backups
kubectl create job --from=cronjob/backup-hodei manual-backup-$(date +%Y%m%d)
```

Create a CronJob for automated backups:

```yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: backup-hodei
  namespace: hodei-jobs
spec:
  schedule: "0 2 * * *"
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: backup
            image: bitnami/kubectl:latest
            command:
            - /scripts/backup.sh
            env:
            - name: S3_BUCKET
              value: "my-backup-bucket"
            volumeMounts:
            - name: scripts
              mountPath: /scripts
          volumes:
          - name: scripts
            configMap:
              name: hodei-backup-scripts
          restartPolicy: OnFailure
```

### Restore

The chart includes a restore script at `/scripts/restore.sh`:

```bash
# Restore from local file
kubectl exec -n hodei-jobs deployment/hodei-jobs-platform -- \
  /scripts/restore.sh --file /tmp/postgres_20231215_120000.sql.gz

# Restore from S3
kubectl exec -n hodei-jobs deployment/hodei-jobs-platform -- \
  /scripts/restore.sh --bucket my-backup-bucket
```

## Uninstalling the Chart

```bash
# Uninstall the release
helm uninstall hodei-jobs-platform

# Uninstall with preserve data (keep PVCs)
helm uninstall hodei-jobs-platform --keep-history
```

## Custom Resource Definitions (CRDs)

The chart includes CRDs for ProviderConfig:

```bash
# Apply CRDs
kubectl apply -f crds/

# Create a ProviderConfig
cat <<EOF | kubectl apply -f -
apiVersion: hodei.io/v1
kind: ProviderConfig
metadata:
  name: kubernetes-provider
  namespace: hodei-jobs
spec:
  type: kubernetes
  kubernetes:
    namespace: hodei-jobs-workers
    serviceAccount: hodei-jobs-worker
    podSecurityStandard: Restricted
    enableHPA: true
EOF
```

## Security Considerations

### Pod Security Standards

The chart enforces the `Restricted` Pod Security Standard by default:

```yaml
podSecurityContext:
  runAsNonRoot: true
  runAsUser: 1000
  runAsGroup: 1000
  fsGroup: 2000
  seccompProfile:
    type: RuntimeDefault

securityContext:
  allowPrivilegeEscalation: false
  readOnlyRootFilesystem: true
  capabilities:
    drop:
    - ALL
```

### Network Policies

Network policies are enabled by default to restrict traffic:

- Ingress: Only from ingress controller and same namespace
- Egress: Only to DNS, PostgreSQL, Redis, and system namespaces

## Troubleshooting

### Check pod status

```bash
kubectl get pods -n hodei-jobs -l app.kubernetes.io/name=hodei-jobs-platform
```

### View logs

```bash
kubectl logs -n hodei-jobs deployment/hodei-jobs-platform -f
```

### Check configuration

```bash
kubectl get configmap hodei-jobs-platform-config -o yaml
```

### Verify PostgreSQL connection

```bash
kubectl exec -n hodei-jobs deployment/hodei-jobs-platform-postgresql -- \
  psql -U postgres -d hodei_jobs -c "SELECT version();"
```

### Check HPA status

```bash
kubectl get hpa -n hodei-jobs
```

## Support

For issues and questions:

- GitHub: https://github.com/rubentxu/hodei-job-platform
- Documentation: https://docs.hodei.io
- Email: team@hodei.io

## License

This chart is licensed under the Apache 2.0 License.

# Provider Configuration Guide

This document describes how to configure and manage infrastructure providers for the Hodei Jobs Platform.

## Overview

Providers are infrastructure adapters that handle worker provisioning across different platforms (Docker, Kubernetes, Lambda, etc.). Each provider has configuration for:

- Connection parameters (credentials, endpoints)
- Resource limits and defaults
- Region and labeling constraints
- Health monitoring settings

## Provider Configuration

### Core Fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | String | Unique provider name |
| `provider_type` | Enum | Type: `docker`, `kubernetes`, `lambda`, etc. |
| `status` | Enum | `ACTIVE`, `DISABLED`, `MAINTENANCE`, `UNHEALTHY` |
| `priority` | Integer | Higher = preferred for scheduling (default: 0) |
| `max_workers` | Integer | Maximum concurrent workers (default: 10) |

### Filtering Fields (US-86.7)

| Field | Type | Description |
|-------|------|-------------|
| `preferred_region` | String? | Provider's preferred region for execution |
| `allowed_regions` | String[] | Regions where this provider can run jobs |
| `required_labels` | Map | Labels that must be present on the provider |
| `annotations` | Map | Metadata for compliance, tier, etc. |

### Example Provider Config (PostgreSQL)

```sql
INSERT INTO provider_configs (
    id, name, provider_type, config, status, priority, max_workers,
    preferred_region, allowed_regions, required_labels, annotations
) VALUES (
    '550e8400-e29b-41d4-a716-446655440000',
    'Production Kubernetes',
    'kubernetes',
    '{
      "type": "kubernetes",
      "namespace": "hodei-jobs",
      "service_account": "hodei-worker",
      "default_image": "hodei-jobs-worker:v3"
    }'::jsonb,
    'ACTIVE',
    100,
    20,
    'us-east-1',
    '["us-east-1", "us-west-2"]'::jsonb,
    '{"environment": "production", "tier": "high-performance"}'::jsonb,
    '{"compliance": "soc2", "audit": "enabled"}'::jsonb
);
```

## Provider Selection Algorithm

When scheduling a job, the system selects the best provider using this logic:

1. **Filter by Requirements**: Exclude providers that don't meet:
   - Resource requirements (CPU, memory, GPU)
   - Required labels
   - Required annotations
   - Allowed regions

2. **Region Fallback Strategy**:
   - If `preferred_region` specified → select providers with matching region
   - If no preferred match → filter by `allowed_regions`
   - If still no match → use any provider meeting basic requirements

3. **Ranking**: Sort remaining providers by:
   - Priority (higher first)
   - Current load (lower utilization first)

## Health Monitoring

Providers are monitored with configurable health checks:

```rust
// Health check timeout (default: 5 seconds)
health_check_timeout = 5s

// Health status types
- Healthy: Provider is fully operational
- Degraded: Provider has issues but works (e.g., partial permissions)
- Unhealthy: Provider cannot accept new workers
```

### Health Check Endpoints

| Provider | Check Method |
|----------|--------------|
| Docker | List containers via socket |
| Kubernetes | List pods in namespace |
| Lambda | Verify AWS credentials |

## Provider Types

### Docker

```json
{
  "type": "docker",
  "socket_path": "/var/run/docker.sock",
  "default_image": "hodei-jobs-worker:latest",
  "network": "host",
  "default_resources": {
    "cpu_limit": "2",
    "memory_limit": "4Gi"
  }
}
```

### Kubernetes

```json
{
  "type": "kubernetes",
  "kubeconfig_path": null,  // null = in-cluster
  "namespace": "hodei-jobs",
  "service_account": "hodei-worker",
  "default_image": "hodei-jobs-worker:latest",
  "node_selector": {
    "node-type": "worker"
  },
  "tolerations": [
    {"key": "dedicated", "operator": "Equal", "value": "worker", "effect": "NoSchedule"}
  ]
}
```

### Lambda

```json
{
  "type": "lambda",
  "region": "us-east-1",
  "runtime": "provided.al2",
  "timeout_seconds": 900,
  "memory_mb": 1024,
  "layers": ["arn:aws:lambda:us-east-1:123456789:layer:common:1"]
}
```

## Updating Providers

### Update via SQL

```sql
UPDATE provider_configs
SET
    status = 'ACTIVE',
    priority = 100,
    max_workers = 50,
    config = config || '{"default_image": "hodei-jobs-worker:v4"}'::jsonb,
    updated_at = NOW()
WHERE name = 'Production Kubernetes';
```

### Update via API (gRPC)

```protobuf
message UpdateProviderRequest {
    string provider_id = 1;
    ProviderConfig config = 2;
}
```

## Troubleshooting

### Provider Not Showing as Available

1. Check status is `ACTIVE`
2. Verify health check passes
3. Check `max_workers` hasn't been reached
4. Verify region/labels match job requirements

### Health Check Failures

| Symptom | Solution |
|---------|----------|
| Connection timeout | Check network access, credentials |
| Permission denied | Verify IAM roles, RBAC permissions |
| Resource exhausted | Increase provider capacity or add providers |

### Labels/Annotations Not Matching

- Labels/annotations are case-sensitive
- Use exact key-value matches
- Check for trailing spaces in values

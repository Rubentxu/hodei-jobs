# Provider Troubleshooting Guide

This guide helps diagnose and resolve common issues with provider configuration and operation.

## Provider Not Registering

### Symptoms
- Provider doesn't appear in `list_providers` output
- Provider shows as inactive despite configuration

### Diagnosis Steps

1. **Check database connection**:
   ```bash
   # Verify provider exists in database
   psql -h localhost -U hodei -d hodei -c "SELECT * FROM provider_configs;"
   ```

2. **Check server logs**:
   ```bash
   kubectl logs -n hodei-jobs deploy/hodei-server | grep -i "provider\|error"
   ```

3. **Verify provider config syntax**:
   ```bash
   # Validate JSON config
   echo '{"type": "kubernetes", "namespace": "hodei-jobs"}' | jq .
   ```

### Common Causes

| Cause | Solution |
|-------|----------|
| JSON syntax error | Validate JSON, check for trailing commas |
| Invalid provider_type | Use lowercase: `docker`, `kubernetes` |
| Missing required fields | Add required fields per provider type |
| Connection string error | Verify credentials, endpoints |

## Health Check Failures

### Symptoms
- Provider status is `UNHEALTHY`
- Jobs not being scheduled to provider
- Health check timeout errors in logs

### Diagnosis Steps

1. **Manual health check**:
   ```bash
   # Docker socket test
   docker -H unix:///var/run/docker.sock info

   # Kubernetes test
   kubectl --kubeconfig=/path/to/kubeconfig get pods -n hodei-jobs
   ```

2. **Check health check timeout**:
   ```bash
   # Default is 5 seconds
   # Increase if network is slow
   ```

3. **Verify permissions**:
   ```bash
   # Docker: Check socket permissions
   ls -la /var/run/docker.sock

   # Kubernetes: Check RBAC
   kubectl auth can-i create pods --as=system:serviceaccount:hodei-jobs:hodei-worker
   ```

### Health Status Meanings

| Status | Meaning | Action |
|--------|---------|--------|
| `Healthy` | Fully operational | No action needed |
| `Degraded` | Partial functionality | Check permissions, resources |
| `Unhealthy` | Cannot accept work | Fix connectivity/credentials |

### Common Health Check Issues

**Docker Provider**
```
Error: Got permission denied while trying to connect to the Docker daemon socket
Solution: Add user to docker group or chmod socket
```

**Kubernetes Provider**
```
Error:Unable to connect to the server: x509: certificate has expired or is not yet valid
Solution: Regenerate kubeconfig, check system clock
```

```
Error: User cannot create pods in namespace "hodei-jobs"
Solution: Verify Role and RoleBinding are applied
```

## Worker Provisioning Failures

### Symptoms
- Jobs stuck in `Provisioning` state
- Error messages about worker creation
- Provider shows healthy but no workers created

### Diagnosis Steps

1. **Check provider capacity**:
   ```sql
   SELECT name, max_workers, active_workers FROM provider_configs;
   ```

2. **Verify resource availability**:
   ```bash
   # Kubernetes
   kubectl describe nodes | grep -A5 "Allocated resources"

   # Docker
   docker stats
   ```

3. **Check for pod/job creation errors**:
   ```bash
   kubectl get events -n hodei-jobs --sort-by='.lastTimestamp'
   ```

### Common Causes and Solutions

| Error | Cause | Solution |
|-------|-------|----------|
| `ImagePullBackOff` | Wrong image name/registry | Verify image exists, check credentials |
| `CrashLoopBackOff` | Worker init failing | Check worker logs, fix configuration |
| `Insufficient cpu/memory` | Not enough cluster resources | Add nodes or reduce job requirements |
| `QuotaExceeded` | Namespace quota reached | Request more quota or reduce max_workers |

## Region/Label Filtering Issues

### Symptoms
- Provider not selected despite appearing eligible
- Jobs pending with no available providers

### Diagnosis Steps

1. **Check job requirements**:
   ```bash
   # View job requirements
   kubectl get job <job-id> -o jsonpath='{.spec.template.spec}'
   ```

2. **Check provider labels/regions**:
   ```sql
   SELECT name, preferred_region, allowed_regions, required_labels
   FROM provider_configs
   WHERE status = 'ACTIVE';
   ```

3. **Debug matching logic**:
   ```bash
   # Enable debug logging
   RUST_LOG=hodei=debug ./hodei-server
   ```

### Common Issues

**Labels not matching**:
- Labels are case-sensitive
- Must match exactly (key and value)
- Use `=` not `==` in queries

**Regions not overlapping**:
- Job's `allowed_regions` must intersect with provider's
- Empty provider `allowed_regions` means "any region"
- Empty job `allowed_regions` means "no restriction"

## Performance Issues

### Slow Health Checks

**Symptoms**: Provider occasionally shows as degraded

**Solution**:
```json
{
  "health_check_config": {
    "timeout_seconds": 10,
    "interval_seconds": 30
  }
}
```

### High Latency in Provider Selection

**Symptoms**: Job scheduling takes several seconds

**Diagnosis**:
```bash
# Check database query performance
EXPLAIN ANALYZE SELECT * FROM provider_configs WHERE status = 'ACTIVE';
```

**Solution**: Add indexes on frequently queried columns

## Database Migration Issues

### Migration Script Failures

**Error**: `column "preferred_region" already exists`

**Solution**:
```sql
-- Use IF NOT EXISTS in ALTER TABLE
ALTER TABLE provider_configs ADD COLUMN IF NOT EXISTS preferred_region VARCHAR(100);
```

### Missing Columns After Migration

**Check migration status**:
```bash
# List migrations applied
SELECT * FROM migrations ORDER BY applied_at DESC;
```

**Re-run specific migration**:
```bash
psql -h localhost -U hodei -d hodei -f migrations/core/20260201140000_add_provider_filtering_columns.sql
```

## Logging and Monitoring

### Enable Debug Logging

```bash
# Environment variable
export RUST_LOG=debug,hodei_server=debug

# Or in config
[logging]
level = "debug"
```

### Key Log Messages

| Message | Meaning |
|---------|---------|
| `Provider registered: {name}` | New provider added |
| `Health check passed for {provider}` | Provider healthy |
| `No available providers for job {id}` | All providers filtered out |
| `Selected provider {name} for job {id}` | Provider selected |

### Metrics to Monitor

```prometheus
# Provider health
hodei_provider_health_status{provider="kubernetes",status="healthy"}

# Worker capacity
hodei_provider_active_workers{provider="docker"}
hodei_provider_max_workers{provider="docker"}

# Selection failures
hodei_provider_selection_failures_total
```

## Emergency Procedures

### Disable a Provider

```sql
UPDATE provider_configs SET status = 'DISABLED' WHERE name = 'Problem Provider';
```

### Force Drain Workers

```sql
-- Set max_workers to current active count
UPDATE provider_configs SET max_workers = active_workers WHERE name = 'Drain Provider';
```

### Reset Provider Health

```sql
UPDATE provider_configs SET status = 'ACTIVE' WHERE name = 'Provider Name';
-- Then restart health check
```

### Rollback Provider Config

```sql
-- Requires backup of previous config
UPDATE provider_configs SET config = '{"previous": "config"}'::jsonb WHERE name = 'Provider Name';
```

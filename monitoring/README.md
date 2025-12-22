# Hodei Job Platform - Advanced Observability Guide

This document describes the observability features implemented in the Hodei Job Platform, focusing on Prometheus metrics, Grafana dashboards, health checks, and Service Level Objectives (SLOs).

## Overview

The Hodei Job Platform implements comprehensive observability features to help SREs and operators monitor the health and performance of the system. The observability stack includes:

1. **Prometheus Metrics** - Comprehensive metrics collection
2. **Grafana Dashboards** - Visual monitoring and alerting
3. **Health Checks** - Extended health monitoring with SLOs/SLIs
4. **Event Recording** - Detailed event tracking

## Prometheus Metrics

### Kubernetes Provider Metrics

The `KubernetesProviderMetrics` struct provides comprehensive metrics for monitoring the Kubernetes provider:

#### Worker Lifecycle Metrics
- `workers_created_total` - Total number of workers created
- `workers_deleted_total` - Total number of workers deleted
- `workers_failed_total` - Total number of workers that failed to create
- `workers_active` - Current number of active workers

#### Performance Metrics
- `pod_creation_duration_ms` - Time taken to create a pod
- `pod_deletion_duration_ms` - Time taken to delete a pod
- `api_request_errors` - Total number of API request errors
- `api_requests_total` - Total number of API requests

#### Resource Metrics
- `cpu_request_total` - Total CPU requests in millicores
- `memory_request_total` - Total memory requests in bytes
- `gpu_count_total` - Total number of GPUs requested

#### State Metrics
- `pods_by_state` - Number of pods by state (Running, Pending, Failed, etc.)

#### Operations Metrics
- `hpa_operations_total` - Total number of HPA operations
- `namespace_operations_total` - Total number of namespace operations
- `network_policy_violations` - Total number of network policy violations

### Using Metrics

```rust
use hodei_server_infrastructure::providers::KubernetesProviderMetrics;

// Create metrics instance
let mut metrics = KubernetesProviderMetrics::new();

// Record worker creation
metrics.increment_workers_created();
metrics.set_workers_active(10);

// Record pod creation time
metrics.observe_pod_creation(1500); // 1.5 seconds

// Record API request
metrics.record_api_request(false); // Success
metrics.record_api_request(true);  // Error

// Get metrics summary
println!("{}", metrics.get_summary());
```

## Grafana Dashboards

### Overview Dashboard

The main Grafana dashboard (`hodei-overview.json`) provides a comprehensive view of the system:

#### Key Panels
1. **Active Workers** - Current number of active workers
2. **Worker Creation Rate** - Rate of worker creation
3. **Pod Creation Duration (P95)** - 95th percentile of pod creation time
4. **API Error Rate** - Rate of API errors
5. **Workers Created/Deleted** - Time series of worker lifecycle
6. **Pod Creation Duration** - Time series of pod creation times (avg, P50, P95, P99)
7. **Pods by State** - Distribution of pods by state
8. **API Request Rate by Endpoint** - API usage breakdown
9. **Resource Utilization** - Total CPU, memory, and GPU usage
10. **HPA Operations** - Time series of HPA operations
11. **Network Policy Violations** - Time series of network policy violations

### Installing Grafana Dashboard

```bash
# Import dashboard via Grafana UI
# 1. Open Grafana in browser
# 2. Navigate to Dashboards > Import
# 3. Upload hodei-overview.json
# 4. Select Prometheus data source
# 5. Click Import
```

### Dashboard Features

- **Real-time metrics** with 30s refresh interval
- **Color-coded thresholds** for quick health assessment
- **Time series panels** for trend analysis
- **Pie charts** for state distribution
- **Statistical panels** (P50, P95, P99) for performance analysis

## Health Checks

### Extended Health Checks

The `KubernetesHealthChecker` provides comprehensive health monitoring:

#### Health Check Types

1. **Liveness Check** - Verify the provider is responsive
2. **Readiness Check** - Verify the provider is ready to serve traffic
3. **Startup Check** - Verify the provider has initialized successfully

#### Health Status Levels

- `Healthy` - All checks passing, normal operation
- `Degraded` - Some checks failing, reduced functionality
- `Unhealthy` - Critical checks failing, immediate attention required

### Using Health Checks

```rust
use hodei_server_infrastructure::providers::{
    KubernetesHealthChecker, HealthCheckResult
};

let checker = KubernetesHealthChecker::new(
    provider_id,
    client,
    "hodei-jobs-workers".to_string(),
);

// Perform liveness check
let liveness = checker.liveness_check().await?;
println!("Liveness: {:?}", liveness.status);

// Perform readiness check
let readiness = checker.readiness_check().await?;
println!("Readiness: {:?}", readiness.status);

// Check details
for (key, value) in &readiness.details {
    println!("  {}: {}", key, value);
}
```

## Service Level Objectives (SLOs) and Indicators (SLIs)

### SLOs

The platform defines clear SLOs for the Kubernetes provider:

```rust
use hodei_server_infrastructure::providers::KubernetesSLOs;

let slos = KubernetesSLOs::default();
// Or customize:
let custom_slos = KubernetesSLOs {
    max_pod_creation_time: 20.0,  // 20 seconds
    min_worker_creation_success_rate: 0.995,  // 99.5%
    max_api_error_rate: 0.005,  // 0.5%
    max_hpa_response_time: 30.0,  // 30 seconds
};
```

#### Default SLOs
- **Max Pod Creation Time**: 30 seconds
- **Min Worker Creation Success Rate**: 99%
- **Max API Error Rate**: 1%
- **Max HPA Response Time**: 60 seconds

### SLIs

The platform tracks key service level indicators:

```rust
use hodei_server_infrastructure::providers::KubernetesSLIs;

let slis = KubernetesSLIs {
    current_pod_creation_time: 15.0,
    worker_creation_success_rate: 0.996,
    current_api_error_rate: 0.003,
    current_hpa_response_time: 25.0,
    workers_created_last_hour: 120,
    workers_failed_last_hour: 1,
};

// Check SLO compliance
let (compliant, violations) = slis.check_slo_compliance(&slos);

if !compliant {
    println!("SLO violations detected:");
    for violation in violations {
        println!("  - {}", violation);
    }
}
```

### SLO Monitoring

SLO compliance is continuously monitored and reported. When SLOs are violated:

1. **Alert is triggered** - Notify operations team
2. **Root cause analysis** - Investigate the cause
3. **Corrective action** - Fix the issue
4. **Post-mortem** - Document the incident

## Event Recording

Events are recorded for all major operations:

### Event Types

- Worker lifecycle events (created, deleted, failed)
- HPA operations (scale up, scale down)
- Namespace operations (created, deleted)
- API request events (success, failure)
- Network policy violations
- Resource allocation events

### Event Structure

```json
{
  "timestamp": "2024-01-15T10:30:00Z",
  "event_type": "worker.created",
  "provider": "kubernetes",
  "worker_id": "worker-123",
  "namespace": "hodei-jobs-workers",
  "duration_ms": 1500,
  "success": true,
  "details": {
    "image": "alpine:latest",
    "cpu_request": "100m",
    "memory_request": "128Mi"
  }
}
```

## Alerting

### Recommended Alerts

1. **High Error Rate**
   - Condition: API error rate > 1% for 5 minutes
   - Severity: Critical

2. **Pod Creation Time**
   - Condition: P95 pod creation time > 30 seconds for 5 minutes
   - Severity: Warning

3. **Worker Creation Failures**
   - Condition: Worker creation failure rate > 5% for 10 minutes
   - Severity: Critical

4. **SLO Violations**
   - Condition: Any SLO violation detected
   - Severity: High

5. **Network Policy Violations**
   - Condition: Any network policy violation
   - Severity: Warning

### Alert Configuration

```yaml
groups:
- name: hodei-kubernetes
  rules:
  - alert: HighAPIErrorRate
    expr: rate(hodei_kubernetes_api_request_errors_total[5m]) / rate(hodei_kubernetes_api_requests_total[5m]) > 0.01
    for: 5m
    labels:
      severity: critical
    annotations:
      summary: "High API error rate detected"
      description: "API error rate is {{ $value | humanizePercentage }}"

  - alert: SlowPodCreation
    expr: histogram_quantile(0.95, rate(hodei_kubernetes_pod_creation_duration_seconds_bucket[5m])) > 30
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "Slow pod creation detected"
      description: "95th percentile pod creation time is {{ $value }}s"
```

## Best Practices

### Metrics Collection
1. **Use histograms** for duration metrics (pod creation, API requests)
2. **Use counters** for events (worker created, deleted, errors)
3. **Use gauges** for current state (active workers, resource usage)
4. **Label consistently** across all metrics

### Health Checks
1. **Check quickly** - Health checks should complete in < 1 second
2. **Fail fast** - Return early if critical dependency is down
3. **Be specific** - Provide detailed information about failures
4. **Monitor trends** - Track health check response times

### SLOs/SLIs
1. **Set realistic targets** - Based on historical performance
2. **Monitor continuously** - Check SLO compliance in real-time
3. **Alert on trends** - Before SLO violations occur
4. **Review regularly** - Adjust SLOs as system evolves

### Alerting
1. **Avoid alert fatigue** - Only alert on actionable issues
2. **Provide context** - Include relevant information in alerts
3. **Route appropriately** - Send alerts to the right team
4. **Document response** - Include runbook links in alerts

## Troubleshooting

### Common Issues

#### High Pod Creation Time
- Check cluster resource availability
- Review scheduling constraints (affinity, taints, tolerations)
- Check image pull performance
- Verify network connectivity

#### High API Error Rate
- Check Kubernetes API server health
- Review authentication/authorization
- Check network policies
- Verify rate limiting

#### Low Worker Success Rate
- Check resource availability
- Review pod specifications
- Check image accessibility
- Verify service account permissions

### Diagnostic Commands

```bash
# Check worker creation rate
kubectl get pods -n hodei-jobs-workers --sort-by=.metadata.creationTimestamp

# View pod creation logs
kubectl logs -n hodei-jobs deployment/hodei-jobs-platform | grep "Creating worker"

# Check HPA status
kubectl get hpa -n hodei-jobs

# View network policies
kubectl get networkpolicy -n hodei-jobs

# Check resource usage
kubectl top nodes
kubectl top pods -n hodei-jobs-workers
```

## References

- [Prometheus Documentation](https://prometheus.io/docs/)
- [Grafana Documentation](https://grafana.com/docs/)
- [Kubernetes Monitoring](https://kubernetes.io/docs/tasks/debug-application-cluster/)
- [SRE Book - Service Level Objectives](https://sre.google/sre-book/service-level-objectives/)

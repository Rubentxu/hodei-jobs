# Hodei Jobs Platform - gRPC API Reference

## 📋 Table of Contents

1. [Quick Start - Watching Job Logs](#quick-start---watching-job-logs)
2. [Available Services](#available-services)
3. [Job Execution Service](#job-execution-service)
4. [Log Stream Service](#log-stream-service)
5. [Scheduler Service](#scheduler-service)
6. [Worker Agent Service](#worker-agent-service)
7. [Audit Service](#audit-service)
8. [Metrics Service](#metrics-service)
9. [Provider Management Service](#provider-management-service)
10. [Common Types](#common-types)

---

## Quick Start - Watching Job Logs

### 🎯 Monitor Specific Job Logs

```bash
# Watch logs for a specific job ID
grpcurl -plaintext -d '{"job_id": "YOUR-JOB-ID", "include_history": true}' \
  localhost:50051 hodei.LogStreamService/SubscribeLogs
```

### 📊 List All Running Jobs

```bash
# Get all running jobs
grpcurl -plaintext -d '{"limit": 50, "status": "JOB_STATUS_RUNNING"}' \
  localhost:50051 hodei.JobExecutionService/ListJobs
```

### 📝 Get Job Details

```bash
# Get complete job information
grpcurl -plaintext -d '{"job_id": {"value": "YOUR-JOB-ID"}}' \
  localhost:50051 hodei.JobExecutionService/GetJob
```

### 🔍 Stream All Active Jobs

```bash
# Automatically watch all active jobs (RUNNING and ASSIGNED)
./scripts/watch_logs.sh

# Watch specific job only
./scripts/watch_logs.sh YOUR-JOB-ID
```

---

## Available Services

| Service | Description | Port |
|---------|-------------|------|
| `hodei.JobExecutionService` | Core job management (create, queue, execute) | 50051 |
| `hodei.LogStreamService` | Real-time log streaming | 50051 |
| `hodei.SchedulerService` | Job scheduling and queue management | 50051 |
| `hodei.WorkerAgentService` | Worker registration and management | 50051 |
| `hodei.AuditService` | Audit logs and compliance | 50051 |
| `hodei.MetricsService` | System and job metrics | 50051 |
| `hodei.ProviderManagementService` | Provider configuration | 50051 |

---

## Job Execution Service

### Core Operations

#### 1. Queue Job

Submit a new job for execution.

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'JSON'
{
  "job_definition": {
    "name": "job-name",
    "command": "/bin/bash",
    "arguments": ["-lc", "echo 'Hello World'"],
    "requirements": {
      "cpu_cores": 1.0,
      "memory_bytes": 1073741824,
      "disk_bytes": 1073741824
    },
    "timeout": {
      "execution_timeout": "300s"
    }
  },
  "queued_by": "user"
}
JSON
```

**Response:**
```json
{
  "success": true,
  "message": "Job queued successfully",
  "queued_at": "2025-12-17T16:30:00Z",
  "jobId": {
    "value": "uuid-here"
  }
}
```

#### 2. Get Job

Retrieve job details by ID.

```bash
grpcurl -plaintext -d '{"job_id": {"value": "JOB-ID"}}' \
  localhost:50051 hodei.JobExecutionService/GetJob
```

#### 3. List Jobs

List jobs with optional filters.

```bash
# List all jobs
grpcurl -plaintext -d '{"limit": 50}' \
  localhost:50051 hodei.JobExecutionService/ListJobs

# List running jobs only
grpcurl -plaintext -d '{"limit": 50, "status": "JOB_STATUS_RUNNING"}' \
  localhost:50051 hodei.JobExecutionService/ListJobs

# Search jobs by name
grpcurl -plaintext -d '{"limit": 50, "search_term": "maven"}' \
  localhost:50051 hodei.JobExecutionService/ListJobs
```

#### 4. Cancel Job

Cancel a running job.

```bash
grpcurl -plaintext -d '{
  "execution_id": {"value": "EXEC-ID"},
  "job_id": {"value": "JOB-ID"},
  "reason": "User requested cancellation"
}' localhost:50051 hodei.JobExecutionService/CancelJob
```

### Job Lifecycle Management

#### 5. Assign Job

Manually assign a job to a specific worker.

```bash
grpcurl -plaintext -d '{
  "job_id": {"value": "JOB-ID"},
  "worker_id": {"value": "WORKER-ID"}
}' localhost:50051 hodei.JobExecutionService/AssignJob
```

#### 6. Update Job Progress

Update progress for a running job.

```bash
grpcurl -plaintext -d '{
  "execution_id": {"value": "EXEC-ID"},
  "job_id": {"value": "JOB-ID"},
  "progress_percentage": 50,
  "current_stage": "Building",
  "message": "Compiling sources..."
}' localhost:50051 hodei.JobExecutionService/UpdateJobProgress
```

#### 7. Complete Job

Mark a job as completed (usually done by worker).

```bash
grpcurl -plaintext -d '{
  "execution_id": {"value": "EXEC-ID"},
  "job_id": {"value": "JOB-ID"},
  "exit_code": "0",
  "output": "Build successful",
  "error_output": ""
}' localhost:50051 hodei.JobExecutionService/CompleteJob
```

#### 8. Fail Job

Mark a job as failed (usually done by worker).

```bash
grpcurl -plaintext -d '{
  "execution_id": {"value": "EXEC-ID"},
  "job_id": {"value": "JOB-ID"},
  "error_type": "BuildError",
  "error_message": "Compilation failed",
  "retryable": true
}' localhost:50051 hodei.JobExecutionService/FailJob
```

### Event Streaming

#### 9. Get Execution Events

Retrieve events for a specific execution.

```bash
grpcurl -plaintext -d '{
  "execution_id": {"value": "EXEC-ID"},
  "event_types": ["JobStarted", "JobCompleted"]
}' localhost:50051 hodei.JobExecutionService/GetExecutionEvents
```

---

## Log Stream Service

### 10. Subscribe to Job Logs

Stream logs in real-time for a specific job.

```bash
# Basic subscription (new logs only)
grpcurl -plaintext -d '{"job_id": "JOB-ID"}' \
  localhost:50051 hodei.LogStreamService/SubscribeLogs

# Include historical logs
grpcurl -plaintext -d '{"job_id": "JOB-ID", "include_history": true}' \
  localhost:50051 hodei.LogStreamService/SubscribeLogs

# Get last 100 lines of history
grpcurl -plaintext -d '{"job_id": "JOB-ID", "include_history": true, "tail_lines": 100}' \
  localhost:50051 hodei.LogStreamService/SubscribeLogs
```

**Streaming Response Format:**
```json
{
  "jobId": "...",
  "line": "Log output line",
  "isStderr": false,
  "timestamp": "2025-12-17T16:30:00Z",
  "sequence": 1
}
```

### 11. Get Historical Logs

Get historical logs (non-streaming).

```bash
grpcurl -plaintext -d '{
  "job_id": "JOB-ID",
  "limit": 1000,
  "since_sequence": 0
}' localhost:50051 hodei.LogStreamService/GetLogs
```

---

## Scheduler Service

### 12. Schedule Job

Schedule a job for execution.

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.SchedulerService/ScheduleJob << 'JSON'
{
  "job_definition": {
    "name": "scheduled-job",
    "command": "/bin/bash",
    "arguments": ["-lc", "echo 'Scheduled'"],
    "scheduling": {
      "priority": "PRIORITY_NORMAL",
      "deadline": "3600s"
    }
  },
  "requested_by": "scheduler"
}
JSON
```

### 13. Get Job Queue Info

Get information about the job queue.

```bash
grpcurl -plaintext -d '{
  "queue_name": "default",
  "include_estimates": true
}' localhost:50051 hodei.SchedulerService/GetJobQueueInfo
```

### 14. Find Available Workers

Find workers that match job requirements.

```bash
grpcurl -plaintext -d '{
  "requirements": {
    "cpu_cores": 2.0,
    "memory_bytes": 2147483648
  },
  "max_results": 10
}' localhost:50051 hodei.SchedulerService/FindAvailableWorkers
```

### 15. Get Scheduler Status

Get current scheduler status.

```bash
grpcurl -plaintext -d '{"scheduler_name": "default"}' \
  localhost:50051 hodei.SchedulerService/GetSchedulerStatus
```

### 16. Requeue Job

Requeue a failed job for retry.

```bash
grpcurl -plaintext -d '{
  "job_id": {"value": "JOB-ID"},
  "reason": "Retry after fix",
  "delay": "60s",
  "max_attempts": 3
}' localhost:50051 hodei.SchedulerService/RequeueJob
```

---

## Worker Agent Service

### 17. Register Worker

Register a new worker (usually done by worker agents).

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.WorkerAgentService/Register << 'JSON'
{
  "auth_token": "worker-token",
  "worker_info": {
    "name": "worker-1",
    "version": "1.0.0",
    "hostname": "worker-host",
    "capacity": {
      "cpu_cores": 4.0,
      "memory_bytes": 8589934592,
      "disk_bytes": 107374182400
    }
  }
}
JSON
```

### 18. Get Available Workers

Get list of available workers.

```bash
grpcurl -plaintext -d '{
  "min_requirements": {
    "cpu_cores": 1.0,
    "memory_bytes": 1073741824
  },
  "max_results": 20
}' localhost:50051 hodei.WorkerAgentService/GetAvailableWorkers
```

### 19. Drain Worker

Drain a worker (prepare for shutdown).

```bash
grpcurl -plaintext -d '{
  "worker_id": {"value": "WORKER-ID"},
  "grace_period": "300s"
}' localhost:50051 hodei.WorkerAgentService/DrainWorker
```

### 20. Unregister Worker

Unregister a worker.

```bash
grpcurl -plaintext -d '{
  "worker_id": {"value": "WORKER-ID"},
  "reason": "Maintenance"
}' localhost:50051 hodei.WorkerAgentService/UnregisterWorker
```

---

## Audit Service

### 21. Get Audit Logs

Get audit logs with filters.

```bash
# Get all audit logs
grpcurl -plaintext -d '{
  "limit": 100
}' localhost:50051 hodei.AuditService/GetAuditLogs

# Filter by event type
grpcurl -plaintext -d '{
  "event_type": "JobCreated",
  "limit": 100
}' localhost:50051 hodei.AuditService/GetAuditLogs

# Filter by time range
grpcurl -plaintext -d '{
  "start_time": "2025-12-17T00:00:00Z",
  "end_time": "2025-12-17T23:59:59Z",
  "limit": 100
}' localhost:50051 hodei.AuditService/GetAuditLogs

# Filter by actor
grpcurl -plaintext -d '{
  "actor": "user",
  "limit": 100
}' localhost:50051 hodei.AuditService/GetAuditLogs
```

### 22. Get Audit Logs by Correlation

Get logs by correlation ID for tracing.

```bash
grpcurl -plaintext -d '{
  "correlation_id": "correlation-uuid"
}' localhost:50051 hodei.AuditService/GetAuditLogsByCorrelation
```

### 23. Get Event Counts

Get count of events grouped by type.

```bash
grpcurl -plaintext -d '{
  "start_time": "2025-12-17T00:00:00Z",
  "end_time": "2025-12-17T23:59:59Z"
}' localhost:50051 hodei.AuditService/GetEventCounts
```

---

## Metrics Service

### 24. Get System Metrics

Get system metrics for workers.

```bash
grpcurl -plaintext -d '{
  "worker_id": {"value": "WORKER-ID"},
  "from_timestamp": "2025-12-17T00:00:00Z",
  "to_timestamp": "2025-12-17T23:59:59Z",
  "metric_types": ["cpu", "memory", "disk"],
  "interval": "60s"
}' localhost:50051 hodei.MetricsService/GetSystemMetrics
```

### 25. Get Job Execution Metrics

Get metrics for a specific job execution.

```bash
grpcurl -plaintext -d '{
  "execution_id": {"value": "EXEC-ID"},
  "interval": "30s"
}' localhost:50051 hodei.MetricsService/GetJobExecutionMetrics
```

### 26. Get Aggregated Metrics

Get aggregated metrics across multiple workers.

```bash
grpcurl -plaintext -d '{
  "metric_name": "cpu_usage",
  "worker_ids": [{"value": "WORKER-1"}, {"value": "WORKER-2"}],
  "from_timestamp": "2025-12-17T00:00:00Z",
  "to_timestamp": "2025-12-17T23:59:59Z",
  "window_size": "300s",
  "aggregation_type": "avg"
}' localhost:50051 hodei.MetricsService/GetAggregatedMetrics
```

### 27. Stream Real-Time Metrics

Stream metrics in real-time.

```bash
grpcurl -plaintext -d '{
  "worker_id": {"value": "WORKER-ID"},
  "metric_types": ["cpu", "memory"],
  "update_interval": "5s",
  "max_points": 100
}' localhost:50051 hodei.MetricsService/RealTimeMetricsStream
```

### 28. Get Historical Metrics

Get historical time series metrics.

```bash
grpcurl -plaintext -d '{
  "metric_name": "cpu_usage_percent",
  "worker_ids": [{"value": "WORKER-ID"}],
  "from_timestamp": "2025-12-17T00:00:00Z",
  "to_timestamp": "2025-12-17T23:59:59Z",
  "resolution": "60s",
  "aggregation": "avg"
}' localhost:50051 hodei.MetricsService/GetHistoricalMetrics
```

### 29. Get Health Metrics

Get system health metrics.

```bash
grpcurl -plaintext -d '{
  "worker_ids": [{"value": "WORKER-1"}, {"value": "WORKER-2"}],
  "lookback_period": "3600s"
}' localhost:50051 hodei.MetricsService/GetHealthMetrics
```

### 30. Create Alert

Create a metric-based alert.

```bash
grpcurl -plaintext -d '{
  "alert_name": "HighCPUUsage",
  "metric_name": "cpu_usage_percent",
  "condition": ">",
  "threshold_value": 80.0,
  "severity": "warning",
  "evaluation_interval": "60s",
  "enabled": true
}' localhost:50051 hodei.MetricsService/CreateAlert
```

### 31. Get Alerts

Get active alerts.

```bash
grpcurl -plaintext -d '{
  "active_only": true
}' localhost:50051 hodei.MetricsService/GetAlerts
```

---

## Provider Management Service

### 32. Register Provider

Register a new provider.

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.ProviderManagementService/RegisterProvider << 'JSON'
{
  "name": "docker-provider",
  "provider_type": "PROVIDER_TYPE_DOCKER",
  "type_config": {
    "docker": {
      "socket_path": "/var/run/docker.sock",
      "default_image": "ubuntu:22.04",
      "network_mode": "bridge"
    }
  },
  "priority": 1,
  "max_workers": 10
}
JSON
```

### 33. Get Provider

Get provider by ID.

```bash
grpcurl -plaintext -d '{
  "provider_id": "PROVIDER-ID"
}' localhost:50051 hodei.ProviderManagementService/GetProvider
```

### 34. Get Provider by Name

Get provider by name.

```bash
grpcurl -plaintext -d '{
  "name": "docker-provider"
}' localhost:50051 hodei.ProviderManagementService/GetProviderByName
```

### 35. List Providers

List all providers with optional filters.

```bash
# List all providers
grpcurl -plaintext -d '{}' \
  localhost:50051 hodei.ProviderManagementService/ListProviders

# List by type
grpcurl -plaintext -d '{
  "provider_type": "PROVIDER_TYPE_DOCKER"
}' localhost:50051 hodei.ProviderManagementService/ListProviders

# List only providers with capacity
grpcurl -plaintext -d '{
  "only_with_capacity": true
}' localhost:50051 hodei.ProviderManagementService/ListProviders
```

### 36. Update Provider

Update provider configuration.

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.ProviderManagementService/UpdateProvider << 'JSON'
{
  "provider": {
    "id": "PROVIDER-ID",
    "name": "docker-provider",
    "max_workers": 20,
    "priority": 2
  }
}
JSON
```

### 37. Enable/Disable Provider

Enable or disable a provider.

```bash
# Enable
grpcurl -plaintext -d '{
  "provider_id": "PROVIDER-ID"
}' localhost:50051 hodei.ProviderManagementService/EnableProvider

# Disable
grpcurl -plaintext -d '{
  "provider_id": "PROVIDER-ID"
}' localhost:50051 hodei.ProviderManagementService/DisableProvider
```

### 38. Delete Provider

Delete a provider.

```bash
grpcurl -plaintext -d '{
  "provider_id": "PROVIDER-ID"
}' localhost:50051 hodei.ProviderManagementService/DeleteProvider
```

### 39. Get Provider Statistics

Get provider statistics.

```bash
grpcurl -plaintext -d '{}' \
  localhost:50051 hodei.ProviderManagementService/GetProviderStats
```

---

## Common Types

### Job Status Values

- `JOB_STATUS_PENDING` - Job created but not queued
- `JOB_STATUS_QUEUED` - Job in queue waiting for worker
- `JOB_STATUS_ASSIGNED` - Job assigned to worker
- `JOB_STATUS_RUNNING` - Job currently executing
- `JOB_STATUS_COMPLETED` - Job completed successfully
- `JOB_STATUS_FAILED` - Job failed
- `JOB_STATUS_CANCELLED` - Job cancelled
- `JOB_STATUS_TIMEOUT` - Job timed out

### Worker Status Values

- `WORKER_STATUS_OFFLINE` - Worker is offline
- `WORKER_STATUS_REGISTERING` - Worker is registering
- `WORKER_STATUS_AVAILABLE` - Worker is available
- `WORKER_STATUS_BUSY` - Worker is busy with jobs
- `WORKER_STATUS_DRAINING` - Worker is draining
- `WORKER_STATUS_ERROR` - Worker has error

### Priority Levels

- `PRIORITY_LOW` - Low priority
- `PRIORITY_NORMAL` - Normal priority
- `PRIORITY_HIGH` - High priority
- `PRIORITY_CRITICAL` - Critical priority

---

## 🛠️ Useful Tips

### Format Job ID

All job-related operations require JobId as:
```json
{"job_id": {"value": "actual-job-id"}}
```

### Format Worker ID

All worker-related operations require WorkerId as:
```json
{"worker_id": {"value": "actual-worker-id"}}
```

### Format Execution ID

All execution-related operations require ExecutionId as:
```json
{"execution_id": {"value": "actual-execution-id"}}
```

### Error Handling

Check `success` field in response. If false, check `message` for error details.

### Streaming Commands

Commands ending with `Stream` return streaming responses. Use Ctrl+C to stop.

### Authentication

Currently using plaintext (`-plaintext` flag). For production, use TLS certificates.

---

## 📚 See Also

- [Maven Workflow Manual](MAVEN_WORKFLOW_MANUAL.md)
- [Postman Configuration Guide](POSTMAN_CONFIGURATION.md)
- [Getting Started Guide](../GETTING_STARTED.md)
- [Maven Solution Working](../MAVEN_SOLUTION_WORKING.md)

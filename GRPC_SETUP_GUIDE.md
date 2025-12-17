# gRPC Setup & Usage Guide

## 📋 Table of Contents

1. [Prerequisites](#prerequisites)
2. [Quick Start](#quick-start)
3. [Watching Job Traces](#watching-job-traces)
4. [Complete gRPC Command Reference](#complete-grpc-command-reference)
5. [Postman Configuration](#postman-configuration)
6. [Maven Workflow](#maven-workflow)
7. [Troubleshooting](#troubleshooting)

---

## Prerequisites

### 1. Install Tools

```bash
# Install grpcurl (gRPC client)
go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest

# Install jq (JSON processor)
# macOS
brew install jq
# Ubuntu/Debian
sudo apt-get install jq

# Install Evans (optional, advanced gRPC client)
# macOS
brew install evans
# Linux
curl -sSL https://raw.githubusercontent.com/ktr0731/evans/master/install.sh | bash
```

### 2. Start Platform

```bash
# Using Docker Compose
docker compose -f docker-compose.prod.yml up -d

# Or run directly
cargo run --bin server
```

### 3. Verify Connection

```bash
# Test connection
grpcurl -plaintext localhost:50051 list

# Expected output:
# hodei.AuditService
# hodei.JobExecutionService
# hodei.LogStreamService
# hodei.MetricsService
# hodei.ProviderManagementService
# hodei.SchedulerService
# hodei.WorkerAgentService
```

---

## Quick Start

### Run Your First Job

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'JSON'
{
  "job_definition": {
    "name": "hello-world",
    "command": "/bin/bash",
    "arguments": ["-lc", "echo 'Hello from Hodei Jobs Platform!'"],
    "requirements": {
      "cpu_cores": 1.0,
      "memory_bytes": 1073741824
    },
    "timeout": {
      "execution_timeout": "60s"
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
  "jobId": {
    "value": "550e8400-e29b-41d4-a716-446655440000"
  }
}
```

**Save the Job ID for monitoring!**

---

## Watching Job Traces

### Option 1: Trace Script (Recommended)

Trace a job from submission to completion:

```bash
# Trace specific job
./scripts/trace-job.sh 550e8400-e29b-41d4-a716-446655440000

# Trace without log streaming
./scripts/trace-job.sh JOB-ID --no-logs

# Custom check interval
./scripts/trace-job.sh JOB-ID --interval 5
```

**Output includes:**
- Job status changes
- Worker assignment
- Execution progress
- Real-time logs
- Final result

### Option 2: Manual Log Streaming

```bash
# Stream logs for specific job
grpcurl -plaintext -d '{"job_id": "JOB-ID", "include_history": true}' \
  localhost:50051 hodei.LogStreamService/SubscribeLogs

# Save logs to file
grpcurl -plaintext -d '{"job_id": "JOB-ID", "include_history": true}' \
  localhost:50051 hodei.LogStreamService/SubscribeLogs \
  | jq -r '.line' > job.log
```

### Option 3: List Jobs and Monitor

```bash
# List all jobs
./scripts/list-jobs.sh

# List running jobs only
./scripts/list-jobs.sh --running

# List with JSON output
./scripts/list-jobs.sh --json

# Search jobs by name
./scripts/list-jobs.sh --search maven
```

---

## Complete gRPC Command Reference

### Job Execution Service

#### 1. Queue Job
```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'JSON'
{
  "job_definition": {
    "name": "job-name",
    "command": "/bin/bash",
    "arguments": ["-lc", "echo 'Hello'"],
    "requirements": {
      "cpu_cores": 1.0,
      "memory_bytes": 1073741824
    },
    "timeout": {"execution_timeout": "300s"}
  },
  "queued_by": "user"
}
JSON
```

#### 2. Get Job Details
```bash
grpcurl -plaintext -d '{"job_id": {"value": "JOB-ID"}}' \
  localhost:50051 hodei.JobExecutionService/GetJob
```

#### 3. List Jobs
```bash
# All jobs
grpcurl -plaintext -d '{"limit": 50}' \
  localhost:50051 hodei.JobExecutionService/ListJobs

# Running jobs only
grpcurl -plaintext -d '{"limit": 50, "status": "JOB_STATUS_RUNNING"}' \
  localhost:50051 hodei.JobExecutionService/ListJobs

# Search by name
grpcurl -plaintext -d '{"limit": 50, "search_term": "maven"}' \
  localhost:50051 hodei.JobExecutionService/ListJobs
```

#### 4. Cancel Job
```bash
grpcurl -plaintext -d '{
  "execution_id": {"value": "EXEC-ID"},
  "job_id": {"value": "JOB-ID"},
  "reason": "User requested"
}' localhost:50051 hodei.JobExecutionService/CancelJob
```

#### 5. Update Progress
```bash
grpcurl -plaintext -d '{
  "execution_id": {"value": "EXEC-ID"},
  "job_id": {"value": "JOB-ID"},
  "progress_percentage": 50,
  "current_stage": "Building"
}' localhost:50051 hodei.JobExecutionService/UpdateJobProgress
```

### Log Stream Service

#### 6. Subscribe to Logs
```bash
# New logs only
grpcurl -plaintext -d '{"job_id": "JOB-ID"}' \
  localhost:50051 hodei.LogStreamService/SubscribeLogs

# Include history
grpcurl -plaintext -d '{"job_id": "JOB-ID", "include_history": true}' \
  localhost:50051 hodei.LogStreamService/SubscribeLogs

# Last 100 lines
grpcurl -plaintext -d '{"job_id": "JOB-ID", "include_history": true, "tail_lines": 100}' \
  localhost:50051 hodei.LogStreamService/SubscribeLogs
```

### Scheduler Service

#### 7. Schedule Job
```bash
grpcurl -plaintext -d @ localhost:50051 hodei.SchedulerService/ScheduleJob << 'JSON'
{
  "job_definition": {
    "name": "scheduled-job",
    "command": "/bin/bash",
    "arguments": ["-lc", "echo 'Scheduled'"]
  },
  "requested_by": "scheduler"
}
JSON
```

#### 8. Get Queue Info
```bash
grpcurl -plaintext -d '{"queue_name": "default"}' \
  localhost:50051 hodei.SchedulerService/GetJobQueueInfo
```

#### 9. Get Scheduler Status
```bash
grpcurl -plaintext -d '{"scheduler_name": "default"}' \
  localhost:50051 hodei.SchedulerService/GetSchedulerStatus
```

### Worker Agent Service

#### 10. Register Worker
```bash
grpcurl -plaintext -d @ localhost:50051 hodei.WorkerAgentService/Register << 'JSON'
{
  "auth_token": "worker-token",
  "worker_info": {
    "name": "worker-1",
    "capacity": {"cpu_cores": 4.0, "memory_bytes": 8589934592}
  }
}
JSON
```

#### 11. Get Available Workers
```bash
grpcurl -plaintext -d '{
  "min_requirements": {"cpu_cores": 1.0},
  "max_results": 20
}' localhost:50051 hodei.WorkerAgentService/GetAvailableWorkers
```

### Audit Service

#### 12. Get Audit Logs
```bash
# All logs
grpcurl -plaintext -d '{"limit": 100}' \
  localhost:50051 hodei.AuditService/GetAuditLogs

# Filter by event type
grpcurl -plaintext -d '{"event_type": "JobCreated", "limit": 100}' \
  localhost:50051 hodei.AuditService/GetAuditLogs

# Filter by time range
grpcurl -plaintext -d '{
  "start_time": "2025-12-17T00:00:00Z",
  "end_time": "2025-12-17T23:59:59Z",
  "limit": 100
}' localhost:50051 hodei.AuditService/GetAuditLogs
```

#### 13. Get Event Counts
```bash
grpcurl -plaintext -d '{
  "start_time": "2025-12-17T00:00:00Z"
}' localhost:50051 hodei.AuditService/GetEventCounts
```

### Metrics Service

#### 14. Get System Metrics
```bash
grpcurl -plaintext -d '{
  "worker_id": {"value": "WORKER-ID"},
  "metric_types": ["cpu", "memory", "disk"]
}' localhost:50051 hodei.MetricsService/GetSystemMetrics
```

#### 15. Stream Real-Time Metrics
```bash
grpcurl -plaintext -d '{
  "worker_id": {"value": "WORKER-ID"},
  "metric_types": ["cpu", "memory"],
  "update_interval": "5s"
}' localhost:50051 hodei.MetricsService/RealTimeMetricsStream
```

### Provider Management Service

#### 16. Register Provider
```bash
grpcurl -plaintext -d @ localhost:50051 hodei.ProviderManagementService/RegisterProvider << 'JSON'
{
  "name": "docker-provider",
  "provider_type": "PROVIDER_TYPE_DOCKER",
  "type_config": {
    "docker": {
      "socket_path": "/var/run/docker.sock",
      "default_image": "ubuntu:22.04"
    }
  },
  "max_workers": 10
}
JSON
```

#### 17. List Providers
```bash
grpcurl -plaintext -d '{}' \
  localhost:50051 hodei.ProviderManagementService/ListProviders
```

---

## Postman Configuration

### Step 1: Import Collection

1. Open Postman
2. Click **Import**
3. Select `postman/Hodei-Jobs-Platform.json`
4. Collection will be imported with all API calls

### Step 2: Set Up Environment

1. Click gear icon (⚙️) → **Environments**
2. Create new environment: `Hodei Jobs Platform`
3. Add variables:
   - `grpc_server`: `localhost:50051`
   - `job_id`: `` (will be auto-generated)
   - `execution_id`: `` (auto-populated)
   - `worker_id`: `` (will be set as needed)

### Step 3: Use Collection

1. Select environment: **Hodei Jobs Platform**
2. Choose request from collection
3. Review request body (already filled)
4. Click **Send** (or use gRPC client if available)

### Note on gRPC Support

Postman has **limited gRPC support**. For full functionality:

- Use **Evans** (recommended): `evans -p 50051 repl`
- Use **grpcurl** (command line)
- Or use the provided scripts

**See:** [Postman Configuration Guide](docs/POSTMAN_CONFIGURATION.md)

---

## Maven Workflow

### Option 1: Docker Provider (Recommended)

Fastest and most reliable for production:

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'JSON'
{
  "job_definition": {
    "name": "maven-build-docker",
    "command": "/bin/bash",
    "arguments": ["-c", "cd /tmp && git clone https://github.com/jenkins-docs/simple-java-maven-app.git && cd simple-java-maven-app && mvn clean package -DskipTests && ls -lh target/"],
    "requirements": {
      "cpu_cores": 2.0,
      "memory_bytes": 2147483648,
      "disk_bytes": 1073741824
    },
    "metadata": {
      "container_image": "maven:3.9.4-eclipse-temurin-17"
    },
    "timeout": {
      "execution_timeout": "600s"
    }
  },
  "queued_by": "user"
}
JSON
```

### Option 2: Using Scripts

```bash
# Run Maven example
./scripts/run_maven_job.sh

# Monitor logs
./scripts/trace-job.sh JOB-ID
```

### Option 3: Complete asdf Workflow

For development/testing with Java/Maven installation:

```bash
# Step 1: Install Java
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'EOF'
{
  "job_definition": {
    "name": "install-java",
    "command": "/bin/bash",
    "arguments": ["-lc", "export ASDF_DATA_DIR='$HOME/.asdf'; export PATH='$ASDF_DATA_DIR/shims:$PATH'; asdf plugin add java; asdf install java temurin-17.0.9+9; asdf set java temurin-17.0.9+9; java -version"],
    "requirements": {"cpu_cores": 1.0, "memory_bytes": 2147483648},
    "timeout": {"execution_timeout": "600s"}
  },
  "queued_by": "user"
}
EOF
```

**See:** [Maven Workflow Manual](docs/MAVEN_WORKFLOW_MANUAL.md)

---

## Troubleshooting

### Connection Refused

**Error:** `Failed to dial: connection refused`

**Solution:**
```bash
# Check if server is running
curl http://localhost:50051/health

# Check port
netstat -an | grep 50051

# Start server
cargo run --bin server
```

### "command not found: mvn"

**Cause:** Java/Maven not installed in worker

**Solution:**
- Use Docker provider: `"metadata": {"container_image": "maven:3.9.4-eclipse-temurin-17"}`
- Or install via asdf first (slower)

### "No space left on device"

**Cause:** Insufficient disk space

**Solution:**
```json
"requirements": {
  "disk_bytes": 10737418240  // 10GB instead of 1GB
}
```

### "Connection timeout"

**Cause:** Job taking too long

**Solution:**
```json
"timeout": {
  "execution_timeout": "1800s"  // 30 minutes
}
```

### Out of memory

**Cause:** Maven needs more RAM

**Solution:**
```json
"requirements": {
  "memory_bytes": 4294967296  // 4GB
},
"environment": {
  "MAVEN_OPTS": "-Xmx2g"
}
```

---

## 📚 Additional Resources

### Documentation
- [gRPC API Reference](docs/GRPC_API_REFERENCE.md) - Complete API documentation
- [Postman Configuration](docs/POSTMAN_CONFIGURATION.md) - Postman setup guide
- [Maven Workflow Manual](docs/MAVEN_WORKFLOW_MANUAL.md) - Maven build guide
- [Quick Reference](docs/GRPC_QUICK_REFERENCE.md) - Quick command reference

### Scripts
- `./scripts/trace-job.sh` - Trace job execution
- `./scripts/list-jobs.sh` - List and filter jobs
- `./scripts/watch_logs.sh` - Watch job logs
- `./scripts/run_maven_job.sh` - Run Maven example

### Example Payloads
- `maven_job_payload.json` - Maven build example

---

## 🎯 Best Practices

1. **Always extract and save Job ID** from QueueJob response
2. **Use Docker provider** for Maven builds (faster, more reliable)
3. **Monitor with trace-job.sh** for complete visibility
4. **Set appropriate timeouts** for long-running jobs
5. **Increase resources** for memory-intensive builds
6. **Use inline scripts** instead of local files (workers are isolated)
7. **Check logs regularly** with SubscribeLogs
8. **Use environment variables** for repeated values

---

**Happy Job Running! 🚀**

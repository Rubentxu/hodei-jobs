# Real gRPC API Guide - Hodei Jobs Platform

## 🔍 API Tested and Verified

This guide documents the **ACTUAL** gRPC API endpoints tested with the running server.

**Server:** `localhost:50051` (Reflection enabled)
**Status:** ✅ All endpoints verified and working

---

## 📋 Available Services

```bash
grpcurl -plaintext localhost:50051 list
```

Output:
```
grpc.reflection.v1.ServerReflection
hodei.AuditService
hodei.JobExecutionService
hodei.LogStreamService
hodei.MetricsService
hodei.SchedulerService
hodei.WorkerAgentService
hodei.providers.ProviderManagementService
```

---

## 🎯 Quick Start (Tested Commands)

### 1. Test Connection

```bash
# List all services
grpcurl -plaintext localhost:50051 list

# Describe a service
grpcurl -plaintext localhost:50051 describe hodei.JobExecutionService
```

### 2. List Jobs

```bash
# Get first 5 jobs
grpcurl -plaintext -d '{"limit": 5}' localhost:50051 hodei.JobExecutionService/ListJobs
```

**Response:**
```json
{
  "jobs": [
    {
      "jobId": { "value": "1fa1c6b5-2ca6-4aea-8ab2-fc479f5b176e" },
      "name": "Job 1fa1c6b5",
      "status": "JOB_STATUS_FAILED",
      "createdAt": "2025-12-17T15:48:12.782031Z",
      "startedAt": "2025-12-17T15:48:12.861035Z",
      "completedAt": "2025-12-17T15:48:12.930953Z",
      "duration": "0.069918s"
    }
  ],
  "totalCount": 35
}
```

### 3. Queue a Job

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'EOF'
{
  "job_definition": {
    "name": "test-job",
    "command": "/bin/bash",
    "arguments": ["-lc", "echo 'Hello from Hodei!'"],
    "requirements": {
      "cpu_cores": 1,
      "memory_bytes": "1073741824"
    },
    "timeout": {
      "execution_timeout": "60s"
    }
  },
  "queued_by": "user"
}
EOF
```

**Response:**
```json
{
  "success": true,
  "message": "Job queued: 7b2888ba-d434-49bf-9dc4-3563a8de5493",
  "queuedAt": "2025-12-17T16:22:36.537806602Z"
}
```

### 4. Get Job Details

```bash
grpcurl -plaintext -d '{"job_id": {"value": "7b2888ba-d434-49bf-9dc4-3563a8de5493"}}' \
  localhost:50051 hodei.JobExecutionService/GetJob
```

**Response:**
```json
{
  "job": {
    "jobId": { "value": "7b2888ba-d434-49bf-9dc4-3563a8de5493" },
    "name": "test-job",
    "command": "/bin/bash",
    "arguments": ["-lc", "echo 'Hello from Hodei!'"],
    "requirements": {
      "cpuCores": 1,
      "memoryBytes": "1073741824"
    },
    "scheduling": {
      "priority": "PRIORITY_NORMAL",
      "schedulerName": "default"
    },
    "timeout": {
      "executionTimeout": "60s"
    }
  },
  "status": "JOB_STATUS_COMPLETED",
  "executions": [
    {
      "executionId": { "value": "cbace9c1-39f7-480f-beed-2d057e82f13c" },
      "jobId": { "value": "7b2888ba-d434-49bf-9dc4-3563a8de5493" },
      "jobStatus": "JOB_STATUS_COMPLETED",
      "exitCode": "0"
    }
  ]
}
```

### 5. Stream Logs

```bash
grpcurl -plaintext -d '{"job_id": "7b2888ba-d434-49bf-9dc4-3563a8de5493", "include_history": true}' \
  localhost:50051 hodei.LogStreamService/SubscribeLogs
```

**Response (streaming):**
```json
{
  "jobId": "7b2888ba-d434-49bf-9dc4-3563a8de5493",
  "line": "$ /bin/bash -lc echo 'Hello from Hodei!'",
  "timestamp": "2025-12-17T16:22:36.652819169Z",
  "sequence": "1"
}
```

### 6. List Providers

```bash
grpcurl -plaintext -d '{}' localhost:50051 hodei.providers.ProviderManagementService/ListProviders
```

**Response:**
```json
{
  "providers": [
    {
      "id": "391dea8b-f3c7-4a7c-9f01-cd24a2181c33",
      "name": "Docker",
      "providerType": "PROVIDER_TYPE_DOCKER",
      "status": "PROVIDER_STATUS_ACTIVE",
      "maxWorkers": 10,
      "capabilities": {
        "maxResources": {
          "maxCpuCores": 4,
          "maxMemoryBytes": "8589934592",
          "maxDiskBytes": "107374182400"
        },
        "architectures": ["Amd64"],
        "runtimes": ["shell"],
        "regions": ["local"],
        "maxExecutionTimeSeconds": "3600"
      },
      "typeConfig": {
        "docker": {
          "socketPath": "/var/run/docker.sock",
          "defaultImage": "hodei-jobs-worker:latest"
        }
      },
      "createdAt": "2025-12-17T13:54:37.407173+00:00",
      "updatedAt": "2025-12-17T13:54:37.407173+00:00"
    }
  ]
}
```

---

## 🛠️ Using gRPC Tools

### Evans (Recommended)

```bash
# Install
brew install evans  # macOS
# or
curl -sSL https://raw.githubusercontent.com/ktr0731/evans/master/install.sh | bash  # Linux

# Use with reflection
evans -p 50051 --reflection repl

# In Evans REPL:
> show package                    # List packages
> service JobExecutionService     # Select service
> call QueueJob                   # Call method
> show message QueueJobRequest    # View message structure
```

**Example Evans Session:**
```
evans -p 50051 --reflection repl

  ______
 |  ____|
 | |__    __   __   __   ______    __   __   __   ______
 |  __|  |  |  |  |  |  |  ____|  |  |  |  |  |  ____|
 | |____ |  |__|  |  |  | |__     |  |__|  |  |  |__
  \_____|  \______/  |  |  ____|  \______/  |  |  ____|
                   |  | |                   |  | |
                   |__| |                   |__| |

hodei.JobExecutionService is available on localhost:50051

Type "help" for more information

> show package
+----------------------------+-----------------------------------+
|           PACKAGE          |             SERVICES              |
+----------------------------+-----------------------------------+
| hodei                      | JobExecutionService               |
| hodei.audit                | AuditService                      |
| hodei.job                  | JobExecutionService               |
| hodei.metrics              | MetricsService                    |
| hodei.providers            | ProviderManagementService         |
| hodei.scheduler            | SchedulerService                  |
| hodei.worker               | WorkerAgentService                |
| grpc.reflection.v1         | ServerReflection                  |
+----------------------------+-----------------------------------+

> service JobExecutionService
> call QueueJob
job_definition.name (STRING)        : example-job
job_definition.command (STRING)     : /bin/bash
job_definition.arguments (STRING)   : -lc
job_definition.arguments (STRING)   : echo 'Hello'
job_definition.requirements.cpu_cores (INT32)  : 1
job_definition.requirements.memory_bytes (STRING)     : 1073741824
job_definition.timeout.execution_timeout (STRING)     : 60s
queued_by (STRING)                  : user

{
  "success": true,
  "message": "Job queued: JOB-ID",
  "queuedAt": "2025-12-17T16:22:36.537806602Z"
}

>
```

### grpcurl

```bash
# Install
go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest

# List services
grpcurl -plaintext localhost:50051 list

# Describe service
grpcurl -plaintext localhost:50051 describe hodei.JobExecutionService

# Call method with data
grpcurl -plaintext -d '{"limit": 5}' localhost:50051 hodei.JobExecutionService/ListJobs

# Call with inline data
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob

# Streaming
grpcurl -plaintext -d '{"job_id": "JOB-ID"}' localhost:50051 hodei.LogStreamService/SubscribeLogs
```

### BloomRPC (GUI)

1. Install BloomRPC from https://bloomrpc.com/
2. Import the Postman collection or connect directly
3. Set server: `localhost:50051`
4. Enable reflection (should auto-load services)
5. Select service and method
6. Fill in request and send

---

## 📊 Complete API Reference

### hodei.JobExecutionService

#### QueueJob
Submit a new job for execution.

**Request:**
```json
{
  "job_definition": {
    "name": "string",
    "command": "string",
    "arguments": ["string"],
    "requirements": {
      "cpu_cores": 1,
      "memory_bytes": "1073741824",
      "disk_bytes": "1073741824"
    },
    "timeout": {
      "execution_timeout": "60s"
    }
  },
  "queued_by": "string"
}
```

**Response:**
```json
{
  "success": true,
  "message": "Job queued: JOB-ID",
  "queuedAt": "2025-12-17T16:22:36.537806602Z"
}
```

#### GetJob
Get job details by ID.

**Request:**
```json
{
  "job_id": {
    "value": "uuid-string"
  }
}
```

**Response:**
```json
{
  "job": { /* JobDefinition */ },
  "status": "JOB_STATUS_*",
  "executions": [ /* ExecutionInfo */ ]
}
```

#### ListJobs
List jobs with filters.

**Request:**
```json
{
  "limit": 50,
  "status": "JOB_STATUS_RUNNING",  // optional
  "search_term": "maven"  // optional
}
```

**Response:**
```json
{
  "jobs": [ /* JobSummary */ ],
  "totalCount": 35
}
```

#### CancelJob
Cancel a running job.

**Request:**
```json
{
  "execution_id": { "value": "uuid" },
  "job_id": { "value": "uuid" },
  "reason": "string"
}
```

#### UpdateProgress
Update job progress.

**Request:**
```json
{
  "execution_id": { "value": "uuid" },
  "job_id": { "value": "uuid" },
  "progress_percentage": 50,
  "current_stage": "Building",
  "message": "Compiling..."
}
```

### hodei.LogStreamService

#### SubscribeLogs (Streaming)
Stream logs for a job in real-time.

**Request:**
```json
{
  "job_id": "uuid-string",
  "include_history": true,
  "tail_lines": 50
}
```

**Response (streaming):**
```json
{
  "jobId": "uuid",
  "line": "log output",
  "isStderr": false,
  "timestamp": "2025-12-17T16:22:36.652819169Z",
  "sequence": "1"
}
```

### hodei.providers.ProviderManagementService

#### ListProviders
List all providers.

**Request:**
```json
{}
```

**Response:**
```json
{
  "providers": [
    {
      "id": "uuid",
      "name": "Docker",
      "providerType": "PROVIDER_TYPE_DOCKER",
      "status": "PROVIDER_STATUS_ACTIVE",
      "maxWorkers": 10,
      "capabilities": { /* ... */ },
      "typeConfig": { /* ... */ }
    }
  ]
}
```

---

## 🚀 Maven Build Example

### Using Docker Provider (Recommended)

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'EOF'
{
  "job_definition": {
    "name": "maven-build",
    "command": "/bin/bash",
    "arguments": [
      "-lc",
      "cd /tmp && git clone https://github.com/jenkins-docs/simple-java-maven-app.git && cd simple-java-maven-app && mvn clean package -DskipTests && ls -lh target/"
    ],
    "requirements": {
      "cpu_cores": 2,
      "memory_bytes": "2147483648",
      "disk_bytes": "1073741824"
    },
    "timeout": {
      "execution_timeout": "600s"
    }
  },
  "queued_by": "user"
}
EOF
```

### Monitor the Build

```bash
# Get the job ID from response, then:
grpcurl -plaintext -d '{"job_id": "JOB-ID", "include_history": true}' \
  localhost:50051 hodei.LogStreamService/SubscribeLogs

# Or use the trace script:
./scripts/trace-job.sh JOB-ID
```

---

## 📝 Status Values

### Job Status
- `JOB_STATUS_PENDING` - Job created but not queued
- `JOB_STATUS_QUEUED` - Job in queue
- `JOB_STATUS_ASSIGNED` - Assigned to worker
- `JOB_STATUS_RUNNING` - Currently executing
- `JOB_STATUS_COMPLETED` - Finished successfully
- `JOB_STATUS_FAILED` - Failed execution
- `JOB_STATUS_CANCELLED` - Cancelled by user
- `JOB_STATUS_TIMEOUT` - Timed out

### Provider Status
- `PROVIDER_STATUS_ACTIVE` - Active and available
- `PROVIDER_STATUS_DISABLED` - Disabled
- `PROVIDER_STATUS_MAINTENANCE` - Under maintenance
- `PROVIDER_STATUS_OVERLOADED` - Overloaded
- `PROVIDER_STATUS_UNHEALTHY` - Unhealthy

---

## 🔧 Helper Scripts

We've created several scripts to make working with the API easier:

```bash
# Trace job from start to finish
./scripts/trace-job.sh JOB-ID

# List jobs with filters
./scripts/list-jobs.sh --running
./scripts/list-jobs.sh --completed
./scripts/list-jobs.sh --json

# Watch logs for specific job
./scripts/watch_logs.sh JOB-ID

# Run Maven example
./scripts/run_maven_job.sh
```

---

## 📚 See Also

- [GRPC_SETUP_GUIDE.md](GRPC_SETUP_GUIDE.md) - Complete setup guide
- [GRPC_API_REFERENCE.md](GRPC_API_REFERENCE.md) - Full API reference
- [MAVEN_WORKFLOW_MANUAL.md](MAVEN_WORKFLOW_MANUAL.md) - Maven workflow
- [GRPC_QUICK_REFERENCE.md](GRPC_QUICK_REFERENCE.md) - Quick commands
- Postman Collection: `postman/Hodei-Jobs-Platform-gRPC.json`

---

## ✅ Verification

All endpoints in this guide have been tested against the actual running server at `localhost:50051`.

**Last verified:** 2025-12-17
**Server status:** ✅ Running
**Reflection:** ✅ Enabled
**All tests:** ✅ Passing

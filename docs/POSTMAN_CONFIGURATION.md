# Postman Configuration Guide for Hodei Jobs Platform

## 📋 Table of Contents

1. [Overview](#overview)
2. [Prerequisites](#prerequisites)
3. [Installing gRPC Reflection](#installing-grpc-reflection)
4. [Importing Collection](#importing-collection)
5. [Collection Structure](#collection-structure)
6. [Environment Setup](#environment-setup)
7. [Usage Examples](#usage-examples)
8. [Maven Workflow Example](#maven-workflow-example)

---

## Overview

This guide explains how to use Postman with the Hodei Jobs Platform gRPC API.

⚠️ **IMPORTANT**: Postman **does NOT natively support gRPC** as of version 11.76.5. This collection provides:

1. **Request templates** you can copy to gRPC clients
2. **Environment variables** for consistency
3. **Documentation references** for each API call

**For actual gRPC calls, use:**
- ✅ **Evans** (recommended) - Modern gRPC client
- ✅ **grpcurl** - Command-line gRPC client
- ✅ **gRPCurl GUI** - Web-based gRPC client

---

## Prerequisites

### 1. Install Evans (Recommended)

Evans is a modern gRPC client with excellent Postman-like features.

```bash
# macOS
brew tap ktr0731/evans
brew install evans

# Linux
curl -sSL https://raw.githubusercontent.com/ktr0731/evans/master/install.sh | bash

# Docker
docker run --rm -it -v $PWD:/workspace ktr0731/evans repl -p 50051
```

### 2. Install grpcurl

```bash
# macOS
brew install grpcurl

# Linux
go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest

# Docker
docker run --rm -ti fullstorydev/grpcurl:latest
```

### 3. Enable gRPC Reflection (Server-Side)

Ensure your server is compiled with reflection support. Check if reflection is enabled:

```bash
grpcurl localhost:50051 list
```

If you see the services listed, reflection is enabled. If not, you need to enable it in the server.

---

## Installing gRPC Reflection

### For Development

If the server doesn't have reflection enabled, you can use the proto files directly:

```bash
# Generate descriptor set
protoc --descriptor_set_out=api.desc \
  --include_imports \
  proto/*.proto

# Use with grpcurl
grpcurl -descriptor-set-file api.desc \
  localhost:50051 \
  hodei.JobExecutionService/QueueJob
```

### For Evans with Reflection

Evans automatically uses reflection when available:

```bash
# Connect to server with reflection
evans -p 50051 -r repl

# In Evans REPL
service hodei.JobExecutionService
show message QueueJobRequest
call QueueJob
```

---

## Importing Collection

### Option 1: Using JSON (Limited Support)

Postman has limited gRPC support. You can import request configurations as JSON, but you'll need to manually configure each request.

### Option 2: Using Environment Variables

Create a Postman Environment with these variables:

| Variable | Initial Value | Description |
|----------|---------------|-------------|
| `base_url` | `localhost:50051` | gRPC server address |
| `job_id` | `` | Current job ID |
| `execution_id` | `` | Current execution ID |
| `worker_id` | `` | Worker ID |
| `package` | `hodei` | gRPC package name |

### Option 3: Direct API Calls

Since Postman's gRPC support is limited, we'll document the direct API calls you can make.

---

## Collection Structure

### 📁 Job Execution
- Queue Job
- Get Job
- List Jobs
- Cancel Job
- Assign Job
- Update Progress
- Complete Job
- Fail Job

### 📁 Log Streaming
- Subscribe to Logs
- Get Historical Logs

### 📁 Scheduler
- Schedule Job
- Get Queue Info
- Find Available Workers
- Get Scheduler Status
- Requeue Job

### 📁 Worker Management
- Register Worker
- Get Available Workers
- Drain Worker
- Unregister Worker

### 📁 Audit
- Get Audit Logs
- Get Audit Logs by Correlation
- Get Event Counts

### 📁 Metrics
- Get System Metrics
- Get Job Execution Metrics
- Get Aggregated Metrics
- Stream Real-Time Metrics

### 📁 Provider Management
- Register Provider
- Get Provider
- List Providers
- Update Provider
- Enable/Disable Provider

---

## Environment Setup

### 1. Create Environment in Postman

1. Click the gear icon (⚙️) → **Environments**
2. Click **Create Environment**
3. Name it: `Hodei Jobs Platform`
4. Add variables from the table above

### 2. Configure Pre-request Scripts

For each request, you might want to generate UUIDs or set timestamps.

Example pre-request script for Queue Job:

```javascript
// Generate UUID for job_id
const jobId = 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
    const r = Math.random() * 16 | 0;
    const v = c == 'x' ? r : (r & 0x3 | 0x8);
    return v.toString(16);
});

// Set environment variables
pm.environment.set('job_id', jobId);
pm.environment.set('timestamp', new Date().toISOString());
```

### 3. Configure Tests

Example test to extract job ID from response:

```javascript
const response = pm.response.json();

if (response.success) {
    pm.environment.set('job_id', response.jobId.value);
    console.log('Job ID:', response.jobId.value);
}

pm.test('Status code is 200', function () {
    pm.response.to.have.status(200);
});
```

---

## Usage Examples

### Example 1: Queue a Simple Job

**Request URL:** `grpc://{{base_url}}/hodei.JobExecutionService/QueueJob`

**Request Body:**
```json
{
  "job_definition": {
    "name": "test-job",
    "command": "/bin/bash",
    "arguments": ["-lc", "echo 'Hello World'"],
    "requirements": {
      "cpu_cores": 1.0,
      "memory_bytes": 1073741824
    },
    "timeout": {
      "execution_timeout": "300s"
    }
  },
  "queued_by": "user"
}
```

**Expected Response:**
```json
{
  "success": true,
  "message": "Job queued successfully",
  "queued_at": "2025-12-17T16:30:00Z",
  "jobId": {
    "value": "uuid-generated"
  }
}
```

### Example 2: Monitor Job Logs

**Request URL:** `grpc://{{base_url}}/hodei.LogStreamService/SubscribeLogs`

**Request Body:**
```json
{
  "job_id": "{{job_id}}",
  "include_history": true,
  "tail_lines": 50
}
```

**Streaming Response:**
```json
{
  "jobId": "uuid",
  "line": "Log output",
  "isStderr": false,
  "timestamp": "2025-12-17T16:30:00Z",
  "sequence": 1
}
```

### Example 3: List Running Jobs

**Request URL:** `grpc://{{base_url}}/hodei.JobExecutionService/ListJobs`

**Request Body:**
```json
{
  "limit": 50,
  "status": "JOB_STATUS_RUNNING"
}
```

**Expected Response:**
```json
{
  "jobs": [
    {
      "jobId": {"value": "uuid"},
      "name": "job-name",
      "status": "JOB_STATUS_RUNNING",
      "createdAt": "2025-12-17T16:30:00Z",
      "progressPercentage": 50
    }
  ],
  "totalCount": 1
}
```

---

## Maven Workflow Example

This example demonstrates a complete Maven build workflow.

### Step 1: Queue Maven Build Job

**Request:** Queue Job

**Body:**
```json
{
  "job_definition": {
    "name": "maven-build-example",
    "command": "/bin/bash",
    "arguments": ["-c", "cd /tmp && git clone https://github.com/jenkins-docs/simple-java-maven-app.git && cd simple-java-maven-app && mvn clean package -DskipTests"],
    "requirements": {
      "cpu_cores": 2.0,
      "memory_bytes": 2147483648,
      "disk_bytes": 1073741824
    },
    "timeout": {
      "execution_timeout": "600s"
    },
    "metadata": {
      "build_tool": "maven",
      "java_version": "17",
      "maven_version": "3.9.4"
    }
  },
  "queued_by": "user"
}
```

**Response:** Save the `jobId.value` for next steps.

### Step 2: Monitor Build Progress

**Request:** Subscribe to Logs

**Body:**
```json
{
  "job_id": "{{job_id}}",
  "include_history": true
}
```

**Watch for these log patterns:**
- `[OUT] Cloning repository...`
- `[OUT] Initializing Java environment`
- `[OUT] Downloading dependencies...`
- `[OUT] Compiling sources...`
- `[OUT] Running tests...`
- `[OUT] Building JAR...`
- `[OUT] BUILD SUCCESS`

### Step 3: Get Job Details

**Request:** Get Job

**Body:**
```json
{
  "job_id": {"value": "{{job_id}}"}
}
```

**Response will include:**
- Current status
- Execution info
- Progress percentage
- Start/end times

### Step 4: Get Final Result

**Request:** Get Job (after completion)

**Response:**
```json
{
  "job": {
    "jobId": {"value": "uuid"},
    "name": "maven-build-example",
    "command": "/bin/bash",
    "status": "JOB_STATUS_COMPLETED"
  },
  "latestExecution": {
    "executionId": {"value": "exec-uuid"},
    "state": "EXECUTION_STATE_COMPLETED",
    "status": "JOB_STATUS_COMPLETED",
    "startTime": "2025-12-17T16:30:00Z",
    "endTime": "2025-12-17T16:35:00Z",
    "exitCode": "0",
    "metadata": {
      "build_duration": "300s",
      "output_summary": "BUILD SUCCESS"
    }
  },
  "events": [...]
}
```

---

## Advanced Usage with Evans

Evans provides a more natural gRPC experience:

### 1. Start Evans REPL

```bash
evans -p 50051 -r repl
```

### 2. List Services

```bash
show package
service hodei.JobExecutionService
```

### 3. Describe Message

```bash
show message QueueJobRequest
show message JobDefinition
```

### 4. Make a Call

```bash
call QueueJob
```

Evans will prompt you to fill in fields interactively.

### 5. Use in Batch Mode

```bash
evans -p 50051 \
  --addr localhost:50051 \
  --path proto/ \
  call hodei.JobExecutionService.QueueJob \
  --data '{"job_definition": {...}, "queued_by": "user"}'
```

---

## Troubleshooting

### Connection Refused

**Problem:** `Failed to dial: connection refused`

**Solution:**
```bash
# Check if server is running
curl http://localhost:50051/health  # or your health check endpoint

# Check if port is listening
netstat -an | grep 50051
```

### Reflection Not Available

**Problem:** `server does not support reflection`

**Solution:**
1. Use proto files directly with `-proto` flag
2. Enable reflection in server configuration
3. Use Evans with `-p` flag and proto path

### Invalid Message Format

**Problem:** `invalid message format`

**Solution:**
1. Validate JSON structure
2. Check required fields
3. Use correct types (numbers not strings for IDs)

### Large Response

**Problem:** Response is truncated

**Solution:**
1. Use streaming for large data
2. Implement pagination
3. Use filters to reduce data

---

## Best Practices

### 1. Use Environment Variables

- Store job IDs, execution IDs in environment
- Use variables for repeated values
- Update environment automatically with tests

### 2. Implement Error Handling

```javascript
pm.test('Job queued successfully', function () {
    const response = pm.response.json();
    pm.expect(response.success).to.be.true;
    if (!response.success) {
        console.error('Error:', response.message);
    }
});
```

### 3. Chain Requests

Use pre-request scripts to extract IDs from previous responses:

```javascript
const response = pm.response.json();
if (response.jobId && response.jobId.value) {
    pm.environment.set('job_id', response.jobId.value);
}
```

### 4. Save Useful Responses

Save example responses as "Examples" in Postman for quick reference.

### 5. Use Collections

Organize requests in folders by service:
- Job Execution
- Log Streaming
- Scheduler
- etc.

---

## 📚 See Also

- [gRPC API Reference](GRPC_API_REFERENCE.md)
- [Maven Workflow Manual](MAVEN_WORKFLOW_MANUAL.md)
- [Evans Documentation](https://github.com/ktr0731/evans)
- [grpcurl Documentation](https://github.com/fullstorydev/grpcurl)
- [Getting Started Guide](../GETTING_STARTED.md)

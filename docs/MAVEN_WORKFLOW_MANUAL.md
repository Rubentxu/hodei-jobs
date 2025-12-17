# Maven Workflow Manual - Hodei Jobs Platform

## 📋 Table of Contents

1. [Overview](#overview)
2. [Prerequisites](#prerequisites)
3. [Quick Start: Maven Build Job](#quick-start-maven-build-job)
4. [Complete Workflow](#complete-workflow)
5. [Using Docker Provider (Recommended)](#using-docker-provider-recommended)
6. [Using asdf (Development)](#using-asdf-development)
7. [Monitoring and Logs](#monitoring-and-logs)
8. [Troubleshooting](#troubleshooting)
9. [Advanced Examples](#advanced-examples)

---

## Overview

This manual demonstrates how to execute Maven build jobs in the Hodei Jobs Platform. It covers three approaches:

1. **Docker Provider** (Recommended for Production)
   - Uses pre-configured Maven images
   - Fast and reliable
   - No need to install Java/Maven at runtime

2. **asdf Installation** (Development/Testing)
   - Installs Java and Maven via asdf
   - Each job is isolated
   - Good for testing different versions

3. **Manual Script** (Custom Builds)
   - Custom build scripts
   - Full control over environment
   - Complex workflows

---

## Prerequisites

### 1. Verify Platform is Running

```bash
# Check if API is running
grpcurl -plaintext localhost:50051 list

# Expected output should include:
# hodei.AuditService
# hodei.JobExecutionService
# hodei.LogStreamService
# hodei.MetricsService
# hodei.ProviderManagementService
# hodei.SchedulerService
# hodei.WorkerAgentService
```

### 2. Install Required Tools

```bash
# Install grpcurl
go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest

# Install jq for JSON processing
# macOS
brew install jq
# Ubuntu/Debian
sudo apt-get install jq
```

### 3. Test Project Setup

We'll use a simple Java-Maven project for testing:
- GitHub: `jenkins-docs/simple-java-maven-app`
- Simple calculator application
- Good for testing builds

---

## Quick Start: Maven Build Job

### One-Command Maven Build

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'JSON'
{
  "job_definition": {
    "name": "quick-maven-build",
    "command": "/bin/bash",
    "arguments": ["-c", "cd /tmp && git clone https://github.com/jenkins-docs/simple-java-maven-app.git && cd simple-java-maven-app && mvn clean package -DskipTests && ls -lh target/*.jar"],
    "requirements": {
      "cpu_cores": 2.0,
      "memory_bytes": 2147483648,
      "disk_bytes": 1073741824
    },
    "timeout": {
      "execution_timeout": "600s"
    },
    "metadata": {
      "build_type": "maven",
      "skip_tests": "true"
    }
  },
  "queued_by": "user"
}
JSON
```

**Expected Response:**
```json
{
  "success": true,
  "message": "Job queued successfully",
  "queued_at": "2025-12-17T16:30:00Z",
  "jobId": {
    "value": "550e8400-e29b-41d4-a716-446655440000"
  }
}
```

**Save the job ID for monitoring!**

---

## Complete Workflow

### Step 1: Queue the Job

```bash
JOB_RESPONSE=$(grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'EOF'
{
  "job_definition": {
    "name": "maven-build-workflow",
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
      "project_url": "https://github.com/jenkins-docs/simple-java-maven-app",
      "build_command": "mvn clean package"
    }
  },
  "queued_by": "developer"
}
EOF
)

echo "Job Response: $JOB_RESPONSE"

# Extract job ID
JOB_ID=$(echo "$JOB_RESPONSE" | jq -r '.jobId.value')
echo "Job ID: $JOB_ID"
```

### Step 2: Check Job Status

```bash
# Get job details
grpcurl -plaintext -d "{\"job_id\": {\"value\": \"$JOB_ID\"}}" \
  localhost:50051 hodei.JobExecutionService/GetJob
```

**Sample Response:**
```json
{
  "job": {
    "jobId": {"value": "550e8400-e29b-41d4-a716-446655440000"},
    "name": "maven-build-workflow",
    "status": "JOB_STATUS_QUEUED"
  },
  "latestExecution": null,
  "events": []
}
```

### Step 3: Monitor Logs

**Option A: Stream Specific Job**

```bash
grpcurl -plaintext -d "{\"job_id\": \"$JOB_ID\", \"include_history\": true}" \
  localhost:50051 hodei.LogStreamService/SubscribeLogs
```

**Option B: Use Watch Script**

```bash
./scripts/watch_logs.sh $JOB_ID
```

**Option C: Background Watcher**

```bash
# Start watcher in background
nohup ./scripts/watch_logs.sh $JOB_ID > build.logs/$JOB_ID.out 2>&1 &

# View logs in real-time
tail -f build.logs/$JOB_ID.out
```

### Step 4: View Job Events

```bash
grpcurl -plaintext -d "{\"execution_id\": {\"value\": \"EXECUTION-ID\"}}" \
  localhost:50051 hodei.JobExecutionService/GetExecutionEvents
```

### Step 5: Get Final Status

```bash
# Wait for completion (optional)
echo "Waiting for job to complete..."
sleep 10

# Get final status
grpcurl -plaintext -d "{\"job_id\": {\"value\": \"$JOB_ID\"}}" \
  localhost:50051 hodei.JobExecutionService/GetJob
```

---

## Using Docker Provider (Recommended)

### Why Docker Provider?

✅ **Pros:**
- Pre-installed Java and Maven
- No runtime installation needed
- Consistent environment
- Fast execution
- Production-ready

❌ **Cons:**
- Requires Docker (usually available)
- Larger image sizes

### Maven Build with Docker Provider

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'JSON'
{
  "job_definition": {
    "name": "maven-docker-build",
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

### Maven with Custom Dockerfile

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'JSON'
{
  "job_definition": {
    "name": "maven-custom-build",
    "command": "/bin/bash",
    "arguments": ["-c", "cd /tmp && git clone https://github.com/my-org/my-project.git && cd my-project && mvn clean deploy -DskipTests -Drevision=$REVISION"],
    "requirements": {
      "cpu_cores": 4.0,
      "memory_bytes": 4294967296,
      "disk_bytes": 10737418240
    },
    "environment": {
      "REVISION": "1.0.0",
      "MAVEN_OPTS": "-Xmx2g"
    },
    "metadata": {
      "container_image": "my-org/maven-builder:1.0.0"
    },
    "timeout": {
      "execution_timeout": "1800s"
    }
  },
  "queued_by": "ci-system"
}
JSON
```

---

## Using asdf (Development)

### Overview

asdf allows installing multiple versions of Java and Maven without root privileges. However, **each job runs in a fresh worker**, so tools must be installed per job.

### Step 1: Test asdf Availability

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'EOF'
{
  "job_definition": {
    "name": "test-asdf",
    "command": "/bin/bash",
    "arguments": ["-lc", "echo 'Testing ASDF...'; which asdf; asdf --version"],
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
EOF
```

### Step 2: Install Java

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'EOF'
{
  "job_definition": {
    "name": "install-java",
    "command": "/bin/bash",
    "arguments": ["-lc", "export ASDF_DATA_DIR='$HOME/.asdf'; export PATH='$ASDF_DATA_DIR/shims:$PATH'; asdf plugin add java; asdf install java temurin-17.0.9+9; asdf set java temurin-17.0.9+9; asdf reshim; java -version"],
    "requirements": {
      "cpu_cores": 1.0,
      "memory_bytes": 2147483648
    },
    "timeout": {
      "execution_timeout": "600s"
    }
  },
  "queued_by": "user"
}
EOF
```

### Step 3: Install Maven

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'EOF'
{
  "job_definition": {
    "name": "install-maven",
    "command": "/bin/bash",
    "arguments": ["-lc", "export ASDF_DATA_DIR='$HOME/.asdf'; export PATH='$ASDF_DATA_DIR/shims:$PATH'; asdf plugin add maven; asdf install maven 3.9.4; asdf set maven 3.9.4; asdf reshim; mvn -version"],
    "requirements": {
      "cpu_cores": 1.0,
      "memory_bytes": 2147483648
    },
    "timeout": {
      "execution_timeout": "600s"
    }
  },
  "queued_by": "user"
}
EOF
```

### Step 4: Build Project (All in One)

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'EOF'
{
  "job_definition": {
    "name": "asdf-maven-build",
    "command": "/bin/bash",
    "arguments": ["-lc", "export ASDF_DATA_DIR='$HOME/.asdf'; export PATH='$ASDF_DATA_DIR/shims:$PATH'; asdf plugin add java maven; asdf install java temurin-17.0.9+9; asdf install maven 3.9.4; asdf set java temurin-17.0.9+9; asdf set maven 3.9.4; asdf reshim; cd /tmp && git clone https://github.com/jenkins-docs/simple-java-maven-app.git && cd simple-java-maven-app && mvn clean package -DskipTests && ls -lh target/*.jar"],
    "requirements": {
      "cpu_cores": 2.0,
      "memory_bytes": 4294967296
    },
    "timeout": {
      "execution_timeout": "900s"
    }
  },
  "queued_by": "user"
}
EOF
```

**⚠️ Note:** This takes ~5-7 minutes and requires substantial resources.

---

## Monitoring and Logs

### Real-Time Log Streaming

```bash
# Watch specific job
grpcurl -plaintext -d '{"job_id": "JOB-ID", "include_history": true}' \
  localhost:50051 hodei.LogStreamService/SubscribeLogs | jq -r '"\(.isStderr | if . then "[ERR] " else "[OUT] " end)\(.line)"'

# Save to file
grpcurl -plaintext -d '{"job_id": "JOB-ID", "include_history": true}' \
  localhost:50051 hodei.LogStreamService/SubscribeLogs \
  | jq -r '.line' > build.logs/JOB-ID.log
```

### Automated Watch Script

Create `monitor-maven-job.sh`:

```bash
#!/bin/bash
set -e

JOB_ID=$1
URL="localhost:50051"

if [ -z "$JOB_ID" ]; then
    echo "Usage: $0 <job-id>"
    exit 1
fi

echo "Monitoring job: $JOB_ID"
echo "Press Ctrl+C to stop"
echo ""

grpcurl -plaintext -d "{\"job_id\": \"$JOB_ID\", \"include_history\": true}" \
  $URL hodei.LogStreamService/SubscribeLogs \
  | jq -r --unbuffered '
    select(.line != null) |
    (.isStderr as $err | .timestamp as $ts | .line as $line) |
    if $err then "[\($ts)] [ERROR] " + $line else "[\($ts)] [INFO]  " + $line end
  '
```

Make executable:

```bash
chmod +x monitor-maven-job.sh
./monitor-maven-job.sh JOB-ID
```

### Check Job Status Periodically

```bash
# Poll job status every 5 seconds
watch -n 5 "
  grpcurl -plaintext -d '{\"job_id\": {\"value\": \"$JOB_ID\"}}' \
    localhost:50051 hodei.JobExecutionService/GetJob \
    | jq -r '.job.status, .latestExecution.state, .latestExecution.progress_percentage'
"
```

---

## Troubleshooting

### Issue 1: "command not found: mvn"

**Cause:** Java/Maven not installed in worker

**Solution:**
- Use Docker provider with `maven:3.9.4-eclipse-temurin-17`
- Or install via asdf before building

### Issue 2: "No space left on device"

**Cause:** Insufficient disk space for Maven downloads

**Solution:**
```bash
# Increase disk allocation
"requirements": {
  "cpu_cores": 2.0,
  "memory_bytes": 4294967296,
  "disk_bytes": 10737418240  # 10GB instead of 1GB
}
```

### Issue 3: "Connection timeout"

**Cause:** Job taking too long

**Solution:**
```bash
# Increase timeout
"timeout": {
  "execution_timeout": "1800s"  # 30 minutes
}
```

### Issue 4: "Out of memory"

**Cause:** Maven needs more RAM

**Solution:**
```bash
# Increase memory and add Maven options
"environment": {
  "MAVEN_OPTS": "-Xmx2g -XX:MaxMetaspaceSize=512m"
},
"requirements": {
  "memory_bytes": 4294967296  # 4GB
}
```

### Issue 5: Worker Cannot Access Files

**Cause:** Workers are isolated containers

**Solution:**
- Use inline scripts (as shown in examples)
- Use Git to clone repositories
- Don't reference local files

---

## Advanced Examples

### Example 1: Multi-Stage Build

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'JSON'
{
  "job_definition": {
    "name": "multi-stage-build",
    "command": "/bin/bash",
    "arguments": ["-c", "set -e; cd /tmp && git clone https://github.com/my-org/multi-module-app.git && cd multi-module-app && mvn clean install -DskipTests && mvn deploy -DskipTests"],
    "requirements": {
      "cpu_cores": 4.0,
      "memory_bytes": 8589934592,
      "disk_bytes": 10737418240
    },
    "timeout": {
      "execution_timeout": "1800s"
    },
    "environment": {
      "MAVEN_OPTS": "-Xmx4g",
      "MAVEN_SKIP_TESTS": "true"
    }
  },
  "queued_by": "ci-system"
}
JSON
```

### Example 2: Build with Tests

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'JSON'
{
  "job_definition": {
    "name": "build-with-tests",
    "command": "/bin/bash",
    "arguments": ["-c", "cd /tmp && git clone https://github.com/jenkins-docs/simple-java-maven-app.git && cd simple-java-maven-app && mvn clean test"],
    "requirements": {
      "cpu_cores": 2.0,
      "memory_bytes": 4294967296,
      "disk_bytes": 1073741824
    },
    "timeout": {
      "execution_timeout": "900s"
    }
  },
  "queued_by": "developer"
}
JSON
```

### Example 3: Release Build

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'JSON'
{
  "job_definition": {
    "name": "release-build",
    "command": "/bin/bash",
    "arguments": ["-c", "cd /tmp && git clone https://github.com/my-org/release-project.git && cd release-project && mvn clean deploy -DskipTests -Drevision=$RELEASE_VERSION -Dcommit=$COMMIT_SHA"],
    "requirements": {
      "cpu_cores": 4.0,
      "memory_bytes": 8589934592,
      "disk_bytes": 10737418240
    },
    "environment": {
      "RELEASE_VERSION": "1.0.0",
      "COMMIT_SHA": "abc123",
      "MAVEN_OPTS": "-Xmx4g"
    },
    "timeout": {
      "execution_timeout": "1800s"
    }
  },
  "queued_by": "release-bot"
}
JSON
```

### Example 4: Custom Maven Settings

```bash
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'JSON'
{
  "job_definition": {
    "name": "custom-maven-settings",
    "command": "/bin/bash",
    "arguments": ["-c", "cd /tmp && git clone https://github.com/my-org/private-project.git && cd private-project && mvn clean package -s settings.xml -DskipTests"],
    "requirements": {
      "cpu_cores": 2.0,
      "memory_bytes": 2147483648
    },
    "environment": {
      "MAVEN_SETTINGS_URL": "https://nexus.example.com/repository/maven-public/settings.xml",
      "MAVEN_OPTS": "-Xmx2g -Dmaven.wagon.http.ssl.insecure=true"
    },
    "timeout": {
      "execution_timeout": "900s"
    }
  },
  "queued_by": "developer"
}
JSON
```

---

## 📚 See Also

- [gRPC API Reference](GRPC_API_REFERENCE.md)
- [Postman Configuration Guide](POSTMAN_CONFIGURATION.md)
- [Getting Started Guide](../GETTING_STARTED.md)
- [Maven Solution Working](../MAVEN_SOLUTION_WORKING.md)
- [asdf Documentation](https://asdf-vm.com/)

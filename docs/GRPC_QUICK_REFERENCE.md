# gRPC Quick Reference Card

## 🚀 Quick Commands

### Watch Logs
```bash
# Watch specific job
./scripts/trace-job.sh JOB-ID

# Watch with custom interval
./scripts/trace-job.sh JOB-ID --interval 5

# List all running jobs
./scripts/list-jobs.sh --running

# List jobs in JSON format
./scripts/list-jobs.sh --json
```

### Maven Build (Recommended)
```bash
# Using Docker provider (fastest)
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob << 'JSON'
{
  "job_definition": {
    "name": "maven-build",
    "command": "/bin/bash",
    "arguments": ["-c", "cd /tmp && git clone https://github.com/jenkins-docs/simple-java-maven-app.git && cd simple-java-maven-app && mvn clean package -DskipTests"],
    "requirements": {"cpu_cores": 2.0, "memory_bytes": 2147483648},
    "timeout": {"execution_timeout": "600s"}
  },
  "queued_by": "user"
}
JSON
```

### Job Operations
```bash
# Queue job
grpcurl -plaintext -d @ localhost:50051 hodei.JobExecutionService/QueueJob

# Get job status
grpcurl -plaintext -d '{"job_id": {"value": "JOB-ID"}}' localhost:50051 hodei.JobExecutionService/GetJob

# List jobs
grpcurl -plaintext -d '{"limit": 50}' localhost:50051 hodei.JobExecutionService/ListJobs

# Cancel job
grpcurl -plaintext -d '{"execution_id": {"value": "EXEC-ID"}, "reason": "User request"}' localhost:50051 hodei.JobExecutionService/CancelJob
```

### Log Streaming
```bash
# Subscribe to logs
grpcurl -plaintext -d '{"job_id": "JOB-ID", "include_history": true}' localhost:50051 hodei.LogStreamService/SubscribeLogs
```

---

## 📚 Common Status Values

| Status | Description |
|--------|-------------|
| `JOB_STATUS_PENDING` | Job created but not queued |
| `JOB_STATUS_QUEUED` | Job in queue |
| `JOB_STATUS_ASSIGNED` | Assigned to worker |
| `JOB_STATUS_RUNNING` | Currently executing |
| `JOB_STATUS_COMPLETED` | Finished successfully |
| `JOB_STATUS_FAILED` | Failed execution |
| `JOB_STATUS_CANCELLED` | Cancelled by user |

---

## 🔧 Useful Scripts

| Script | Description |
|--------|-------------|
| `./scripts/trace-job.sh JOB-ID` | Trace job from start to finish |
| `./scripts/list-jobs.sh --running` | List running jobs |
| `./scripts/watch_logs.sh JOB-ID` | Watch logs for specific job |
| `./scripts/run_maven_job.sh` | Run Maven example |

---

## 📖 Full Documentation

- [gRPC API Reference](GRPC_API_REFERENCE.md) - Complete API documentation
- [Postman Configuration](POSTMAN_CONFIGURATION.md) - Postman setup guide
- [Maven Workflow Manual](MAVEN_WORKFLOW_MANUAL.md) - Maven build guide
- [Maven Solution Working](../MAVEN_SOLUTION_WORKING.md) - Maven solution

---

## ⚡ Quick Tips

1. **Use Docker Provider** for Maven builds (faster, more reliable)
2. **Extract Job ID** from QueueJob response for tracking
3. **Monitor with trace-job.sh** for complete visibility
4. **Check logs regularly** with SubscribeLogs
5. **Set appropriate timeouts** for long-running jobs

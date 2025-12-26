# Error Reference and Troubleshooting Guide

**Hodei Job Platform Error Documentation**

**Version:** 1.0.0  
**Last Updated:** 2024-12-26  
**Status:** Active

---

## Table of Contents

1. [Overview](#overview)
2. [Error Code Reference](#error-code-reference)
3. [Troubleshooting Guide](#troubleshooting-guide)
4. [Error Recovery Procedures](#error-recovery-procedures)
5. [Event Documentation](#event-documentation)

---

## Overview

This document provides comprehensive reference for all error types in Hodei Job Platform, including error codes, descriptions, causes, and suggested recovery actions. The error transparency system categorizes failures into structured types, enabling automated diagnostics and faster resolution.

### Error Severity Levels

| Level | Description | Response Time |
|-------|-------------|---------------|
| `USER` | Error caused by job configuration or code | User action required |
| `INFRASTRUCTURE` | Error caused by system infrastructure | Operator action required |
| `CRITICAL` | System-level failure requiring immediate attention | Immediate response required |

---

## Error Code Reference

### Job Execution Errors (1000-1999)

Error codes in this range relate to failures during job execution on workers.

| Code | Error Type | Severity | Description |
|------|------------|----------|-------------|
| `ERR_1001` | `COMMAND_NOT_FOUND` | USER | Executable command not found in PATH |
| `ERR_1002` | `PERMISSION_DENIED` | USER | Insufficient permissions to access resource |
| `ERR_1003` | `FILE_NOT_FOUND` | USER | Required file or directory does not exist |
| `ERR_1004` | `PROCESS_SPAWN_FAILED` | INFRASTRUCTURE | Operating system failed to spawn process |
| `ERR_1005` | `EXECUTION_TIMEOUT` | USER | Job exceeded configured timeout limit |
| `ERR_1006` | `SIGNAL_RECEIVED` | USER | Process terminated by signal (SIGTERM, SIGKILL) |
| `ERR_1007` | `NON_ZERO_EXIT_CODE` | USER | Command exited with non-zero status |
| `ERR_1008` | `INFRASTRUCTURE_ERROR` | INFRASTRUCTURE | Infrastructure-level failure during execution |
| `ERR_1009` | `VALIDATION_ERROR` | USER | Job configuration validation failed |
| `ERR_1010` | `CONFIGURATION_ERROR` | USER | Invalid configuration parameter |
| `ERR_1011` | `SECRET_INJECTION_ERROR` | USER | Failed to inject required secrets |
| `ERR_1012` | `IO_ERROR` | INFRASTRUCTURE | Input/output operation failed |
| `ERR_1099` | `UNKNOWN` | INFRASTRUCTURE | Unclassified error (requires investigation) |

### Job Dispatch Errors (2000-2999)

Error codes in this range relate to failures in dispatching jobs to workers.

| Code | Error Type | Severity | Description |
|------|------------|----------|-------------|
| `ERR_2001` | `DISPATCH_WORKER_NOT_READY` | INFRASTRUCTURE | Worker not in ready state for dispatch |
| `ERR_2002` | `DISPATCH_TIMEOUT` | INFRASTRUCTURE | Dispatch operation timed out |
| `ERR_2003` | `DISPATCH_WORKER_CRASHED` | INFRASTRUCTURE | Worker crashed during dispatch |
| `ERR_2004` | `DISPATCH_CHANNEL_CLOSED` | INFRASTRUCTURE | Communication channel closed unexpectedly |
| `ERR_2005` | `DISPATCH_PROTOCOL_ERROR` | INFRASTRUCTURE | Protocol violation during dispatch |

### Worker Provisioning Errors (3000-3999)

Error codes in this range relate to failures in worker provisioning.

| Code | Error Type | Severity | Description |
|------|------------|----------|-------------|
| `ERR_3001` | `PROVISION_IMAGE_PULL_FAILED` | INFRASTRUCTURE | Failed to pull worker container image |
| `ERR_3002` | `PROVISION_RESOURCE_FAILED` | INFRASTRUCTURE | Failed to allocate required resources |
| `ERR_3003` | `PROVISION_NETWORK_FAILED` | INFRASTRUCTURE | Network configuration failed |
| `ERR_3004` | `PROVISION_AUTH_FAILED` | INFRASTRUCTURE | Authentication with provider failed |
| `ERR_3005` | `PROVISION_TIMEOUT` | INFRASTRUCTURE | Provisioning exceeded time limit |
| `ERR_3006` | `PROVISION_PROVIDER_UNAVAILABLE` | INFRASTRUCTURE | Target provider is unavailable |
| `ERR_3007` | `PROVISION_INTERNAL_ERROR` | CRITICAL | Internal provisioning error |

### Scheduling Errors (4000-4999)

Error codes in this range relate to scheduling decision failures.

| Code | Error Type | Severity | Description |
|------|------------|----------|-------------|
| `ERR_4001` | `SCHEDULE_NO_PROVIDERS` | INFRASTRUCTURE | No providers available matching requirements |
| `ERR_4002` | `SCHEDULE_NO_MATCH` | USER | No provider matches job requirements |
| `ERR_4003` | `SCHEDULE_OVERLOADED` | INFRASTRUCTURE | All providers at capacity |
| `ERR_4004` | `SCHEDULE_UNHEALTHY` | INFRASTRUCTURE | All matching providers unhealthy |
| `ERR_4005` | `SCHEDULE_RESOURCES` | USER | Resource requirements cannot be met |
| `ERR_4006` | `SCHEDULE_INTERNAL` | CRITICAL | Internal scheduler error |

### Provider Errors (5000-5999)

Error codes in this range relate to provider-specific failures.

| Code | Error Type | Severity | Description |
|------|------------|----------|-------------|
| `ERR_5001` | `PROVIDER_CONNECTION_LOST` | INFRASTRUCTURE | Connection to provider lost |
| `ERR_5002` | `PROVIDER_AUTH_FAILED` | INFRASTRUCTURE | Provider authentication failed |
| `ERR_5003` | `PROVIDER_RESOURCE_LIMIT` | INFRASTRUCTURE | Provider resource limit exceeded |
| `ERR_5004` | `PROVIDER_NOT_FOUND` | INFRASTRUCTURE | Provider not found |
| `ERR_5005` | `PROVIDER_PROVISION_FAILED` | INFRASTRUCTURE | Provider provisioning failed |
| `ERR_5006` | `PROVIDER_TIMEOUT` | INFRASTRUCTURE | Provider operation timed out |
| `ERR_5007` | `PROVIDER_CONFIG_ERROR` | INFRASTRUCTURE | Provider configuration error |
| `ERR_5008` | `PROVIDER_INTERNAL` | CRITICAL | Provider internal error |

---

## Troubleshooting Guide

### ERR_1001: COMMAND_NOT_FOUND

**Symptoms:**
- Job fails immediately after starting
- Error message contains "not found" or "command not found"
- Exit code: 127

**Diagnosis Steps:**
1. Check if the command exists in the worker image
2. Verify PATH includes the command location
3. Confirm the command binary is installed

**Solutions:**
```bash
# Verify command exists in container
docker run --rm <image> which <command>

# If missing, add to Dockerfile
RUN apt-get update && apt-get install -y <package>
# OR
RUN pip install <package>
```

**Suggested Actions:**
- Verify the command is installed in the worker image
- Check PATH environment variable
- Use absolute path to the executable
- Add installation step to job or custom image

---

### ERR_1002: PERMISSION_DENIED

**Symptoms:**
- Job fails when accessing files or directories
- Error message contains "Permission denied"
- Exit code: 126

**Diagnosis Steps:**
1. Identify the file/directory from error message
2. Check current user permissions
3. Verify SELinux/AppArmor context (if applicable)

**Solutions:**
```bash
# Check permissions
ls -la /path/to/file

# Fix permissions
chmod +r /path/to/file      # Read access
chmod +x /path/to/script.sh # Execute access

# For directories, need execute permission to traverse
chmod +X /path/to/directory
```

**Suggested Actions:**
- Verify file permissions with `ls -la`
- Add execute permission to scripts with `chmod +x`
- Check parent directory permissions for path traversal
- Ensure worker user has required access rights

---

### ERR_1003: FILE_NOT_FOUND

**Symptoms:**
- Job fails when trying to read/write a file
- Error message contains "No such file or directory"
- Exit code: 2

**Diagnosis Steps:**
1. Verify the file path is correct
2. Check if file was created in previous step
3. Confirm working directory is as expected

**Solutions:**
```bash
# Verify file exists
ls -la /expected/path/file.csv

# Check working directory
pwd
echo $WORKDIR

# Create missing directory
mkdir -p /expected/path
```

**Suggested Actions:**
- Verify the file path in error message
- Use absolute paths instead of relative paths
- Create necessary directories before accessing them
- Check that previous job steps created the expected file

---

### ERR_1004: PROCESS_SPAWN_FAILED

**Symptoms:**
- Job fails during process creation
- Error message contains "spawn" or "fork" related terms
- Exit code: -1

**Diagnosis Steps:**
1. Check system resources (memory, file descriptors)
2. Verify user process limits
3. Check for system-level restrictions

**Solutions:**
```bash
# Check process limits
ulimit -a

# Increase limits in /etc/security/limits.conf
# username soft nofile 65536
# username hard nofile 65536
```

**Suggested Actions:**
- Check system resource availability
- Verify user process limits
- Monitor memory and CPU usage
- Review system ulimits

---

### ERR_1005: EXECUTION_TIMEOUT

**Symptoms:**
- Job running longer than configured timeout
- Error message indicates timeout exceeded
- Exit code: 124 (if using timeout command)

**Diagnosis Steps:**
1. Review job execution time history
2. Check for performance degradation
3. Identify bottlenecks in job logic

**Solutions:**
```yaml
# Increase timeout in job definition
timeout_config:
  execution_timeout: 3600s  # 1 hour instead of default
```

**Optimization:**
```bash
# Profile job execution
time ./job_script.sh

# Check for unnecessary waits or retries
```

**Suggested Actions:**
- Increase timeout in job configuration
- Optimize job script performance
- Check for infinite loops or long waits
- Consider breaking large jobs into smaller steps

---

### ERR_1006: SIGNAL_RECEIVED

**Symptoms:**
- Job terminates abruptly
- Error message shows signal number
- Exit code: 128 + signal_number (e.g., 143 = SIGTERM)

**Common Signals:**
| Signal | Number | Cause |
|--------|--------|-------|
| SIGTERM | 15 | Graceful termination request |
| SIGKILL | 9 | Forced termination |
| SIGSEGV | 11 | Segmentation fault |
| SIGABRT | 6 | Abort signal |

**Diagnosis Steps:**
1. Identify source of signal (system, user, or job)
2. Check for OOM killer activity
3. Review job cleanup handlers

**Solutions:**
```bash
# Check OOM killer logs
dmesg | grep -i oom

# Increase memory limits
resource_requirements:
  memory_bytes: 4294967296  # 4GB
```

**Suggested Actions:**
- Check memory limits and increase if needed
- Review job for memory leaks
- Handle signals in job scripts
- Check for external termination requests

---

### ERR_1007: NON_ZERO_EXIT_CODE

**Symptoms:**
- Command completes but returns non-zero exit code
- Exit code indicates specific error condition
- Exit code: 1-125 (custom error codes)

**Diagnosis Steps:**
1. Review stdout/stderr for error details
2. Check command documentation for exit codes
3. Identify which step in job pipeline failed

**Solutions:**
```bash
# Capture detailed output
./script.sh 2>&1 | tee job.log

# Check specific exit code meaning
# Exit code 1: General error
# Exit code 2: Misuse of shell command
# Exit code 126: Command not executable
# Exit code 127: Command not found
```

**Suggested Actions:**
- Review job logs for specific error messages
- Check command documentation for exit code meanings
- Fix the underlying issue in the job script
- Add proper error handling

---

### ERR_1008: INFRASTRUCTURE_ERROR

**Symptoms:**
- Job fails due to infrastructure issues
- Error message indicates system-level problems
- May be transient or persistent

**Diagnosis Steps:**
1. Check provider status
2. Review worker health
3. Check network connectivity
4. Examine infrastructure logs

**Solutions:**
```bash
# Check provider health
hodei provider status

# Check worker logs
hodei worker logs <worker_id>

# Restart affected services
hodei service restart
```

**Suggested Actions:**
- Check provider health status
- Review worker logs for details
- Retry the job (may be transient)
- Contact infrastructure team if persistent

---

### ERR_1009: VALIDATION_ERROR

**Symptoms:**
- Job rejected during submission
- Error message indicates field validation failure
- Error context shows specific field

**Diagnosis Steps:**
1. Review error context for field name
2. Check job definition against schema
3. Validate resource requirements format

**Solutions:**
```yaml
# Example: Fix invalid field
job_definition:
  name: "valid-name"  # No spaces, special chars
  requirements:
    cpu_cores: 2.0     # Positive number
    memory_bytes: 1073741824  # Positive integer
```

**Suggested Actions:**
- Check the specific field mentioned in error
- Verify job definition against schema
- Use provided SDK/CLI for job creation
- Review job definition examples

---

### ERR_1010: CONFIGURATION_ERROR

**Symptoms:**
- Job fails due to configuration issues
- Error message shows configuration key
- May affect multiple jobs

**Diagnosis Steps:**
1. Identify the configuration key from error
2. Check environment-specific config
3. Verify configuration file syntax

**Solutions:**
```bash
# Check configuration
hodei config show

# Validate YAML syntax
yamllint /etc/hodei/config.yaml

# Set missing configuration
hodei config set <key> <value>
```

**Suggested Actions:**
- Check system configuration
- Validate configuration file syntax
- Review configuration requirements for job type
- Contact operator for missing configuration

---

### ERR_1011: SECRET_INJECTION_ERROR

**Symptoms:**
- Job fails when accessing secrets
- Error message indicates secret not found or access denied
- Environment variables not set

**Diagnosis Steps:**
1. Verify secret exists in vault
2. Check secret permissions for worker
3. Review secret injection configuration

**Solutions:**
```bash
# Check available secrets
hodei secret list

# Verify secret exists
hodei secret get <secret_name>

# Update secret permissions
hodei secret grant <secret_name> --to worker=<worker_id>
```

**Suggested Actions:**
- Verify secret exists in vault
- Check worker has access to secret
- Review secret injection configuration
- Create or update secret before resubmission

---

### ERR_1012: IO_ERROR

**Symptoms:**
- Job fails during file I/O operations
- Error message indicates read/write failure
- May be disk-related or network storage issue

**Diagnosis Steps:**
1. Check disk space availability
2. Verify storage mount points
3. Test network storage connectivity

**Solutions:**
```bash
# Check disk space
df -h

# Check inodes
df -i

# Test storage write
touch /mount/test && rm /mount/test
```

**Suggested Actions:**
- Check disk space with `df -h`
- Verify storage mount points
- Test write capability to storage
- Contact operator for storage issues

---

### Scheduling Errors (ERR_4001-ERR_4006)

**Diagnosis Steps:**
1. Check provider availability: `hodei provider list`
2. Review resource requirements: `hodei job describe <job_id>`
3. Check provider capacity: `hodei provider capacity`

**Solutions:**
```yaml
# Relax scheduling constraints
scheduling:
  required_labels: {}  # Remove strict requirements
  preferred_provider: null  # Any provider
  
# Or increase resources
requirements:
  cpu_cores: 1  # Reduce from higher value
  memory_bytes: 1073741824  # Reduce from higher value
```

**Suggested Actions:**
- Check available providers
- Review job resource requirements
- Relax scheduling constraints if possible
- Wait for provider capacity to become available

---

## Error Recovery Procedures

### General Recovery Workflow

```
1. Identify Error Type
   └── Check event logs for JobExecutionError
   
2. Classify Severity
   └── USER: Fix job configuration
   └── INFRASTRUCTURE: Contact operator
   └── CRITICAL: Immediate response required
   
3. Execute Recovery
   └── Apply appropriate solution from guide
   
4. Verify Recovery
   └── Resubmit job and monitor
   └── Check new execution events
```

### Retry Configuration

```yaml
scheduling:
  retry_policy:
    max_attempts: 3
    backoff_multiplier: 2.0
    initial_delay: 60s
    retryable_errors:
      - ERR_1008  # INFRASTRUCTURE_ERROR
      - ERR_2002  # DISPATCH_TIMEOUT
      - ERR_5001  # PROVIDER_CONNECTION_LOST
```

### Emergency Procedures

**For CRITICAL errors (ERR_3007, ERR_4006, ERR_5008):**

1. Stop new job submissions: `hodei system maintenance --mode=drain`
2. Check system status: `hodei system status`
3. Review recent events: `hodei events --severity=CRITICAL`
4. Contact infrastructure team
5. After resolution: `hodei system maintenance --mode=active`

---

## Event Documentation

### JobExecutionError Event

Published when a job fails during execution.

```json
{
  "event_type": "JobExecutionError",
  "job_id": "uuid",
  "worker_id": "uuid",
  "failure_reason": {
    "type": "PERMISSION_DENIED",
    "path": "/home/user/script.sh",
    "operation": "execute"
  },
  "exit_code": 126,
  "command": "/home/user/script.sh",
  "arguments": ["--input", "data.csv"],
  "working_dir": "/tmp/work",
  "execution_time_ms": 45,
  "suggested_actions": [
    "Verify script permissions: chmod +x /home/user/script.sh",
    "Check user has execute permission on parent directory"
  ],
  "occurred_at": "2024-12-26T10:30:00Z",
  "correlation_id": "uuid"
}
```

### WorkerProvisioningError Event

Published when worker provisioning fails.

```json
{
  "event_type": "WorkerProvisioningError",
  "worker_id": "uuid",
  "provider_id": "uuid",
  "failure_reason": {
    "type": "IMAGE_PULL_FAILED",
    "image": "hodei/worker:v2.0",
    "message": "connection timeout"
  },
  "occurred_at": "2024-12-26T10:25:00Z",
  "correlation_id": "uuid"
}
```

### SchedulingDecisionFailed Event

Published when scheduling cannot complete.

```json
{
  "event_type": "SchedulingDecisionFailed",
  "job_id": "uuid",
  "failure_reason": {
    "type": "NO_PROVIDERS_AVAILABLE"
  },
  "attempted_providers": ["docker-1", "kubernetes"],
  "occurred_at": "2024-12-26T10:20:00Z",
  "correlation_id": "uuid"
}
```

---

## Related Documentation

- [Architecture Overview](architecture.md)
- [Worker Lifecycle Design](worker-lifecycle-design.md)
- [Provider Architecture](PROVIDER_ARCHITECTURE.md)
- [Troubleshooting Guide](TROUBLESHOOTING.md)
- [Analysis: Error Transparency](analysis/ANALYSIS-error-transparency-2024.md)

---

## Change Log

| Version | Date | Description |
|---------|------|-------------|
| 1.0.0 | 2024-12-26 | Initial error documentation |

---

*For questions or corrections, please open an issue or contact the platform team.*

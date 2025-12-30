---
name: pruebas-cli
description: Complete E2E testing skill for Hodei Job Platform. Tests manual job execution across Docker and Kubernetes providers, validates the complete job lifecycle from creation to completion, verifies log stream persistence, checks domain event publishing sequence, validates worker lifecycle states, and confirms worker resource cleanup. Use this skill after platform changes, for debugging job failures, validating deployments, or running integration tests. Se tiene que observa que se levante los worker en kubernetes o docker que devuelva streams de logs, revisa que esta pasando en el codigo consultando la BBDD, flujos secuencial y coherente de estados y eventos e investiga en que porcion de codigo puede estar el posible problema
license: MIT
---

# Pruebas CLI - Hodei E2E Testing Skill

Comprehensive end-to-end testing skill for Hodei Job Platform that validates the entire job lifecycle across infrastructure providers.

## Overview

This skill enables Claude to execute thorough integration tests of the Hodei Job Platform. It validates all infrastructure components, executes test jobs through the CLI, and verifies proper functioning of job lifecycle, log streaming, domain events, worker lifecycle states, and worker cleanup. The skill produces detailed verification reports for each component tested.

## When to Use This Skill

Use this skill in these scenarios:

- **Post-change validation**: Run after code changes to verify the platform works correctly
- **Infrastructure health check**: Validate PostgreSQL, gRPC server, Docker, and Kubernetes before deployments
- **Job failure investigation**: Debug why a specific job failed or got stuck in a non-terminal state
- **Event flow validation**: Verify domain events are published in correct sequence and order
- **Audit trail verification**: Check that all actions are properly audited with correlation IDs
- **Lifecycle coherence**: Validate job and worker state transitions are sequential and consistent
- **Log stream verification**: Confirm logs are persisted correctly without data loss
- **Worker cleanup verification**: Confirm workers are terminated properly after job completion
- **Multi-provider testing**: Test job execution on both Docker and Kubernetes providers
- **Performance comparison**: Compare execution metrics between providers

## Instructions

Follow this workflow when using this skill:

### Step 1: Validate Infrastructure

Always start by checking all infrastructure components are available and responsive:

```bash
skill: pruebas-cli infra          # Full infrastructure check (db, server, docker, k8s)
skill: pruebas-cli health         # Quick health summary
skill: pruebas-cli network        # Network connectivity check for workers
```

### Network Connectivity Check

Critical for worker→server communication:

```bash
skill: pruebas-cli network        # Verify workers can reach gRPC server
```

**What Gets Validated:**

| Check | Description | Common Issues |
|-------|-------------|---------------|
| **gRPC Reachability** | Server port 50051 is accessible | Firewall, wrong IP |
| **Docker Network** | Docker bridge is configured | `host.docker.internal` not resolvable |
| **Kubernetes Network** | Pods can reach server | `192.168.65.2` not routable from K8s |
| **Worker→Server Path** | Verify worker→server connectivity | NetworkUnreachable errors |

**Common Network Issues:**

| Symptom | Cause | Solution |
|---------|-------|----------|
| `NetworkUnreachable` | Worker can't route to server IP | Set `HODEI_SERVER_HOST` to accessible IP |
| `Connection refused` | Server not running or wrong port | Check `GRPC_PORT` env var |
| `host.docker.internal` unknown | Docker DNS not configured | Add to `/etc/hosts` or use IP directly |
| K8s pods can't connect | Different network namespace | Use LoadBalancer or NodePort |

**Network Verification Commands:**

```bash
# Get server IP
SERVER_IP=$(hostname -I | awk '{print $1}')
echo "Server IP: $SERVER_IP"

# Test from Docker
docker run --rm busybox wget -qO- http://$SERVER_IP:50051/health

# Test from K8s
kubectl run -it --rm debug --image=busybox --restart=Never -- \
  wget -qO- http://$SERVER_IP:50051/health

# Check if port is listening
netstat -tlnp | grep 50051

# Verify HODEI_SERVER_HOST
echo "HODEI_SERVER_HOST: ${HODEI_SERVER_HOST:-not set}"
```

### Step 2: Create and Execute Test Jobs

Run test jobs on the desired provider using in-situ job creation:

```bash
# Basic job execution
skill: pruebas-cli run docker           # Execute default job on Docker
skill: pruebas-cli run kubernetes       # Execute default job on Kubernetes

# In-situ job creation with custom characteristics
skill: pruebas-cli create echo          # Simple echo job
skill: pruebas-cli create sleep 60      # Long-running job (60s)
skill: pruebas-cli create fail          # Intentional failure test
skill: pruebas-cli create resources     # High resource job
skill: pruebas-cli create env           # Job with environment variables
skill: pruebas-cli create args "echo hello world"  # Custom command

# Multi-provider test suite
skill: pruebas-cli smoke                # Quick smoke test (single job)
skill: pruebas-cli job-flow             # Complete workflow test
skill: pruebas-cli compare              # Docker vs Kubernetes comparison
skill: pruebas-cli full                 # Full E2E test suite
```

### Step 3: Report and Analyze Results

Get comprehensive reports for any job:

```bash
skill: pruebas-cli report <job-id>      # Full verification report with findings
skill: pruebas-cli errors <job-id>      # Business logic errors only
skill: pruebas-cli logs <job-id>        # Job stream logs extraction
skill: pruebas-cli verify <job-id>      # Complete 6-point verification
skill: pruebas-cli events <job-id>      # Domain event sequence and flow
skill: pruebas-cli audit <job-id>       # Audit trail verification
skill: pruebas-cli lifecycle <job-id>   # Job and worker state coherence
skill: pruebas-cli stream <job-id>      # Verify log stream completeness
skill: pruebas-cli cleanup <job-id>     # Verify worker termination
skill: pruebas-cli trace <job-id>       # Full traceability trace
```

### Step 4: Batch Analysis

Analyze multiple jobs or run comprehensive checks:

```bash
skill: pruebas-cli batch 5              # Create and verify 5 test jobs
skill: pruebas-cli findings             # All findings from recent tests
skill: pruebas-cli failures             # All failed jobs with error analysis
```

## What Gets Validated

### Infrastructure Components

| Component | What Gets Checked |
|-----------|-------------------|
| **PostgreSQL** | Container running, responsive, schema loaded, tables exist |
| **gRPC Server** | Process running, port 50051 open, CLI connection successful |
| **Docker** | Daemon running, provider configured in database |
| **Kubernetes** | Cluster accessible, kubectl working, provider configured |

### Job Execution Verification (7-Point Check)

| Check | Description | Pass Criteria |
|-------|-------------|---------------|
| **1. Status** | Job reached terminal state | `SUCCEEDED` or `FAILED` |
| **2. Events** | All critical events published | Correct sequence, no gaps |
| **3. Logs** | Log stream persisted | At least 1 log file entry in database |
| **4. Provider** | Correct provider assigned | Provider ID recorded, type matches request |
| **5. Provider Isolation** | Job runs on correct infra | K8s jobs in K8s, Docker jobs in Docker |
| **6. Cleanup** | Worker properly handled | Worker terminated or in valid state |
| **7. Timing** | Execution metrics recorded | Duration calculated and stored |

### Provider Verification Checklist

For Kubernetes jobs:
- [ ] Provider selected is `kubernetes`
- [ ] kubectl pod exists in `hodei-jobs-workers` namespace
- [ ] Pod status is Running/Completed
- [ ] No Docker containers created for this job
- [ ] Logs retrieved via `kubectl logs` match database

For Docker jobs:
- [ ] Provider selected is `docker`
- [ ] Docker container exists with `hodei-worker-*` name
- [ ] Container status is Running
- [ ] No Kubernetes pods created for this job
- [ ] Logs retrieved via `docker logs` match database

### Domain Event Flow Verification

Verify events are published in correct order:

```bash
skill: pruebas-cli events <job-id>
```

**Expected Event Sequence for Job:**
```
1. JobCreated          → Job was created
2. JobAssigned         → Worker assigned to job
3. JobStatusChanged → ASSIGNED
4. JobStatusChanged → RUNNING
5. JobStatusChanged → SUCCEEDED/FAILED
```

**Expected Event Sequence for Worker:**
```
1. WorkerRegistered    → Worker registered with server
2. WorkerAssigned      → Worker assigned to job
3. JobStatusChanged → RUNNING (worker's current job)
4. WorkerTerminated    → Worker terminated (if ephemeral)
```

**Validation Criteria:**
- Events are chronologically ordered (no out-of-order events)
- No missing events in the sequence
- Timestamp consistency across events
- Correct aggregate_id references

### Audit Trail Verification

Verify all actions are properly audited:

```bash
skill: pruebas-cli audit <job-id>
```

**What Gets Verified:**
- All domain events have corresponding audit entries
- Actor field is populated correctly
- Correlation ID links related actions
- Timestamps are consistent with events
- Action types match event types

**Audit Entry Fields:**
| Field | Description | Validation |
|-------|-------------|------------|
| `event_type` | Type of action | Matches domain event |
| `actor` | Who performed action | Not null |
| `correlation_id` | Links related actions | Consistent across job |
| `occurred_at` | When it happened | Chronological |
| `payload` | Action details | JSON valid |

### Lifecycle State Coherence

Validate job and worker state transitions:

```bash
skill: pruebas-cli lifecycle <job-id>
```

**Job State Machine:**
```
CREATED → PENDING → ASSIGNED → RUNNING → SUCCEEDED
                                        ↓
                                   (or FAILED)

Valid transitions:
- CREATED → PENDING
- PENDING → ASSIGNED
- ASSIGNED → RUNNING
- RUNNING → SUCCEEDED
- RUNNING → FAILED

Invalid transitions (should not happen):
- CREATED → RUNNING (skips states)
- PENDING → SUCCEEDED (skips ASSIGNED, RUNNING)
- Any state → itself (duplicate events)
```

**Worker State Machine:**
```
REGISTERED → BUSY → READY → TERMINATED

Valid transitions:
- REGISTERED → BUSY (job assigned)
- BUSY → READY (job completed)
- READY → BUSY (new job)
- READY → TERMINATED (cleanup)

Invalid transitions:
- REGISTERED → TERMINATED (no job ever ran)
- BUSY → TERMINATED (skips READY)
```

**Coherence Validation:**
- No state regression (e.g., RUNNING → PENDING)
- No skipped states in critical paths
- State duration is reasonable
- Terminal states (SUCCEEDED/FAILED/TERMINATED) are final

## Domain Events Verified

| Event | Purpose | Required |
|-------|---------|----------|
| `JobCreated` | Job was created in system | Yes |
| `JobAssigned` | Worker assigned to job | Yes |
| `JobStatusChanged` → `ASSIGNED` | Job assigned to worker | Yes |
| `JobStatusChanged` → `RUNNING` | Execution started | Yes |
| `JobStatusChanged` → `SUCCEEDED/FAILED` | Execution completed | Yes |
| `WorkerRegistered` | Worker registered | If worker created |
| `WorkerAssigned` | Worker got a job | If worker used |
| `WorkerTerminated` | Worker cleaned up | If applicable |

## Requirements

### Mandatory

- Docker and Docker Compose running
- Rust toolchain (`cargo`) installed
- PostgreSQL container (`hodei-jobs-postgres`) running
- `hodei-jobs-cli` binary built at `./target/debug/hodei-jobs-cli`

### Optional

- `kubectl` installed and configured for Kubernetes tests
- Access to a Kubernetes cluster
- Kubernetes provider configured in database

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `HODEI_GRPC_URL` | `http://localhost:50051` | gRPC server address |
| `CLI_BIN` | `./target/debug/hodei-jobs-cli` | Path to CLI binary |

### Database Connection

```
Container: hodei-jobs-postgres
Database:  hodei_jobs
User:      postgres
```

### Output Locations

Test evidence and logs are stored in:

- `build/test-evidence/` - Test results and verification evidence
- `logs/` - CLI execution logs
- `build/test-results/` - Structured test result files

## Usage Examples

### Quick Infrastructure Check

```bash
# Full infrastructure validation
skill: pruebas-cli infra

# Quick health summary
skill: pruebas-cli health
```

### Test Docker Provider

```bash
# Run a test job on Docker
skill: pruebas-cli run docker

# Run smoke test on Docker
skill: pruebas-cli smoke
```

### Test Kubernetes Provider

```bash
# Run a test job on Kubernetes
skill: pruebas-cli run kubernetes
```

### Verify Event Flow for a Job

```bash
# Get job ID from previous run, then check event sequence
skill: pruebas-cli events 123e4567-e89b-12d3-a456-426614174000

# Expected output shows:
# - Event timeline in order
# - State transitions
# - Timestamps for each event
# - Any gaps or out-of-order events
```

### Check Audit Trail

```bash
# Verify all actions are audited
skill: pruebas-cli audit 123e4567-e89b-12d3-a456-426614174000

# Expected output shows:
# - All audit entries for the job
# - Actor for each action
# - Correlation ID linking
# - Timestamp consistency
```

### Validate Lifecycle Coherence

```bash
# Check job and worker state transitions
skill: pruebas-cli lifecycle 123e4567-e89b-12d3-a456-426614174000

# Expected output shows:
# - Job state machine diagram
# - Worker state transitions
# - Invalid transitions (if any)
# - State duration analysis
```

### Debug a Failed Job

```bash
# Comprehensive verification
skill: pruebas-cli verify 123e4567-e89b-12d3-a456-426614174000

# Check event flow
skill: pruebas-cli events 123e4567-e89b-12d3-a456-426614174000

# Check audit trail
skill: pruebas-cli audit 123e4567-e89b-12d3-a456-426614174000

# Check lifecycle coherence
skill: pruebas-cli lifecycle 123e4567-e89b-12d3-a456-426614174000

# Check log stream
skill: pruebas-cli stream 123e4567-e89b-12d3-a456-426614174000

# Full traceability
skill: pruebas-cli trace 123e4567-e89b-12d3-a456-426614174000
```

### Complete Testing Workflow

```bash
# 1. Validate infrastructure
skill: pruebas-cli infra

# 2. Test Docker provider
skill: pruebas-cli run docker

# 3. Test Kubernetes provider (if available)
skill: pruebas-cli run kubernetes

# 4. Compare provider performance
skill: pruebas-cli compare
```

### Full Test Suite

```bash
# Run complete E2E test suite across all providers
skill: pruebas-cli full
```

## Verification Procedures

This section documents the detailed verification procedures performed by the skill to validate job execution and system coherence.

### Step 1: Build CLI Binary

Before running any tests, ensure the CLI is built:

```bash
cargo build -p hodei-jobs-cli --quiet
```

**Expected**: Binary at `./target/debug/hodei-jobs-cli`

### Automated E2E Testing Script

For complete automation of the E2E testing procedure, use the automated script:

```bash
# Run all E2E test phases
./scripts/testing/e2e_test_procedure.sh all

# Run specific phase
./scripts/testing/e2e_test_procedure.sh infra       # Phase 1: Infrastructure
./scripts/testing/e2e_test_procedure.sh build       # Phase 2: Compile
./scripts/testing/e2e_test_procedure.sh start       # Phase 3: Start Services
./scripts/testing/e2e_test_procedure.sh providers   # Phase 4: Configure Providers
./scripts/testing/e2e_test_procedure.sh jobs        # Phase 5: Execute Test Jobs
./scripts/testing/e2e_test_procedure.sh validate    # Phase 6: Validation
./scripts/testing/e2e_test_procedure.sh k8s         # Phase 7: Kubernetes Testing
./scripts/testing/e2e_test_procedure.sh batch       # Phase 8: Batch Testing

# Generate report only
./scripts/testing/e2e_test_procedure.sh report
```

**Script Location**: `scripts/testing/e2e_test_procedure.sh`

**Evidence Output**: `build/test-evidence/`

**Logs Output**: `logs/`

### Step 2: Infrastructure Validation

Verify all infrastructure components are running:

```bash
# Check Docker containers
docker ps --filter "name=hodei" --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"

# Verify required containers:
# - hodei-jobs-postgres (healthy)
# - hodei-test-control-plane (for K8s)
# - hodei-worker-* (active workers)
```

### Step 3: Execute Test Job

Run a test job on the desired provider:

```bash
# Kubernetes test
./target/debug/hodei-jobs-cli job run -n "test-$(date +%s)" \
  --provider kubernetes \
  -c "echo 'Hello from Kubernetes'; sleep 5; echo 'Done'"

# Docker test
./target/debug/hodei-jobs-cli job run -n "test-$(date +%s)" \
  --provider docker \
  -c "echo 'Hello from Docker'; sleep 5; echo 'Done'"
```

**Output captured**:
- Job ID (UUID)
- Log stream in real-time
- Summary with logs received and duration

### Step 4: Job Status Verification

Query job status via CLI:

```bash
./target/debug/hodei-jobs-cli job get --id <job-id>
```

**Expected fields**:
- Job Name
- Command
- Status (numeric: 4 = SUCCEEDED)

### Step 5: Domain Event Flow Verification

Query domain events from database:

```bash
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT event_type, aggregate_id, occurred_at::text
FROM domain_events 
WHERE aggregate_id = '<job-id>'
ORDER BY occurred_at;"
```

**Expected Event Sequence**:
```
1. JobCreated           → Job was created
2. JobAssigned          → Worker assigned
3. JobAccepted          → Job accepted by worker
4. JobStatusChanged     → RUNNING state
5. JobStatusChanged     → SUCCEEDED/FAILED state
```

**Validation Criteria**:
- ✅ Events in chronological order
- ✅ No missing events
- ✅ Timestamps are consistent
- ✅ Correct aggregate_id references

### Step 6: Job State Verification

Query job state from database:

```bash
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT id, state, selected_provider_id, created_at, started_at, completed_at, error_message
FROM jobs 
WHERE id = '<job-id>';"
```

**Expected fields**:
| Field | Expected Value | Description |
|-------|----------------|-------------|
| `state` | `SUCCEEDED` or `FAILED` | Terminal state |
| `selected_provider_id` | Valid UUID | Provider used |
| `started_at` | Timestamp | When execution started |
| `completed_at` | Timestamp | When execution ended |
| `error_message` | NULL (for success) | Error details |

### Step 7: Log Stream Verification

Verify log persistence in database:

```bash
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT id, job_id, storage_uri, size_bytes, entry_count, created_at::text
FROM job_log_files 
WHERE job_id = '<job-id>';"
```

**Expected fields**:
| Field | Expected Value | Description |
|-------|----------------|-------------|
| `storage_uri` | `file:///tmp/hodei-logs/...` | Log file path |
| `size_bytes` | > 0 | Log file size |
| `entry_count` | > 0 | Number of log entries |

**Log File Content** (from storage_uri):
```
16:08:47.256 [OUT] Hello from Kubernetes
16:08:52.260 [OUT] Done
```

### Step 8: 6-Point Verification Checklist

Perform complete verification:

| # | Check | Command/Query | Pass Criteria |
|---|-------|---------------|---------------|
| 1 | **Status** | `job get --id` | `SUCCEEDED` or `FAILED` |
| 2 | **Events** | Domain events query | All events present, correct sequence |
| 3 | **Logs** | Log files query | Entry count > 0, storage_uri valid |
| 4 | **Provider** | Jobs query | `selected_provider_id` not null |
| 5 | **Cleanup** | Workers query | Worker in valid state |
| 6 | **Timing** | Jobs query | `started_at` and `completed_at` set |

### Step 9: Generate Verification Report

Compile all findings into a report:

```bash
# Create report structure
cat > build/test-evidence/<job-id>.md << EOF
# Job Verification Report

## Basic Info
- Job ID: <job-id>
- Status: SUCCEEDED
- Provider: kubernetes
- Duration: ~5.1 seconds

## Event Flow
\`\`\`
1. JobCreated        → timestamp
2. JobAssigned       → timestamp
3. JobAccepted       → timestamp
4. JobStatusChanged  → RUNNING
5. JobStatusChanged  → SUCCEEDED
\`\`\`

## Log Stream
\`\`\`
<log entries>
\`\`\`

## Findings
✅ No business logic errors detected
- Event sequence is valid
- State transitions are sequential
- Job reached terminal state
- Log file persisted correctly
EOF
```

## Provider Verification with kubectl

**CRITICAL**: Always verify that the job actually runs on the requested provider. Use kubectl to confirm Kubernetes execution and Docker inspection for Docker jobs.

### Step 1: Verify Provider Assignment Matches Request

```bash
# Get job details and selected provider
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT 
    j.id, 
    j.selected_provider_id,
    pc.provider_type,
    pc.status as provider_status
FROM jobs j
JOIN provider_configs pc ON j.selected_provider_id = pc.id
WHERE j.id = '<job-id>';"
```

**Expected Results**:
| Requested | Selected Provider Type | Pass/Fail |
|-----------|----------------------|-----------|
| kubernetes | kubernetes | ✅ PASS |
| docker | docker | ✅ PASS |
| kubernetes | docker | ❌ FAIL (BUG) |
| docker | kubernetes | ❌ FAIL (BUG) |

### Step 2: Kubernetes Pod Verification

For Kubernetes jobs, verify the pod exists and is running:

```bash
# Check all namespaces for Hodei worker pods
kubectl get pods -A -l hodei-worker --field-selector=status.phase=Running

# Check specific namespace (hodei-jobs-workers)
kubectl get pods -n hodei-jobs-workers -o wide

# Get pod details for the job
kubectl get pods -n hodei-jobs-workers -l job-id=<job-id> -o yaml

# Check pod logs
kubectl logs -n hodei-jobs-workers <pod-name> --tail=50

# Describe pod for events
kubectl describe pod -n hodei-jobs-workers <pod-name>
```

**Expected Fields**:
| Field | Expected Value | Description |
|-------|----------------|-------------|
| `status.phase` | Running/Completed | Pod is executing |
| `speccontainers[0].image` | hodei-worker-* | Worker image |
| `metadata.labels.job-id` | <job-id> | Job correlation |

### Step 3: Docker Container Verification

For Docker jobs, verify the container exists:

```bash
# List Hodei worker containers
docker ps --filter "name=hodei-worker" --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"

# Check container details
docker inspect <container-name> --format '{{.State.Status}}'

# Get container logs
docker logs <container-name> --tail 50

# Verify container is running on Docker (not in K8s)
docker info | grep -i "server version"
```

**Expected Fields**:
| Field | Expected Value | Description |
|-------|----------------|-------------|
| `.State.Running` | true | Container is running |
| `.Config.Image` | hodei-worker:* | Worker image |
| `.HostConfig.NetworkMode` | bridge/host | Docker networking |

### Step 4: Cross-Provider Validation

Ensure jobs don't run on wrong provider:

```bash
# FOR KUBERNETES JOBS: Verify NO Docker container was created
echo "=== Checking for Docker containers (should be empty for K8s job) ==="
docker ps --filter "name=hodei-worker" --format "{{.Names}}" | grep -c . || echo "0 containers found"

# FOR DOCKER JOBS: Verify NO Kubernetes pod was created  
echo "=== Checking for K8s pods (should be empty for Docker job) ==="
kubectl get pods -A -l hodei-worker --no-headers | wc -l
```

**Validation Rules**:
| Job Type | Docker Containers | K8s Pods | Result |
|----------|------------------|----------|--------|
| kubernetes | 0 | > 0 | ✅ PASS |
| kubernetes | > 0 | > 0 | ❌ FAIL (dual infra) |
| docker | > 0 | 0 | ✅ PASS |
| docker | > 0 | > 0 | ❌ FAIL (dual infra) |

### Step 5: Provider-Specific Log Stream Verification

Verify logs are streamed from the correct infrastructure:

```bash
# For Kubernetes jobs: Get logs from pod AND database
kubectl logs -n hodei-jobs-workers <pod-name> --tail=100

# Compare with database log persistence
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT storage_uri, size_bytes, entry_count 
FROM job_log_files 
WHERE job_id = '<job-id>';"

# For Docker jobs: Get logs from container AND database
docker logs <container-name> --tail 100

# Compare logs match
diff <(kubectl logs -n hodei-jobs-workers <pod-name> 2>/dev/null) \
     <(docker logs <container-name> 2>/dev/null) || echo "Logs differ (expected)"
```

### Step 6: Provider Configuration Verification

Verify both providers are configured correctly:

```bash
# Check all configured providers
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT id, provider_type, status FROM provider_configs ORDER BY provider_type;"

# Verify Kubernetes provider is ACTIVE
kubectl get componentstatuses 2>&1 | head -10

# Verify Docker provider endpoint
docker info 2>&1 | grep -i "server version"
```

**Expected Provider Config**:
| Provider Type | Status | Verification |
|--------------|--------|--------------|
| docker | ACTIVE | `docker ps` works |
| kubernetes | ACTIVE | `kubectl get pods` works |

### Step 7: Complete Provider Verification Report

```bash
#!/bin/bash
# Complete provider verification script

JOB_ID=$1
REQUESTED_PROVIDER=$2  # docker or kubernetes

echo "=========================================="
echo "PROVIDER VERIFICATION REPORT"
echo "=========================================="
echo "Job ID: $JOB_ID"
echo "Requested Provider: $REQUESTED_PROVIDER"
echo ""

# Get selected provider from database
SELECTED_PROVIDER=$(docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT pc.provider_type FROM jobs j 
JOIN provider_configs pc ON j.selected_provider_id = pc.id 
WHERE j.id = '$JOB_ID';" | xargs)

echo "Selected Provider: $SELECTED_PROVIDER"

# Check if provider matches
if [ "$REQUESTED_PROVIDER" = "$SELECTED_PROVIDER" ]; then
    echo "✅ Provider Assignment: CORRECT"
else
    echo "❌ Provider Assignment: INCORRECT (BUG!)"
    echo "   Expected: $REQUESTED_PROVIDER, Got: $SELECTED_PROVIDER"
fi

# Provider-specific checks
if [ "$SELECTED_PROVIDER" = "kubernetes" ]; then
    echo ""
    echo "=== Kubernetes Checks ==="
    K8S_PODS=$(kubectl get pods -A -l hodei-worker --no-headers 2>/dev/null | wc -l)
    echo "K8s Pods found: $K8S_PODS"
    
    DOCKER_CONTAINERS=$(docker ps --filter "name=hodei-worker" --format "{{.Names}}" 2>/dev/null | wc -l)
    echo "Docker containers found: $DOCKER_CONTAINERS"
    
    if [ "$K8S_PODS" -gt 0 ] && [ "$DOCKER_CONTAINERS" -eq 0 ]; then
        echo "✅ Infrastructure Isolation: CORRECT (K8s only)"
    else
        echo "❌ Infrastructure Isolation: INCORRECT"
    fi
elif [ "$SELECTED_PROVIDER" = "docker" ]; then
    echo ""
    echo "=== Docker Checks ==="
    DOCKER_CONTAINERS=$(docker ps --filter "name=hodei-worker" --format "{{.Names}}" 2>/dev/null | wc -l)
    echo "Docker containers found: $DOCKER_CONTAINERS"
    
    K8S_PODS=$(kubectl get pods -A -l hodei-worker --no-headers 2>/dev/null | wc -l)
    echo "K8s pods found: $K8S_PODS"
    
    if [ "$DOCKER_CONTAINERS" -gt 0 ] && [ "$K8S_PODS" -eq 0 ]; then
        echo "✅ Infrastructure Isolation: CORRECT (Docker only)"
    else
        echo "❌ Infrastructure Isolation: INCORRECT"
    fi
fi

echo ""
echo "=========================================="
```

## Database Schema Reference

### Core Tables

| Table | Purpose | Key Columns |
|-------|---------|-------------|
| `jobs` | Job definitions | `id`, `state`, `selected_provider_id`, `created_at`, `started_at`, `completed_at` |
| `domain_events` | Event store | `event_type`, `aggregate_id`, `occurred_at`, `payload` |
| `job_log_files` | Log persistence | `job_id`, `storage_uri`, `size_bytes`, `entry_count` |
| `job_queue` | Job queue | `job_id`, `enqueued_at` |
| `workers` | Worker registry | `id`, `state`, `provider_id` |
| `provider_configs` | Provider configs | `id`, `provider_type` |

### Status Values

| Value | Numeric | Description |
|-------|---------|-------------|
| `CREATED` | 0 | Job created |
| `PENDING` | 1 | Waiting for worker |
| `ASSIGNED` | 2 | Worker assigned |
| `RUNNING` | 3 | Execution in progress |
| `SUCCEEDED` | 4 | Completed successfully |
| `FAILED` | 5 | Execution failed |

## Example Test Run

### Test: Kubernetes Job Execution

```bash
# 1. Build CLI
cargo build -p hodei-jobs-cli --quiet

# 2. Run job
./target/debug/hodei-jobs-cli job run -n "k8s-test-$(date +%s)" \
  --provider kubernetes \
  -c "echo 'Hello from Kubernetes'; sleep 5; echo 'Done'"

# 3. Get Job ID from output (e.g., 728af646-2834-46e1-9dcc-012636a7258a)

# 4. Verify events
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c \
  "SELECT event_type, aggregate_id, occurred_at::text FROM domain_events \
   WHERE aggregate_id = '728af646-2834-46e1-9dcc-012636a7258a' ORDER BY occurred_at;"

# 5. Verify job state
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c \
  "SELECT id, state, selected_provider_id, created_at, started_at, completed_at \
   FROM jobs WHERE id = '728af646-2834-46e1-9dcc-012636a7258a';"

# 6. Verify log persistence
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c \
  "SELECT id, storage_uri, size_bytes, entry_count FROM job_log_files \
   WHERE job_id = '728af646-2834-46e1-9dcc-012636a7258a';"

# 7. Get worker info
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c \
  "SELECT id, provider_type, provider_resource_id, state, handle FROM workers \
   WHERE created_at > '2025-12-26 16:08:00' AND created_at < '2025-12-26 16:09:00' \
   ORDER BY created_at LIMIT 1;"
```

### Expected Results

| Check | Result |
|-------|--------|
| Job Status | ✅ SUCCEEDED |
| Event Count | 5 events |
| Event Sequence | ✅ Valid |
| Log Entry Count | 2 entries |
| Provider | Kubernetes |
| Duration | ~5.1 seconds |

## Complete Execution Report Template

Generate a comprehensive report with all relevant execution data:

```bash
# Generate complete execution report
cat > build/test-evidence/<job-id>-report.md << 'EOF'
# Hodei Job Execution Report

## Job Information

| Field | Value |
|-------|-------|
| **Job ID** | `728af646-2834-46e1-9dcc-012636a7258a` |
| **Job Name** | `k8s-test-1766765327` |
| **Status** | `SUCCEEDED` (Status Code: 4) |
| **Provider** | `kubernetes` |
| **Provider ID** | `055c5de8-1609-46f9-8a64-114c3030eee4` |
| **Provider Type** | Docker |
| **Command** | `echo 'Hello from Kubernetes'; sleep 5; echo 'Done'` |
| **Created At** | `2025-12-26 16:08:47.125979+00` |
| **Started At** | `2025-12-26 16:08:47.264406+00` |
| **Completed At** | `2025-12-26 16:08:52.271088+00` |
| **Duration** | `5.146s` |

## Worker Information

| Field | Value |
|-------|-------|
| **Worker ID** | `b4055298-f750-4674-90c4-8facad362e46` |
| **Container Name** | `hodei-worker-b4055298-f750-4674-90c4-8facad362e46` |
| **Provider Resource ID** | `fb2262feeab7ff19ded3a677a352e302871859a4d3974a63a10ebf11e8df2514` |
| **Worker State** | `TERMINATED` |
| **Worker Created At** | `2025-12-26 16:08:38.498622+00` |
| **Worker Updated At** | `2025-12-26 16:13:08.601444+00` |

## Execution Context

```json
{
  "job_id": "728af646-2834-46e1-9dcc-012636a7258a",
  "provider_id": "055c5de8-1609-46f9-8a64-114c3030eee4",
  "provider_execution_id": "exec-94192ccd-6029-4861-92ef-3acffa5aac44",
  "submitted_at": "2025-12-26T16:08:47.226456291Z",
  "status": "Submitted",
  "result": null
}
```

## Job Specification

```json
{
  "command": {
    "Script": {
      "content": "echo 'Hello from Kubernetes'; sleep 5; echo 'Done'",
      "interpreter": "bash"
    }
  },
  "resources": {
    "cpu_cores": 1.0,
    "memory_mb": 1024,
    "storage_mb": 1024,
    "architecture": "x86_64",
    "gpu_required": false
  },
  "timeout_ms": 600000,
  "constraints": [],
  "preferences": {
    "priority": "Normal",
    "allow_retry": true,
    "required_labels": {},
    "preferred_region": null,
    "preferred_provider": null
  }
}
```

## Event Flow

| # | Event | Timestamp | Status |
|---|-------|-----------|--------|
| 1 | `JobCreated` | `2025-12-26 16:08:47.158679+00` | ✅ |
| 2 | `JobAssigned` | `2025-12-26 16:08:47.238696+00` | ✅ |
| 3 | `JobAccepted` | `2025-12-26 16:08:47.276519+00` | ✅ |
| 4 | `JobStatusChanged` | `2025-12-26 16:08:47.290305+00` | RUNNING |
| 5 | `JobStatusChanged` | `2025-12-26 16:08:52.284701+00` | SUCCEEDED |

## Log Stream

| Field | Value |
|-------|-------|
| **Log File ID** | `55c57acf-e15f-4f33-8c2d-472737f22735` |
| **Storage URI** | `file:///tmp/hodei-logs/728af646-2834-46e1-9dcc-012636a7258a.log` |
| **Size** | `85 bytes` |
| **Entry Count** | `2` |

### Log Entries

```
16:08:47.256 [OUT] Hello from Kubernetes
16:08:52.260 [OUT] Done
```

## Verification Summary

| Check | Status | Details |
|-------|--------|---------|
| **1. Job Status** | ✅ PASS | Job reached terminal state `SUCCEEDED` |
| **2. Event Flow** | ✅ PASS | All 5 events in correct sequence |
| **3. Log Persistence** | ✅ PASS | 2 log entries persisted (85 bytes) |
| **4. Provider** | ✅ PASS | Provider `055c5de8-...` assigned correctly |
| **5. Worker Cleanup** | ✅ PASS | Worker `TERMINATED` properly |
| **6. Timing** | ✅ PASS | Duration recorded: 5.146s |

## Findings

### ✅ No Business Logic Errors Detected

- Event sequence is valid (no gaps or out-of-order events)
- State transitions are sequential: `CREATED → RUNNING → SUCCEEDED`
- Job reached terminal state without issues
- Log file persisted correctly to storage
- Worker cleanup completed successfully
- All timing metadata recorded accurately

## Status Summary

```
╔══════════════════════════════════════════════════════════════════════╗
║                    VERIFICATION COMPLETE                             ║
╠══════════════════════════════════════════════════════════════════════╣
║  Total Checks: 6                                                     ║
║  Passed: 6                                                           ║
║  Failed: 0                                                           ║
║  Status: ✅ ALL CHECKS PASSED                                        ║
╚══════════════════════════════════════════════════════════════════════╝
```
EOF
```

## Troubleshooting

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| No events found | Job ID incorrect | Verify job ID from CLI output |
| Missing log file | Log stream not persisted | Check log aggregator service |
| Job stuck in RUNNING | Worker hung | Check worker status and restart |
| Provider not assigned | No workers available | Provision workers before running |
| Events out of order | Event publishing issue | Check event bus ordering |
| **Job stuck in ASSIGNED** | **Worker can't reach gRPC server** | **Check `HODEI_SERVER_HOST` and network** |
| **Workers never connect** | **NetworkUnreachable error** | **Set accessible IP in `HODEI_SERVER_HOST`** |

### Network Issues Diagnostic

If jobs stay in ASSIGNED and workers can't connect:

```bash
# Check for ASSIGNED jobs (symptom of network issue)
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c \
  "SELECT id, state, created_at FROM jobs WHERE state = 'ASSIGNED' ORDER BY created_at DESC LIMIT 5;"

# Check worker connectivity (should show workers with no channels)
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c \
  "SELECT id, state, current_job_id FROM workers WHERE current_job_id IS NOT NULL;"

# Check worker logs for connection errors
kubectl logs -n hodei-jobs-workers -l app=hodei-worker --tail=50 2>/dev/null | grep -i "connect\|unreachable\|refused"

# Verify server IP configuration
echo "HODEI_SERVER_HOST: ${HODEI_SERVER_HOST:-not set}"
echo "Server IP: $(hostname -I | awk '{print $1}')"

# Test K8s pod connectivity to server
kubectl run -it --rm debug --image=busybox --restart=Never -- \
  wget -qO- http://$(hostname -I | awk '{print $1}'):50051 2>&1 | head -5
```

### Fix Network Issues

```bash
# Option 1: Set correct server host (most common fix)
export HODEI_SERVER_HOST=$(hostname -I | awk '{print $1}')
echo "export HODEI_SERVER_HOST=$HODEI_SERVER_HOST"

# Option 2: Add host.docker.internal to /etc/hosts
echo "$(hostname -I | awk '{print $1}') host.docker.internal" | sudo tee -a /etc/hosts

# Option 3: For K8s, ensure LoadBalancer or NodePort is configured
kubectl get svc -A | grep -i loadbalancer

# Restart server after configuration change
pkill -f hodei-server-bin
./target/debug/hodei-server-bin &
```

### Diagnostic Flow for ASSIGNED Jobs

```
1. Check job state:
   SELECT state FROM jobs WHERE id = '<job-id>';
   - ASSIGNED = Job dispatched but worker never received RUN_JOB
   - RUNNING = Worker received job but didn't send ACK

2. Check worker state:
   SELECT state FROM workers WHERE current_job_id = '<job-id>';
   - READY = Worker exists but no channel open (NETWORK ISSUE!)
   - BUSY = Worker has channel, job in progress

3. Check worker logs:
   kubectl logs -n hodei-jobs-workers <pod-name> --tail=100
   - "NetworkUnreachable" = Can't route to server IP
   - "Connection refused" = Server not running or wrong port
   - "Registered successfully" = Worker connected ✓

4. Verify server configuration:
   echo $HODEI_SERVER_HOST
   netstat -tlnp | grep 50051
```

### Verifying Worker→Server Connectivity

```bash
# Get server IP
SERVER_IP=$(hostname -I | awk '{print $1}')
echo "Server IP: $SERVER_IP"

# From Docker container
docker run --rm --network host busybox sh -c \
  "wget -qO- http://$SERVER_IP:50051 2>&1 | head -3"

# From K8s pod
kubectl run network-test --image=busybox --rm -it --restart=Never -- \
  wget -qO- http://$SERVER_IP:50051 2>&1 | head -3

# Check if port is listening
netstat -tlnp 2>/dev/null | grep 50051 || ss -tlnp | grep 50051
```

### Diagnostic Commands

```bash
# Check all jobs in database
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c \
  "SELECT id, state, created_at FROM jobs ORDER BY created_at DESC LIMIT 10;"

# Check recent events
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c \
  "SELECT event_type, aggregate_id, occurred_at FROM domain_events \
   ORDER BY occurred_at DESC LIMIT 20;"

# Check workers
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c \
  "SELECT id, state, provider_id FROM workers ORDER BY created_at DESC LIMIT 10;"
```

## Intuition Features

This skill includes intelligent detection of common issues:

- **Event Sequence Detection**: Alerts when events are out of order or missing
- **Audit Gaps**: Flags missing audit entries or inconsistent actors
- **State Regression Detection**: Identifies invalid state transitions
- **Stuck Job Detection**: Alerts when jobs remain in non-terminal state
- **Log Stream Issues**: Detects missing or incomplete log persistence
- **Worker Cleanup Failures**: Identifies workers not properly terminated
- **Performance Comparison**: Measures and compares provider latency
- **Infrastructure Pre-checks**: Validates prerequisites before running tests

## In-Situ Job Creation

Create test jobs on-the-fly with specific characteristics to validate different scenarios:

### Job Types

| Type | Command | Purpose | Expected Result |
|------|---------|---------|-----------------|
| **echo** | `echo "Hello from Hodei"` | Basic functionality test | SUCCEEDED |
| **sleep** | `sleep 60` | Long-running job test | SUCCEEDED |
| **fail** | `exit 1` | Failure handling test | FAILED |
| **resources** | Custom resource-heavy | Resource allocation test | SUCCEEDED |
| **env** | Env vars test | Environment variable handling | SUCCEEDED |
| **args** | Custom args | Argument parsing test | SUCCEEDED |

### Creating Jobs for Different Providers

```bash
# Create job on Docker with specific characteristics
skill: pruebas-cli create echo --provider docker
skill: pruebas-cli create fail --provider docker

# Create job on Kubernetes with specific characteristics
skill: pruebas-cli create echo --provider kubernetes
skill: pruebas-cli create sleep 120 --provider kubernetes

# Create job with custom command and arguments
skill: pruebas-cli create --command "python3" --args "script.py" --provider docker

# Create job with environment variables
skill: pruebas-cli create --env "VAR1=value1,VAR2=value2" --provider kubernetes

# Create job with resource requirements
skill: pruebas-cli create --cpu 2 --memory 512Mi --provider docker
```

### Job Characteristics Matrix

| Characteristic | Flag | Example |
|---------------|------|---------|
| Command | `--command` | `--command "python3"` |
| Arguments | `--args` | `--args "-c 'print(1)'"` |
| Environment | `--env` | `--env "KEY=VALUE"` |
| CPU | `--cpu` | `--cpu 2` |
| Memory | `--memory` | `--memory 512Mi` |
| Timeout | `--timeout` | `--timeout 300` |
| Provider | `--provider` | `--provider kubernetes` |

## Efficient Reporting

Get structured reports for quick analysis:

### Report Types

| Command | Output | Use Case |
|---------|--------|----------|
| `report <job-id>` | Full 6-point verification + findings summary | Complete analysis |
| `errors <job-id>` | Business logic errors only | Quick error check |
| `logs <job-id>` | Job stream logs only | Log inspection |
| `findings` | All findings from recent tests | Batch overview |
| `failures` | Failed jobs with error details | Failure analysis |

### Report Structure

Each report includes:

```
# Job Report: <job-id>

## Basic Info
- Job ID: <uuid>
- Status: SUCCEEDED|FAILED
- Provider: docker|kubernetes
- Duration: <seconds>

## Findings Summary
[ERROR] Business logic error detected
[WARN]  Warning condition
[INFO]  Informational note

## Event Flow
1. JobCreated - timestamp
2. JobAssigned - timestamp
...

## Log Stream
<log entries>

## Business Logic Errors
- Error description and location
- Stack trace if available
```

### Quick Error Report

```bash
# Get only business logic errors
skill: pruebas-cli errors 123e4567-e89b-12d3-a456-426614174000

# Output format:
# [ERROR] Job failed with exit code 1
# [ERROR] Worker not properly cleaned up
# [ERROR] Event sequence gap: missing JobStatusChanged(RUNNING)
```

### Log Stream Report

```bash
# Extract and display job stream logs
skill: pruebas-cli logs 123e4567-e89b-12d3-a456-426614174000

# Output includes:
# - All log entries in order
# - Timestamps for each entry
# - Log level (INFO, WARN, ERROR)
# - Correlation to job events
```

## Business Logic Error Detection

Automatically detect and categorize business logic errors:

### Error Categories

| Category | Description | Detection Method |
|----------|-------------|------------------|
| **State Errors** | Invalid state transitions | Lifecycle validation |
| **Event Errors** | Missing/out-of-order events | Event sequence analysis |
| **Audit Errors** | Missing audit entries | Audit trail check |
| **Resource Errors** | Resource allocation failures | Provider response analysis |
| **Cleanup Errors** | Worker not terminated | Cleanup verification |
| **Timing Errors** | Timeout or hang detected | Duration analysis |
| **Data Errors** | Inconsistent or missing data | Database query validation |

### Running Error Detection

```bash
# Check for business logic errors in a job
skill: pruebas-cli errors <job-id>

# Get error summary for all recent jobs
skill: pruebas-cli findings

# Run full analysis on failed jobs
skill: sacerdotes-cli failures
```

### Error Detection Output

```
## Business Logic Errors

[CRITICAL] State Regression: Job transitioned from RUNNING to PENDING
  Location: crates/server/application/src/jobs/mod.rs:142
  Impact: Job state machine violated

[ERROR] Event Gap: Missing JobStatusChanged(RUNNING)
  Location: crates/server/domain/src/events/job.rs
  Impact: Event sequence incomplete

[WARN] Audit Entry Missing for JobCreated
  Location: crates/server/infrastructure/src/audit.rs
  Impact: Audit trail incomplete

[ERROR] Worker Cleanup Failed
  Location: crates/server/infrastructure/src/providers/docker.rs
  Impact: Worker resource leak
```

## Usage Examples

### Quick Infrastructure Check

```bash
# Full infrastructure validation
skill: pruebas-cli infra

# Quick health summary
skill: pruebas-cli health
```

### Test Docker Provider

```bash
# Run a test job on Docker
skill: pruebas-cli run docker

# Create custom job on Docker
skill: pruebas-cli create echo --provider docker

# Create failing job to test error handling
skill: pruebas-cli create fail --provider docker
```

### Test Kubernetes Provider

```bash
# Run a test job on Kubernetes
skill: pruebas-cli run kubernetes

# Create long-running job on Kubernetes
skill: pruebas-cli create sleep 60 --provider kubernetes

# Create job with custom resources on K8s
skill: pruebas-cli create --cpu 1 --memory 1Gi --provider kubernetes
```

### Create Custom Jobs

```bash
# Simple echo job
skill: pruebas-cli create echo

# Job with custom arguments
skill: pruebas-cli create --command "echo" --args "Hello World"

# Job with environment variables
skill: pruebas-cli create --env "ENV=production,DEBUG=true"

# Job with timeout
skill: pruebas-cli create sleep 30 --timeout 60
```

### Report and Analyze Results

```bash
# Get comprehensive report for a job
skill: pruebas-cli report 123e4567-e89b-12d3-a456-426614174000

# Get only business logic errors
skill: pruebas-cli errors 123e4567-e89b-12d3-a456-426614174000

# Extract job stream logs
skill: pruebas-cli logs 123e4567-e89b-12d3-a456-426614174000

# Check event flow for a job
skill: pruebas-cli events 123e4567-e89b-12d3-a456-426614174000

# Check audit trail
skill: pruebas-cli audit 123e4567-e89b-12d3-a456-426614174000

# Validate lifecycle coherence
skill: pruebas-cli lifecycle 123e4567-e89b-12d3-a456-426614174000
```

### Debug a Failed Job

```bash
# Comprehensive verification
skill: pruebas-cli verify 123e4567-e89b-12d3-a456-426614174000

# Get error-focused report
skill: pruebas-cli report 123e4567-e89b-12d3-a456-426614174000

# Check for business logic errors
skill: precios-cli errors 123e4567-e89b-12d3-a456-426614174000

# Extract log stream for debugging
skill: pruebas-cli logs 123e4567-e89b-12d3-a456-426614174000

# Check event flow
skill: pruebas-cli events 123e4567-e89b-12d3-a456-426614174000

# Check audit trail
skill: pruebas-cli audit 123e4567-e89b-12d3-a456-426614174000

# Check lifecycle coherence
skill: pruebas-cli lifecycle 123e4567-e89b-12d3-a456-426614174000

# Full traceability
skill: pruebas-cli trace 123e4567-e89b-12d3-a456-426614174000
```

### Batch Testing

```bash
# Create and verify 5 test jobs on different providers
skill: pruebas-cli batch 5

# Get all findings from recent tests
skill: pruebas-cli findings

# Analyze all failures
skill: pruebas-cli failures

# Compare provider performance
skill: pruebas-cli compare
```

### Complete Testing Workflow

```bash
# 1. Validate infrastructure
skill: pruebas-cli infra

# 2. Create custom jobs on different providers
skill: pruebas-cli create echo --provider docker
skill: pruebas-cli create fail --provider docker
skill: pruebas-cli create echo --provider kubernetes
skill: pruebas-cli create sleep 30 --provider kubernetes

# 3. Get reports for all jobs
skill: pruebas-cli findings

# 4. Compare provider performance
skill: pruebas-cli compare
```

### Full Test Suite

```bash
# Run complete E2E test suite across all providers
skill: pruebas-cli full
```

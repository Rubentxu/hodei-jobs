# Hodei Jobs E2E Testing Procedure

**Document Version:** 1.3
**Date:** 2026-01-01
**Status:** Ready for Execution

> **⚡ Automation Available**: This procedure can be fully automated using the `just` task runner. All commands have been streamlined for ease of use.
>
> **Quick Start:**
> - `just dev` - Start full development environment
> - `just e2e` - Run complete end-to-end test
> - `just job-hello-world` - Run a simple test job
> - `just test-multi-provider` - Run multi-provider integration tests
>
> **Individual Phases with just:**
> - `just dev-db && just db-migrate` - Infrastructure setup
> - `just build` - Compile all binaries
> - `just job-examples-all` - Execute test jobs
> - Manual validation required (see Phase 6)
> - `just test-multi-provider-k8s` - Kubernetes testing

This document describes the complete procedure for running end-to-end tests of the Hodei Jobs platform, including job execution on Docker and Kubernetes providers, log stream validation, and worker lifecycle verification.

---

## Prerequisites

### System Requirements

| Component | Version | Required |
|-----------|---------|----------|
| Rust | 1.70+ | Yes |
| Docker | 20.10+ | Yes |
| Docker Compose | 2.0+ | Yes |
| kubectl | 1.25+ | Optional (for K8s) |
| just (task runner) | Latest | **Recommended** |
| PostgreSQL Client | 15+ | Optional (for manual DB access) |

### Infrastructure Requirements

```bash
# Docker containers must be running
docker ps --filter "name=hodei" --format "table {{.Names}}\t{{.Status}}"

# Expected output:
# NAMES                 STATUS
# hodei-jobs-postgres   Up (healthy)
```

### Environment Variables

**With `just` commands, environment variables are pre-configured!** However, you can override them if needed:

```bash
# Using just (recommended - variables pre-configured in justfile)
just dev-db
just db-migrate
just build

# Manual override (if needed)
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei_jobs"
export HODEI_SERVER_HOST="host.docker.internal"
export HODEI_DOCKER_ENABLED=1
export HODEI_KUBERNETES_ENABLED=1

# For Kubernetes testing
export HODEI_K8S_KUBECONFIG="$HOME/.kube/config"
export HODEI_K8S_NAMESPACE="hodei-jobs-workers"
```

---

## Phase 1: Infrastructure Setup

### Step 1.1: Start PostgreSQL

**Using `just` (recommended):**
```bash
just dev-db
```

**Manual approach (alternative):**
```bash
# Start PostgreSQL container
docker run -d \
  --name hodei-jobs-postgres \
  -e POSTGRES_PASSWORD=postgres \
  -e POSTGRES_DB=hodei_jobs \
  -p 5432:5432 \
  postgres:15-alpine

# Wait for initialization
docker exec hodei-jobs-postgres pg_isready -U postgres
```

### Step 1.2: Run Database Migrations

**Using `just` (recommended):**
```bash
just db-migrate
```

**Manual approach (alternative):**
```bash
# Core tables (domain_events)
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c "
CREATE TABLE IF NOT EXISTS domain_events (
    id UUID PRIMARY KEY,
    occurred_at TIMESTAMPTZ NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    aggregate_id VARCHAR(255) NOT NULL,
    correlation_id VARCHAR(255),
    actor VARCHAR(255),
    payload JSONB NOT NULL
);
CREATE INDEX idx_domain_events_aggregate_id ON domain_events(aggregate_id);
CREATE INDEX idx_domain_events_correlation_id ON domain_events(correlation_id);
CREATE INDEX idx_domain_events_occurred_at ON domain_events(occurred_at);
"

# Outbox events (Transactional Outbox Pattern)
cat crates/server/infrastructure/migrations/20241223_add_outbox_events.sql | \
  docker exec -i hodei-jobs-postgres psql -U postgres -d hodei_jobs

# Saga tables
cat migrations/20251230_add_saga_tables.sql | \
  docker exec -i hodei-jobs-postgres psql -U postgres -d hodei_jobs

# Verify tables
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c \
  "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' ORDER BY table_name;"
```

**Expected Output:**
```
    table_name
-------------------
 domain_events
 job_log_files
 jobs
 outbox_events
 sagas
 saga_audit_events
 saga_steps
 workers
 provider_configs
(9 rows)
```

### Step 1.3: Build Worker Image

**Using `just` (recommended):**
```bash
just build-worker
```

**Manual approach (alternative):**
```bash
# Build the worker image
docker build -t hodei-jobs-worker:latest -f crates/worker/Dockerfile .

# Verify image
docker images hodei-jobs-worker:latest
```

---

## Phase 2: Compile Binaries

### Step 2.1: Set Database URL for SQLx

**Using `just` (recommended):**
```bash
# Environment variables are pre-configured in justfile
just build
```

**Manual approach (alternative):**
```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei_jobs"
```

**Important:** SQLx requires database connection to compile query macros.

### Step 2.2: Build Server Binary

**Using `just` (recommended):**
```bash
just build-server
```

**Manual approach (alternative):**
```bash
cargo build -p hodei-server-bin

# Expected location: target/debug/hodei-server-bin
ls -la target/debug/hodei-server-bin
```

### Step 2.3: Build CLI Binary

**Using `just` (recommended):**
```bash
just build-cli
```

**Manual approach (alternative):**
```bash
cargo build -p hodei-jobs-cli

# Expected location: target/debug/hodei-jobs-cli
ls -la target/debug/hodei-jobs-cli
```

---

## Phase 3: Start Services

### Step 3.1: Start gRPC Server

**Using `just` (recommended):**
```bash
just dev-server
```

**Manual approach (alternative):**
```bash
# Start server in background
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei_jobs"
export HODEI_SERVER_HOST="$(hostname -I | awk '{print $1}')"
export HODEI_DOCKER_ENABLED=1
export HODEI_KUBERNETES_ENABLED=0  # Start with Docker only

nohup ./target/debug/hodei-server-bin > server.log 2>&1 &

# Verify server is running
sleep 3
cat server.log | tail -20

# Check port is listening
netstat -tlnp | grep 50051
```

**Quick start alternative:**
```bash
# Complete environment setup in one command
just dev
```

**Expected Server Log Output:**
```
╔═══════════════════════════════════════════════════════════════╗
║           Hodei Jobs Platform - gRPC Server                   ║
╚═══════════════════════════════════════════════════════════════╝
Starting server on 0.0.0.0:50051
Connected to database
Database migrations completed
  ✓ Docker provider initialized (id: ...)
  ✓ WorkerProvisioningService configured
  ✓ WorkerLifecycleManager started (background cleanup enabled)
```

### Step 3.2: Verify Server Health

```bash
# Check server is responding (gRPC health check)
grpcurl -plaintext localhost:50051 list

# Expected services:
# - hodei.jobs.JobExecutionService
# - hodei.jobs.WorkerAgentService
# - hodei.jobs.SchedulerService
# - hodei.jobs.ProviderManagementService
# - hodei.jobs.MetricsService
# - hodei.jobs.LogStreamService
```

---

## Phase 4: Configure Providers

### Step 4.1: Verify Providers in Database

```bash
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c "
SELECT id, provider_type, status FROM provider_configs ORDER BY provider_type;
"
```

**Expected Output:**
```
                  id                  | provider_type | status
--------------------------------------+---------------+--------
 11111111-1111-1111-1111-111111111111 | docker        | ACTIVE
 22222222-2222-2222-2222-222222222222 | kubernetes    | ACTIVE
(2 rows)
```

### Step 4.2: Check Docker Provider

```bash
# Verify Docker is available
docker info | head -5

# Verify Docker worker CLI is available
docker --version

# List any existing Hodei workers
docker ps --filter "name=hodei-worker" --format "table {{.Names}}\t{{.Status}}\t{{.Image}}"
```

### Step 4.3: Check Kubernetes Provider (Optional)

```bash
# Verify kubectl is available
kubectl version --client

# Check Kubernetes cluster connectivity
kubectl cluster-info

# List any existing Hodei worker pods
kubectl get pods -n hodei-jobs-workers -l hodei-worker --no-headers 2>/dev/null || echo "No K8s workers found or namespace not accessible"
```

---

## Phase 5: Execute Test Jobs

**Using `just` (recommended for all test jobs):**

### Step 5.1: Job Type 1 - Simple Echo (Docker)

```bash
just job-docker-hello
```

**Manual approach (alternative):**
```bash
JOB_ID=$(./target/debug/hodei-jobs-cli job run \
  --name "test-echo-$(date +%s)" \
  --provider docker \
  --command "echo 'Hello from Hodei Docker Worker!'" \
  --timeout 30)

echo "Job ID: $JOB_ID"

# Monitor job status
./target/debug/hodei-jobs-cli job get --id $JOB_ID
```

**Expected Result:**
```
Status: SUCCEEDED (Status Code: 4)
```

### Step 5.2: Job Type 2 - Multi-Command (Docker)

**Using `just`:**
```bash
just job-docker-data
```

**Manual approach:**
```bash
JOB_ID=$(./target/debug/hodei-jobs-cli job run \
  --name "test-multicmd-$(date +%s)" \
  --provider docker \
  --command "bash -c 'echo Step1 && sleep 1 && echo Step2 && sleep 1 && echo Step3'" \
  --timeout 60)

echo "Job ID: $JOB_ID"
./target/debug/hodei-jobs-cli job get --id $JOB_ID
```

### Step 5.3: Job Type 3 - Long-Running (Docker)

**Using `just`:**
```bash
just job-docker-cpu
```

**Manual approach:**
```bash
JOB_ID=$(./target/debug/hodei-jobs-cli job run \
  --name "test-long-$(date +%s)" \
  --provider docker \
  --command "for i in 1 2 3 4 5; do echo Iteration \$i; sleep 2; done" \
  --timeout 120)

echo "Job ID: $JOB_ID"
./target/debug/hodei-jobs-cli job get --id $JOB_ID
```

### Step 5.4: Job Type 4 - Environment Variables (Docker)

**Using `just` (with custom env vars):**
```bash
cargo run --bin hodei-jobs-cli -- job run \
  --name "test-env-$(date +%s)" \
  --provider docker \
  --command "bash -c 'echo MY_VAR=\$MY_VAR && echo NUM_VAR=\$NUM_VAR'" \
  --env "MY_VAR=hello_world,NUM_VAR=42" \
  --timeout 30
```

**Manual approach:**
```bash
JOB_ID=$(./target/debug/hodei-jobs-cli job run \
  --name "test-env-$(date +%s)" \
  --provider docker \
  --command "bash -c 'echo MY_VAR=\$MY_VAR && echo NUM_VAR=\$NUM_VAR'" \
  --env "MY_VAR=hello_world,NUM_VAR=42" \
  --timeout 30)

echo "Job ID: $JOB_ID"
./target/debug/hodei-jobs-cli job get --id $JOB_ID
```

### Step 5.5: Job Type 5 - With Arguments (Docker)

**Using `just` (with arguments):**
```bash
cargo run --bin hodei-jobs-cli -- job run \
  --name "test-args-$(date +%s)" \
  --provider docker \
  --command "bash" \
  --args "-c 'echo Processing file test.txt && echo Done'" \
  --timeout 30
```

**Manual approach:**
```bash
JOB_ID=$(./target/debug/hodei-jobs-cli job run \
  --name "test-args-$(date +%s)" \
  --provider docker \
  --command "bash" \
  --args "-c 'echo Processing file test.txt && echo Done'" \
  --timeout 30)

echo "Job ID: $JOB_ID"
./target/debug/hodei-jobs-cli job get --id $JOB_ID
```

### Run All Docker Jobs at Once

```bash
just job-docker-all
```

### Step 5.6: Monitor Job Execution and Worker Startup

```bash
JOB_ID="<JOB_ID>"

echo "=== Monitoring job execution for $JOB_ID ==="

# Set timeout for log stream (10 seconds max)
TIMEOUT=10
START_TIME=$(date +%s)

# Check job status periodically
while true; do
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))
    
    if [ $ELAPSED -ge $TIMEOUT ]; then
        echo "❌ TIMEOUT: No log stream received after ${TIMEOUT}s - job may be stuck"
        break
    fi
    
    # Query job state from database
    JOB_STATE=$(docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
        SELECT state FROM jobs WHERE id = '$JOB_ID';" 2>/dev/null | xargs)
    
    echo "[${ELAPSED}s] Job state: $JOB_STATE"
    
    if [ "$JOB_STATE" = "SUCCEEDED" ] || [ "$JOB_STATE" = "FAILED" ]; then
        echo "✅ Job reached terminal state: $JOB_STATE"
        break
    fi
    
    # Check for Docker worker (if Docker provider)
    DOCKER_WORKERS=$(docker ps --filter "name=hodei-worker" --format "{{.Names}}" 2>/dev/null | wc -l)
    if [ "$DOCKER_WORKERS" -gt 0 ]; then
        echo "🐳 Docker workers running: $DOCKER_WORKERS"
        docker ps --filter "name=hodei-worker" --format "table {{.Names}}\t{{.Status}}" | head -5
    fi
    
    # Check for Kubernetes pod (if Kubernetes provider)
    K8S_PODS=$(kubectl get pods -n hodei-jobs-workers -l hodei-worker --no-headers 2>/dev/null | wc -l)
    if [ "$K8S_PODS" -gt 0 ]; then
        echo "☸️  Kubernetes pods running: $K8S_PODS"
        kubectl get pods -n hodei-jobs-workers -l hodei-worker -o wide --no-headers 2>/dev/null | head -5
    fi
    
    sleep 2
done

# Final status check
echo ""
echo "=== Final Job Status ==="
./target/debug/hodei-jobs-cli job get --id $JOB_ID
```

---

## Phase 6: Validation

### Step 6.1: Validate Job Status

```bash
JOB_ID="<JOB_ID>"

# Query job status from database
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT id, state, selected_provider_id, created_at, started_at, completed_at, error_message
FROM jobs WHERE id = '$JOB_ID';
"

# Expected:
# - state: SUCCEEDED or FAILED
# - selected_provider_id: Valid UUID (Docker or Kubernetes)
# - started_at and completed_at: Timestamps set
```

### Step 6.2: Validate Domain Events

```bash
JOB_ID="<JOB_ID>"

docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT event_type, aggregate_id, occurred_at::text
FROM domain_events
WHERE aggregate_id = '$JOB_ID'
ORDER BY occurred_at;
"

# Expected Event Sequence:
# 1. JobCreated
# 2. JobAssigned
# 3. JobAccepted
# 4. JobStatusChanged (RUNNING)
# 5. JobStatusChanged (SUCCEEDED/FAILED)
```

### Step 6.3: Validate Log Stream

```bash
JOB_ID="<JOB_ID>"

docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT id, storage_uri, size_bytes, entry_count, created_at::text
FROM job_log_files
WHERE job_id = '$JOB_ID';
"

# Expected:
# - entry_count: > 0
# - size_bytes: > 0
# - storage_uri: file:///tmp/hodei-logs/...
```

### Step 6.4: Validate Worker Lifecycle

```bash
# Check worker was created and terminated
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT id, state, provider_type, provider_resource_id, created_at, updated_at
FROM workers
WHERE created_at > NOW() - INTERVAL '1 hour'
ORDER BY created_at DESC LIMIT 5;
"

# Expected:
# - Worker state: TERMINATED (after job completion)
# - provider_type: docker or kubernetes
# - provider_resource_id: Container ID or Pod name
```

### Step 6.5: Validate Docker Isolation (for Docker jobs)

```bash
# Check no K8s pods were created for Docker job
kubectl get pods -A -l hodei-worker --no-headers 2>/dev/null | wc -l

# Expected: 0 (for Docker-only jobs)
```

### Step 6.6: Validate Provider Isolation (for K8s jobs)

```bash
# Check no Docker containers were created for K8s job
docker ps --filter "name=hodei-worker" --format "{{.Names}}" 2>/dev/null | wc -l

# Expected: 0 (for K8s-only jobs)
```

### Step 6.7: Validate Saga Execution (Optional - Debugging)

```bash
# Check saga status for a specific job (via correlation_id or job_id)
JOB_ID="<JOB_ID>"

# Find saga by correlation_id (same as job_id)
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT id, saga_type, state, correlation_id, error_message, started_at, completed_at
FROM sagas
WHERE correlation_id = '$JOB_ID'
ORDER BY started_at DESC;
"

# Expected:
# - saga_type: PROVISIONING, EXECUTION, or RECOVERY
# - state: COMPLETED (success) or FAILED/COMPENSATING (needs attention)
# - error_message: NULL if successful

# Check saga steps for detailed execution trace
SAGA_ID="<SAGA_ID_FROM_PREVIOUS_QUERY>"

docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT step_name, state, retry_count, error_message, started_at, completed_at
FROM saga_steps
WHERE saga_id = '$SAGA_ID'
ORDER BY step_order;
"

# Expected:
# - All steps: COMPLETED (success path)
# - Any FAILED step indicates where the saga stopped
# - Any COMPENSATING state indicates rollback in progress

# Check saga audit trail for full execution history
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT event_type, step_name, message, occurred_at::text
FROM saga_audit_events
WHERE saga_id = '$SAGA_ID'
ORDER BY occurred_at;
"

# Expected events:
# - SagaStarted
# - StepStarted (<step_name>)
# - StepCompleted (<step_name>)
# - ... (all steps)
# - SagaCompleted
```

---

## Phase 7: Kubernetes Testing (Optional)

### Step 7.1: Configure Kubernetes

**Using `just` (recommended):**
```bash
just dev-k8s
```

**Manual approach:**
```bash
# Set Kubernetes configuration
export HODEI_K8S_KUBECONFIG="$HOME/.kube/config"
export HODEI_K8S_NAMESPACE="hodei-jobs-workers"

# Verify kubectl access
kubectl cluster-info
kubectl get nodes
```

### Step 7.2: Enable Kubernetes Provider

**Using `just` (recommended):**
```bash
just test-k8s-provider
```

**Manual approach:**
```bash
# Restart server with K8s enabled
pkill -f hodei-server-bin
export HODEI_KUBERNETES_ENABLED=1

nohup ./target/debug/hodei-server-bin > server.log 2>&1 &
sleep 5
```

### Step 7.3: Run Job on Kubernetes

**Using `just`:**
```bash
just job-k8s-hello
```

**Manual approach:**
```bash
JOB_ID=$(./target/debug/hodei-jobs-cli job run \
  --name "test-k8s-$(date +%s)" \
  --provider kubernetes \
  --command "echo 'Hello from Kubernetes Worker!'" \
  --timeout 60)

echo "Job ID: $JOB_ID"
./target/debug/hodei-jobs-cli job get --id $JOB_ID
```

### Step 7.4: Verify Kubernetes Pod

**Using `just`:**
```bash
just job-k8s-pods
```

**Manual approach:**
```bash
# Check pod was created
kubectl get pods -n hodei-jobs-workers -l hodei-worker --no-headers

# Get pod logs
POD_NAME=$(kubectl get pods -n hodei-jobs-workers -l hodei-worker -o jsonpath='{.items[0].metadata.name}')
kubectl logs -n hodei-jobs-workers $POD_NAME --tail=20
```

### Run All Kubernetes Jobs at Once

```bash
just job-k8s-all
```

---

## Phase 8: Batch Testing

### Step 8.1: Run Multiple Jobs

**Using `just` (recommended):**
```bash
just job-provider-comparison
```

**Manual approach:**
```bash
# Create 5 test jobs on Docker
for i in 1 2 3 4 5; do
  JOB_ID=$(./target/debug/hodei-jobs-cli job run \
    --name "batch-test-$i-$(date +%s)" \
    --provider docker \
    --command "echo 'Batch test \$i'" \
    --timeout 30)
  echo "Job $i: $JOB_ID"
  sleep 2
done
```

### Step 8.2: Verify All Jobs Completed

**Using `just` (for concurrent test):**
```bash
just job-concurrent-test
```

**Manual approach:**
```bash
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT state, COUNT(*) as count
FROM jobs
WHERE created_at > NOW() - INTERVAL '1 hour'
GROUP BY state;
"

# Expected:
#  state   | count
# ---------+-------
# SUCCEEDED | 5
```

---

## Troubleshooting

### Issue: Jobs stuck in ASSIGNED state

**Using `just` (recommended):**
```bash
just stop-server
just dev-server
```

**Manual approach:**
```bash
# Check for network issues
echo "HODEI_SERVER_HOST: ${HODEI_SERVER_HOST:-not set}"
echo "Server IP: $(hostname -I | awk '{print $1}')"

# Restart server with correct host
export HODEI_SERVER_HOST=$(hostname -I | awk '{print $1}')
pkill -f hodei-server-bin
nohup ./target/debug/hodei-server-bin > server.log 2>&1 &
```

### Issue: Docker provider not available

**Using `just` (recommended):**
```bash
just verify-providers
just check-provider-health
```

**Manual approach:**
```bash
# Verify Docker is running
docker info 2>&1 | head -3

# Check provider in database
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT id, provider_type, status FROM provider_configs WHERE provider_type = 'docker';
"
```

### Issue: Compilation fails with sqlx errors

**Using `just` (recommended):**
```bash
just clean
just build
```

**Manual approach:**
```bash
# Ensure DATABASE_URL is set
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei_jobs"

# Verify database is accessible
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c "SELECT 1;"

# Clean and rebuild
cargo clean
cargo build -p hodei-server-bin
```

---

## Expected Results Summary

| Test | Provider | Expected Status | Validation |
|------|----------|-----------------|------------|
| Echo | Docker | SUCCEEDED | Logs present, worker terminated |
| Multi-command | Docker | SUCCEEDED | Multiple log entries |
| Long-running | Docker | SUCCEEDED | Duration > 10s |
| Env vars | Docker | SUCCEEDED | Env variables processed |
| Arguments | Docker | SUCCEEDED | Arguments passed correctly |
| Echo | Kubernetes | SUCCEEDED | Pod created, logs present |
| Multi-command | Kubernetes | SUCCEEDED | Pod logs match database |

---

## Quick Reference Commands

### Using `just` (Recommended)

```bash
# Complete E2E setup and test
just dev              # Full development environment (DB + Server)
just e2e              # Run complete end-to-end test
just job-hello-world  # Run simple test job
just job-docker-all   # Run all Docker provider jobs
just job-k8s-all      # Run all Kubernetes provider jobs
just test-multi-provider  # Run multi-provider tests

# Individual phases
just dev-db           # Start database only
just build            # Build all binaries
just dev-server       # Start server only
just stop-server      # Stop running server
just clean            # Clean build artifacts

# Job operations
just job-test         # Create simple test job
just job-list         # List all jobs
just job-details <id> # Get job details
just job-cancel <id>  # Cancel a job

# Monitoring and debugging
just status           # Show system status
just logs             # Show all logs
just debug-jobs       # Show PENDING jobs
just debug-workers    # Show registered workers
just watch-jobs       # Watch jobs table live
just verify-providers # Verify provider configs
```

### Manual Commands (Alternative)

```bash
# Start everything (manual)
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei_jobs"
export HODEI_SERVER_HOST="$(hostname -I | awk '{print $1}')"
nohup ./target/debug/hodei-server-bin > server.log 2>&1 &

# Run test job (manual)
JOB_ID=$(./target/debug/hodei-jobs-cli job run --name "test-$(date +%s)" --provider docker --command "echo hello" --timeout 30)
echo "Job: $JOB_ID"

# Check status (manual)
./target/debug/hodei-jobs-cli job get --id $JOB_ID

# Check events (manual)
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c "SELECT event_type FROM domain_events WHERE aggregate_id = '$JOB_ID' ORDER BY occurred_at;"

# Check logs (manual)
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c "SELECT * FROM job_log_files WHERE job_id = '$JOB_ID';"

# Check workers (manual)
docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c "SELECT state, provider_type FROM workers WHERE created_at > NOW() - INTERVAL '1 hour';"
```

### Common `just` Workflows

```bash
# Quick start for new developers
just dev-db && just db-migrate && just build && just dev-server

# Full test suite
just test-integration && just test-multi-provider

# Debug workflow
just debug-jobs && just debug-workers && just status

# Complete Kubernetes test
just dev-k8s && just job-k8s-all && just job-k8s-pods

# Provider comparison
just job-provider-comparison
```

---

## Document Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.3 | 2026-01-01 | **Major Update**: Integrated `just` task runner throughout document. Added `just` commands for all phases (dev-db, db-migrate, build, dev-server, job-docker-all, job-k8s-all, test-multi-provider, etc.). Updated Quick Reference with comprehensive `just` workflows. Added just commands to Troubleshooting section. |
| 1.2 | 2025-12-30 | Added Step 4.3: Check Kubernetes Provider; Added Step 5.6: Monitor Job Execution with 10s timeout and worker verification via docker/kubectl CLI |
| 1.1 | 2025-12-30 | Added Step 6.7: Validate Saga Execution for debugging |
| 1.0 | 2025-12-30 | Initial document |


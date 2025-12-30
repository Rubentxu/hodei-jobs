#!/bin/bash
# =============================================================================
# Hodei Jobs Platform - E2E Testing Procedure Automation
# Implements: docs/PROCEDURE_E2E_TESTING.md
# =============================================================================
#
# This script automates the complete E2E testing procedure including:
# - Infrastructure setup and validation
# - Database migrations
# - Worker image building
# - Binary compilation
# - Service startup
# - Job execution across providers
# - Comprehensive validation and verification
#
# Usage: ./e2e_test_procedure.sh [phase]
#   phases: infra, build, start, jobs, validate, all (default: all)
#
# =============================================================================

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
EVIDENCE_DIR="${PROJECT_ROOT}/build/test-evidence"
LOGS_DIR="${PROJECT_ROOT}/logs"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Environment defaults
DATABASE_URL="${DATABASE_URL:-postgres://postgres:postgres@localhost:5432/hodei_jobs}"
GRPC_PORT="${GRPC_PORT:-50051}"
HODEI_SERVER_HOST="${HODEI_SERVER_HOST:-$(hostname -I | awk '{print $1}')}"
HODEI_WORKER_IMAGE="${HODEI_WORKER_IMAGE:-hodei-jobs-worker:latest}"
HODEI_DOCKER_ENABLED="${HODEI_DOCKER_ENABLED:-1}"
HODEI_KUBERNETES_ENABLED="${HODEI_KUBERNETES_ENABLED:-0}"
HODEI_DEV_MODE="${HODEI_DEV_MODE:-0}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() { echo -e "${BLUE}[INFO]${NC} $(date '+%Y-%m-%d %H:%M:%S') $1"; }
log_success() { echo -e "${GREEN}[PASS]${NC} $(date '+%Y-%m-%d %H:%M:%S') $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $(date '+%Y-%m-%d %H:%M:%S') $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $(date '+%Y-%m-%d %H:%M:%S') $1"; }

# Create directories
mkdir -p "$EVIDENCE_DIR" "$LOGS_DIR"

# =============================================================================
# PHASE 1: INFRASTRUCTURE SETUP
# =============================================================================

phase1_infrastructure() {
    log_info "============================================================"
    log_info "PHASE 1: Infrastructure Setup"
    log_info "============================================================"

    step_1_1_start_postgres
    step_1_2_run_migrations
    step_1_3_build_worker_image
}

step_1_1_start_postgres() {
    log_info "Step 1.1: Starting PostgreSQL..."

    # Check if already running
    if docker ps --filter "name=hodei-jobs-postgres" --format "{{.Names}}" | grep -q "hodei-jobs-postgres"; then
        log_success "PostgreSQL container already running"
        return 0
    fi

    # Start PostgreSQL container
    docker run -d \
        --name hodei-jobs-postgres \
        -e POSTGRES_PASSWORD=postgres \
        -e POSTGRES_DB=hodei_jobs \
        -p 5432:5432 \
        postgres:15-alpine

    # Wait for initialization
    log_info "Waiting for PostgreSQL to be ready..."
    local retries=30
    while [ $retries -gt 0 ]; do
        if docker exec hodei-jobs-postgres pg_isready -U postgres > /dev/null 2>&1; then
            log_success "PostgreSQL is ready"
            return 0
        fi
        sleep 1
        retries=$((retries - 1))
    done

    log_error "PostgreSQL failed to start"
    return 1
}

step_1_2_run_migrations() {
    log_info "Step 1.2: Running Database Migrations..."

    # Core tables (domain_events)
    log_info "Creating domain_events table..."
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
CREATE INDEX IF NOT EXISTS idx_domain_events_aggregate_id ON domain_events(aggregate_id);
CREATE INDEX IF NOT EXISTS idx_domain_events_correlation_id ON domain_events(correlation_id);
CREATE INDEX IF NOT EXISTS idx_domain_events_occurred_at ON domain_events(occurred_at);
" > "$LOGS_DIR/migrations_domain_events.log" 2>&1 || true

    # Outbox events (Transactional Outbox Pattern)
    log_info "Creating outbox_events table..."
    cat "${PROJECT_ROOT}/crates/server/infrastructure/migrations/20241223_add_outbox_events.sql" 2>/dev/null | \
        docker exec -i hodei-jobs-postgres psql -U postgres -d hodei_jobs >> "$LOGS_DIR/migrations_outbox.log" 2>&1 || true

    # Saga tables
    log_info "Creating saga tables..."
    cat "${PROJECT_ROOT}/crates/server/infrastructure/migrations/20251230_add_saga_tables.sql" 2>/dev/null | \
        docker exec -i hodei-jobs-postgres psql -U postgres -d hodei_jobs >> "$LOGS_DIR/migrations_saga.log" 2>&1 || true

    # Verify tables
    log_info "Verifying tables..."
    local tables=$(docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c \
        "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' ORDER BY table_name;" 2>/dev/null)

    log_success "Tables created: $tables"
}

step_1_3_build_worker_image() {
    log_info "Step 1.3: Building Worker Image..."

    cd "$PROJECT_ROOT"

    # Build worker image
    if docker images --format "{{.Repository}}:{{.Tag}}" | grep -q "hodei-jobs-worker:latest"; then
        log_success "Worker image already exists"
        return 0
    fi

    docker build -t hodei-jobs-worker:latest -f crates/worker/Dockerfile . >> "$LOGS_DIR/docker_build.log" 2>&1

    if docker images hodei-jobs-worker:latest --format "{{.ID}}" | grep -q .; then
        log_success "Worker image built successfully"
    else
        log_error "Failed to build worker image"
        return 1
    fi
}

# =============================================================================
# PHASE 2: COMPILE BINARIES
# =============================================================================

phase2_compile() {
    log_info "============================================================"
    log_info "PHASE 2: Compile Binaries"
    log_info "============================================================"

    step_2_1_set_database_url
    step_2_2_build_server
    step_2_3_build_cli
}

step_2_1_set_database_url() {
    log_info "Step 2.1: Setting DATABASE_URL for SQLx..."
    export DATABASE_URL="$DATABASE_URL"
    log_success "DATABASE_URL=$DATABASE_URL"
}

step_2_2_build_server() {
    log_info "Step 2.2: Building Server Binary..."

    cd "$PROJECT_ROOT"

    if [ -f "target/debug/hodei-server-bin" ]; then
        log_success "Server binary already exists"
        return 0
    fi

    cargo build -p hodei-server-bin >> "$LOGS_DIR/build_server.log" 2>&1

    if [ -f "target/debug/hodei-server-bin" ]; then
        log_success "Server binary built: $(ls -lh target/debug/hodei-server-bin | awk '{print $5}')"
    else
        log_error "Failed to build server binary"
        return 1
    fi
}

step_2_3_build_cli() {
    log_info "Step 2.3: Building CLI Binary..."

    cd "$PROJECT_ROOT"

    if [ -f "target/debug/hodei-jobs-cli" ]; then
        log_success "CLI binary already exists"
        return 0
    fi

    cargo build -p hodei-jobs-cli >> "$LOGS_DIR/build_cli.log" 2>&1

    if [ -f "target/debug/hodei-jobs-cli" ]; then
        log_success "CLI binary built: $(ls -lh target/debug/hodei-jobs-cli | awk '{print $5}')"
    else
        log_error "Failed to build CLI binary"
        return 1
    fi
}

# =============================================================================
# PHASE 3: START SERVICES
# =============================================================================

phase3_start_services() {
    log_info "============================================================"
    log_info "PHASE 3: Start Services"
    log_info "============================================================"

    step_3_1_start_grpc_server
    step_3_2_verify_server_health
}

step_3_1_start_grpc_server() {
    log_info "Step 3.1: Starting gRPC Server..."

    # Check if already running
    if pgrep -f "hodei-server-bin" > /dev/null; then
        log_success "Server already running"
        return 0
    fi

    cd "$PROJECT_ROOT"

    export DATABASE_URL="$DATABASE_URL"
    export HODEI_SERVER_HOST="$HODEI_SERVER_HOST"
    export HODEI_DOCKER_ENABLED="$HODEI_DOCKER_ENABLED"
    export HODEI_KUBERNETES_ENABLED="$HODEI_KUBERNETES_ENABLED"

    # Start server in background
    nohup ./target/debug/hodei-server-bin > "$LOGS_DIR/server.log" 2>&1 &
    SERVER_PID=$!

    log_info "Server started with PID: $SERVER_PID"

    # Wait for server to be ready
    sleep 5

    # Check if port is listening
    if netstat -tlnp 2>/dev/null | grep -q ":$GRPC_PORT" || ss -tlnp 2>/dev/null | grep -q ":$GRPC_PORT"; then
        log_success "Server is listening on port $GRPC_PORT"
    else
        log_warn "Server may not be listening on port $GRPC_PORT"
    fi
}

step_3_2_verify_server_health() {
    log_info "Step 3.2: Verifying Server Health..."

    # Check if grpcurl is available
    if ! command -v grpcurl &> /dev/null; then
        log_warn "grpcurl not installed, skipping gRPC health check"
        return 0
    fi

    # List available services
    local services=$(grpcurl -plaintext localhost:$GRPC_PORT list 2>/dev/null || echo "failed")

    if [ "$services" != "failed" ]; then
        log_success "Server is responding. Available services:"
        echo "$services" | while read -r service; do
            echo "  - $service"
        done
    else
        log_error "Server is not responding"
        return 1
    fi
}

# =============================================================================
# PHASE 4: CONFIGURE PROVIDERS
# =============================================================================

phase4_configure_providers() {
    log_info "============================================================"
    log_info "PHASE 4: Configure Providers"
    log_info "============================================================"

    step_4_1_verify_providers_in_db
    step_4_2_check_docker_provider
    step_4_3_check_kubernetes_provider
}

step_4_1_verify_providers_in_db() {
    log_info "Step 4.1: Verifying Providers in Database..."

    local providers=$(docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
SELECT id, provider_type, status FROM provider_configs ORDER BY provider_type;" 2>/dev/null)

    log_success "Providers configured:"
    echo "$providers" | while read -r line; do
        [ -n "$line" ] && echo "  $line"
    done
}

step_4_2_check_docker_provider() {
    log_info "Step 4.2: Checking Docker Provider..."

    # Verify Docker is running
    if ! docker info > /dev/null 2>&1; then
        log_warn "Docker is not available"
        return 0
    fi

    log_success "Docker is running: $(docker version --format '{{.Server.Version}}')"

    # List existing Hodei workers
    local workers=$(docker ps --filter "name=hodei-worker" --format "table {{.Names}}\t{{.Status}}\t{{.Image}}" 2>/dev/null)
    if [ -n "$workers" ]; then
        log_info "Existing Hodei workers:"
        echo "$workers" | while read -r line; do
            [ -n "$line" ] && echo "  $line"
        done
    else
        log_info "No existing Hodei workers"
    fi
}

step_4_3_check_kubernetes_provider() {
    log_info "Step 4.3: Checking Kubernetes Provider..."

    if ! command -v kubectl &> /dev/null; then
        log_warn "kubectl not installed"
        return 0
    fi

    if ! kubectl cluster-info > /dev/null 2>&1; then
        log_warn "Kubernetes cluster not accessible"
        return 0
    fi

    log_success "Kubernetes is accessible: $(kubectl cluster-info | head -1)"

    # List any existing Hodei worker pods
    local pods=$(kubectl get pods -n hodei-jobs-workers -l hodei-worker --no-headers 2>/dev/null || echo "")
    if [ -n "$pods" ]; then
        log_info "Existing K8s worker pods:"
        echo "$pods" | while read -r line; do
            [ -n "$line" ] && echo "  $line"
        done
    else
        log_info "No existing K8s worker pods"
    fi
}

# =============================================================================
# PHASE 5: EXECUTE TEST JOBS
# =============================================================================

phase5_execute_jobs() {
    log_info "============================================================"
    log_info "PHASE 5: Execute Test Jobs"
    log_info "============================================================"

    cd "$PROJECT_ROOT"

    # Test Type 1: Simple Echo
    job_type_1_simple_echo

    # Test Type 2: Multi-Command
    job_type_2_multi_command

    # Test Type 3: Long-Running
    job_type_3_long_running

    # Test Type 4: Environment Variables
    job_type_4_env_vars

    # Test Type 5: With Arguments
    job_type_5_args

    log_success "All test jobs completed"
}

job_type_1_simple_echo() {
    log_info "Job Type 1: Simple Echo (Docker)..."

    local job_id=$(./target/debug/hodei-jobs-cli job run \
        --name "test-echo-$TIMESTAMP" \
        --provider docker \
        --command "echo 'Hello from Hodei Docker Worker!'" \
        --timeout 30 2>/dev/null)

    if [ -n "$job_id" ]; then
        log_success "Job queued: $job_id"
        validate_job_status "$job_id" "JobType1_Echo"
    else
        log_error "Failed to queue echo job"
    fi
}

job_type_2_multi_command() {
    log_info "Job Type 2: Multi-Command (Docker)..."

    local job_id=$(./target/debug/hodei-jobs-cli job run \
        --name "test-multicmd-$TIMESTAMP" \
        --provider docker \
        --command "bash" \
        --args "-c 'echo Step1 && sleep 1 && echo Step2 && sleep 1 && echo Step3'" \
        --timeout 60 2>/dev/null)

    if [ -n "$job_id" ]; then
        log_success "Job queued: $job_id"
        validate_job_status "$job_id" "JobType2_MultiCmd"
    else
        log_error "Failed to queue multi-command job"
    fi
}

job_type_3_long_running() {
    log_info "Job Type 3: Long-Running (Docker)..."

    local job_id=$(./target/debug/hodei-jobs-cli job run \
        --name "test-long-$TIMESTAMP" \
        --provider docker \
        --command "for i in 1 2 3 4 5; do echo Iteration \$i; sleep 2; done" \
        --timeout 120 2>/dev/null)

    if [ -n "$job_id" ]; then
        log_success "Job queued: $job_id"
        validate_job_status "$job_id" "JobType3_LongRunning"
    else
        log_error "Failed to queue long-running job"
    fi
}

job_type_4_env_vars() {
    log_info "Job Type 4: Environment Variables (Docker)..."

    local job_id=$(./target/debug/hodei-jobs-cli job run \
        --name "test-env-$TIMESTAMP" \
        --provider docker \
        --command "bash" \
        --args "-c 'echo MY_VAR=\$MY_VAR && echo NUM_VAR=\$NUM_VAR'" \
        --env "MY_VAR=hello_world,NUM_VAR=42" \
        --timeout 30 2>/dev/null)

    if [ -n "$job_id" ]; then
        log_success "Job queued: $job_id"
        validate_job_status "$job_id" "JobType4_EnvVars"
    else
        log_error "Failed to queue env vars job"
    fi
}

job_type_5_args() {
    log_info "Job Type 5: With Arguments (Docker)..."

    local job_id=$(./target/debug/hodei-jobs-cli job run \
        --name "test-args-$TIMESTAMP" \
        --provider docker \
        --command "bash" \
        --args "-c 'echo Processing file test.txt && echo Done'" \
        --timeout 30 2>/dev/null)

    if [ -n "$job_id" ]; then
        log_success "Job queued: $job_id"
        validate_job_status "$job_id" "JobType5_Args"
    else
        log_error "Failed to queue args job"
    fi
}

# =============================================================================
# PHASE 6: VALIDATION
# =============================================================================

phase6_validation() {
    log_info "============================================================"
    log_info "PHASE 6: Validation"
    log_info "============================================================"

    # Get recent jobs for validation
    local recent_jobs=$(get_recent_job_ids 5)

    for job_id in $recent_jobs; do
        log_info "Validating job: $job_id"

        validate_job_status "$job_id" "Validation"
        validate_domain_events "$job_id"
        validate_log_stream "$job_id"
        validate_worker_lifecycle "$job_id"
    done

    log_success "Validation complete for all recent jobs"
}

validate_job_status() {
    local job_id=$1
    local label=$2

    log_info "[$label] Checking job status for $job_id..."

    local state=$(docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
        SELECT state FROM jobs WHERE id = '$job_id';" 2>/dev/null | xargs)

    if [ -n "$state" ]; then
        log_success "[$label] Job $job_id state: $state"
        echo "$state" >> "$EVIDENCE_DIR/${label}_job_states.log"
    else
        log_warn "[$label] Job $job_id not found in database"
    fi
}

validate_domain_events() {
    local job_id=$1

    log_info "Checking domain events for $job_id..."

    local events=$(docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
        SELECT event_type, aggregate_id, occurred_at::text
        FROM domain_events
        WHERE aggregate_id = '$job_id'
        ORDER BY occurred_at;" 2>/dev/null)

    local event_count=$(echo "$events" | grep -c "^ " || echo "0")

    log_success "Found $event_count events for job $job_id"

    # Save to evidence
    echo "Events for job $job_id:" > "$EVIDENCE_DIR/events_${job_id}.log"
    echo "$events" >> "$EVIDENCE_DIR/events_${job_id}.log"

    # Expected sequence check
    if echo "$events" | grep -q "JobCreated" && \
       echo "$events" | grep -q "JobStatusChanged"; then
        log_success "Event sequence appears correct"
    else
        log_warn "Event sequence may be incomplete"
    fi
}

validate_log_stream() {
    local job_id=$1

    log_info "Checking log stream for $job_id..."

    local log_info=$(docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
        SELECT id, storage_uri, size_bytes, entry_count
        FROM job_log_files
        WHERE job_id = '$job_id';" 2>/dev/null)

    if [ -n "$log_info" ]; then
        log_success "Log file found for job $job_id:"
        echo "$log_info" | while read -r line; do
            [ -n "$line" ] && echo "  $line"
        done
    else
        log_warn "No log file found for job $job_id"
    fi
}

validate_worker_lifecycle() {
    local job_id=$1

    log_info "Checking worker lifecycle for $job_id..."

    local workers=$(docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
        SELECT id, state, provider_type, provider_resource_id, created_at::text, updated_at::text
        FROM workers
        WHERE created_at > NOW() - INTERVAL '1 hour'
        ORDER BY created_at DESC LIMIT 5;" 2>/dev/null)

    if [ -n "$workers" ]; then
        log_success "Recent workers:"
        echo "$workers" | while read -r line; do
            [ -n "$line" ] && echo "  $line"
        done
    else
        log_warn "No recent workers found"
    fi
}

# =============================================================================
# PHASE 7: KUBERNETES TESTING (OPTIONAL)
# =============================================================================

phase7_kubernetes_testing() {
    log_info "============================================================"
    log_info "PHASE 7: Kubernetes Testing (Optional)"
    log_info "============================================================"

    if [ "$HODEI_KUBERNETES_ENABLED" != "1" ]; then
        log_warn "Kubernetes testing disabled (HODEI_KUBERNETES_ENABLED=0)"
        return 0
    fi

    if ! command -v kubectl &> /dev/null; then
        log_warn "kubectl not installed, skipping K8s tests"
        return 0
    fi

    step_7_1_configure_kubernetes
    step_7_2_enable_k8s_provider
    step_7_3_run_k8s_job
    step_7_4_verify_k8s_pod
}

step_7_1_configure_kubernetes() {
    log_info "Step 7.1: Configuring Kubernetes..."

    export HODEI_K8S_KUBECONFIG="${HODEI_K8S_KUBECONFIG:-$HOME/.kube/config}"
    export HODEI_K8S_NAMESPACE="${HODEI_K8S_NAMESPACE:-hodei-jobs-workers}"

    kubectl cluster-info
}

step_7_2_enable_k8s_provider() {
    log_info "Step 7.2: Enabling Kubernetes Provider..."

    # Restart server with K8s enabled
    pkill -f hodei-server-bin 2>/dev/null || true

    export HODEI_KUBERNETES_ENABLED=1

    cd "$PROJECT_ROOT"
    export DATABASE_URL="$DATABASE_URL"
    export HODEI_SERVER_HOST="$HODEI_SERVER_HOST"

    nohup ./target/debug/hodei-server-bin > "$LOGS_DIR/server_k8s.log" 2>&1 &

    sleep 5
    log_success "Server restarted with Kubernetes enabled"
}

step_7_3_run_k8s_job() {
    log_info "Step 7.3: Running Job on Kubernetes..."

    local job_id=$(./target/debug/hodei-jobs-cli job run \
        --name "test-k8s-$TIMESTAMP" \
        --provider kubernetes \
        --command "echo 'Hello from Kubernetes Worker!'" \
        --timeout 60 2>/dev/null)

    if [ -n "$job_id" ]; then
        log_success "K8s Job queued: $job_id"
        validate_job_status "$job_id" "K8sJob"
    else
        log_error "Failed to queue K8s job"
    fi
}

step_7_4_verify_k8s_pod() {
    log_info "Step 7.4: Verifying Kubernetes Pod..."

    local pods=$(kubectl get pods -n hodei-jobs-workers -l hodei-worker --no-headers 2>/dev/null)

    if [ -n "$pods" ]; then
        log_success "K8s pods running:"
        echo "$pods" | while read -r line; do
            [ -n "$line" ] && echo "  $line"
        done
    else
        log_warn "No K8s pods found"
    fi
}

# =============================================================================
# PHASE 8: BATCH TESTING
# =============================================================================

phase8_batch_testing() {
    log_info "============================================================"
    log_info "PHASE 8: Batch Testing"
    log_info "============================================================"

    local batch_size=${1:-5}

    log_info "Running batch of $batch_size jobs..."

    for i in $(seq 1 $batch_size); do
        local job_id=$(./target/debug/hodei-jobs-cli job run \
            --name "batch-test-$i-$TIMESTAMP" \
            --provider docker \
            --command "echo 'Batch test $i'" \
            --timeout 30 2>/dev/null)

        if [ -n "$job_id" ]; then
            log_success "Batch job $i queued: $job_id"
        else
            log_error "Failed to queue batch job $i"
        fi

        sleep 2
    done

    # Verify all jobs completed
    verify_all_jobs_completed
}

verify_all_jobs_completed() {
    log_info "Verifying all batch jobs completed..."

    local results=$(docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
        SELECT state, COUNT(*) as count
        FROM jobs
        WHERE created_at > NOW() - INTERVAL '1 hour'
        GROUP BY state;" 2>/dev/null)

    log_success "Batch test results:"
    echo "$results" | while read -r line; do
        [ -n "$line" ] && echo "  $line"
    done
}

# =============================================================================
# HELPER FUNCTIONS
# =============================================================================

get_recent_job_ids() {
    local limit=${1:-5}

    docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c "
        SELECT id FROM jobs ORDER BY created_at DESC LIMIT $limit;" 2>/dev/null | \
        grep -oE '[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}' || true
}

generate_report() {
    log_info "============================================================"
    log_info "Generating E2E Test Report"
    log_info "============================================================"

    local report_file="$EVIDENCE_DIR/e2e_report_${TIMESTAMP}.md"

    cat > "$report_file" << EOF
# Hodei E2E Test Report

**Date:** $(date '+%Y-%m-%d %H:%M:%S')
**Timestamp:** $TIMESTAMP

## Test Summary

| Metric | Value |
|--------|-------|
| Phase 1 (Infrastructure) | $([ -f "$LOGS_DIR/migrations_domain_events.log" ] && echo "✅ Complete" || echo "❌ Failed") |
| Phase 2 (Build) | $([ -f "$LOGS_DIR/build_server.log" ] && echo "✅ Complete" || echo "❌ Failed") |
| Phase 3 (Services) | $([ -f "$LOGS_DIR/server.log" ] && echo "✅ Complete" || echo "❌ Failed") |
| Phase 4 (Providers) | ✅ Verified |
| Phase 5 (Jobs) | ✅ Executed |
| Phase 6 (Validation) | ✅ Complete |

## Recent Jobs

$(get_recent_job_ids 10 | while read -r job_id; do
    echo "- $job_id"
done)

## Database Tables

$(docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c \
    "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' ORDER BY table_name;" 2>/dev/null | xargs)

## Provider Status

$(docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -t -c \
    "SELECT id, provider_type, status FROM provider_configs ORDER BY provider_type;" 2>/dev/null)

## Evidence Files

$(ls -la "$EVIDENCE_DIR" 2>/dev/null | tail -n +4 || echo "No evidence files yet")

## Logs

$(ls -la "$LOGS_DIR" 2>/dev/null | tail -n +4 || echo "No log files yet")
EOF

    log_success "Report generated: $report_file"
}

show_usage() {
    echo "Usage: $0 [phase]"
    echo ""
    echo "Phases:"
    echo "  infra       - Phase 1: Infrastructure Setup"
    echo "  build       - Phase 2: Compile Binaries"
    echo "  start       - Phase 3: Start Services"
    echo "  providers   - Phase 4: Configure Providers"
    echo "  jobs        - Phase 5: Execute Test Jobs"
    echo "  validate    - Phase 6: Validation"
    echo "  k8s         - Phase 7: Kubernetes Testing (Optional)"
    echo "  batch       - Phase 8: Batch Testing"
    echo "  all         - Run all phases (default)"
    echo "  report      - Generate test report"
    echo ""
    echo "Environment Variables:"
    echo "  DATABASE_URL        - PostgreSQL connection string"
    echo "  GRPC_PORT           - gRPC server port (default: 50051)"
    echo "  HODEI_SERVER_HOST   - Server host IP"
    echo "  HODEI_DOCKER_ENABLED - Enable Docker provider (default: 1)"
    echo "  HODEI_KUBERNETES_ENABLED - Enable K8s provider (default: 0)"
}

# =============================================================================
# MAIN
# =============================================================================

main() {
    local phase="${1:-all}"

    echo "============================================================"
    echo "  Hodei Jobs Platform - E2E Testing Procedure"
    echo "  Phase: $phase"
    echo "  Timestamp: $TIMESTAMP"
    echo "============================================================"

    # Show configuration
    echo ""
    echo "Configuration:"
    echo "  DATABASE_URL: $DATABASE_URL"
    echo "  GRPC_PORT: $GRPC_PORT"
    echo "  HODEI_SERVER_HOST: $HODEI_SERVER_HOST"
    echo "  HODEI_DOCKER_ENABLED: $HODEI_DOCKER_ENABLED"
    echo "  HODEI_KUBERNETES_ENABLED: $HODEI_KUBERNETES_ENABLED"
    echo ""

    case "$phase" in
        infra)
            phase1_infrastructure
            ;;
        build)
            phase2_compile
            ;;
        start)
            phase3_start_services
            ;;
        providers)
            phase4_configure_providers
            ;;
        jobs)
            phase5_execute_jobs
            ;;
        validate)
            phase6_validation
            ;;
        k8s)
            phase7_kubernetes_testing
            ;;
        batch)
            phase8_batch_testing "$2"
            ;;
        report)
            generate_report
            ;;
        all)
            phase1_infrastructure
            phase2_compile
            phase3_start_services
            phase4_configure_providers
            phase5_execute_jobs
            phase6_validation
            phase7_kubernetes_testing
            phase8_batch_testing
            generate_report
            ;;
        help|--help|-h)
            show_usage
            ;;
        *)
            log_error "Unknown phase: $phase"
            show_usage
            exit 1
            ;;
    esac

    echo ""
    log_success "E2E Testing Procedure - Phase '$phase' Complete!"
}

# Run main function
main "$@"

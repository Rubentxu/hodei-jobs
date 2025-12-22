# =============================================================================
# Hodei Job Platform v8.0 - Development Commands
# =============================================================================
# Architecture: Event-Driven gRPC System with Hexagonal Design
# Components: Server (gRPC), Worker (mTLS), EventBus (Postgres), CLI
#
# Install: cargo install just
# Usage: just <command>
# =============================================================================
# Configuration

export RUST_BACKTRACE := "1"
export RUST_LOG := "debug"
export DATABASE_URL := "postgres://postgres:postgres@localhost:5432/hodei_dev"

# Default target
_default:
    @echo "🚀 Hodei Job Platform v8.0"
    @echo "=========================="
    @echo ""
    @echo "💡 Quick commands:"
    @echo "  just dev              Start full dev environment (requires Docker)"
    @echo "  just dev-no-docker    Start dev environment WITHOUT Docker"
    @echo "  just build            Build the project"
    @echo "  just test             Run all tests"
    @echo "  just help             Show all commands"
    @echo ""
    @just --list

# =============================================================================
# BUILD COMMANDS
# =============================================================================

# Build entire workspace
build:
    @echo "🔨 Building workspace..."
    cargo build --workspace
    @echo "✅ Build complete"

# Build release
build-release:
    @echo "🔨 Building release..."
    cargo build --workspace --release
    @echo "✅ Release build complete"

# Build server only
build-server:
    @echo "🔨 Building server..."
    cargo build --package hodei-server-bin
    @echo "✅ Server build complete"

# Build worker only
build-worker:
    @echo "🔨 Building worker..."
    cargo build --package hodei-worker-bin
    @echo "✅ Worker build complete"

# Build CLI only
build-cli:
    @echo "🔨 Building CLI..."
    cargo build --package hodei-jobs-cli
    @echo "✅ CLI build complete"

# Generate protobuf code
generate:
    @echo "📝 Generating protobuf code..."
    cargo build --package hodei-server-interface
    @echo "✅ Code generation complete"

# =============================================================================
# DEVELOPMENT COMMANDS
# =============================================================================

# Start development database
dev-db:
    @echo "🗄️  Starting PostgreSQL database..."
    @if ! command -v docker >/dev/null 2>&1; then \
        echo "❌ Docker is not installed or not in PATH"; \
        exit 1; \
    fi
    @if ! docker info >/dev/null 2>&1; then \
        echo "❌ Docker daemon is not running"; \
        echo "💡 Try: sudo systemctl start docker"; \
        echo "💡 Or use: just dev-no-docker"; \
        exit 1; \
    fi
    docker compose -f docker-compose.dev.yml up -d postgres
    @sleep 2
    @echo "✅ Database ready"

# Run database migrations
db-migrate:
    @echo "📦 Running migrations..."
    @just stop-server || true
    @echo "Waiting for port to be free..."
    @sleep 2
    @cd crates/server/bin && timeout 5 cargo run --bin hodei-server-bin 2>&1 || true
    @echo "✅ Migrations complete (server exited after startup)"

# Start server in development mode (auto-compiles + idempotent)
dev-server:
    @bash -c 'scripts/dev-start.sh'

# Start full development environment (idempotent + recompile)
dev:
    @echo "🚀 Starting full development environment (server + auto-provisioning workers)..."
    @just dev-db
    @just db-migrate
    @bash scripts/dev-server.sh

# Start development environment without Docker
dev-no-docker:
    @echo "🚀 Starting development environment (NO DOCKER)..."
    @echo "⚠️  Running without database - limited functionality"
    @echo "💡 For full functionality, start Docker: sudo systemctl start docker"
    @bash /home/rubentxu/Proyectos/rust/package/hodei-job-platform/dev_no_docker.sh

# Stop any running server
stop-server:
    @echo "Checking if server is running on port 50051..."
    @fuser -k 50051/tcp 2>/dev/null || true
    @sleep 1
    @lsof -Pi :50051 -sTCP:LISTEN -t >/dev/null 2>&1 && (echo "⚠️  Port still in use, forcing..."; lsof -Pi :50051 -sTCP:LISTEN -t | xargs kill -9 2>/dev/null || true; sleep 1) || echo "✅ Port 50051 is free"

# Watch mode for development
watch:
    @echo "👀 Watching for changes..."
    cargo watch -x "build --package hodei-server-bin" -x "build --package hodei-worker-bin"

# =============================================================================
# TEST COMMANDS
# =============================================================================

# Run all tests
test:
    @echo "🧪 Running all tests..."
    cargo test --workspace
    @echo "✅ All tests passed"

# Run server tests
test-server:
    @echo "🧪 Running server tests..."
    cargo test -p hodei-server-domain
    cargo test -p hodei-server-application
    cargo test -p hodei-server-infrastructure
    @echo "✅ Server tests passed"

# Run worker tests
test-worker:
    @echo "🧪 Running worker tests..."
    cargo test -p hodei-worker-domain
    cargo test -p hodei-worker-application
    cargo test -p hodei-worker-infrastructure
    @echo "✅ Worker tests passed"

# Run integration tests
test-integration:
    @echo "🧪 Running integration tests..."
    cargo test -p hodei-server-integration --test postgres_integration
    @echo "✅ Integration tests passed"

# Run multi-provider integration tests (Docker + Kubernetes)
test-multi-provider:
    @echo "🧪 Running multi-provider integration tests..."
    ./scripts/test-multi-provider.sh docker
    @echo "✅ Multi-provider tests passed"

# Run multi-provider tests with Kubernetes enabled
test-multi-provider-k8s:
    @echo "🧪 Running multi-provider tests (including Kubernetes)..."
    HODEI_K8S_TEST=1 ./scripts/test-multi-provider.sh all
    @echo "✅ Multi-provider tests (K8s) passed"

# Run tests with output
test-verbose:
    @echo "🧪 Running tests (verbose)..."
    cargo test --workspace -- --nocapture
    @echo "✅ Tests complete"

# Run specific test
test-one name:
    @echo "🧪 Running test: {{ name }}..."
    cargo test --package {{ name }}
    @echo "✅ Test passed"

# =============================================================================
# CODE QUALITY
# =============================================================================

# Check code quality
check:
    @echo "🔍 Running code quality checks..."
    @just lint
    @just format
    @just typecheck
    @echo "✅ Code quality check complete"

# Lint code
lint:
    @echo "🔍 Linting code..."
    cargo clippy --workspace -- -D warnings
    @echo "✅ Linting complete"

# Format code
format:
    @echo "✨ Formatting code..."
    cargo fmt --all
    @echo "✅ Formatting complete"

# Type check
typecheck:
    @echo "🔍 Type checking..."
    cargo check --workspace
    @echo "✅ Type check complete"

# =============================================================================
# END-TO-END TESTS
# =============================================================================

# Full end-to-end test
e2e:
    @echo "🎯 Running full end-to-end test..."
    @if ! docker info >/dev/null 2>&1; then \
        echo "❌ Docker daemon is not running"; \
        echo "💡 Start Docker: sudo systemctl start docker"; \
        exit 1; \
    fi
    @just dev-db
    @sleep 2
    @just db-migrate
    @echo "Starting server..."
    cargo run --bin hodei-server-bin &
    SERVER_PID=$$!
    sleep 5
    @echo "Starting worker..."
    cargo run --bin hodei-worker-bin &
    WORKER_PID=$$!
    sleep 5
    @echo "✅ E2E environment running"
    @echo "Server PID: $$SERVER_PID"
    @echo "Worker PID: $$WORKER_PID"
    @echo "Run 'kill $$SERVER_PID $$WORKER_PID' to stop"

# Test complete job flow
e2e-job-flow:
    @echo "🎯 Testing complete job flow..."
    @if ! docker info >/dev/null 2>&1; then \
        echo "❌ Docker daemon is not running"; \
        echo "💡 Start Docker: sudo systemctl start docker"; \
        exit 1; \
    fi
    @just e2e &
    sleep 10
    @echo "Creating job..."
    cargo run --bin hodei-jobs-cli -- job run --name "Test Job" --command "echo 'Hello from Hodei!'"
    sleep 5
    @echo "Checking job status..."
    cargo run --bin hodei-jobs-cli -- job list
    @echo "✅ Job flow test complete"

# =============================================================================
# SECURITY
# =============================================================================

# Generate mTLS certificates
cert-generate:
    @echo "🔐 Generating mTLS certificates..."
    mkdir -p certs
    @echo "Generating CA certificate..."
    openssl genrsa -out certs/ca.key 4096
    openssl req -new -x509 -days 365 -key certs/ca.key -out certs/ca.crt -subj "/CN=Hodei CA"
    @echo "Generating server certificate..."
    openssl genrsa -out certs/server.key 2048
    openssl req -new -key certs/server.key -out certs/server.csr -subj "/CN=localhost"
    openssl x509 -req -in certs/server.csr -CA certs/ca.crt -CAkey certs/ca.key -CAcreateserial -out certs/server.crt -days 365
    @echo "Generating worker certificate..."
    openssl genrsa -out certs/worker.key 2048
    openssl req -new -key certs/worker.key -out certs/worker.csr -subj "/CN=worker"
    openssl x509 -req -in certs/worker.csr -CA certs/ca.crt -CAkey certs/ca.key -CAcreateserial -out certs/worker.crt -days 365
    @echo "✅ Certificates generated in certs/"

# Check certificate status
cert-check:
    @echo "🔐 Checking certificate status..."
    @if [ -f "certs/server.crt" ]; then \
        openssl x509 -in certs/server.crt -noout -dates; \
    else \
        echo "No certificates found. Run 'just cert-generate'"; \
    fi

# =============================================================================
# DATABASE OPERATIONS
# =============================================================================

# Open database shell
db-shell:
    @echo "🗄️  Opening database shell..."
    @if ! docker info >/dev/null 2>&1; then \
        echo "❌ Docker daemon is not running"; \
        exit 1; \
    fi
    @CONTAINER_ID=$$(docker ps -q -f name=postgres 2>/dev/null); \
    if [ -z "$$CONTAINER_ID" ]; then \
        echo "❌ PostgreSQL container is not running"; \
        echo "💡 Start it with: just dev-db"; \
        exit 1; \
    fi
    docker exec -it $$CONTAINER_ID psql -U postgres -d hodei_dev

# Show database status
db-status:
    @echo "📊 Database status:"
    @if ! docker info >/dev/null 2>&1; then \
        echo "❌ Docker daemon is not running"; \
        exit 1; \
    fi
    @CONTAINER_ID=$$(docker ps -q -f name=postgres 2>/dev/null); \
    if [ -z "$$CONTAINER_ID" ]; then \
        echo "⚠️  PostgreSQL container is not running"; \
        echo "💡 Start it with: just dev-db"; \
        exit 1; \
    fi
    docker exec -t $$CONTAINER_ID psql -U postgres -d hodei_dev -c "SELECT COUNT(*) as total_jobs FROM jobs;" 2>/dev/null || echo "Database not ready"
    docker exec -t $$CONTAINER_ID psql -U postgres -d hodei_dev -c "SELECT COUNT(*) as total_workers FROM workers;" 2>/dev/null || echo "Database not ready"

# Reset database
db-reset:
    @echo "⚠️  Resetting database..."
    @if ! docker info >/dev/null 2>&1; then \
        echo "❌ Docker daemon is not running"; \
        exit 1; \
    fi
    docker compose -f docker-compose.dev.yml down -v
    docker compose -f docker-compose.dev.yml up -d postgres
    sleep 2
    @just db-migrate
    @echo "✅ Database reset complete"

# =============================================================================
# JOB OPERATIONS
# =============================================================================
# =============================================================================
# JOB OPERATIONS (Example Scripts - Type kubectl-style)
# =============================================================================

# Run example job: Hello World (simple demo)
job-hello-world:
    @echo "🚀 Running Hello World job..."
    @cargo run --bin hodei-jobs-cli -- job run --name "Hello World Demo" --script scripts/examples/01-hello-world.sh

# Run example job: Data Processing (ETL pipeline)
job-data-processing:
    @echo "📦 Running Data Processing job..."
    @cargo run --bin hodei-jobs-cli -- job run --name "Data Processing ETL" --script scripts/examples/02-data-processing.sh

# Run example job: ML Training
job-ml-training:
    @echo "🤖 Running ML Training job..."
    @cargo run --bin hodei-jobs-cli -- job run --name "ML Training" --script scripts/examples/03-ml-training.sh

# Run example job: CI/CD Build
job-cicd-build:
    @echo "🚀 Running CI/CD Build job..."
    @cargo run --bin hodei-jobs-cli -- job run --name "CI/CD Build Pipeline" --script scripts/examples/04-cicd-build.sh

# Run example job: CPU Stress Test
job-cpu-stress:
    @echo "🔥 Running CPU Stress Test..."
    @cargo run --bin hodei-jobs-cli -- job run --name "CPU Stress Test" --script scripts/examples/05-cpu-stress.sh

# Run example job: Error Handling Demo
job-error-handling:
    @echo "⚠️  Running Error Handling Demo..."
    @cargo run --bin hodei-jobs-cli -- job run --name "Error Handling Demo" --script scripts/examples/06-error-handling.sh

# Run all example jobs sequentially
job-examples-all:
    @echo "🎯 Running all example jobs..."
    @just job-hello-world
    @echo ""
    @just job-data-processing
    @echo ""
    @just job-ml-training
    @echo ""
    @just job-cicd-build
    @echo ""
    @echo "✅ All example jobs completed!"

# =============================================================================
# JOB OPERATIONS (Basic)
# =============================================================================

# Create a simple test job
job-test:
    @echo "📤 Creating test job..."
    cargo run --bin hodei-jobs-cli -- job run --name "Quick Test" --command "echo 'Test job executed successfully!'"
    @echo "✅ Job created"

# Create a long-running job
job-long:
    @echo "📤 Creating long-running job..."
    cargo run --bin hodei-jobs-cli -- job run --name "Long Job" --command "bash -c 'for i in {1..10}; do echo \"Step $${i}/10\"; sleep 1; done; echo \"Job complete!\"'"
    @echo "✅ Job created"

# Create a job with error
job-error:
    @echo "📤 Creating job that will fail..."
    cargo run --bin hodei-jobs-cli -- job run --name "Error Job" --command "echo 'This will fail'; exit 1"
    @echo "✅ Job created"

# List all jobs
job-list:
    @echo "📋 Listing all jobs..."
    @cargo run --bin hodei-jobs-cli -- job list

# Watch job logs (need job-id)
job-watch:
    @echo "👀 Watching job logs..."
    @echo "Usage: cargo run --bin hodei-jobs-cli -- logs follow --job-id <JOB_ID>"
    @echo "First run: cargo run --bin hodei-jobs-cli -- job list"

# Get job details
job-details id:
    @echo "🔍 Getting job details for ID: {{ id }}"
    @cargo run --bin hodei-jobs-cli -- job get --job-id {{ id }}

# Cancel a job
job-cancel id:
    @echo "🛑 Canceling job: {{ id }}"
    @cargo run --bin hodei-jobs-cli -- job cancel --job-id {{ id }}

# =============================================================================
# MONITORING
# =============================================================================

# Show system status
status:
    @echo "📊 System Status:"
    @echo "=================="
    @echo "Docker containers:"
    @if docker info >/dev/null 2>&1; then \
        docker ps --filter "name=hodei" 2>/dev/null || echo "No containers running"; \
    else \
        echo "❌ Docker daemon is not running"; \
    fi
    @echo ""
    @echo "Workers in database:"
    @if docker info >/dev/null 2>&1; then \
        CONTAINER_ID=$$(docker ps -q -f name=postgres 2>/dev/null); \
        if [ -n "$$CONTAINER_ID" ]; then \
            docker exec -t $$CONTAINER_ID psql -U postgres -d hodei_dev -c "SELECT id, state, last_heartbeat FROM workers ORDER BY last_heartbeat DESC LIMIT 5;" 2>/dev/null || echo "Database not accessible"; \
        else \
            echo "⚠️  PostgreSQL container is not running"; \
        fi; \
    else \
        echo "❌ Docker daemon is not running"; \
    fi
    @echo ""
    @echo "Jobs in queue:"
    @if docker info >/dev/null 2>&1; then \
        CONTAINER_ID=$$(docker ps -q -f name=postgres 2>/dev/null); \
        if [ -n "$$CONTAINER_ID" ]; then \
            docker exec -t $$CONTAINER_ID psql -U postgres -d hodei_dev -c "SELECT COUNT(*) as pending_jobs FROM job_queue;" 2>/dev/null || echo "Database not accessible"; \
        else \
            echo "⚠️  PostgreSQL container is not running"; \
        fi; \
    else \
        echo "❌ Docker daemon is not running"; \
    fi

# Show logs
logs:
    @echo "📜 Showing logs..."
    @if ! docker info >/dev/null 2>&1; then \
        echo "❌ Docker daemon is not running"; \
        exit 1; \
    fi
    docker compose -f docker-compose.dev.yml logs -f

# Show database logs only
logs-db:
    @echo "📜 Database logs..."
    @if ! docker info >/dev/null 2>&1; then \
        echo "❌ Docker daemon is not running"; \
        exit 1; \
    fi
    docker compose -f docker-compose.dev.yml logs -f postgres

# =============================================================================
# CLEANUP
# =============================================================================

# Clean build artifacts
clean:
    @echo "🧹 Cleaning build artifacts..."
    cargo clean
    @echo "✅ Clean complete"

# Clean everything
clean-all: clean
    @echo "🗑️  Cleaning all data..."
    @if docker info >/dev/null 2>&1; then \
        docker compose -f docker-compose.dev.yml down -v 2>/dev/null || true; \
    fi
    rm -rf certs/
    @echo "⚠️  All data removed"

# =============================================================================
# PRODUCTION
# =============================================================================

# Build production images
prod-build:
    @echo "🏗️  Building production images..."
    @if ! docker info >/dev/null 2>&1; then \
        echo "❌ Docker daemon is not running"; \
        exit 1; \
    fi
    docker build -t hodei-server:latest -f Dockerfile .
    docker build -t hodei-worker:latest -f Dockerfile.worker .
    @echo "✅ Production images built"

# Start production
prod-up:
    @echo "🚀 Starting production..."
    @if ! docker info >/dev/null 2>&1; then \
        echo "❌ Docker daemon is not running"; \
        exit 1; \
    fi
    docker compose -f docker-compose.prod.yml up -d
    @echo "✅ Production started"

# Stop production
prod-down:
    @echo "⏹️  Stopping production..."
    @if docker info >/dev/null 2>&1; then \
        docker compose -f docker-compose.prod.yml down 2>/dev/null || true; \
    fi
    @echo "✅ Production stopped"

# =============================================================================
# UTILITIES
# =============================================================================

# Install development tools
install-tools:
    @echo "🛠️  Installing development tools..."
    cargo install just cargo-watch cargo-expand
    @echo "✅ Tools installed"

# Show help
help:
    @echo "Hodei Job Platform v8.0 - Development Commands"
    @echo "================================================"
    @echo ""
    @echo "Quick Start:"
    @echo "  just dev              Start full development environment"
    @echo "  just dev-no-docker    Start env WITHOUT Docker (limited)"
    @echo "  just build            Build workspace"
    @echo ""
    @echo "Development:"
    @echo "  just dev-db           Start database"
    @echo "  just dev              Start full env (DB + Server, auto-restart)"
    @echo "  just dev-no-docker    Start env without Docker"
    @echo "  just dev-server       Start server (auto-kill + recompile + run)"
    @echo "  just stop-server      Manually stop server"
    @echo "  just watch            Watch for changes"
    @echo ""
    @echo "Testing:"
    @echo "  just test             Run all tests"
    @echo "  just test-server      Run server tests"
    @echo "  just test-worker      Run worker tests"
    @echo "  just test-integration Run integration tests"
    @echo "  just e2e              End-to-end test"
    @echo ""
    @echo "Code Quality:"
    @echo "  just check            Run lint, format, typecheck"
    @echo "  just lint             Lint code"
    @echo "  just format           Format code"
    @echo ""
    @echo ""
    @echo "Server Features (v8.0):"
    @echo "  just test-eventbus     Check EventBus integration"
    @echo "  just test-job-queue    Verify job queue"
    @echo "  just test-metrics      Check Prometheus metrics"
    @echo ""
    @echo "Security:"
    @echo "  just cert-generate     Generate mTLS certificates"
    @echo "  just cert-check        Check certificate status"
    @echo ""
    @echo "Job Examples (kubectl-style):"
    @echo "  just job-hello-world       Run Hello World demo"
    @echo "  just job-data-processing   Run ETL pipeline demo"
    @echo "  just job-ml-training       Run ML training demo"
    @echo "  just job-cicd-build        Run CI/CD pipeline demo"
    @echo "  just job-cpu-stress        Run CPU stress test"
    @echo "  just job-error-handling    Run error handling demo"
    @echo "  just job-examples-all      Run all example jobs"
    @echo ""
    @echo "Job Operations:"
    @echo "  just job-test          Create test job"
    @echo "  just job-long          Create long-running job"
    @echo "  just job-list          List all jobs"
    @echo "  just job-details <id>  Get job details"
    @echo "  just job-cancel <id>   Cancel a job"
    @echo "  just job-watch         Watch logs (see usage)"
    @echo ""
    @echo "Monitoring:"
    @echo "  just status            Show system status"
    @echo "  just logs              Show all logs"
    @echo ""
    @echo "Database:"
    @echo "  just db-shell          Open database shell"
    @echo "  just db-status         Show database status"
    @echo "  just db-reset          Reset database"
    @echo ""
    @echo "Cleanup:"
    @echo "  just clean             Clean build artifacts"
    @echo "  just clean-all         Clean everything"
    @echo ""
    @echo "Docker Troubleshooting:"
    @echo "  just dev-no-docker     Run without Docker if daemon is down"
    @echo "  sudo systemctl start docker   Start Docker daemon"
    @echo ""
    @echo "Debug Commands (v8.1+):"
    @echo "  just debug-system         Show system dashboard"
    @echo "  just debug-jobs           Show PENDING jobs"
    @echo "  just debug-queue          Show jobs in queue"
    @echo "  just debug-workers        Show registered workers"
    @echo "  just debug-job <ID>       Detailed job diagnosis"
    @echo "  just logs-server          Live server logs"
    @echo "  just logs-worker          Live worker logs"
    @echo "  just watch-jobs           Watch jobs table (live)"
    @echo "  just watch-workers        Watch workers table (live)"
    @echo "  just watch-queue          Watch queue length (live)"
    @echo "  just restart-all          Restart entire system"
    @echo "  just clean-all            Clean everything"
    @echo ""
    @echo "For more information, see README.md"

# =============================================================================
# DEBUG COMMANDS (v8.1+)
# =============================================================================

# Show system status dashboard
debug-system:
    @./scripts/system-status.sh

# Show jobs in PENDING state
debug-jobs:
    @echo "=== JOBS EN ESTADO PENDING ==="
    docker exec hodei-jobs-postgres psql -U postgres -c "SELECT id, state, attempts, created_at, EXTRACT(EPOCH FROM (now() - created_at)) as seconds_in_pending FROM jobs WHERE state = 'PENDING' ORDER BY created_at DESC;"

# Show jobs in queue
debug-queue:
    @echo "=== JOBS EN COLA ==="
    docker exec hodei-jobs-postgres psql -U postgres -c "SELECT jq.job_id, jq.enqueued_at, j.state, EXTRACT(EPOCH FROM (now() - jq.enqueued_at)) as seconds_in_queue FROM job_queue jq JOIN jobs j ON jq.job_id = j.id ORDER BY jq.enqueued_at DESC;"

# Show registered workers
debug-workers:
    @echo "=== WORKERS REGISTRADOS ==="
    docker exec hodei-jobs-postgres psql -U postgres -c "SELECT id, state, last_heartbeat, EXTRACT(EPOCH FROM (now() - last_heartbeat)) as seconds_since_heartbeat, current_job_id FROM workers ORDER BY last_heartbeat DESC;"

# Detailed job diagnosis
debug-job job_id:
    @./scripts/debug-job.sh {{ job_id }}

# Job timeline visualization
debug-job-timeline job_id:
    @./scripts/debug-job-timeline.sh {{ job_id }}

# Live server logs (tail)
logs-server:
    @if [ -f /tmp/server.log ]; then \
        tail -f /tmp/server.log | grep -E "JobDispatcher|JobController|JobCreated|JobAssigned|RUN_JOB|JobStatusChanged"; \
    else \
        echo "❌ Server log not found at /tmp/server.log"; \
        echo "💡 Start server with: cargo run --bin hodei-server-bin"; \
    fi

# Live worker logs (tail latest container)
logs-worker:
    @./scripts/logs-worker.sh

# Watch jobs table (live updates)
watch-jobs:
    @watch -n 1 "docker exec hodei-jobs-postgres psql -U postgres -c 'SELECT id, state, attempts, created_at FROM jobs ORDER BY created_at DESC LIMIT 5;'"

# Watch workers table (live updates)
watch-workers:
    @watch -n 1 "docker exec hodei-jobs-postgres psql -U postgres -c 'SELECT id, state, last_heartbeat, EXTRACT(EPOCH FROM (now() - last_heartbeat)) as seconds_ago FROM workers ORDER BY last_heartbeat DESC LIMIT 5;'"

# Watch queue length (live updates)
watch-queue:
    @watch -n 1 "docker exec hodei-jobs-postgres psql -U postgres -c 'SELECT COUNT(*) as queued_jobs FROM job_queue;'"

# Restart entire system (clean + start)
restart-all:
    @./scripts/restart-system.sh

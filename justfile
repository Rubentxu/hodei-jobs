# =============================================================================
# Hodei Job Platform - Just Commands for Rapid Development
# =============================================================================
# Install: cargo install just
# Usage: just <command>
#
# This Justfile provides a fast development workflow with hot reload,
# parallel execution, and optimized build times.
# Configuration

export DEV_DATABASE_URL := "postgres://postgres:postgres@localhost:5432/hodei_dev"
export RUST_BACKTRACE := "1"
export RUST_LOG := "debug,hodei=trace,sqlx=warn"

# Default target
_default:
    @just --list

# =============================================================================
# DEVELOPMENT WORKFLOW
# =============================================================================

# Start full development environment (database + backend + frontend)
dev:
    @echo "🚀 Starting Hodei development environment..."
    @just dev-db
    @just dev-backend &
    @just dev-frontend

# Start database only
dev-db:
    @echo "🗄️  Starting PostgreSQL database..."
    docker compose -f docker-compose.dev.yml up -d postgres
    @sleep 2
    @just db-wait
    @echo "✅ Database ready at localhost:5432"

# Start backend with hot reload
dev-backend:
    @echo "🔧 Starting Rust backend with hot reload..."
    @cd crates/grpc && cargo install --path . 2>/dev/null || true
    bacon run

# Start frontend with hot reload
dev-frontend:
    @echo "⚛️  Starting React frontend with HMR..."
    @cd web && npm run dev

# Watch mode for tests
dev-test:
    @echo "🧪 Running tests in watch mode..."
    @bacon test

# =============================================================================
# DATABASE COMMANDS
# =============================================================================

# Wait for database to be ready
db-wait:
    @echo "⏳ Waiting for database..."
    @until docker exec hodei-jobs-postgres pg_isready -U postgres -d hodei; do \
        sleep 0.5; \
    done

# Run migrations
db-migrate:
    @echo "📦 Running database migrations..."
    @cd crates/infrastructure && cargo run --bin migrate
    @echo "✅ Migrations complete"

# Reset database (WARNING: destructive!)
db-reset:
    @echo "⚠️  Resetting development database..."
    @docker compose -f docker-compose.dev.yml down -v
    @just dev-db
    @just db-migrate
    @echo "✅ Database reset complete"

# Seed database with test data
db-seed:
    @echo "🌱 Seeding database with test data..."
    @cd crates/infrastructure && cargo run --bin seed
    @echo "✅ Database seeded"

# Open PostgreSQL interactive terminal
db-shell:
    @docker exec -it hodei-jobs-postgres psql -U postgres -d hodei

# =============================================================================
# BUILD COMMANDS
# =============================================================================

# Build everything
build:
    @echo "🔨 Building project..."
    @just build-backend
    @just build-frontend
    @echo "✅ Build complete"

# Build backend
build-backend:
    @echo "🔨 Building Rust backend..."
    @cd crates/grpc && cargo build --release
    @echo "✅ Backend build complete"

# Build frontend
build-frontend:
    @echo "🔨 Building React frontend..."
    @cd web && npm run build
    @echo "✅ Frontend build complete"

# =============================================================================
# TEST COMMANDS
# =============================================================================

# Run all tests
test:
    @echo "🧪 Running all tests..."
    @just test-backend
    @just test-frontend
    @echo "✅ All tests passed"

# Run backend tests
test-backend:
    @echo "🧪 Running backend tests..."
    @cargo test --workspace
    @echo "✅ Backend tests passed"

# Run frontend tests
test-frontend:
    @echo "🧪 Running frontend tests..."
    @cd web && npm test -- --run
    @echo "✅ Frontend tests passed"

# Run E2E tests
test-e2e:
    @echo "🎭 Running E2E tests (Playwright)..."
    @cd web && npm run test:e2e
    @echo "✅ E2E tests passed"

# Run Docker Provider E2E tests (Rust)
test-docker-provider:
    @echo "🐳 Running Docker Provider E2E tests..."
    @./scripts/e2e/run-docker-e2e.sh
    @echo "✅ Docker Provider E2E tests passed"

# Test coverage
test-coverage:
    @echo "📊 Running test coverage..."
    @cargo tarpaulin --workspace --out html
    @cd web && npm run test:coverage
    @echo "📊 Coverage report generated"

# =============================================================================
# CODE QUALITY
# =============================================================================

# Lint and format code
check:
    @echo "🔍 Running code quality checks..."
    @just lint
    @just format

# Lint code
lint:
    @echo "🔍 Linting code..."
    @cargo clippy --all-targets --all-features -- -D warnings
    @cd web && npm run lint
    @echo "✅ Linting complete"

# Format code
format:
    @echo "✨ Formatting code..."
    @cargo fmt --all
    @cd web && npx prettier --write "**/*.{ts,tsx,css,md}"
    @echo "✅ Formatting complete"

# Type check
typecheck:
    @echo "🔍 Type checking..."
    @cd web && npm run typecheck
    @echo "✅ Type checking complete"

# =============================================================================
# DEVELOPMENT TOOLS
# =============================================================================

# Install development tools
install-tools:
    @echo "🛠️  Installing development tools..."
    @cargo install just bacon cargo-expand
    @npm install -g @bufbuild/buf
    @echo "✅ Development tools installed"

# Generate protobuf code
generate:
    @echo "📝 Generating protobuf code..."
    @cd web && npm run generate
    @cargo build --package hodei-jobs-proto
    @echo "✅ Code generation complete"

# Clean build artifacts
clean:
    @echo "🧹 Cleaning build artifacts..."
    @cargo clean
    @cd web && rm -rf node_modules dist
    @docker system prune -f
    @echo "✅ Clean complete"

# Reset everything (WARNING: destructive!)
clean-all: clean
    @echo "🗑️  Removing all development data..."
    @docker compose -f docker-compose.dev.yml down -v
    @docker system prune -af
    @echo "⚠️  All data removed. Run 'just dev' to restart."

# =============================================================================
# PRODUCTION
# =============================================================================

# Build production image
prod-build:
    @echo "🏗️  Building production image..."
    @docker build -t hodei-jobs:latest .
    @echo "✅ Production image built"

# Start production environment
prod-up:
    @echo "🚀 Starting production environment..."
    @docker compose -f docker-compose.prod.yml up -d
    @echo "✅ Production environment started"

# Stop production environment
prod-down:
    @echo "⏹️  Stopping production environment..."
    @docker compose -f docker-compose.prod.yml down
    @echo "✅ Production environment stopped"

# =============================================================================
# UTILITIES
# =============================================================================

# Show logs
logs:
    @docker compose -f docker-compose.dev.yml logs -f

# Backend logs
logs-backend:
    @docker compose -f docker-compose.dev.yml logs -f api

# Frontend logs
logs-frontend:
    @docker compose -f docker-compose.dev.yml logs -f web

# Database logs
logs-db:
    @docker compose -f docker-compose.dev.yml logs -f postgres

# Watch job logs (requires hodei-server running)
watch-logs:
    @./scripts/Monitoring_and_Debugging/watch_logs.sh

# =============================================================================
# JOB EXECUTION - Using hodei-jobs-cli
# =============================================================================

# Run a quick test job with live logs
test-job:
    @cargo run --bin hodei-jobs-cli -- job run --name "Test Job" --command "echo 'Hello from Hodei!'; sleep 1; echo 'Job completed successfully!'"

# Demo: Simple job
job-demo-simple:
    @cargo run --bin hodei-jobs-cli -- job run --name "Demo Job" --command "echo '=== HODEI DEMO JOB ==='; echo '[1/3] Processing data...'; sleep 1; echo '✓ Data processed'; echo '[2/3] Generating report...'; sleep 1; echo '✓ Report generated'; echo '[3/3] Complete!'; echo '=== JOB FINISHED ==='"

# Demo: ETL Pipeline
job-demo-etl:
    @cargo run --bin hodei-jobs-cli -- job run --name "ETL Demo" --command "echo '=== ETL DEMO ==='; echo '[1/4] Generating 10K records...'; sleep 1; echo '✓ 10,000 records'; echo '[2/4] Transforming data...'; sleep 1; echo '✓ Transformed'; echo '[3/4] Validating...'; sleep 1; echo '✓ Validated'; echo '[4/4] Generating report...'; sleep 1; echo '✓ ETL Complete'"

# Demo: ML Training
job-demo-ml:
    @cargo run --bin hodei-jobs-cli -- job run --name "ML Demo" --command "echo '=== ML TRAINING DEMO ==='; echo '[1/5] Loading dataset...'; sleep 1; echo '✓ 50K records loaded'; echo '[2/5] Training model...'; sleep 2; echo '✓ Model trained'; echo '[3/5] Evaluating...'; sleep 1; echo '✓ Accuracy: 86.8%'; echo '[4/5] Cross-validation...'; sleep 1; echo '✓ CV: 86.5%'; echo '[5/5] Saving model...'; sleep 1; echo '✓ Model saved'"

# Demo: Security Scan
job-demo-security:
    @cargo run --bin hodei-jobs-cli -- job run --name "Security Demo" --command "echo '=== SECURITY SCAN DEMO ==='; echo '[1/4] Scanning hosts...'; sleep 1; echo '✓ 50 hosts scanned'; echo '[2/4] Finding vulnerabilities...'; sleep 2; echo '✓ 105 vulnerabilities found'; echo '[3/4] Analyzing risks...'; sleep 1; echo '✓ Risk score: 6.2/10'; echo '[4/4] Generating report...'; sleep 1; echo '✓ Report ready'"

# Demo: Infrastructure
job-demo-infra:
    @cargo run --bin hodei-jobs-cli -- job run --name "Infra Demo" --command "echo '=== INFRASTRUCTURE DEMO ==='; echo '[1/5] Building Docker image...'; sleep 1; echo '✓ Image built (145MB)'; echo '[2/5] Running security scan...'; sleep 1; echo '✓ Scan passed'; echo '[3/5] Deploying to K8s...'; sleep 2; echo '✓ Deployed (3 pods)'; echo '[4/5] Configuring ingress...'; sleep 1; echo '✓ Ingress configured'; echo '[5/5] Health check...'; sleep 1; echo '✓ All healthy'"

# Demo: CI/CD Build
job-demo-cicd:
    @cargo run --bin hodei-jobs-cli -- job run --name "CICD Build" --command "echo '=== CI/CD BUILD ==='; echo '[1/5] Initializing...'; sleep 1; echo '✓ Ready'; echo '[2/5] Installing deps...'; sleep 2; echo '✓ 45 packages'; echo '[3/5] Linting...'; sleep 1; echo '✓ 0 errors'; echo '[4/5] Testing...'; sleep 2; echo '✓ 42/42 passed'; echo '[5/5] Building...'; sleep 2; echo '✓ Bundle: 1.2MB'"

# Run all demo jobs
job-demo-all:
    @echo "🎯 Running All Demo Jobs..."
    @just job-demo-simple
    @just job-demo-etl
    @just job-demo-ml
    @just job-demo-security
    @just job-demo-infra
    @just job-demo-cicd
    @echo "✅ All demo jobs completed!"

# =============================================================================
# PROFESSIONAL SCRIPTS - Self-contained worker tests
# =============================================================================

# CPU stress test - intensive computation
job-cpu-stress:
    @cargo run --bin hodei-jobs-cli -- job run --name "CPU Stress Test" --script scripts/examples/cpu-stress.sh

# File processing ETL pipeline
job-file-processing:
    @cargo run --bin hodei-jobs-cli -- job run --name "File Processing ETL" --script scripts/examples/file-processing.sh

# High-volume log generator - test log streaming
job-log-generator:
    @cargo run --bin hodei-jobs-cli -- job run --name "Log Generator" --script scripts/examples/log-generator.sh

# System information collector
job-system-info:
    @cargo run --bin hodei-jobs-cli -- job run --name "System Info" --script scripts/examples/system-info.sh

# Long running multi-phase job (~60s)
job-long-running:
    @cargo run --bin hodei-jobs-cli -- job run --name "Long Running Job" --script scripts/examples/long-running.sh

# Error scenarios - test error handling
job-error-scenarios:
    @cargo run --bin hodei-jobs-cli -- job run --name "Error Scenarios" --script scripts/examples/error-scenarios.sh

# Parallel tasks simulation
job-parallel-tasks:
    @cargo run --bin hodei-jobs-cli -- job run --name "Parallel Tasks" --script scripts/examples/parallel-tasks.sh

# Run all professional scripts
job-all-scripts:
    @echo "🎯 Running All Professional Scripts..."
    @just job-system-info
    @just job-log-generator
    @just job-file-processing
    @just job-error-scenarios
    @just job-parallel-tasks
    @just job-long-running
    @echo "✅ All scripts completed!"

# Run a script file as a job
# Usage: just job-script scripts/my-script.sh "My Job Name"
job-script script name="Script Job":
    @cargo run --bin hodei-jobs-cli -- job run --name "{{name}}" --script "{{script}}"

# Queue a job without streaming logs
job-queue name command:
    @cargo run --bin hodei-jobs-cli -- job queue --name "{{name}}" --command "{{command}}"

# List jobs
job-list:
    @cargo run --bin hodei-jobs-cli -- job list

# Get job details
job-get id:
    @cargo run --bin hodei-jobs-cli -- job get --id "{{id}}"

# Cancel a job
job-cancel id:
    @cargo run --bin hodei-jobs-cli -- job cancel --job-id "{{id}}"

# Follow logs for a job
job-logs id:
    @cargo run --bin hodei-jobs-cli -- logs follow --job-id "{{id}}"

# Get scheduler queue status
scheduler-status:
    @cargo run --bin hodei-jobs-cli -- scheduler queue-status

# List available workers
scheduler-workers:
    @cargo run --bin hodei-jobs-cli -- scheduler workers

# Generate mTLS certificates for Zero Trust security
cert-generate:
    @echo "🔐 Generating mTLS certificates..."
    @./scripts/Worker_Management/generate-certificates.sh
    @echo "✅ Certificates generated in certs/ directory"

# Rebuild worker Docker image and restart containers
rebuild-worker:
    @echo "🔨 Rebuilding worker Docker image..."
    @./scripts/Worker_Management/rebuild_worker.sh --restart
    @echo "✅ Worker rebuilt and restarted"

# Rebuild server Docker image and restart containers
rebuild-server:
    @echo "🔨 Rebuilding server Docker image..."
    @./scripts/Server_Management/rebuild_server.sh --restart
    @echo "✅ Server rebuilt and restarted"

# Rebuild both server and worker
rebuild-all:
    @echo "🔨 Rebuilding all Docker images..."
    @just rebuild-server
    @just rebuild-worker
    @echo "✅ All images rebuilt and restarted"

# Start server with Docker-in-Docker for worker provisioning
dev-server:
    @echo "🚀 Starting Hodei server with Docker-in-Docker support..."
    @docker compose -f docker-compose.dev.yml up -d api
    @just db-wait
    @echo "✅ Server ready at localhost:50051"

# Start full environment with auto-provisioning
dev-full:
    @echo "🚀 Starting full development environment with auto-provisioning..."
    @just dev-db
    @just dev-server
    @echo "✅ Full environment ready"

# Clean phantom workers from database
clean-workers:
    @echo "🧹 Cleaning phantom workers from database..."
    @docker exec hodei-jobs-postgres psql -U postgres -d hodei -c "DELETE FROM workers WHERE last_heartbeat < NOW() - INTERVAL '2 minutes';" || true
    @echo "✅ Phantom workers cleaned"

# Test worker auto-provisioning flow
test-auto-provision:
    @echo "🧪 Testing worker auto-provisioning..."
    @echo "This will:"
    @echo "  1. Check server is running"
    @echo "  2. Clean phantom workers"
    @echo "  3. Queue a test job"
    @echo "  4. Monitor worker provisioning and log streaming"
    @just status
    @just clean-workers
    @echo "📤 Queueing test job..."
    @cargo run --bin hodei-jobs-cli -- job queue --name "Auto-Provision Test" --command "echo 'Worker provisioned successfully!'"
    @echo "👀 Starting log monitor (Ctrl+C to stop)..."
    @just watch-logs

# Test improved shell execution (bash -c, pipes, env vars)
test-shell-features:
    @echo "🐚 Testing shell execution features..."
    @echo "  1. Testing pipes and redirections"
    @cargo run --bin hodei-jobs-cli -- job queue --name "Pipe Test" --command "echo 'test' | grep 'test'"
    @sleep 5
    @echo "  2. Testing environment variables"
    @cargo run --bin hodei-jobs-cli -- job queue --name "Env Test" --command "echo 'HOME: ' \$HOME"
    @sleep 5
    @echo "  3. Testing compound commands"
    @cargo run --bin hodei-jobs-cli -- job queue --name "Compound Test" --command "cd /tmp && pwd && ls -la | head -5"
    @sleep 5
    @echo "✅ Shell feature tests queued"

# Check system status
status:
    @echo "📊 System Status:"
    @echo "=================="
    @docker ps --filter "name=hodei" 2>/dev/null || echo "No containers running"
    @echo ""
    @echo "Workers in database:"
    @docker exec hodei-jobs-postgres psql -U postgres -d hodei -c "SELECT id, state, created_at FROM workers ORDER BY created_at DESC LIMIT 5;" 2>/dev/null || echo "Database not accessible"
    @echo ""
    @cargo --version
    @node --version 2>/dev/null || echo "Node not installed"
    @npm --version 2>/dev/null || echo "npm not installed"

help:
    @echo "Hodei Job Platform v8.0 - Development Commands"
    @echo "================================================"
    @echo ""
    @echo "Quick Start:"
    @echo "  just dev              Start full development environment"
    @echo "  just dev-db           Start database only"
    @echo ""
    @echo "Development:"
    @echo "  just dev-backend      Start backend with hot reload"
    @echo "  just dev-frontend     Start frontend with HMR"
    @echo "  just dev-test         Run tests in watch mode"
    @echo ""
    @echo "Database:"
    @echo "  just db-migrate       Run migrations"
    @echo "  just db-seed          Seed database"
    @echo "  just db-reset         Reset database (destructive!)"
    @echo "  just db-shell         Open database shell"
    @echo ""
    @echo "Testing:"
    @echo "  just test             Run all tests (277 tests)"
    @echo "  just test-backend     Run backend tests"
    @echo "  just test-frontend    Run frontend tests"
    @echo "  just test-e2e         Run E2E tests"
    @echo "  just maven-job        Run Maven job (simple)"
    @echo "  just maven-job-complex Run Maven job (complex with asdf)"
    @echo ""
    @echo "Code Quality:"
    @echo "  just check            Run lint and format"
    @echo "  just lint             Lint code"
    @echo "  just format           Format code"
    @echo "  just typecheck        Type check"
    @echo ""
    @echo "Certificates (v8.0):"
    @echo "  just cert-generate    Generate mTLS certificates for Zero Trust"
    @echo ""
    @echo "Worker Management:"
    @echo "  just rebuild-worker   Rebuild worker image with latest code"
    @echo ""
    @echo "Utilities:"
    @echo "  just status           Show system status"
    @echo "  just logs             Show all logs"
    @echo "  just watch-logs       Monitor job logs in real-time"
    @echo "  just clean            Clean build artifacts"
    @echo "  just clean-all        Clean everything (destructive!)"
    @echo ""
    @echo "Worker Agent v8.0 Features:"
    @echo "  - LogBatching (90-99% gRPC overhead reduction)"
    @echo "  - Zero-Copy I/O (memory efficient)"
    @echo "  - Secret Injection (stdin, JSON)"
    @echo "  - mTLS/Zero Trust (PKI infrastructure)"
    @echo "  - Cached Metrics (35s TTL, non-blocking)"
    @echo "  - Write-Execute Pattern (Jenkins/K8s style)"
    @echo "  - Backpressure Handling (try_send)"
    @echo ""
    @echo "For more information: https://github.com/your-org/hodei-job-platform"

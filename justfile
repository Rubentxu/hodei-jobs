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
    @./scripts/watch_logs.sh

# Check system status
status:
    @echo "📊 System Status:"
    @echo "=================="
    @docker ps --filter "name=hodei" || echo "No containers running"
    @echo ""
    @cargo --version
    @node --version 2>/dev/null || echo "Node not installed"
    @npm --version 2>/devnpm not installed"


help:
    @echo "Hodei Job Platform - Development Commands"
    @echo "=========================================="
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
    @echo "  just test             Run all tests"
    @echo "  just test-backend     Run backend tests"
    @echo "  just test-frontend    Run frontend tests"
    @echo "  just test-e2e         Run E2E tests"
    @echo ""
    @echo "Code Quality:"
    @echo "  just check            Run lint and format"
    @echo "  just lint             Lint code"
    @echo "  just format           Format code"
    @echo "  just typecheck        Type check"
    @echo ""
    @echo "Utilities:"
    @echo "  just status           Show system status"
    @echo "  just logs             Show all logs"
    @echo "  just clean            Clean build artifacts"
    @echo "  just clean-all        Clean everything (destructive!)"
    @echo ""
    @echo "For more information: https://github.com/your-org/hodei-job-platform"

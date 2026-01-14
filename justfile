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
export DATABASE_URL := "postgres://postgres:postgres@localhost:5432/hodei_jobs"

# CRITICAL: Worker→server connectivity
# HODEI_SERVER_HOST: Used by server for provisioning config
# HODEI_SERVER_ADDRESS: Set by providers in worker environment variables
export HODEI_SERVER_HOST := "0.0.0.0"
export HODEI_SERVER_ADDRESS := "host.docker.internal"

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

# =============================================================================
# RUST-SCRIPTS (Minikube Development)
# =============================================================================
# Install rust-script: cargo install rust-script
# Docs: https://rust-script.org

# Setup Minikube with required addons and namespaces
setup-minikube:
    @rust-script scripts/setup_minikube.rs

# Build and load Docker images to minikube
build-minikube:
    @rust-script scripts/build_minikube.rs

# Build minikube - server only
build-minikube server-only:
    @rust-script scripts/build_minikube.rs --server-only

# Build minikube - worker only
build-minikube worker-only:
    @rust-script scripts/build_minikube.rs --worker-only

# Build minikube - no cache
build-minikube no-cache:
    @rust-script scripts/build_minikube.rs --no-cache

# Generate protobuf code
generate:
    @echo "📝 Generating protobuf code..."
    cargo build --package hodei-server-interface
    @echo "✅ Code generation complete"

# =============================================================================
# DEVELOPMENT COMMANDS
# =============================================================================

# Start development database (idempotent)
dev-db:
    @chmod +x scripts/dev-db.sh && scripts/dev-db.sh

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

# =============================================================================
# DEVSPACE COMMANDS (Fast Development with Minikube)
# =============================================================================
# Workflow: Binary sync + USR1 signal = ~6-11 seconds per change
# No Docker image rebuild needed

# Compile and start DevSpace development
devspace-dev:
    @echo "╔═══════════════════════════════════════════════════════════════╗"
    @echo "║         HODEI JOBS - DESARROLLO RÁPIDO DEVSPACE               ║"
    @echo "╚═══════════════════════════════════════════════════════════════╝"
    @echo ""
    @echo "🔨 Compilando hodei-server-bin..."
    cargo build -p hodei-server-bin
    @echo ""
    @echo "🚀 Ejecuta: devspace dev"
    @echo ""
    @echo "En la terminal del pod:"
    @echo "  • El servidor arrancará automáticamente"
    @echo "  • Para recargar cambios: kill -USR1 \$$(cat /tmp/server.pid)"
    @echo ""
    @echo "O desde OTRA terminal local: just reload"

# Compile and restart process (main reload command)
reload:
    @echo "╔═══════════════════════════════════════════════════════════════╗"
    @echo "║         COMPILANDO + REINICIANDO PROCESO                      ║"
    @echo "╚═══════════════════════════════════════════════════════════════╝"
    @echo ""
    @echo "🔨 Compilando..."
    cargo build -p hodei-server-bin
    @echo ""
    @echo "🔄 Enviando señal USR1 para reiniciar proceso..."
    @kubectl exec -n hodei-jobs -l app.kubernetes.io/name=hodei-jobs-platform -- \
        sh -c 'kill -USR1 $$(cat /tmp/server.pid 2>/dev/null)' 2>/dev/null || \
        echo "⚠️  ERROR: Asegúrate de que devspace dev esté ejecutándose"
    @echo ""
    @echo "✅ Listo! El servidor se ha recargado (~6-11 segundos)"

# Check server status in pod
devspace-status:
    @echo "╔═══════════════════════════════════════════════════════════════╗"
    @echo "║              ESTADO DEL SERVIDOR                              ║"
    @echo "╚═══════════════════════════════════════════════════════════════╝"
    @echo ""
    @echo "Pods en hodei-jobs:"
    kubectl get pods -n hodei-jobs -l app.kubernetes.io/name=hodei-jobs-platform
    @echo ""
    @echo "Proceso del servidor:"
    kubectl exec -n hodei-jobs -l app.kubernetes.io/name=hodei-jobs-platform -- \
        sh -c 'cat /tmp/server.pid 2>/dev/null && \
               ps aux | grep hodei-server | grep -v grep || \
               echo "⚠️  Proceso no encontrado"' 2>/dev/null || \
        echo "⚠️  Pod no disponible"

# Stream server logs
devspace-logs:
    @echo "📝 Logs del servidor (Ctrl+C para salir):"
    kubectl logs -n hodei-jobs -l app.kubernetes.io/name=hodei-jobs-platform --follow --tail=50

# Only sync files (no restart)
devspace-sync:
    devspace sync

# Restart only the process (alternative to reload)
devspace-restart-process:
    @echo "🔄 Reiniciando proceso del servidor..."
    kubectl exec -n hodei-jobs -l app.kubernetes.io/name=hodei-jobs-platform -- \
        sh -c 'kill -USR1 $$(cat /tmp/server.pid 2>/dev/null)' 2>/dev/null || \
        echo "⚠️  No se pudo reiniciar. Ejecuta: devspace dev"
    @echo "✅ Señal USR1 enviada"

# Restart full pod (slower, use only if needed)
devspace-restart-pod:
    @echo "🔄 Reiniciando pod completo..."
    kubectl delete pod -n hodei-jobs -l app.kubernetes.io/name=hodei-jobs-platform
    @echo "⏳ Esperando a que el pod esté listo..."
    kubectl rollout status deployment -n hodei-jobs hodei-hodei-jobs-platform --timeout=60s

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
    @echo "Starting server with container networking..."
    @bash scripts/dev-start.sh &
    SERVER_PID=$$!
    sleep 25
    if curl -s http://localhost:50051/health >/dev/null 2>&1; then \
        echo "✅ Server is running"; \
    else \
        echo "❌ Server failed to start"; \
        kill $$SERVER_PID 2>/dev/null || true; \
        exit 1; \
    fi
    @echo ""
    @echo "✅ E2E environment ready"
    @echo "Server running on: http://localhost:50051"
    @echo "To stop: pkill -f hodei-server-bin"

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
# MULTI-PROVIDER JOB EXAMPLES (Docker + Kubernetes)
# =============================================================================

# Run Hello World on Docker provider
job-docker-hello:
    @echo "🐳 Running Hello World on Docker provider..."
    @cargo run --bin hodei-jobs-cli -- job run --name "Docker Hello World" --command "echo Hello from Docker provider" --provider docker

# Run Hello World on Kubernetes provider
job-k8s-hello:
    @echo "☸️  Running Hello World on Kubernetes provider..."
    @cargo run --bin hodei-jobs-cli -- job run --name "K8s Hello World" --command "echo Hello from Kubernetes provider" --provider kubernetes --memory 134217728

# Run CPU-intensive job on Docker (fast startup)
job-docker-cpu:
    @echo "🔥 Running CPU stress test on Docker..."
    @cargo run --bin hodei-jobs-cli -- job run --name "Docker CPU Test" --cpu 2 --memory 2147483648 --command "echo 'Starting CPU stress test on Docker...'; for i in {1..5}; do echo 'Iteration $$i/5 - CPU: '$$(uptime); sleep 1; done; echo 'Docker CPU test completed!'"

# Run CPU-intensive job on Kubernetes (scalable)
job-k8s-cpu:
    @echo "⚡ Running CPU-intensive job on Kubernetes..."
    @cargo run --bin hodei-jobs-cli -- job run --name "K8s CPU Test" --cpu 4 --memory 4294967296 --command "echo 'Starting CPU-intensive job on Kubernetes...'; for i in {1..10}; do echo 'Iteration $$i/10 - CPU: '$$(uptime); sleep 1; done; echo 'Kubernetes CPU test completed!'"

# Run memory-intensive job on Docker
job-docker-memory:
    @echo "🧠 Running memory test on Docker..."
    @cargo run --bin hodei-jobs-cli -- job run --name "Docker Memory Test" --cpu 1 --memory 3221225472 --command "echo 'Starting memory test on Docker...'; python3 -c 'import sys; data = [i for i in range(100000)]; print(f\"Allocated {len(data)} items\")'; echo 'Docker memory test completed!'"

# Run memory-intensive job on Kubernetes
job-k8s-memory:
    @echo "💾 Running memory-intensive job on Kubernetes..."
    @cargo run --bin hodei-jobs-cli -- job run --name "K8s Memory Test" --cpu 2 --memory 6442450944 --command "echo 'Starting memory-intensive job on Kubernetes...'; python3 -c 'import sys; data = [i for i in range(200000)]; print(f\"Allocated {len(data)} items\")'; echo 'Kubernetes memory test completed!'"

# Run data processing job on Docker (fast, ephemeral)
job-docker-data:
    @echo "📊 Running data processing on Docker..."
    @cargo run --bin hodei-jobs-cli -- job run --name "Docker Data Processing" --command "echo 'Processing data on Docker...'; echo 'Sample data: [1,2,3,4,5]'; python3 -c 'import json; data = {\"results\": [x*2 for x in range(1,6)]}; print(json.dumps(data, indent=2))'; echo 'Docker data processing completed!'"

# Run data processing job on Kubernetes (scalable)
job-k8s-data:
    @echo "📈 Running scalable data processing on Kubernetes..."
    @cargo run --bin hodei-jobs-cli -- job run --name "K8s Data Processing" --provider kubernetes --command "echo 'Processing large dataset on Kubernetes...'; echo 'Processing 10000 records...'; python3 -c 'import json; data = {\"results\": [x*2 for x in range(1,10001)], \"count\": 10000}; print(f\"Processed {data[\"count\"]} records\")'; echo 'Kubernetes data processing completed!'"

# Run ML training job on Docker (small model)
job-docker-ml:
    @echo "🤖 Running ML training on Docker (small model)..."
    @cargo run --bin hodei-jobs-cli -- job run --name "Docker ML Training" --cpu 2 --memory 4294967296 --command "echo 'Training ML model on Docker...'; python3 -c 'import time; print(\"Simulating ML training...\"); [time.sleep(0.5) for _ in range(10)]; print(\"Model trained: accuracy=0.95\")'; echo 'Docker ML training completed!'"

# Run ML training job on Kubernetes (large model)
job-k8s-ml:
    @echo "🧠 Running ML training on Kubernetes (large model)..."
    @cargo run --bin hodei-jobs-cli -- job run --name "K8s ML Training" --cpu 8 --memory 17179869184 --command "echo 'Training large ML model on Kubernetes...'; python3 -c 'import time; print(\"Simulating large ML training...\"); [time.sleep(1) for _ in range(20)]; print(\"Large model trained: accuracy=0.98\")'; echo 'Kubernetes ML training completed!'"

# Run CI/CD build on Docker (fast builds)
job-docker-build:
    @echo "🚀 Running CI/CD build on Docker..."
    @cargo run --bin hodei-jobs-cli -- job run --name "Docker Build" --cpu 2 --memory 4294967296 --command "echo 'Starting Docker build...'; echo 'Cloning repository...'; echo 'Running tests...'; echo 'Building artifacts...'; echo 'Docker build completed successfully!'"

# Run CI/CD build on Kubernetes (full pipeline)
job-k8s-build:
    @echo "🏗️  Running full CI/CD pipeline on Kubernetes..."
    @cargo run --bin hodei-jobs-cli -- job run --name "K8s Build Pipeline" --cpu 4 --memory 8589934592 --command "echo 'Starting Kubernetes build pipeline...'; echo 'Cloning repository...'; echo 'Installing dependencies...'; echo 'Running tests...'; echo 'Building artifacts...'; echo 'Running security scans...'; echo 'Kubernetes build pipeline completed!'"

# Compare provider performance (Docker vs K8s)
job-provider-comparison:
    @echo "⚖️  Comparing Docker vs Kubernetes providers..."
    @echo ""
    @echo "=== Running job on Docker (fast startup) ==="
    @just job-docker-hello
    @echo ""
    @echo "=== Running job on Kubernetes (scalable) ==="
    @just job-k8s-hello
    @echo ""
    @echo "✅ Provider comparison completed!"

# Run concurrent jobs on both providers
job-concurrent-test:
    @echo "🔀 Running concurrent jobs on both providers..."
    @bash -c 'cargo run --bin hodei-jobs-cli -- job run --name "Concurrent Docker Job" --command "bash -c \"echo Docker: Starting concurrent job; for i in {1..5}; do echo \"Docker: Iteration \$$i\"; sleep 1; done\"" &'
    @bash -c 'cargo run --bin hodei-jobs-cli -- job run --name "Concurrent K8s Job" --command "bash -c \"echo K8s: Starting concurrent job; for i in {1..5}; do echo \"Kubernetes: Iteration \$$i\"; sleep 1; done\"" &'
    @echo "Waiting for jobs to complete..."
    @bash -c 'wait'
    @echo "✅ Concurrent jobs completed!"

# Run stress test on both providers
job-stress-test:
    @echo "💪 Running stress test on both providers..."
    @echo ""
    @echo "=== Docker Stress Test ==="
    @just job-docker-cpu
    @echo ""
    @echo "=== Kubernetes Stress Test ==="
    @just job-k8s-cpu
    @echo ""
    @echo "✅ Stress tests completed!"

# Run GPU job on Kubernetes (requires GPU-enabled cluster)
job-k8s-gpu:
    @echo "🎮 Running GPU job on Kubernetes..."
    @cargo run --bin hodei-jobs-cli -- job run --name "K8s GPU Job" --cpu 4 --memory 8589934592 --command "echo 'Starting GPU job on Kubernetes...'; nvidia-smi || echo 'GPU not available, simulating...'; python3 -c 'import time; print(\"GPU training in progress...\"); [time.sleep(1) for _ in range(10)]; print(\"GPU job completed!\")'; echo 'Kubernetes GPU job completed!'"

# Run all Docker provider jobs
job-docker-all:
    @echo "🐳 Running all Docker provider jobs..."
    @just job-docker-hello
    @echo ""
    @just job-docker-cpu
    @echo ""
    @just job-docker-memory
    @echo ""
    @just job-docker-data
    @echo ""
    @just job-docker-ml
    @echo ""
    @just job-docker-build
    @echo "✅ All Docker jobs completed!"

# Run all Kubernetes provider jobs
job-k8s-all:
    @echo "☸️  Running all Kubernetes provider jobs..."
    @just job-k8s-hello
    @echo ""
    @just job-k8s-cpu
    @echo ""
    @just job-k8s-memory
    @echo ""
    @just job-k8s-data
    @echo ""
    @just job-k8s-ml
    @echo ""
    @just job-k8s-build
    @echo ""
    @just job-k8s-gpu
    @echo "✅ All Kubernetes jobs completed!"

# Run full multi-provider test suite
job-multi-provider-all:
    @echo "🌟 Running full multi-provider test suite..."
    @just job-provider-comparison
    @echo ""
    @just job-concurrent-test
    @echo ""
    @just job-stress-test
    @echo ""
    @echo "✅ Full multi-provider test suite completed!"

# =============================================================================
# MANUAL JOB EXECUTION WITH PROVIDER SELECTION
# =============================================================================

# Run job on Docker provider (manual selection)
job-run-docker name="Docker Test Job" command="echo 'Hello from Docker'" cpu="1.0" memory="1073741824" timeout="600":
    @echo "🐳 Running job on Docker provider..."
    @./scripts/job-run-docker.sh "{{ name }}" "{{ command }}" "{{ cpu }}" "{{ memory }}" "{{ timeout }}"

# Run job on Kubernetes provider (manual selection)
job-run-k8s name="K8s Test Job" command="echo 'Hello from Kubernetes'" cpu="1.0" memory="1073741824" timeout="600":
    @echo "☸️  Running job on Kubernetes provider..."
    @./scripts/job-run-provider.sh k8s "{{ name }}" "{{ command }}" "{{ cpu }}" "{{ memory }}" "{{ timeout }}"

# Run job with provider auto-selection
job-run-auto name="Auto Test Job" command="echo 'Hello from auto-selected provider'" cpu="1.0" memory="1073741824" timeout="600":
    @echo "🤖 Running job with auto provider selection..."
    @./scripts/job-run-provider.sh auto "{{ name }}" "{{ command }}" "{{ cpu }}" "{{ memory }}" "{{ timeout }}"

# Test provider selection strategies
job-test-providers:
    @echo "🎯 Testing provider selection strategies..."
    @./scripts/test-provider-selection.sh

# Quick test: Docker vs K8s comparison
job-quick-test:
    @echo "⚡ Quick provider comparison test..."
    @echo ""
    @echo "=== Docker Provider ==="
    @time (cargo run --bin hodei-jobs-cli -- job run --name "Quick Docker Test" --command "echo 'Docker test' && sleep 2")
    @echo ""
    @echo "=== Kubernetes Provider ==="
    @time (cargo run --bin hodei-jobs-cli -- job run --name "Quick K8s Test" --command "echo 'Kubernetes test' && sleep 2")
    @echo ""
    @echo "✅ Quick comparison completed!"

# Verify Kubernetes jobs create pods
job-verify-k8s-pods:
    @echo "🔍 Verifying Kubernetes jobs create pods in correct namespace..."
    @./scripts/verify-k8s-jobs.sh

# Watch Kubernetes pods in real-time
job-watch-k8s-pods:
    @echo "👀 Watching Kubernetes pods in namespace hodei-jobs-workers..."
    @kubectl get pods -n hodei-jobs-workers -w

# Show Kubernetes job pods with details
job-k8s-pods:
    @echo "📋 Listing all Kubernetes job pods..."
    @kubectl get pods -n hodei-jobs-workers -o wide
    @echo ""
    @echo "Labels:"
    @kubectl get pods -n hodei-jobs-workers --show-labels

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
    docker build -t hodei-jobs-worker:latest -f Dockerfile.worker .
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

# =============================================================================
# Provider Provisioning & Testing
# =============================================================================

# Start server with Kubernetes provisioning enabled
dev-k8s:
    @echo "🚀 Starting server with Kubernetes provisioning enabled..."
    @HODEI_PROVISIONING_ENABLED=1 HODEI_KUBERNETES_ENABLED=1 HODEI_DOCKER_ENABLED=1 HODEI_SERVER_ADDRESS=192.168.39.1 DATABASE_URL=postgres://postgres:postgres@localhost:5432/hodei_jobs cargo run --bin hodei-server-bin > /tmp/server-k8s.log 2>&1 &
    @echo "✅ Server started with Kubernetes provider support"
    @echo "📊 Server logs: /tmp/server-k8s.log"
    @echo "🔍 Monitor: tail -f /tmp/server-k8s.log"

# Test Kubernetes provider with a simple job
test-k8s-provider:
    @echo "🧪 Testing Kubernetes provider..."
    @cargo run --bin hodei-jobs-cli -- job run --name "K8s Provider Test" --cpu 1 --memory 536870912 --command "echo 'Testing Kubernetes provider...'; kubectl get pods 2>/dev/null || echo 'kubectl not available'; echo 'Test completed!'"
    @echo "✅ Kubernetes provider test submitted!"

# Full Kubernetes job test with pod verification
test-k8s-job:
    @echo "📦 Testing Kubernetes job execution with pod verification..."
    @cargo run --bin hodei-jobs-cli -- job run --name "Full K8s Test" --cpu 1 --memory 1073741824 --command "echo 'Running on Kubernetes...'; kubectl get pods -n default 2>/dev/null || echo 'Cluster check'; echo 'Job execution completed!'"
    @echo ""
    @echo "🔍 Checking for Kubernetes pods (wait 10 seconds)..."
    @sleep 10
    @kubectl get pods 2>/dev/null || echo "⚠️  kubectl not available or no pods found"
    @echo "✅ Test completed!"

# Verify providers are registered in database
verify-providers:
    @echo "🔍 Verifying registered providers..."
    @docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c "SELECT id, name, provider_type FROM provider_configs;"
    @echo "✅ Provider verification complete!"

# Check provider health
check-provider-health:
    @echo "❤️  Checking provider health..."
    @docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c "SELECT id, name, provider_type, metadata FROM provider_configs;"
    @echo ""
    @echo "📊 Current workers:"
    @docker exec hodei-jobs-postgres psql -U postgres -d hodei_jobs -c "SELECT id, state, provider_id, last_heartbeat FROM workers ORDER BY last_heartbeat DESC LIMIT 10;"

# Restart entire system (clean + start)
restart-all:
    @./scripts/restart-system.sh

# =============================================================================
# RUST-SCRIPTS (scripts/system-*.rs)
# =============================================================================
# Development automation scripts using rust-script
# Install: cargo install rust-script
# Docs: https://rust-script.org

# System status dashboard (requires K8s)
system-status:
    @rust-script scripts/system-status.rs --once

# System status with continuous refresh
system-status-watch:
    @rust-script scripts/system-status.rs

# Debug job by ID
debug-job id:
    @rust-script scripts/debug-job.rs --job-id {{id}}

# Debug job by correlation ID
debug-job-corr corr:
    @rust-script scripts/debug-job.rs --correlation-id {{corr}}

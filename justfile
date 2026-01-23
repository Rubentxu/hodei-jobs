# =============================================================================
# Hodei Job Platform - Development Commands (Telepresence + k3s)
# =============================================================================
# Install: cargo install just
# Usage: just <command>
#
# Environment variables:
#   - Values below are defaults
#   - Create a .env file to override any value
#   - .env file is loaded automatically if it exists
# =============================================================================

# =============================================================================
# DEFAULT CONFIGURATION (just syntax, can be overridden by .env)
# =============================================================================

export RUST_BACKTRACE := "1"
export RUST_LOG := "info,saga_engine_core=debug,hodei_server_application=debug,hodei_server_infrastructure=debug"

# Database (via Telepresence to k8s PostgreSQL)
export HODEI_DATABASE_URL := "postgresql://postgres:postgres@hodei-hodei-jobs-platform-postgresql.hodei-jobs.svc.cluster.local:5432/hodei_jobs"

# NATS (via Telepresence to k8s NATS)
export HODEI_NATS_URL := "nats://hodei-hodei-jobs-platform-nats.hodei-jobs.svc.cluster.local:4222"

# Server Configuration (local server binds to 0.0.0.0 for Telepresence intercept)
export HODEI_GRPC_ADDRESS := "0.0.0.0:9090"

# Worker Pod Configuration (workers connect via cluster service)
# Note: When using k3s containerd direct import, use localhost/image:latest
export HODEI_WORKER_IMAGE := "localhost/hodei-jobs-worker:latest"
export HODEI_K8S_SERVICE_NAME := "hodei-server.hodei-jobs.svc.cluster.local"

# Provider Configuration
export HODEI_K8S_ENABLED := "1"
export HODEI_DOCKER_ENABLED := "0"
export HODEI_DEV_MODE := "1"

# Kubernetes Configuration
export HODEI_K8S_NAMESPACE := "hodei-jobs"
export KUBECONFIG := "/etc/rancher/k3s/k3s.yaml"

# Default CLI server URL
HODEI_SERVER_URL := "http://localhost:9090"

# =============================================================================
# BUILD COMMANDS
# =============================================================================

build:
    @echo "🔨 Building workspace..."
    cargo build --workspace

build-server:
    @echo "🔨 Building server..."
    cargo build --package hodei-server-bin

build-worker-image:
    @echo "🔨 Building worker image for k3s..."
    @echo "Building with podman..."
    podman build -f Dockerfile.worker -t localhost/hodei-jobs-worker:latest .
    @echo "✅ Image built successfully"

deploy-worker-to-k3s: build-worker-image
    @echo "🚀 Deploying worker image to k3s containerd..."
    @echo "Exporting image from podman..."
    podman save localhost/hodei-jobs-worker:latest | k3s ctr images import -
    @echo "✅ Image deployed to k3s containerd"
    @echo "📋 Verifying image..."
    k3s ctr images list | grep hodei-jobs-worker || echo "⚠️  Image not found in containerd"

# =============================================================================
# KUBERNETES SERVICES
# =============================================================================

deploy-services:
    @echo "╔═══════════════════════════════════════════════════════════════╗"
    @echo "║    DESPLIEGANDO SERVICIOS BASE (PostgreSQL + NATS)          ║"
    @echo "╚═══════════════════════════════════════════════════════════════╝"
    export KUBECONFIG="/etc/rancher/k3s/k3s.yaml" && \
    helm upgrade --install hodei ./deploy/hodei-jobs-platform \
        -n hodei-jobs \
        --create-namespace \
        -f ./deploy/hodei-jobs-platform/values.yaml \
        -f ./deploy/hodei-jobs-platform/values-dev.yaml \
        --set postgresql.enabled=true \
        --set nats.enabled=true \
        --set server.enabled=false \
        --set kubernetesProvider.enabled=false \
        --set operator.enabled=false \
        --set web.enabled=false \
        --set development.enabled=false \
        --wait --timeout 300s

# =============================================================================
# TELEPRESENCE COMMANDS
# =============================================================================

telepresence-connect:
    @./scripts/dev-telepresence.sh connect

telepresence-status:
    @./scripts/dev-telepresence.sh status

telepresence-start: deploy-services telepresence-connect
    @echo ""
    @echo "╔═══════════════════════════════════════════════════════════════╗"
    @echo "║          ✅ ¡ENTORNO DE DESARROLLO LISTO!                     ║"
    @echo "╚═══════════════════════════════════════════════════════════════╝"

# =============================================================================
# SERVER COMMANDS
# =============================================================================

server-start:
    @echo "╔═══════════════════════════════════════════════════════════════╗"
    @echo "║           INICIANDO SERVIDOR HODEI (Development)             ║"
    @echo "╚═══════════════════════════════════════════════════════════════╝"
    @echo ""
    @echo "📋 Configuración:"
    @echo "   HODEI_GRPC_ADDRESS:       {{HODEI_GRPC_ADDRESS}}"
    @echo "   HODEI_WORKER_IMAGE:       {{HODEI_WORKER_IMAGE}}"
    @echo "   HODEI_K8S_SERVICE_NAME:   {{HODEI_K8S_SERVICE_NAME}}"
    @echo "   HODEI_DATABASE_URL:       {{HODEI_DATABASE_URL}}"
    @echo "   HODEI_NATS_URL:           {{HODEI_NATS_URL}}"
    @echo ""
    @echo "💡 Para probar: just job-k8s-hello"
    @echo "🛑 Para detener: Ctrl+C"
    @echo "╔═══════════════════════════════════════════════════════════════╗"
    @echo ""
    RUST_LOG="$RUST_LOG" \
    HODEI_GRPC_ADDRESS="$HODEI_GRPC_ADDRESS" \
    HODEI_DATABASE_URL="$HODEI_DATABASE_URL" \
    HODEI_NATS_URL="$HODEI_NATS_URL" \
    HODEI_WORKER_IMAGE="$HODEI_WORKER_IMAGE" \
    HODEI_K8S_SERVICE_NAME="$HODEI_K8S_SERVICE_NAME" \
    HODEI_K8S_ENABLED="$HODEI_K8S_ENABLED" \
    HODEI_DOCKER_ENABLED="$HODEI_DOCKER_ENABLED" \
    HODEI_DEV_MODE="$HODEI_DEV_MODE" \
    ./target/debug/hodei-server-bin

server-start-bg:
    @echo "🚀 Iniciando servidor en background..."
    RUST_LOG="$RUST_LOG" \
    HODEI_GRPC_ADDRESS="$HODEI_GRPC_ADDRESS" \
    HODEI_DATABASE_URL="$HODEI_DATABASE_URL" \
    HODEI_NATS_URL="$HODEI_NATS_URL" \
    HODEI_WORKER_IMAGE="$HODEI_WORKER_IMAGE" \
    HODEI_K8S_SERVICE_NAME="$HODEI_K8S_SERVICE_NAME" \
    HODEI_K8S_ENABLED="$HODEI_K8S_ENABLED" \
    HODEI_DOCKER_ENABLED="$HODEI_DOCKER_ENABLED" \
    HODEI_DEV_MODE="$HODEI_DEV_MODE" \
    nohup ./target/debug/hodei-server-bin > /tmp/hodei-server.log 2>&1 & \
    echo "Server PID: $!" && \
    sleep 3 && \
    tail -20 /tmp/hodei-server.log

server-stop:
    @echo "🛑 Deteniendo servidor..."
    @pkill -f "hodei-server-bin" || echo "No server running"
    @echo "✅ Servidor detenido"

server-logs:
    @tail -50 /tmp/hodei-server.log

server-logs-error:
    @grep -i "error\|failed\|panic" /tmp/hodei-server.log | tail -20 || echo "No errors found"

# =============================================================================
# HOT RELOAD
# =============================================================================

dev-hotreload:
    @./scripts/dev-hotreload.sh

# =============================================================================
# JOB TESTING COMMANDS
# =============================================================================

job-k8s-hello:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "🚀 Lanzando job hello-world..."
    echo "💡 Servidor: http://localhost:9090 (desarrollo local)"
    echo "---"
    export RUST_LOG="{{RUST_LOG}}"
    export HODEI_GRPC_ADDRESS="{{HODEI_GRPC_ADDRESS}}"
    export HODEI_DATABASE_URL="{{HODEI_DATABASE_URL}}"
    export HODEI_NATS_URL="{{HODEI_NATS_URL}}"
    export HODEI_WORKER_IMAGE="{{HODEI_WORKER_IMAGE}}"
    export HODEI_K8S_SERVICE_NAME="{{HODEI_K8S_SERVICE_NAME}}"
    export HODEI_K8S_ENABLED="{{HODEI_K8S_ENABLED}}"
    export HODEI_DOCKER_ENABLED="{{HODEI_DOCKER_ENABLED}}"
    export HODEI_DEV_MODE="{{HODEI_DEV_MODE}}"
    export HODEI_SERVER_URL="{{HODEI_SERVER_URL}}"
    exec cargo run -p hodei-jobs-cli -- job run \
        --name "hello-$(date +%s)" \
        --command "/bin/sh -c 'echo Hello from Hodei Jobs!; sleep 2; echo Done!'" \
        --cpu "0.1" \
        --memory "67108864" \
        --timeout "60" \
        --provider kubernetes \
        --server "$HODEI_SERVER_URL"

# =============================================================================
# UTILITY COMMANDS
# =============================================================================

k8s-events:
    @echo "📋 Eventos recientes en namespace hodei-jobs:"
    kubectl get events -n hodei-jobs --sort-by='.lastTimestamp' | tail -20

k8s-pods:
    @echo "🫛 Pods en namespace hodei-jobs:"
    kubectl get pods -n hodei-jobs -o wide

k8s-jobs:
    @echo "💼 Jobs en namespace hodei-jobs-workers:"
    kubectl get jobs -n hodei-jobs-workers -o wide

show-env:
    @echo "📋 Variables de entorno actuales:"
    @echo ""
    @echo "RUST_LOG:              {{RUST_LOG}}"
    @echo "HODEI_GRPC_ADDRESS:    {{HODEI_GRPC_ADDRESS}}"
    @echo "HODEI_DATABASE_URL:    {{HODEI_DATABASE_URL}}"
    @echo "HODEI_NATS_URL:        {{HODEI_NATS_URL}}"
    @echo "HODEI_WORKER_IMAGE:    {{HODEI_WORKER_IMAGE}}"
    @echo "HODEI_K8S_SERVICE_NAME:{{HODEI_K8S_SERVICE_NAME}}"
    @echo "HODEI_SERVER_URL:      {{HODEI_SERVER_URL}}"
    @echo ""
    @echo "💡 Para sobrescribir, crea un archivo .env"

# =============================================================================
# DEFAULT TARGET
# =============================================================================

_default:
    @echo "🚀 Hodei Job Platform - Development"
    @echo "======================================"
    @echo ""
    @echo "💡 Flujo de desarrollo:"
    @echo "  1. just deploy-services          # Desplegar servicios k8s"
    @echo "  2. just telepresence-connect     # Conectar Telepresence"
    @echo "  3. just telepresence-status      # Verificar conexión"
    @echo "  4. just build-server             # Compilar servidor"
    @echo "  5. just deploy-worker-to-k3s     # Construir y desplegar worker image"
    @echo "  6. just server-start             # Iniciar servidor"
    @echo "  7. just job-k8s-hello            # Probar job"
    @echo ""
    @echo "🔧 Comandos útiles:"
    @echo "  just build-worker-image          # Construir imagen worker (sin desplegar)"
    @echo "  just server-start-bg            # Iniciar en background"
    @echo "  just server-stop                # Detener servidor"
    @echo "  just server-logs                # Ver logs"
    @echo "  just server-logs-error          # Ver solo errores"
    @echo "  just show-env                   # Ver variables de entorno"
    @echo ""
    @echo "📝 Archivo .env:"
    @echo "  Crea un archivo .env para sobrescribir las variables por defecto"
    @echo ""
    @just --list

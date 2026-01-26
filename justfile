# =============================================================================
# Hodei Job Platform - Development Commands (CRC OpenShift v4 + Telepresence)
# =============================================================================
# Install: cargo install just
# Usage: just <command>
#
# Prerequisites:
#   1. CodeReady Containers (CRC) running with OpenShift cluster
#   2. Telepresence OSS v2 installed
#   3. oc CLI installed
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

# CRC OpenShift Configuration
export CRC_KUBECONFIG := "/home/rubentxu/.crc/machines/crc/kubeconfig"
export KUBECONFIG := CRC_KUBECONFIG
export HODEI_K8S_NAMESPACE := "hodei-jobs"

# Database (via Telepresence to OpenShift PostgreSQL - use FQDN)
export HODEI_DATABASE_URL := "postgresql://postgres:postgres@hodei-hodei-jobs-platform-postgresql.hodei-jobs.svc.cluster.local:5432/hodei_jobs"

# NATS (requires port-forward - use localhost due to Telepresence limitation with async_nats)
# Run: just nats-port-forward (or ./scripts/dev-setup.sh start)
export HODEI_NATS_URL := "nats://localhost:4222"

# Server Configuration (local server binds to 0.0.0.0 for Telepresence intercept)
export HODEI_GRPC_ADDRESS := "0.0.0.0:9090"

# Worker Pod Configuration (workers connect via cluster service)
# For OpenShift: use internal registry image
# After pushing: just push-worker-image
export HODEI_WORKER_IMAGE := "image-registry.openshift-image-registry.svc:5000/hodei-jobs/hodei-jobs-worker:latest"
export HODEI_K8S_SERVICE_NAME := "hodei-server.hodei-jobs.svc.cluster.local"

# Provider Configuration
export HODEI_K8S_ENABLED := "1"
export HODEI_DOCKER_ENABLED := "0"
export HODEI_DEV_MODE := "1"

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
    @echo "🔨 Building worker image..."
    podman build -f crates/worker/bin/Dockerfile -t hodei-jobs-worker:latest .
    @echo "✅ Image built: hodei-jobs-worker:latest"

# =============================================================================
# OPENSHIFT REGISTRY COMMANDS
# =============================================================================

# Push worker image to OpenShift internal registry
push-worker-image: build-worker-image
    @echo "🚀 Pushing worker image to OpenShift registry..."
    @echo "   Source: hodei-jobs-worker:latest"
    @echo "   Target: image-registry.openshift-image-registry.svc:5000/hodei-jobs/hodei-jobs-worker:latest"
    @echo ""
    @echo "💡 Ensure you're logged in to OpenShift registry:"
    @echo "   export REGISTRY_TOKEN=\$(kubectl get secret -n hodei-jobs hodei-jobs-worker-dockercfg -o jsonpath='{.data.*}' | base64 -d | jq -r '.auths[\"image-registry.openshift-image-registry.svc:5000\"].auth')"
    @echo "   podman login -u unused -p $REGISTRY_TOKEN image-registry.openshift-image-registry.svc:5000"
    @echo ""
    podman tag hodei-jobs-worker:latest image-registry.openshift-image-registry.svc:5000/hodei-jobs/hodei-jobs-worker:latest
    @echo "🏷️  Tagged successfully"
    podman push image-registry.openshift-image-registry.svc:5000/hodei-jobs/hodei-jobs-worker:latest --auth-file /tmp/openshift-registry-auth.json
    @echo "✅ Image pushed to OpenShift registry"
    @echo ""
    @echo "📋 Verifying image availability..."
    kubectl get images | grep hodei-jobs-worker || echo "⚠️  Image may still be propagating"

# Alias for push-worker-image
push-image: push-worker-image

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
# TELEPRESENCE + DEV-SETUP COMMANDS (CRC OpenShift)
# =============================================================================

# Connect Telepresence to CRC OpenShift
telepresence-connect:
    @./scripts/dev-telepresence.sh connect

# Check Telepresence status
telepresence-status:
    @./scripts/dev-telepresence.sh status

# Start NATS port-forward (required for async_nats to work through Telepresence)
nats-port-forward:
    @echo "🔀 Starting NATS port-forward (localhost:4222)..."
    @echo "   This is required because async_nats doesn't work well with Telepresence"
    @echo "   Press Ctrl+C to stop"
    @kubectl --kubeconfig=/home/rubentxu/.crc/machines/crc/kubeconfig \
        -n hodei-jobs port-forward svc/hodei-jobs-hodei-jobs-platform-nats 4222:4222

# Start NATS port-forward in background
nats-port-forward-bg:
    @echo "🔀 Starting NATS port-forward in background..."
    kubectl --kubeconfig=/home/rubentxu/.crc/machines/crc/kubeconfig \
        -n hodei-jobs port-forward svc/hodei-jobs-hodei-jobs-platform-nats 4222:4222 \
        > /tmp/nats-port-forward.log 2>&1 & \
        echo "NATS port-forward PID: $$!" && \
        sleep 2 && \
        tail -5 /tmp/nats-port-forward.log

# Stop NATS port-forward
nats-port-forward-stop:
    @echo "🛑 Stopping NATS port-forward..."
    @pkill -f "kubectl.*port-forward.*4222" || echo "No port-forward running"
    @echo "✅ NATS port-forward stopped"

# Full development environment setup (services + telepresence + port-forwards)
dev-setup:
    @echo "╔═══════════════════════════════════════════════════════════════╗"
    @echo "║    SETUP COMPLETO ENTORNO DESARROLLO (CRC OpenShift)         ║"
    @echo "╚═══════════════════════════════════════════════════════════════╝"
    @echo ""
    @echo "📋 Pasos a ejecutar:"
    @echo "   1. Conectar Telepresence"
    @echo "   2. Crear service account para workers"
    @echo "   3. Iniciar port-forward para NATS"
    @echo ""
    ./scripts/dev-setup.sh start

# Stop development environment
dev-setup-stop:
    @echo "🛑 Deteniendo entorno de desarrollo..."
    ./scripts/dev-setup.sh stop
    @echo "✅ Entorno detenido"

# Show development environment status
dev-setup-status:
    @echo "╔═══════════════════════════════════════════════════════════════╗"
    @echo "║          ESTADO ENTORNO DESARROLLO                           ║"
    @echo "╚═══════════════════════════════════════════════════════════════╝"
    @echo ""
    @echo "📋 Telepresence:"
    ./scripts/dev-telepresence.sh status || echo "❌ Telepresence no conectado"
    @echo ""
    @echo "📋 NATS port-forward:"
    @pgrep -f "kubectl.*port-forward.*4222" > /dev/null && echo "✅ NATS port-forward activo (localhost:4222)" || echo "❌ NATS port-forward no activo"
    @echo ""
    @echo "📋 Service Account:"
    kubectl --kubeconfig=/home/rubentxu/.crc/machines/crc/kubeconfig \
        get sa hodei-jobs-worker -n hodei-jobs-workers > /dev/null 2>&1 \
        && echo "✅ hodei-jobs-worker service account existe" \
        || echo "❌ hodei-jobs-worker service account no existe"
    @echo ""
    @echo "📋 kubectl context:"
    kubectl --kubeconfig=/home/rubentxu/.crc/machines/crc/kubeconfig \
        config current-context || echo "❌ No hay contexto activo"

# Quick start: Connect everything needed for development
dev-start: telepresence-connect dev-setup-status
    @echo ""
    @echo "╔═══════════════════════════════════════════════════════════════╗"
    @echo "║          ✅ ¡ENTORNO DE DESARROLLO LISTO!                     ║"
    @echo "╚═══════════════════════════════════════════════════════════════╝"
    @echo ""
    @echo "💡 Próximos pasos:"
    @echo "   1. just build-server              # Compilar servidor"
    @echo "   2. just push-worker-image         # Push imagen worker a OpenShift"
    @echo "   3. just server-start              # Iniciar servidor"
    @echo "   4. just job-k8s-hello             # Probar job"
    @echo ""
    @echo "💡 Verificar con: just dev-setup-status"

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
    kubectl --kubeconfig=/home/rubentxu/.crc/machines/crc/kubeconfig \
        get events -n hodei-jobs --sort-by='.lastTimestamp' | tail -20

k8s-pods:
    @echo "🫛 Pods en namespace hodei-jobs:"
    kubectl --kubeconfig=/home/rubentxu/.crc/machines/crc/kubeconfig \
        get pods -n hodei-jobs -o wide

k8s-jobs:
    @echo "💼 Jobs en namespace hodei-jobs-workers:"
    kubectl --kubeconfig=/home/rubentxu/.crc/machines/crc/kubeconfig \
        get jobs -n hodei-jobs-workers -o wide

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
    @echo "🚀 Hodei Job Platform - Development (CRC OpenShift v4 + Telepresence)"
    @echo "========================================================================"
    @echo ""
    @echo "💡 Flujo de desarrollo recomendado:"
    @echo "   1. just dev-start                 # Conectar Telepresence + verificar entorno"
    @echo "   2. just build-server              # Compilar servidor"
    @echo "   3. just push-worker-image         # Push imagen worker a OpenShift registry"
    @echo "   4. just server-start              # Iniciar servidor"
    @echo "   5. just job-k8s-hello             # Probar job"
    @echo ""
    @echo "🔧 Comandos de desarrollo:"
    @echo "  just dev-start              # Setup rápido: Telepresence + status"
    @echo "  just dev-setup              # Setup completo: Telepresence + SA + NATS port-forward"
    @echo "  just dev-setup-status       # Verificar estado del entorno"
    @echo "  just dev-setup-stop         # Detener entorno de desarrollo"
    @echo ""
    @echo "🔧 Comandos Telepresence:"
    @echo "  just telepresence-connect   # Conectar Telepresence"
    @echo "  just telepresence-status    # Ver estado Telepresence"
    @echo ""
    @echo "🔧 Comandos NATS:"
    @echo "  just nats-port-forward      # Iniciar port-forward (Ctrl+C para detener)"
    @echo "  just nats-port-forward-bg   # Iniciar port-forward en background"
    @echo "  just nats-port-forward-stop # Detener port-forward"
    @echo ""
    @echo "🔧 Comandos imagen worker:"
    @echo "  just build-worker-image     # Construir imagen worker"
    @echo "  just push-worker-image      # Push a OpenShift registry"
    @echo ""
    @echo "🔧 Comandos servidor:"
    @echo "  just build-server           # Compilar servidor"
    @echo "  just server-start           # Iniciar servidor"
    @echo "  just server-start-bg        # Iniciar en background"
    @echo "  just server-stop            # Detener servidor"
    @echo "  just server-logs            # Ver logs"
    @echo ""
    @echo "🔧 Comandos jobs:"
    @echo "  just job-k8s-hello          # Lanzar job hello-world"
    @echo ""
    @echo "🔧 Comandos Kubernetes:"
    @echo "  just k8s-pods               # Ver pods"
    @echo "  just k8s-jobs               # Ver jobs"
    @echo "  just k8s-events             # Ver eventos"
    @echo ""
    @echo "📝 Archivo .env:"
    @echo "  Crea un archivo .env para sobrescribir las variables por defecto"
    @echo ""
    @just --list

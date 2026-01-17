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
# RUST-SCRIPTS (k3s Development)
# =============================================================================
# Install rust-script: cargo install rust-script
# Docs: https://rust-script.org
#
# k3s is a lightweight Kubernetes that comes with containerd built-in.
# Installation: curl -sfL https://get.k3s.io | sh -
# Configure: export KUBECONFIG=/etc/rancher/k3s/k3s.yaml or copy to ~/.kube/config

# Setup k3s with required namespaces
setup-k3s:
    @rust-script scripts/setup_k3s.rs

# Build and load images to k3s containerd
build-k3s:
    @rust-script scripts/build_k3s.rs

# Build k3s - worker only
build-k3s-worker:
    @rust-script scripts/build_k3s.rs --worker-only

# Build k3s - no cache
build-k3s-no-cache:
    @rust-script scripts/build_k3s.rs --no-cache

# =============================================================================
# RUST-SCRIPTS (All Development Scripts)
# =============================================================================
# Install rust-script: cargo install rust-script
# Docs: https://rust-script.org

# Development database
dev-db:
    @rust-script scripts/dev_db.rs

# Development server
dev-server:
    @rust-script scripts/dev_server.rs

# Development start (full environment)
dev-start:
    @rust-script scripts/dev_start.rs

# Clean system
clean-system:
    @rust-script scripts/clean_system.rs

# Restart system
restart-system:
    @rust-script scripts/restart_system.rs

# System status dashboard
# Install: cargo install rust-script
build-local:
    @rust-script scripts/build_local.rs

# Build and push to registry
build-and-push:
    @rust-script scripts/build_and_push.rs

# =============================================================================
# DEBUG COMMANDS
# =============================================================================

# Debug jobs
debug-jobs:
    @rust-script scripts/debug_job.rs

# Job timeline
debug-jobs-timeline:
    @rust-script scripts/debug_job_timeline.rs

# Debug workers
debug-workers:
    @rust-script scripts/system_status.rs

# Worker logs (hint)
logs-worker-hint:
    @echo "💡 Run: docker logs -f hodei-worker"
    @echo "   Or: kubectl logs -n hodei-jobs -l app.kubernetes.io/name=hodei-worker"

# =============================================================================
# KUBERNETES COMMANDS
# =============================================================================

# Deploy base services (PostgreSQL + NATS) for local development
deploy-services:
    @echo "╔═══════════════════════════════════════════════════════════════╗"
    @echo "║    DESPLIEGANDO SERVICIOS BASE (PostgreSQL + NATS)          ║"
    @echo "╚═══════════════════════════════════════════════════════════════╝"
    @echo ""
    @echo "💡 Flujo de desarrollo completo:"
    @echo "   1. just deploy-services              # Este comando"
    @echo "   2. just telepresence-connect         # Conectar (primera vez: login en navegador)"
    @echo "   3. cargo build --release -p hodei-server-bin"
    @echo "   4. ./target/release/hodei-server-bin"
    @echo ""
    @echo "🛑 Para terminar:"
    @echo "   just telepresence-quit"
    @echo ""
    export KUBECONFIG=/etc/rancher/k3s/k3s.yaml && \
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

# Cleanup all K8s resources (for clean restart)
deploy-cleanup:
    @echo "🧹 Limpiando todos los recursos de hodei-jobs..."
    export KUBECONFIG=/etc/rancher/k3s/k3s.yaml
    kubectl delete deployment -n hodei-jobs --all 2>/dev/null || true
    kubectl delete pods -n hodei-jobs --all 2>/dev/null || true
    kubectl delete svc -n hodei-jobs --all 2>/dev/null || true
    kubectl delete pvc -n hodei-jobs --all 2>/dev/null || true
    @echo "✅ Namespace limpio"

# =============================================================================
# TELEPRESENCE COMMANDS (Local Development with k8s Services)
# =============================================================================
# FLUJO: Compila y ejecuta localmente, conecta a servicios k8s via Telepresence OSS
# Instala Telepresence desde GitHub (versión OSS - SIN autenticación cloud)
# Docs: docs/DEVELOPMENT_TELEPRESENCE.md
#
# 📌 NOTA IMPORTANTE - DNS de Kubernetes:
#    Telepresence permite que tu máquina acceda a servicios k8s por nombre:
#    • postgresql → resuelve a IP del pod
#    • nats → resuelve a IP del pod
#
# Flujo de desarrollo:
#   1. just deploy-services              # Desplegar PostgreSQL + NATS
#   2. just telepresence-connect         # Conectar (VPN mode)
#   3. ./scripts/dev-hotreload.sh        # Compilar + hot reload
#   4. just job-k8s-hello                # Probar jobs
#   5. just telepresence-quit            # Desconectar

# Conectar al cluster (VPN mode - versión OSS, sin cloud login)
telepresence-connect:
    @./scripts/dev-telepresence.sh connect

# Desconectar del cluster
telepresence-quit:
    @./scripts/dev-telepresence.sh quit

# Mostrar estado de conexión
telepresence-status:
    @./scripts/dev-telepresence.sh status

# Alternativa: Port-forward (más simple, sin login)
telepresence-port-forward:
    @./scripts/dev-telepresence.sh port-forward

# Detener port-forwards
telepresence-stop:
    @./scripts/dev-telepresence.sh stop

# Flujo completo: desplegar servicios + conectar
telepresence-start: deploy-services telepresence-connect
    @echo ""
    @echo "╔═══════════════════════════════════════════════════════════════╗"
    @echo "║          ✅ ¡ENTORNO DE DESARROLLO LISTO!                     ║"
    @echo "╚═══════════════════════════════════════════════════════════════╝"
    @echo ""
    @echo "📦 Servicios desplegados:"
    @echo "   • PostgreSQL (puerto 5432)"
    @echo "   • NATS (puerto 4222)"
    @echo ""
    @echo "🌐 Tu máquina está conectada al cluster"
    @echo ""
    @echo "💡 Próximos pasos:"
    @echo "   1. cargo build --release -p hodei-server-bin"
    @echo "   2. ./target/release/hodei-server-bin"
    @echo "   3. just job-k8s-hello"
    @echo ""
    @echo "🛑 Para terminar:"
    @echo "   just telepresence-quit"
    @echo ""

# Hot reload para desarrollo local (requiere telepresence-connect)
dev-hotreload:
    @./scripts/dev-hotreload.sh

# Configuración completa con Telepresence: instalar + desplegar + conectar + compilar + hot reload
dev-telepresence:
    @./scripts/dev-start.sh

# K8s workflow (build, load, deploy)
k8s-workflow:
    @rust-script scripts/k8s_workflow.rs

# Verify K8s jobs
verify-k8s-jobs:
    @rust-script scripts/verify_k8s_jobs.rs

# =============================================================================
# TEST COMMANDS
# =============================================================================

# Test multi-provider
test-multi-provider:
    @rust-script scripts/test_multi_provider.rs

# Test provider selection
test-provider-selection:
    @rust-script scripts/test_provider_selection.rs

# Test timeout
test-timeout:
    @rust-script scripts/test_timeout.rs

# =============================================================================
# DEVSPACE COMMANDS (Fast Development with Minikube)
# =============================================================================
# Workflow: Compile locally → sync to pod → reload with USR1
# Time per change: ~6-11 seconds (no Docker rebuild)

# Initialize development environment
dev-init:
    @echo "🚀 Initializing development environment..."
    @rust-script scripts/dev_workflow.rs init

# =============================================================================
# DEVSPACE - DESARROLLO COMPLETO CON MINIKUBE
# =============================================================================
# Workflow completo: deploy → sync → hotreload → cleanup automático
#
# Usage:
#   just devspace-dev   # Deploy + sync + terminal + cleanup (Ctrl+C)
#   just devspace-status # Ver estado del servidor
#   just devspace-logs  # Ver logs en tiempo real
#
# El chart se deploya al inicio, el código se sincroniza automáticamente,
# y al salir (Ctrl+C) los recursos se limpian automáticamente.
# =============================================================================

# Compile release and start DevSpace development (FULL WORKFLOW)
devspace-dev:
    @echo "╔═══════════════════════════════════════════════════════════════╗"
    @echo "║         HODEI JOBS - DESARROLLO RÁPIDO DEVSPACE               ║"
    @echo "╚═══════════════════════════════════════════════════════════════╝"
    @echo ""
    @echo "🚀 INICIANDO SESIÓN DE DESARROLLO..."
    @echo ""
    @echo "Este comando:"
    @echo "  1️⃣  Deploya el Helm chart con valores de desarrollo"
    @echo "  2️⃣  Sincroniza código automáticamente"
    @echo "  3️⃣  Abre terminal en el pod"
    @echo "  4️⃣  Limpia recursos al salir (Ctrl+C)"
    @echo ""
    @echo "📝 En la terminal del pod:"
    @echo "  • El servidor compilará y arrancará automáticamente"
    @echo "  • Edita archivos localmente - se sincronizan solos"
    @echo "  • Para recompilar: cargo build --release -p hodei-server-bin"
    @echo ""
    @echo "🛑 Para SALIR: Ctrl+C (los recursos se limpian automáticamente)"
    @echo ""
    KUBECONFIG=/etc/rancher/k3s/k3s.yaml devspace dev --namespace hodei-jobs

# Cleanup DevSpace + Docker space
devspace-cleanup-all:
    @echo "🧹 Limpiando recursos de desarrollo y Docker..."
    @echo ""
    @echo "📦 Limpiando recursos de DevSpace..."
    devspace purge --namespace hodei-jobs 2>/dev/null || true
    helm uninstall hodei -n hodei-jobs 2>/dev/null || true
    @echo ""
    @echo "🐳 Limpiando espacio Docker..."
    minikube ssh "docker system prune -af --volumes" 2>/dev/null || true
    @echo ""
    @echo "✅ Cleanup completo"

# =============================================================================
# HODEI-CLI COMMANDS - Job Testing
# =============================================================================
# Launch test jobs using hodei-cli (requires running server)
# Configure server URL via environment:
#   - Local: HODEI_SERVER_URL="http://localhost:9090" (DEFAULT - desarrollo local)
#   - k8s:   HODEI_SERVER_URL="http://hodei-hodei-jobs-platform.hodei-jobs.svc.cluster.local:9090"
HODEI_SERVER_URL := "http://localhost:9090"

# Job simple de prueba
job-k8s-hello:
    @echo "🚀 Lanzando job hello-world..."
    @echo "---"
    @echo "💡 Servidor: localhost:9090 (desarrollo local)"
    cargo run -p hodei-jobs-cli -- job run \
        --name "hello-$$(date +%s)" \
        --command "/bin/sh -c 'echo Hello from Hodei Jobs!; sleep 2; echo Done!'" \
        --cpu "0.1" \
        --memory "67108864" \
        --timeout "60" \
        --provider kubernetes \
        --server "{{HODEI_SERVER_URL}}" || \
    echo "⚠️  Verificar que hodei.local resuelva a la IP del ingress (192.168.1.232)"

# Test de CPU intensivo
job-k8s-cpu:
    @echo "🚀 Lanzando job CPU stress..."
    cargo run -p hodei-jobs-cli -- job run \
        --name "cpu-stress-$$(date +%s)" \
        --command "/bin/sh -c 'echo CPU Stress Test; for i in \$$(seq 1 10); do echo \$$i; done'" \
        --cpu "0.5" \
        --memory "134217728" \
        --timeout "120" \
        --provider kubernetes \
        --server "{{HODEI_SERVER_URL}}"

# Test de memoria
job-k8s-memory:
    @echo "🚀 Lanzando job memory test..."
    cargo run -p hodei-jobs-cli -- job run \
        --name "memory-test-$$(date +%s)" \
        --command "/bin/sh -c 'echo Memory Test; echo Allocating... && sleep 1 && echo Done'" \
        --cpu "0.2" \
        --memory "268435456" \
        --timeout "60" \
        --provider kubernetes \
        --server "{{HODEI_SERVER_URL}}"

# Test de datos
job-k8s-data:
    @echo "🚀 Lanzando job data processing..."
    cargo run -p hodei-jobs-cli -- job run \
        --name "data-proc-$$(date +%s)" \
        --command "/bin/sh -c 'echo Processing data...; seq 1 100 | while read n; do echo \$$n; done; echo Data processed!'" \
        --cpu "0.2" \
        --memory "134217728" \
        --timeout "120" \
        --provider kubernetes \
        --server "{{HODEI_SERVER_URL}}"

# Test de ML
job-k8s-ml:
    @echo "🚀 Lanzando job ML inference..."
    cargo run -p hodei-jobs-cli -- job run \
        --name "ml-inference-$$(date +%s)" \
        --command "/bin/sh -c 'echo ML Inference Test; echo Model loaded; sleep 1 && echo Inference complete'" \
        --cpu "1.0" \
        --memory "536870912" \
        --timeout "180" \
        --provider kubernetes \
        --server "{{HODEI_SERVER_URL}}"

# Test de CI/CD
job-k8s-build:
    @echo "🚀 Lanzando job build..."
    cargo run -p hodei-jobs-cli -- job run \
        --name "build-$$(date +%s)" \
        --command "/bin/sh -c 'echo Starting build...; echo Compiling...; sleep 1 && echo Build complete!'" \
        --cpu "0.5" \
        --memory "268435456" \
        --timeout "300" \
        --provider kubernetes \
        --server "{{HODEI_SERVER_URL}}"

# Test GPU (si disponible)
job-k8s-gpu:
    @echo "🚀 Lanzando job GPU test..."
    cargo run -p hodei-jobs-cli -- job run \
        --name "gpu-test-$$(date +%s)" \
        --command "/bin/sh -c 'echo GPU Test - Checking device...; nvidia-smi || echo No GPU available; echo Done'" \
        --cpu "0.2" \
        --memory "134217728" \
        --timeout "120" \
        --provider kubernetes \
        --server "{{HODEI_SERVER_URL}}"

# Ejecutar todos los jobs de K8s
job-k8s-all:
    @echo "╔═══════════════════════════════════════════════════════╗"
    @echo "║    EJECUTANDO SUITE COMPLETA DE JOBS K8S             ║"
    @echo "╚═══════════════════════════════════════════════════════╝"
    @echo ""
    just job-k8s-hello
    sleep 2
    just job-k8s-cpu
    sleep 2
    just job-k8s-memory
    sleep 2
    just job-k8s-data
    sleep 2
    just job-k8s-build
    @echo ""
    @echo "✅ Suite de jobs completada"

# Ver estado de jobs
job-status:
    @echo "📊 Estado de jobs en hodei-jobs-workers:"
    kubectl get jobs -n hodei-jobs-workers -o wide
    @echo ""
    @echo "🫛 Pods de jobs:"
    kubectl get pods -n hodei-jobs-workers -o wide

# Ver logs de un job específico
job-logs:
    @echo "📝 Logs del job (especifica el nombre):"
    @echo "  kubectl logs -n hodei-jobs-workers job/<job-name>"

# Limpiar jobs completados
job-cleanup:
    @echo "🧹 Limpiando jobs completados..."
    kubectl delete job -n hodei-jobs-workers --field-selector status.successful=1
    @echo "✅ Jobs completados eliminados"

# Ver estado del servidor
devspace-status:
    @echo "╔═══════════════════════════════════════════════════════════════╗"
    @echo "║              ESTADO DEL SERVIDOR                              ║"
    @echo "╚═══════════════════════════════════════════════════════════════╝"
    @echo ""
    @echo "📦 Deployments:"
    kubectl get deployments -n hodei-jobs -l app.kubernetes.io/name=hodei-jobs-platform
    @echo ""
    @echo "🫛 Pods:"
    kubectl get pods -n hodei-jobs -l app.kubernetes.io/name=hodei-jobs-platform
    @echo ""
    @echo "🔄 Proceso del servidor:"
    kubectl exec -n hodei-jobs -l app.kubernetes.io/name=hodei-jobs-platform -- \
        sh -c 'cat /tmp/server.pid 2>/dev/null && \
               ps aux | grep -E "hodei-server" | grep -v grep || \
               echo "⚠️  Proceso no encontrado"' 2>/dev/null || \
        echo "⚠️  Pod no disponible"

# Stream server logs
devspace-logs:
    @echo "📝 Logs del servidor (Ctrl+C para salir):"
    kubectl logs -n hodei-jobs -l app.kubernetes.io/name=hodei-jobs-platform --follow --tail=100

# Restart full pod (slow - use only if needed)
devspace-restart:
    @echo "🔄 Reiniciando pod completo..."
    kubectl delete pod -n hodei-jobs -l app.kubernetes.io/name=hodei-jobs-platform
    @echo "⏳ Esperando a que el pod esté listo..."
    kubectl rollout status deployment -n hodei-jobs hodei-hodei-jobs-platform --timeout=120s

# Deploy chart with development values (solo si no usas devspace dev)
deploy-dev:
    @echo "╔═══════════════════════════════════════════════════════════════╗"
    @echo "║    DEPLOY CHART CON VALORES DE DESARROLLO                    ║"
    @echo "╚═══════════════════════════════════════════════════════════════╝"
    @echo "⚠️  Nota: Usa 'just devspace-dev' para el workflow completo"
    helm upgrade --install hodei ./deploy/hodei-jobs-platform \
        --namespace hodei-jobs \
        --create-namespace \
        -f ./deploy/hodei-jobs-platform/values.yaml \
        -f ./deploy/hodei-jobs-platform/values-dev.yaml \
        --wait --timeout 300s

# Cleanup resources manually
devspace-cleanup:
    @echo "🧹 Limpiando recursos de desarrollo..."
    helm uninstall hodei -n hodei-jobs 2>/dev/null || true
    kubectl delete pvc -n hodei-jobs -l app.kubernetes.io/name=hodei-jobs-platform 2>/dev/null || true
    @echo "✅ Recursos limpiados"

# =============================================================================
# gRPC TESTING COMMANDS
# =============================================================================

# Test gRPC connection via port-forward (development)
grpc-test-portforward:
    @echo "🔌 Iniciando port-forward para gRPC..."
    @echo "💡 En otra terminal ejecuta: grpcurl -plaintext localhost:9090 list"
    kubectl port-forward -n hodei-jobs svc/hodei-hodei-jobs-platform 9090:9090

# Test gRPC connection via ingress (requires TLS certificate)
grpc-test-ingress:
    @echo "🔌 Testing gRPC via NGINX Ingress..."
    @echo "💡 gRPC endpoint: https://hodei.local:443"
    @echo "💡 Con certificado autofirmado usa:"
    @echo "   grpcurl -insecure hodei.local:443 hodei.JobExecutionService/QueueJob"
    @grpcurl -insecure hodei.local:443 list 2>&1 || echo "⚠️  Verificar que hodei.local resuelva a la IP del ingress"

# Install NGINX Ingress for gRPC (required for production gRPC)
install-nginx-ingress:
    @echo "🔌 Instalando NGINX Ingress Controller..."
    arkade install ingress-nginx --kubeconfig=/etc/rancher/k3s/k3s.yaml

# Create TLS certificate for gRPC development
create-grpc-tls:
    @echo "🔐 Creando certificado TLS para gRPC..."
    openssl req -x509 -newkey rsa:2048 -keyout /tmp/tls.key -out /tmp/tls.crt -days 365 -nodes \
        -subj "/CN=hodei.local" \
        -addext "subjectAltName=DNS:hodei.local,DNS:*.hodei.local,IP:127.0.0.1"
    kubectl create secret tls hodei-tls-secret --cert=/tmp/tls.crt --key=/tmp/tls.key -n hodei-jobs
    @echo "✅ Certificado TLS creado"

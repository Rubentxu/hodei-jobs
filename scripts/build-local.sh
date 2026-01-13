#!/bin/bash
# =============================================================================
# Hodei Build & Push - Local Registry Version
# =============================================================================
# Compila y sube imágenes al registry local de minikube
# Mucho más rápido para desarrollo que usar registries externos
#
# Usage:
#   ./scripts/build-local.sh    # Compilar y desplegar
# =============================================================================

set -e

# Usar registry local de minikube
REGISTRY="${REGISTRY:-localhost:5000}"
SERVER_IMAGE="${REGISTRY}/hodei-server"
WORKER_IMAGE="${REGISTRY}/hodei-worker"

NAMESPACE="hodei-system"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colores
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log_info() { echo -e "${BLUE}ℹ️  $1${NC}"; }
log_success() { echo -e "${GREEN}✅ $1${NC}"; }
log_warn() { echo -e "${YELLOW}⚠️  $1${NC}"; }
log_error() { echo -e "${RED}❌ $1${NC}"; }
log_build() { echo -e "${CYAN}🔨 $1${NC}"; }

# Verificar registry local
check_registry() {
    log_info "Verificando registry local..."

    # Intentar conectar al registry
    if curl -s "http://${REGISTRY}/v2/" &>/dev/null; then
        log_success "Registry disponible: $REGISTRY"
        return 0
    fi

    log_warn "Registry no disponible, iniciando..."
    # El registry debería estar ejecutándose en el puerto 5000
    # Esto se puede hacer con: docker run -d -p 5000:5000 --name registry registry:2

    # Verificar si minikube tiene registry
    if command -v minikube &>/dev/null; then
        minikube addons enable registry 2>/dev/null || true
        minikube addons enable registry-aliases 2>/dev/null || true
    fi
}

# Compilar binarios
build_binaries() {
    log_build "Compilando binarios..."

    cd "$PROJECT_ROOT"

    # Compilar servidor
    log_info "Compilando servidor..."
    cargo build --release -p hodei-server-bin 2>&1 | tail -3

    # Compilar worker
    log_info "Compilando worker..."
    cargo build --release -p hodei-worker-bin 2>&1 | tail -3

    log_success "Binarios compilados"
}

# Construir imágenes Docker
build_images() {
    log_build "Construyendo imágenes Docker..."

    cd "$PROJECT_ROOT"

    # Imagen del servidor
    log_info "Construyendo imagen del servidor..."
    docker build \
        -f Dockerfile.server \
        -t "${SERVER_IMAGE}:latest" \
        --build-arg IMAGE_TAG=hodei-server \
        . 2>&1 | tail -5

    # Imagen del worker
    log_info "Construyendo imagen del worker..."
    docker build \
        -f Dockerfile.worker \
        -t "${WORKER_IMAGE}:latest" \
        --build-arg IMAGE_TAG=hodei-worker \
        . 2>&1 | tail -5

    log_success "Imágenes construidas"
}

# Subir al registry local
push_images() {
    log_info "Subiendo imágenes al registry local..."

    docker tag "${SERVER_IMAGE}:latest" "${SERVER_IMAGE}:$(git rev-parse --short HEAD 2>/dev/null || echo 'local')"
    docker tag "${WORKER_IMAGE}:latest" "${WORKER_IMAGE}:$(git rev-parse --short HEAD 2>/dev/null || echo 'local')"

    docker push "${SERVER_IMAGE}:latest" 2>&1 | tail -3
    docker push "${WORKER_IMAGE}:latest" 2>&1 | tail -3

    log_success "Imágenes subidas a ${REGISTRY}"
}

# Desplegar en k8s
deploy() {
    log_info "Desplegando en Kubernetes..."

    cd "$PROJECT_ROOT"

    # Namespace
    kubectl create namespace "$NAMESPACE" 2>/dev/null || true

    # Deployment del servidor
    kubectl apply -f - <<EOF
apiVersion: apps/v1
kind: Deployment
metadata:
  name: hodei-server
  namespace: $NAMESPACE
  labels:
    app: hodei-server
spec:
  replicas: 1
  selector:
    matchLabels:
      app: hodei-server
  template:
    metadata:
      labels:
        app: hodei-server
    spec:
      containers:
      - name: server
        image: ${SERVER_IMAGE}:latest
        imagePullPolicy: Always
        ports:
        - containerPort: 50051
        env:
        - name: HODEI_SERVER_GRPC_PORT
          value: "50051"
        - name: RUST_LOG
          value: "info,hodei_server=debug"
        - name: DATABASE_URL
          value: "postgres://postgres:postgres@postgres-postgresql.hodei-jobs-workers.svc.cluster.local:5432/hodei_jobs"
        - name: HODEI_NATS_URLS
          value: "nats://nats-headless.hodei-jobs-workers.svc.cluster.local:4222"
        resources:
          requests:
            cpu: 250m
            memory: 256Mi
          limits:
            cpu: 1000m
            memory: 1Gi
---
apiVersion: v1
kind: Service
metadata:
  name: hodei-server
  namespace: $NAMESPACE
spec:
  selector:
    app: hodei-server
  ports:
  - port: 50051
    targetPort: 50051
  type: ClusterIP
EOF

    log_success "Servidor desplegado"

    # Esperar que esté listo
    kubectl wait --for=condition=available --timeout=120s deployment/hodei-server -n "$NAMESPACE" 2>/dev/null || true

    # Verificar
    kubectl get pods -n "$NAMESPACE" -l app=hodei-server
}

# Port-forward
forward() {
    log_info "Iniciando port-forward..."
    kubectl port-forward -n "$NAMESPACE" svc/hodei-server 30051:50051 &
    sleep 3

    if curl -s http://localhost:30051 &>/dev/null; then
        log_success "Port-forward activo en localhost:30051"
        fg
    else
        log_error "No se pudo conectar"
    fi
}

# Ver estado
status() {
    echo "=== Pods ==="
    kubectl get pods -n "$NAMESPACE" -l app=hodei-server
    echo ""
    echo "=== Imágenes locales ==="
    docker images | grep -E "${REGISTRY}|hodei"
}

# Full workflow
all() {
    log_info "=== Full Build & Deploy Workflow ==="

    check_registry
    build_binaries
    build_images
    push_images
    deploy

    log_success ""
    log_success "=== Deployment completo ==="
    log_info "Servidor: hodei-server.${NAMESPACE}.svc.cluster.local:50051"
    log_info "Para acceder localmente: kubectl port-forward -n ${NAMESPACE} svc/hodei-server 30051:50051"
}

# Help
help() {
    echo "Hodei Build & Deploy - Local Registry"
    echo ""
    echo "Uso: $0 <comando>"
    echo ""
    echo "Comandos:"
    echo "  build    - Compilar binarios"
    echo "  images   - Construir imágenes Docker"
    echo "  push     - Subir imágenes al registry local"
    echo "  deploy   - Desplegar en k8s"
    echo "  forward  - Port-forward"
    echo "  status   - Ver estado"
    echo "  all      - Workflow completo"
    echo ""
    echo "Registry local: $REGISTRY"
}

case "${1:-help}" in
    build) build_binaries ;;
    images) build_images ;;
    push) push_images ;;
    deploy) deploy ;;
    forward) forward ;;
    status) status ;;
    all) all ;;
    help|--help|-h) help ;;
    *) help ;;
esac

#!/bin/bash
# =============================================================================
# Hodei K8s Development Workflow - v2.0
# =============================================================================
# Workflow completo para testing de jobs en Kubernetes
# Integración con build-and-push.sh para compilaciones inteligentes
#
# Usage:
#   ./scripts/k8s-workflow.sh build    # Compilar y subir imágenes (solo si hay cambios)
#   ./scripts/k8s-workflow.sh up       # Desplegar servidor en k8s
#   ./scripts/k8s-workflow.sh test     # Ejecutar job de prueba
#   ./scripts/k8s-workflow.sh forward  # Port-forward para acceso local
#   ./scripts/k8s-workflow.sh down     # Limpiar
# =============================================================================

set -e

NAMESPACE="hodei-system"
SERVER_PORT=50051
K8S_PORT=30051
REGISTRY="${REGISTRY:-ghcr.io/rubentxu/hodei-jobs}"

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

# Verificar cluster
check_cluster() {
    log_info "Verificando cluster Kubernetes..."
    if ! kubectl cluster-info &>/dev/null; then
        log_error "Cluster Kubernetes no disponible"
        exit 1
    fi
    log_success "Cluster disponible"
}

# Compilar y subir imágenes
build() {
    log_build "Compilando y subiendo imágenes..."
    cd /home/rubentxu/Proyectos/rust/hodei-jobs

    # Usar el script de build inteligente
    if [ -f "scripts/build-and-push.sh" ]; then
        ./scripts/build-and-push.sh all
    else
        log_warn "scripts/build-and-push.sh no encontrado, saltando compilación"
    fi
}

# Verificar si hay cambios
check() {
    log_info "Verificando estado de cambios..."
    cd /home/rubentxu/Proyectos/rust/hodei-jobs

    if [ -f "scripts/build-and-push.sh" ]; then
        ./scripts/build-and-push.sh check
    else
        log_error "scripts/build-and-push.sh no encontrado"
    fi
}

# Levantar servidor en k8s
up() {
    log_info "Desplegando Hodei Server en Kubernetes..."
    check_cluster

    # Crear namespace si no existe
    kubectl create namespace "$NAMESPACE" 2>/dev/null || true

    # Determinar imagen a usar
    local image="${REGISTRY}/server:latest"

    # Verificar si la imagen existe localmente
    if ! docker image inspect "$image" &>/dev/null; then
        log_warn "Imagen local no encontrada: $image"
        log_info "Ejecuta './scripts/k8s-workflow.sh build' primero"
    fi

    # Aplicar deployment
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
        image: $image
        imagePullPolicy: Always
        ports:
        - containerPort: $SERVER_PORT
          name: grpc
        - containerPort: 8080
          name: http
        env:
        - name: HODEI_SERVER_GRPC_PORT
          value: "$SERVER_PORT"
        - name: HODEI_SERVER_HTTP_PORT
          value: "8080"
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
        livenessProbe:
          httpGet:
            path: /health
            port: http
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: http
          initialDelaySeconds: 10
          periodSeconds: 5
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
  - port: $SERVER_PORT
    targetPort: $SERVER_PORT
    name: grpc
  - port: 8080
    targetPort: 8080
    name: http
  type: ClusterIP
EOF

    log_success "Deployment creado"

    # Esperar que esté listo
    log_info "Esperando que el servidor esté disponible..."
    kubectl wait --for=condition=available --timeout=120s deployment/hodei-server -n "$NAMESPACE" 2>/dev/null || true

    # Verificar pods
    echo ""
    kubectl get pods -n "$NAMESPACE" -l app=hodei-server
    log_success "Servidor desplegado en: hodei-server.${NAMESPACE}.svc.cluster.local:${SERVER_PORT}"
}

# Port-forward para acceso local
forward() {
    log_info "Iniciando port-forward..."
    kubectl port-forward -n "$NAMESPACE" svc/hodei-server "$K8S_PORT:$SERVER_PORT" &
    PF_PID=$!
    sleep 3

    if curl -s http://localhost:$K8S_PORT &>/dev/null; then
        log_success "Port-forward activo en localhost:$K8S_PORT"
        log_info "Presiona Ctrl+C para detener"
        wait $PF_PID
    else
        log_error "No se pudo conectar al servidor"
        kill $PF_PID 2>/dev/null || true
    fi
}

# Ejecutar job de prueba
test() {
    log_info "Ejecutando job de prueba..."
    cd /home/rubentxu/Proyectos/rust/hodei-jobs

    # Asegurar port-forward activo
    if ! curl -s http://localhost:$K8S_PORT &>/dev/null; then
        log_warn "Iniciando port-forward..."
        kubectl port-forward -n "$NAMESPACE" svc/hodei-server "$K8S_PORT:$SERVER_PORT" &
        sleep 3
    fi

    # Ejecutar job simple (30 segundos timeout)
    timeout 30 cargo run --bin hodei-jobs-cli -- job run \
        --name "K8s Test Job" \
        --command "echo 'Hello from Kubernetes!' && sleep 5 && echo 'Done!'" \
        --provider kubernetes \
        --memory 134217728 \
        --timeout 30 \
        || log_error "Job falló o timeout"
}

# Ver estado
status() {
    echo "=== Estado del Cluster ==="
    kubectl cluster-info
    echo ""
    echo "=== Pods en $NAMESPACE ==="
    kubectl get pods -n "$NAMESPACE"
    echo ""
    echo "=== Pods workers ==="
    kubectl get pods -n hodei-jobs-workers -o wide | head -10
    echo ""
    echo "=== Jobs Recientes ==="
    PGPASSWORD=postgres psql -h localhost -p 5432 -U postgres -d hodei_jobs \
        -c "SELECT id, state, created_at FROM jobs ORDER BY created_at DESC LIMIT 5;" 2>/dev/null || \
        echo "No se pudo conectar a la base de datos"
}

# Ver logs
logs() {
    if [ -n "$2" ]; then
        kubectl logs -n "$NAMESPACE" "$2" --tail=100 -f
    else
        kubectl logs -n "$NAMESPACE" -l app=hodei-server --tail=100 -f
    fi
}

# Ver eventos
events() {
    kubectl get events -n "$NAMESPACE" --sort-by='.lastTimestamp' | tail -20
}

# Limpiar
down() {
    log_info "Limpiando recursos..."
    kubectl delete deployment hodei-server -n "$NAMESPACE" 2>/dev/null || true
    kubectl delete service hodei-server -n "$NAMESPACE" 2>/dev/null || true
    log_success "Limpieza completa"
}

# Full workflow: build + up + test
full() {
    log_info "=== Full K8s Workflow ==="
    check
    build
    up
    forward &
    sleep 5
    test
    status
    log_success "Workflow completo"
}

# Help
help() {
    echo "Hodei K8s Development Workflow v2.0"
    echo ""
    echo "Sistema inteligente de compilación: usa hash SHA256 del código"
    echo "para determinar si hay cambios antes de compilar."
    echo ""
    echo "Comandos:"
    echo "  build   - Compilar y subir imágenes (inteligente)"
    echo "  check   - Verificar si hay cambios pendientes"
    echo "  up      - Desplegar servidor en k8s"
    echo "  forward - Port-forward para acceso local"
    echo "  test    - Ejecutar job de prueba"
    echo "  status  - Ver estado del cluster y jobs"
    echo "  logs    - Ver logs del servidor"
    echo "  events  - Ver eventos del namespace"
    echo "  down    - Limpiar recursos"
    echo "  full    - Workflow completo: build + up + test"
    echo ""
    echo "Variables:"
    echo "  REGISTRY - Registry de imágenes (default: ghcr.io/rubentxu/hodei-jobs)"
}

# Main
case "${1:-help}" in
    build) build ;;
    check) check ;;
    up) up ;;
    forward) forward ;;
    test) test ;;
    status) status ;;
    logs) logs "$@" ;;
    events) events ;;
    down) down ;;
    full) full ;;
    help|--help|-h) help ;;
    *) help ;;
esac

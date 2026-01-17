#!/bin/bash
# dev-k8s-services.sh - Inicia servicios de k8s para desarrollo local
# Usage: ./dev-k8s-services.sh [start|stop|status|logs]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NAMESPACE="hodei-jobs"
PID_DIR="/tmp/hodei-dev"

# Colores
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_status() { echo -e "${BLUE}[ℹ]${NC} $1"; }
print_success() { echo -e "${GREEN}[✓]${NC} $1"; }
print_error() { echo -e "${RED}[✗]${NC} $1"; }
print_warning() { echo -e "${YELLOW}[!]${NC} $1"; }

# Servicios a mapear
declare -A SERVICES=(
    ["postgresql"]="5432:5432"
    ["nats"]="4222:4222"
    ["registry"]="5000:5000"
)

# Crear directorio para PIDs
mkdir -p "$PID_DIR"

# Verificar que kubectl está configurado
check_kubectl() {
    if ! kubectl cluster-info &>/dev/null; then
        print_error "kubectl no configurado. Ejecuta:"
        echo "  export KUBECONFIG=/etc/rancher/k3s/k3s.yaml"
        exit 1
    fi
    print_success "kubectl configurado"
}

# Verificar que los pods están corriendo
check_pods() {
    print_status "Verificando pods de servicios..."

    local missing_pods=()

    for service in "${!SERVICES[@]}"; do
        local pod=$(kubectl get pods -n "$NAMESPACE" -l "app.kubernetes.io/name=$service" -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || echo "")
        if [ -z "$pod" ]; then
            missing_pods+=("$service")
        fi
    done

    if [ ${#missing_pods[@]} -gt 0 ]; then
        print_error "Pods no encontrados: ${missing_pods[*]}"
        print_status "Desplegando servicios..."
        helm upgrade --install hodei ./deploy/hodei-jobs-platform \
            -n "$NAMESPACE" \
            --create-namespace \
            -f ./deploy/hodei-jobs-platform/values.yaml \
            -f ./deploy/hodei-jobs-platform/values-dev.yaml \
            --wait --timeout 300s
    fi

    print_success "Todos los pods están ejecutándose"
}

# Iniciar port-forward para un servicio
start_port_forward() {
    local service=$1
    local ports=$2
    local local_port=$(echo $ports | cut -d: -f1)
    local remote_port=$(echo $ports | cut -d: -f2)
    local pod=$(kubectl get pods -n "$NAMESPACE" -l "app.kubernetes.io/name=$service" -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || echo "")

    if [ -z "$pod" ]; then
        print_error "No se encontró pod para $service"
        return 1
    fi

    # Verificar si ya hay un port-forward activo
    if lsof -i :$local_port &>/dev/null; then
        print_warning "$service ya está disponible en localhost:$local_port"
        return 0
    fi

    print_status "Iniciando port-forward para $service (localhost:$local_port -> $remote_port)..."

    # Iniciar en background
    kubectl port-forward -n "$NAMESPACE" "pod/$pod" "$local_port:$remote_port" &>/dev/null &
    local pid=$!
    echo $pid > "$PID_DIR/${service}.pid"

    # Esperar a que esté listo
    sleep 2

    if kill -0 $pid 2>/dev/null; then
        print_success "$service disponible en localhost:$local_port"
    else
        print_error "Error iniciando port-forward para $service"
        rm -f "$PID_DIR/${service}.pid"
        return 1
    fi
}

# Iniciar todos los servicios
start_all() {
    echo ""
    echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
    echo -e "${MAGENTA}║${NC}  🚀 Hodei Dev Services - K8s Port Forwards    ${MAGENTA}║${NC}"
    echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
    echo ""

    check_kubectl
    check_pods

    echo ""
    print_status "Iniciando port-forwards..."
    echo ""

    # Detener port-forwards existentes
    stop_all

    for service in "${!SERVICES[@]}"; do
        start_port_forward "$service" "${SERVICES[$service]}"
    done

    echo ""
    print_success "Todos los servicios están disponibles:"
    echo ""
    echo "  🗄️  PostgreSQL: localhost:5432 (postgres/postgres)"
    echo "  📨  NATS:       localhost:4222"
    echo "  🐳  Registry:   localhost:5000"
    echo ""
    print_status "Conexión de ejemplo:"
    echo '  DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei_jobs"'
    echo ""
    print_warning "Para SALIR: ./dev-k8s-services.sh stop"
}

# Detener todos los port-forwards
stop_all() {
    print_status "Deteniendo port-forwards..."

    for service in "${!SERVICES[@]}"; do
        local pid_file="$PID_DIR/${service}.pid"
        if [ -f "$pid_file" ]; then
            local pid=$(cat "$pid_file")
            if kill -0 $pid 2>/dev/null; then
                kill $pid 2>/dev/null || true
                print_status "Detenido $service"
            fi
            rm -f "$pid_file"
        fi
    done

    # Limpiar cualquier port-forward huérfano
    pkill -f "kubectl port-forward.*hodei" 2>/dev/null || true

    print_success "Port-forwards detenido"
}

# Verificar estado
status() {
    echo ""
    echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
    echo -e "${MAGENTA}║${NC}  📊 Estado de Servicios de Desarrollo          ${MAGENTA}║${NC}"
    echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
    echo ""

    check_kubectl

    echo "📦 Pods:"
    kubectl get pods -n "$NAMESPACE" -l "app.kubernetes.io/name in (postgresql,nats,registry,hodei-jobs-platform)" -o wide
    echo ""

    echo "🔌 Port-forwards:"
    for service in "${!SERVICES[@]}"; do
        local port=$(echo "${SERVICES[$service]}" | cut -d: -f1)
        if lsof -i :$port &>/dev/null; then
            echo -e "  ${GREEN}✓${NC} $service: localhost:$port"
        else
            echo -e "  ${RED}✗${NC} $service: NO disponible"
        fi
    done
}

# Mostrar ayuda
help() {
    echo ""
    echo -e "${MAGENTA}Hodei Dev Services${NC}"
    echo ""
    echo "Uso: $0 <comando>"
    echo ""
    echo "Comandos:"
    echo "  start     Inicia port-forwards a los servicios de k8s"
    echo "  stop      Detiene todos los port-forwards"
    echo "  status    Muestra estado de servicios y pods"
    echo "  logs      Muestra logs de los pods"
    echo "  help      Muestra esta ayuda"
    echo ""
    echo "Servicios disponibles:"
    for service in "${!SERVICES[@]}"; do
        echo "  - $service: ${SERVICES[$service]}"
    done
}

# Main
case "${1:-start}" in
    start)
        start_all
        ;;
    stop)
        stop_all
        ;;
    status)
        status
        ;;
    logs)
        kubectl logs -n "$NAMESPACE" -l "app.kubernetes.io/name in (postgresql,nats)" --follow --tail=50
        ;;
    help|--help|-h)
        help
        ;;
    *)
        print_error "Comando desconocido: $1"
        help
        exit 1
        ;;
esac

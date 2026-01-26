#!/bin/bash
# dev-setup.sh - Script de configuración para desarrollo local con Telepresence
#
# Este script configura todo lo necesario para desarrollo local:
# 1. Service account para workers
# 2. Port-forward para NATS
# 3. Configuración de variables de entorno
#
# Usage: ./scripts/dev-setup.sh [opción]
#   start   - Configurar y empezar (default)
#   stop    - Detener port-forward
#   status  - Ver estado

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRC_KUBECONFIG="${HOME}/.crc/machines/crc/kubeconfig"
NAMESPACE="hodei-jobs"
WORKERS_NAMESPACE="hodei-jobs-workers"

# Colores
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

print_status() { echo -e "${BLUE}[ℹ]${NC} $1"; }
print_success() { echo -e "${GREEN}[✓]${NC} $1"; }
print_error() { echo -e "${RED}[✗]${NC} $1"; }
print_warning() { echo -e "${YELLOW}[!]${NC} $1"; }

# Configurar KUBECONFIG
setup_kubeconfig() {
    if [ ! -f "$CRC_KUBECONFIG" ]; then
        print_error "No se encontró el kubeconfig de CRC en: $CRC_KUBECONFIG"
        print_error "Asegúrate de que CRC esté ejecutándose"
        exit 1
    fi
    export KUBECONFIG="$CRC_KUBECONFIG"
    print_success "KUBECONFIG configurado: $CRC_KUBECONFIG"
}

# Crear service account para workers
setup_service_account() {
    print_status "Verificando service account para workers..."

    if kubectl get sa hodei-jobs-worker -n "$WORKERS_NAMESPACE" &>/dev/null; then
        print_success "Service account hodei-jobs-worker ya existe"
    else
        print_status "Creando service account hodei-jobs-worker..."
        kubectl create serviceaccount hodei-jobs-worker -n "$WORKERS_NAMESPACE"
        kubectl label serviceaccount hodei-jobs-worker -n "$WORKERS_NAMESPACE" \
            app.kubernetes.io/name=hodei-jobs-worker \
            app.kubernetes.io/managed-by=hodei-jobs 2>/dev/null || true
        print_success "Service account creado"
    fi
}

# Iniciar port-forward para NATS
start_nats_port_forward() {
    # Verificar si ya hay un port-forward activo
    if lsof -i :4222 &>/dev/null; then
        print_warning "Ya hay un proceso escuchando en el puerto 4222"
        return 0
    fi

    print_status "Iniciando port-forward para NATS (puerto 4222)..."
    kubectl -n "$NAMESPACE" port-forward svc/hodei-jobs-hodei-jobs-platform-nats 4222:4222 > /tmp/nats-port-forward.log 2>&1 &
    sleep 3

    if nc -zv localhost 4222 &>/dev/null; then
        print_success "Port-forward para NATS activo en localhost:4222"
    else
        print_error "No se pudo iniciar el port-forward para NATS"
        cat /tmp/nats-port-forward.log
    fi
}

# Detener port-forward
stop_nats_port_forward() {
    print_status "Deteniendo port-forward para NATS..."
    pkill -f "kubectl.*port-forward.*4222" 2>/dev/null || true
    print_success "Port-forward detenido"
}

# Verificar servicios
check_services() {
    echo ""
    echo -e "${CYAN}═════════════════════════════════════════════════════${NC}"
    echo -e "${CYAN}  🔍 Verificación de Servicios                    ${NC}"
    echo -e "${CYAN}═════════════════════════════════════════════════════${NC}"
    echo ""

    # Telepresence
    if telepresence status 2>/dev/null | grep -q "Connected"; then
        print_success "Telepresence conectado"
    else
        print_warning "Telepresence no conectado (ejecuta: ./scripts/dev-telepresence.sh connect)"
    fi

    # PostgreSQL
    if nc -zv postgresql.hodei-jobs.svc.cluster.local 5432 &>/dev/null; then
        print_success "PostgreSQL accesible"
    else
        print_error "PostgreSQL no accesible"
    fi

    # NATS
    if nc -zv localhost 4222 &>/dev/null; then
        print_success "NATS accesible (via port-forward)"
    else
        print_warning "NATS no accesible (necesita port-forward)"
    fi

    # Service account
    if kubectl get sa hodei-jobs-worker -n "$WORKERS_NAMESPACE" &>/dev/null; then
        print_success "Service account hodei-jobs-worker existe"
    else
        print_warning "Service account hodei-jobs-worker no existe"
    fi

    echo ""
}

# Mostrar instrucciones
show_instructions() {
    echo ""
    echo -e "${CYAN}═════════════════════════════════════════════════════${NC}"
    echo -e "${CYAN}  🚀 Entorno de Desarrollo Configurado             ${NC}"
    echo -e "${CYAN}═════════════════════════════════════════════════════${NC}"
    echo ""
    echo "Para ejecutar el servidor:"
    echo ""
    echo "  export HODEI_DATABASE_URL=\"postgres://postgres:postgres@postgresql.hodei-jobs.svc.cluster.local:5432/hodei_jobs\""
    echo "  export HODEI_NATS_URL=\"nats://localhost:4222\""
    echo "  export HODEI_K8S_ENABLED=\"1\""
    echo "  export HODEI_DEV_MODE=\"1\""
    echo "  export KUBECONFIG=\"${CRC_KUBECONFIG}\""
    echo ""
    echo "  cargo run -p hodei-server-bin"
    echo ""
    echo "Para probar un job:"
    echo ""
    echo "  just job-k8s-hello"
    echo ""
    echo -e "${YELLOW}Comandos útiles:${NC}"
    echo ""
    echo "  ./scripts/dev-setup.sh status   # Verificar servicios"
    echo "  ./scripts/dev-setup.sh stop     # Detener port-forward"
    echo "  ./scripts/dev-telepresence.sh quit  # Desconectar Telepresence"
    echo ""
}

# Start
start() {
    setup_kubeconfig
    setup_service_account
    start_nats_port_forward
    check_services
    show_instructions
}

# Stop
stop() {
    stop_nats_port_forward
}

# Status
status() {
    setup_kubeconfig
    check_services
}

# Help
help() {
    echo ""
    echo -e "${MAGENTA}Dev Setup - Hodei Jobs Platform${NC}"
    echo ""
    echo "Usage: $0 <comando>"
    echo ""
    echo "Comandos:"
    echo "  start   Configurar entorno y empezar (default)"
    echo "  stop    Detener port-forward para NATS"
    echo "  status  Verificar estado de servicios"
    echo "  help    Esta ayuda"
    echo ""
    echo "Ejemplo:"
    echo "   $0 start    # Configurar todo"
    echo "   $0 status   # Verificar"
    echo "   $0 stop     # Limpiar al terminar"
    echo ""
}

case "${1:-start}" in
    start)
        start
        ;;
    stop)
        stop
        ;;
    status)
        status
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

#!/bin/bash
# dev-telepresence.sh - Desarrollo local con Telepresence v2
#
# FLUJO:
#   1. just telepresence-connect  # Conectar al cluster
#   2. just dev-telepresence      # Compilar y ejecutar con hot reload
#   3. just job-k8s-hello         # Probar jobs
#   4. just telepresence-quit     # Desconectar
#
# REQUISITOS:
#   - kubectl configurado
#   - Telepresence CLI instalado
#   - Cluster k8s accesible

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NAMESPACE="hodei-jobs"

# Colores
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m'

print_status() { echo -e "${BLUE}[ℹ]${NC} $1"; }
print_success() { echo -e "${GREEN}[✓]${NC} $1"; }
print_error() { echo -e "${RED}[✗]${NC} $1"; }
print_warning() { echo -e "${YELLOW}[!]${NC} $1"; }

# Instalar Telepresence
install_telepresence() {
    if command -v telepresence &>/dev/null; then
        VER=$(telepresence version 2>/dev/null | grep "OSS Client" | head -1 || echo "")
        if [ -n "$VER" ]; then
            print_success "Telepresence: $VER"
            return 0
        fi
    fi

    print_status "Instalando Telepresence v2.20.2..."

    ARCH=$(uname -m)
    case "$ARCH" in
        x86_64)
            TELE_URL="https://github.com/telepresenceio/telepresence/releases/download/v2.20.2/telepresence-linux-amd64"
            ;;
        aarch64|arm64)
            TELE_URL="https://github.com/telepresenceio/telepresence/releases/download/v2.20.2/telepresence-linux-arm64"
            ;;
        *)
            print_error "Arquitectura no soportada: $ARCH"
            exit 1
            ;;
    esac

    curl -sL "$TELE_URL" -o /tmp/telepresence
    chmod +x /tmp/telepresence
    sudo mv /tmp/telepresence /usr/local/bin/telepresence

    print_success "Telepresence instalado"
}

# Verificar cluster
check_cluster() {
    if ! kubectl cluster-info &>/dev/null; then
        print_error "Cluster no accesible"
        echo "   Verifica kubectl con: kubectl cluster-info"
        exit 1
    fi
    print_success "Cluster accesible"
}

# Instalar Traffic Manager
install_traffic_manager() {
    if kubectl get namespace ambassador &>/dev/null; then
        if kubectl get deployment traffic-manager -n ambassador &>/dev/null; then
            print_success "Traffic Manager ya instalado"
            return 0
        fi
    fi

    print_status "Instalando Traffic Manager..."
    telepresence helm install
    print_success "Traffic Manager instalado"
}

# Conectar
connect() {
    echo ""
    echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
    echo -e "${MAGENTA}║${NC}  🚀 Conectando Telepresence al cluster        ${MAGENTA}║${NC}"
    echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
    echo ""

    install_telepresence
    check_cluster
    install_traffic_manager

    # Verificar si ya conectado
    if telepresence status 2>/dev/null | grep -q "Connected"; then
        print_warning "Ya conectado al cluster"
        show_env
        return 0
    fi

    print_status "Conectando a namespace: $NAMESPACE"
    telepresence connect --namespace "$NAMESPACE"

    if [ $? -eq 0 ]; then
        print_success "¡Conectado!"
        show_env
    else
        print_error "Error conectando"
        exit 1
    fi
}

# Desconectar
quit() {
    print_status "Desconectando..."
    telepresence quit -s 2>/dev/null || true
    print_success "Desconectado"
}

# Estado
status() {
    echo ""
    echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
    echo -e "${MAGENTA}║${NC}  📊 Estado de Telepresence                    ${MAGENTA}║${NC}"
    echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
    echo ""

    telepresence status 2>/dev/null || echo "No conectado"

    echo ""
    echo "Servicios en $NAMESPACE:"
    kubectl get svc -n "$NAMESPACE" -o name 2>/dev/null | head -10 || echo "  No hay servicios"

    echo ""
    echo "Verificando DNS..."
    if getent hosts hodei-hodei-jobs-platform-postgresql.hodei-jobs.svc.cluster.local &>/dev/null; then
        print_success "DNS: hodei-hodei-jobs-platform-postgresql resuelve"
    else
        print_warning "DNS: Resolviendo solo con nombre largo"
    fi
}

# Mostrar entorno
show_env() {
    echo ""
    echo -e "${CYAN}═════════════════════════════════════════════════════${NC}"
    echo -e "${CYAN}  🌐 Conectado al cluster                          ${NC}"
    echo -e "${CYAN}═════════════════════════════════════════════════════${NC}"
    echo ""
    echo "📦 Servicios accesibles por nombre DNS:"
    echo ""
    echo "   postgresql:5432       →  hodei-hodei-jobs-platform-postgresql.hodei-jobs.svc.cluster.local:5432"
    echo "   nats:4222             →  hodei-hodei-jobs-platform-nats.hodei-jobs.svc.cluster.local:4222"
    echo "   servidor:9090         →  hodei-hodei-jobs-platform.hodei-jobs.svc.cluster.local:9090"
    echo ""
    echo "💡 Para desarrollo, usa:"
    echo ""
    echo '   export DATABASE_URL="postgres://postgres:postgres@postgresql:5432/hodei_jobs"'
    echo '   export HODEI_NATS_URL="nats://nats:4222"'
    echo ""
    echo -e "${YELLOW}🛑 Para terminar: ./scripts/dev-telepresence.sh quit${NC}"
    echo ""
}

# Help
help() {
    echo ""
    echo -e "${MAGENTA}Hodei Dev - Telepresence${NC}"
    echo ""
    echo "Usage: $0 <comando>"
    echo ""
    echo "Comandos:"
    echo "  connect     Conectar al cluster"
    echo "  quit        Desconectar"
    echo "  status      Ver estado"
    echo "  help        Esta ayuda"
    echo ""
    echo "Ejemplo:"
    echo "   $0 connect"
    echo "   # Luego ejecutar el servidor con las variables correctas"
    echo "   DATABASE_URL='postgres://postgres:postgres@postgresql:5432/hodei_jobs' \\"
    echo "   HODEI_NATS_URL='nats://nats:4222' \\"
    echo "   cargo run --release -p hodei-server-bin"
    echo ""
}

case "${1:-help}" in
    connect|start)
        connect
        ;;
    quit|disconnect|stop)
        quit
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

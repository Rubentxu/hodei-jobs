#!/bin/bash
# dev-start.sh - Desarrollo local con port-forward a k8s
#
# FLUJO SIMPLE:
#   1. Despliega servicios en k8s (PostgreSQL + NATS)
#   2. Inicia port-forwards automáticos
#   3. Compila y ejecuta el servidor local
#   4. Hot reload para desarrollo
#
# USO: ./scripts/dev-start.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

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
print_step() { echo -e "${CYAN}[▶]${NC} $1"; }

# ============================================================
# VERIFICAR/PRERREQUISITOS
# ============================================================
check_prerequisites() {
    echo ""
    echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
    echo -e "${MAGENTA}║${NC}  🔍 VERIFICANDO PRERREQUISITOS               ${MAGENTA}║${NC}"
    echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
    echo ""

    # kubectl
    if ! command -v kubectl &>/dev/null; then
        print_error "kubectl no instalado"
        exit 1
    fi
    print_success "kubectl encontrado"

    # helm
    if ! command -v helm &>/dev/null; then
        print_error "Helm no instalado"
        exit 1
    fi
    print_success "Helm encontrado"

    # cargo
    if ! command -v cargo &>/dev/null; then
        print_error "Cargo no instalado"
        exit 1
    fi
    print_success "Cargo encontrado"

    # Cluster
    if ! kubectl cluster-info &>/dev/null; then
        print_error "Cluster k8s no accesible"
        exit 1
    fi
    print_success "Cluster k8s accesible"
}

# ============================================================
# PORT-FORWARDS
# ============================================================
start_port_forwards() {
    echo ""
    echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
    echo -e "${MAGENTA}║${NC}  🔌 INICIANDO PORT-FORWARDS                  ${MAGENTA}║${NC}"
    echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
    echo ""

    # Detener port-forwards previos
    pkill -f "kubectl port-forward.*5432" 2>/dev/null || true
    pkill -f "kubectl port-forward.*4222" 2>/dev/null || true
    sleep 1

    # PostgreSQL
    print_status "Iniciando PostgreSQL port-forward..."
    kubectl port-forward -n hodei-jobs svc/hodei-hodei-jobs-platform-postgresql 5432:5432 &>/dev/null &
    sleep 2
    if pg_isready -h localhost -p 5432 -U postgres &>/dev/null; then
        print_success "PostgreSQL: localhost:5432 ✓"
    else
        print_warning "PostgreSQL no accesible"
    fi

    # NATS
    print_status "Iniciando NATS port-forward..."
    kubectl port-forward -n hodei-jobs svc/hodei-hodei-jobs-platform-nats 4222:4222 &>/dev/null &
    sleep 2
    if nc -zv localhost 4222 &>/dev/null; then
        print_success "NATS: localhost:4222 ✓"
    else
        print_warning "NATS no accesible"
    fi
}

# ============================================================
# COMPILAR Y EJECUTAR
# ============================================================
start_server() {
    echo ""
    echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
    echo -e "${MAGENTA}║${NC}  🚀 INICIANDO SERVIDOR                        ${MAGENTA}║${NC}"
    echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
    echo ""

    # Detect if we are using Telepresence or host.docker.internal
    if telepresence status 2>/dev/null | grep -q "Connected"; then
        export HODEI_DATABASE_URL="postgres://postgres:postgres@hodei-hodei-jobs-platform-postgresql.hodei-jobs.svc.cluster.local:5432/hodei_jobs"
        export HODEI_NATS_URL="nats://hodei-hodei-jobs-platform-nats.hodei-jobs.svc.cluster.local:4222"
        export HODEI_SERVER_ADDRESS="http://hodei-server-placeholder.hodei-jobs.svc.cluster.local:9090"
        export HODEI_WORKER_IMAGE="registry.local:31500/hodei-jobs-worker:latest"
    else
        export HODEI_DATABASE_URL="postgres://postgres:postgres@localhost:5432/hodei_jobs"
        export HODEI_NATS_URL="nats://localhost:4222"
        export HODEI_SERVER_ADDRESS="http://localhost:9090"
        export HODEI_WORKER_IMAGE="hodei-jobs-worker:latest"
    fi

    export RUST_LOG="${RUST_LOG:-info,hodei_server=debug,hodei_jobs=debug}"

    # Compilar
    print_status "Compilando hodei-server..."
    cargo build --release -p hodei-server-bin

    # Verificar que está corriendo
    sleep 2
    if curl -s --connect-timeout 3 "http://localhost:9090" &>/dev/null; then
        print_success "Servidor corriendo en localhost:9090"
    else
        print_warning "Servidor iniciando (puede tomar unos segundos)..."
    fi
}

# ============================================================
# LIMPIEZA
# ============================================================
cleanup() {
    echo ""
    echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
    echo -e "${MAGENTA}║${NC}  🧹 LIMPIEZA                                 ${MAGENTA}║${NC}"
    echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
    echo ""

    pkill -f "hodei-server" 2>/dev/null || true
    pkill -f "kubectl port-forward.*5432" 2>/dev/null || true
    pkill -f "kubectl port-forward.*4222" 2>/dev/null || true

    print_success "Cleanup completado"
}

trap cleanup EXIT

# ============================================================
# INICIO
# ============================================================

echo ""
echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
echo -e "${MAGENTA}║${NC}  🚀 HODEI JOBS - DESARROLLO LOCAL              ${MAGENTA}║${NC}"
echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
echo ""
echo "   Este script:"
echo "   1. Verifica prerrequisitos"
echo "   2. Inicia port-forwards a PostgreSQL y NATS"
echo "   3. Compila y ejecuta el servidor"
echo ""
echo -e "${YELLOW}   💡 Ctrl+C para salir${NC}"
echo ""

check_prerequisites
start_port_forwards
start_server

echo ""
echo -e "${GREEN}╔════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║${NC}  ✅ ¡ENTORNO DE DESARROLLO LISTO!              ${GREEN}║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════════════════╝${NC}"
echo ""
echo "   📦 Servicios accesibles:"
echo "      • PostgreSQL: localhost:5432"
echo "      • NATS: localhost:4222"
echo "      • Servidor: localhost:9090"
echo ""
echo "   💡 Pruébalo:"
echo "      just job-k8s-hello"
echo ""
echo -e "${YELLOW}   Presiona ENTER para iniciar hot reload o Ctrl+C para salir...${NC}"
read -r

# Hot reload simple
echo ""
print_status "Iniciando modo desarrollo con hot reload..."
echo "   Edita código en crates/ y guarda para recompilar"
echo ""

# Script simple de hot reload
(
    cd "$SCRIPT_DIR/.."
    last_mtime=$(find crates -name "*.rs" -type f -exec stat -c %Y {} \; 2>/dev/null | sort -rn | head -1 || echo 0)

    while sleep 2; do
        current_mtime=$(find crates -name "*.rs" -type f -exec stat -c %Y {} \; 2>/dev/null | sort -rn | head -1 || echo 0)

        if [ "$current_mtime" != "$last_mtime" ] && [ "$last_mtime" != 0 ]; then
            echo ""
            echo -e "${CYAN}[▶]${NC}Cambio detectado - recompilando..."
            cargo build --release -p hodei-server-bin
            echo -e "${GREEN}[✓]${NC}Compilado"
            echo -e "${YELLOW}   Esperando cambios...${NC}"
        fi

        last_mtime=$current_mtime
    done
)

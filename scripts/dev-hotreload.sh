#!/bin/bash
# dev-hotreload.sh - Hot reload para desarrollo con Telepresence
#
# FLUJO:
#   1. just deploy-services              # Desplegar PostgreSQL + NATS
#   2. just telepresence-connect         # Conectar al cluster
#   3. ./scripts/dev-hotreload.sh        # Compilar + hot reload
#   4. just job-k8s-hello                # Probar jobs
#   5. Ctrl+C para salir
#   6. just telepresence-quit            # Desconectar
#
# REQUISITOS:
#   - k3s con PostgreSQL y NATS desplegados
#   - Telepresence OSS conectado

set -eo pipefail

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

# Verificar DNS de servicios críticos
check_dns() {
    local service_name="$1"
    local full_dns="$2"
    local short_name="$3"

    # Try short name first (via Telepresence DNS)
    if getent hosts "$short_name" &>/dev/null; then
        local short_ip=$(getent hosts "$short_name" | awk '{print $1}' | head -1)
        print_success "DNS: $short_name → $short_ip"
        return 0
    fi

    # Try full DNS name
    if getent hosts "$full_dns" &>/dev/null; then
        local full_ip=$(getent hosts "$full_dns" | awk '{print $1}' | head -1)
        print_success "DNS: $full_dns → $full_ip"
        return 0
    fi

    # Try with nslookup if available
    if command -v nslookup &>/dev/null; then
        if nslookup "$short_name" &>/dev/null; then
            print_success "DNS: $short_name (nslookup OK)"
            return 0
        fi
    fi

    print_error "DNS: $service_name no resuelve"
    return 1
}

# Extraer servicio DNS de una URL
extract_service_from_url() {
    local url="$1"
    # Extraer hostname de URL (soporta: scheme://host, scheme://host:port, scheme://user:pass@host:port/path)
    # Eliminar el esquema primero
    local without_scheme="${url#*://}"
    # Si hay @, eliminar las credenciales
    if [[ "$without_scheme" == *@* ]]; then
        without_scheme="${without_scheme#*@}"
    fi
    # Extraer hostname (hasta : o /)
    local host="${without_scheme%%[:/]*}"
    echo "$host"
}

# Construir nombre DNS completo de Kubernetes
build_k8s_dns() {
    local service_name="$1"
    local namespace="${2:-hodei-jobs}"

    # Si ya es un nombre completo, retornarlo tal cual
    if echo "$service_name" | grep -q '\.svc\.cluster\.local'; then
        echo "$service_name"
        return
    fi

    # Si contiene un punto, probablemente ya es un nombre calificado
    if echo "$service_name" | grep -q '\.'; then
        echo "$service_name"
        return
    fi

    # Construir nombre completo usando el namespace por defecto
    echo "$service_name.$namespace.svc.cluster.local"
}

# Extraer puerto de una URL
extract_port_from_url() {
    local url="$1"
    # Extraer puerto de URL (ej: postgres://host:port -> port)
    if echo "$url" | grep -q ':'; then
        echo "$url" | sed -E 's|^[^:]+://[^:]+:([0-9]+).*$|\1|'
    else
        echo "5432" # Default PostgreSQL port
    fi
}

# Verificar Telepresence
check_telepresence() {
    echo ""
    echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
    echo -e "${MAGENTA}║${NC}  🔍 Verificando Telepresence                  ${MAGENTA}║${NC}"
    echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
    echo ""

    if ! telepresence status 2>/dev/null | grep -q "Connected"; then
        print_error "Telepresence no conectado"
        echo ""
        echo "Ejecuta primero:"
        echo "   just telepresence-connect"
        exit 1
    fi

    print_success "Telepresence conectado"
}

# Configurar variables de entorno
setup_env() {
    echo ""
    echo -e "${CYAN}═════════════════════════════════════════════════════${NC}"
    echo -e "${CYAN}  ⚙️  Configuración de Entorno                     ${NC}"
    echo -e "${CYAN}═════════════════════════════════════════════════════${NC}"
    echo ""

    # Cargar variables desde .env.development si existe
    if [ -f ".env.development" ]; then
        print_status "Cargando .env.development..."
        set -a
        source .env.development
        set +a
        print_success ".env.development cargado"
    else
        print_error ".env.development no encontrado"
        exit 1
    fi

    # Verificar que las variables requeridas estén definidas
    if [ -z "$HODEI_DATABASE_URL" ]; then
        print_error "HODEI_DATABASE_URL no está definido en .env.development"
        exit 1
    fi

    if [ -z "$HODEI_NATS_URL" ]; then
        print_error "HODEI_NATS_URL no está definido en .env.development"
        exit 1
    fi

    if [ -z "$HODEI_SERVER_BIND" ]; then
        print_error "HODEI_SERVER_BIND no está definido en .env.development"
        exit 1
    fi

    if [ -z "$HODEI_SERVER_ADDRESS" ]; then
        print_error "HODEI_SERVER_ADDRESS no está definido en .env.development"
        exit 1
    fi

    export RUST_LOG="${RUST_LOG:-info,saga_engine_core=debug,hodei_server_application=debug,hodei_server_infrastructure=debug}"
    export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

    # Configurar path de migraciones (incluye core/ e infra/)
    export HODEI_MIGRATIONS_PATH="${HODEI_MIGRATIONS_PATH:-crates/server/infrastructure/src/persistence/postgres/migrations/core,crates/server/infrastructure/src/persistence/postgres/migrations/infra}"
}

# Mostrar todas las variables de entorno antes de compilar
print_env_summary() {
    echo ""
    echo -e "${CYAN}╔════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║${NC}  📋 Variables de Entorno para Compilación      ${CYAN}║${NC}"
    echo -e "${CYAN}╚════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${YELLOW}  🗄️  Database:${NC}"
    echo "     HODEI_DATABASE_URL: ${HODEI_DATABASE_URL:-no definido}"
    echo ""
    echo -e "${YELLOW}  📨  NATS:${NC}"
    echo "     HODEI_NATS_URL: ${HODEI_NATS_URL:-no definido}"
    echo ""
    echo -e "${YELLOW}  🔌  gRPC Server:${NC}"
    echo "     HODEI_SERVER_BIND: ${HODEI_SERVER_BIND:-no definido}"
    echo "     HODEI_SERVER_ADDRESS: ${HODEI_SERVER_ADDRESS:-no definido}"
    echo "     HODEI_SERVER_PROTOCOL: ${HODEI_SERVER_PROTOCOL:-http (default)}"
    echo ""
    echo -e "${YELLOW}  🐳  Worker:${NC}"
    echo "     HODEI_WORKER_IMAGE: ${HODEI_WORKER_IMAGE:-no definido}"
    echo ""
    echo -e "${YELLOW}  📁  Migraciones:${NC}"
    echo "     HODEI_MIGRATIONS_PATH: ${HODEI_MIGRATIONS_PATH:-no definido}"
    echo ""
    echo -e "${YELLOW}  📝  Logging:${NC}"
    echo "     RUST_LOG: ${RUST_LOG:-no definido}"
    echo "     RUST_BACKTRACE: ${RUST_BACKTRACE:-no definido}"
    echo ""
    echo -e "${GREEN}  ✅ Listo para compilar${NC}"
    echo ""
}

# Verificar DNS de servicios configurados
check_dns_services() {
    echo ""
    echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
    echo -e "${MAGENTA}║${NC}  🌐 Verificando DNS de Servicios               ${MAGENTA}║${NC}"
    echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
    echo ""

    local all_ok=true

    # Extraer servicios de las variables de entorno y construir nombres DNS completos
    local db_service_short=$(extract_service_from_url "$HODEI_DATABASE_URL")
    local db_service=$(build_k8s_dns "$db_service_short")

    local nats_service_short=$(extract_service_from_url "$HODEI_NATS_URL")
    local nats_service=$(build_k8s_dns "$nats_service_short")

    # PostgreSQL
    if ! check_dns "PostgreSQL" "$db_service" "$db_service_short"; then
        all_ok=false
    fi

    # NATS
    if ! check_dns "NATS" "$nats_service" "$nats_service_short"; then
        all_ok=false
    fi

    # gRPC bind address - mostrar modo local si es una dirección IP local
    local grpc_host=$(echo "$HODEI_SERVER_BIND" | cut -d: -f1)
    if [[ "$grpc_host" == "0.0.0.0" || "$grpc_host" == "127.0.0.1" || "$grpc_host" == "localhost" ]]; then
        print_success "gRPC: Modo local ($HODEI_SERVER_BIND)"
    else
        print_success "gRPC: $HODEI_SERVER_BIND"
    fi

    # Verificar DNS del server address para workers
    local server_hostname="$HODEI_SERVER_ADDRESS"
    if ! check_dns "Hodei Server" "$(build_k8s_dns "$server_hostname")" "$server_hostname"; then
        all_ok=false
    fi

    # Server platform service (para service mesh)
    if ! check_dns "Hodei Platform" "hodei-hodei-jobs-platform.hodei-jobs.svc.cluster.local" "hodei-hodei-jobs-platform"; then
        all_ok=false
    fi

    echo ""
    if [ "$all_ok" = true ]; then
        print_success "Todos los DNS resuelven correctamente"
    else
        print_error "Algunos DNS no resuelven - no se puede continuar"
        echo ""
        echo "Verifica Telepresence con:"
        echo "   just telepresence-status"
        echo ""
        echo "O reconecta con:"
        echo "   just telepresence-connect"
        exit 1
    fi
}

# Verificar schema de base de datos
check_database_schema() {
    echo ""
    echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
    echo -e "${MAGENTA}║${NC}  🗄️  Verificando schema de BD                  ${MAGENTA}║${NC}"
    echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
    echo ""

    if psql "$HODEI_DATABASE_URL" -t -c "SELECT 1 FROM information_schema.columns WHERE table_name = 'jobs' AND column_name = 'max_attempts';" 2>/dev/null | grep -q 1; then
        print_success "Columna max_attempts existe"
    else
        print_warning "Falta columna max_attempts, agregándola..."
        psql "$HODEI_DATABASE_URL" -c "ALTER TABLE jobs ADD COLUMN IF NOT EXISTS max_attempts INTEGER NOT NULL DEFAULT 3;" 2>/dev/null && \
            print_success "Columna max_attempts agregada" || \
            print_error "No se pudo agregar la columna"
    fi
}

# ============================================================
# COMPILACIÓN
# ============================================================

compile() {
    local start_time=$(date +%s)

    print_status "Compilando hodei-server..."

    if cargo build --release -p hodei-server-bin; then
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        print_success "Compilación completada: ${duration}s"
        print_env_summary
        return 0
    else
        print_error "Compilación fallida"
        return 1
    fi
}

# ============================================================
# INICIAR SERVIDOR
# ============================================================

start_server() {
    print_status "Iniciando servidor..."

    cd "$SCRIPT_DIR/.."

    # Matar servidor anterior
    pkill -f "hodei-server-bin" 2>/dev/null || true
    sleep 1

    # Arrancar servidor local
    HODEI_DATABASE_URL="$HODEI_DATABASE_URL" \
    HODEI_NATS_URL="$HODEI_NATS_URL" \
    HODEI_GRPC_ADDRESS="$HODEI_GRPC_ADDRESS" \
    HODEI_WORKER_IMAGE="$HODEI_WORKER_IMAGE" \
    HODEI_WORKER_ADDRESS="$HODEI_WORKER_ADDRESS" \
    HODEI_K8S_SERVICE_NAME="$HODEI_K8S_SERVICE_NAME" \
    RUST_LOG="$RUST_LOG" \
    nohup ./target/release/hodei-server-bin > /tmp/hodei-server.log 2>&1 &

    SERVER_PID=$!
    sleep 5

    # Verificar que está corriendo
    if curl -s --connect-timeout 5 "http://localhost:9090" &>/dev/null; then
        print_success "Servidor corriendo en localhost:9090 (PID: $SERVER_PID)"
        return 0
    else
        print_warning "Servidor iniciando, revisando logs..."
        tail -15 /tmp/hodei-server.log
        return 1
    fi
}

# ============================================================
# HOT RELOAD LOOP
# ============================================================

hot_reload() {
    echo ""
    echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
    echo -e "${MAGENTA}║${NC}  🔥 Hot Reload Activo                         ${MAGENTA}║${NC}"
    echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo "   📝 Edita código en crates/ y guarda para recompilar"
    echo "   📋 Ver logs: tail -f /tmp/hodei-server.log"
    echo "   🚀 Probar jobs: just job-k8s-hello"
    echo ""
    echo -e "${YELLOW}   Ctrl+C para salir${NC}"
    echo ""

    cd "$SCRIPT_DIR/.."

    # Primera compilación e inicio
    compile || exit 1
    start_server

    echo ""
    echo -e "${CYAN}Esperando cambios en crates/...${NC}"
    echo ""

    # Detectar cambios en archivos .rs
    last_mtime=$(find crates -name "*.rs" -type f -exec stat -c %Y {} \; 2>/dev/null | sort -rn | head -1 || echo 0)

    while sleep 2; do
        current_mtime=$(find crates -name "*.rs" -type f -exec stat -c %Y {} \; 2>/dev/null | sort -rn | head -1 || echo 0)

        if [ "$current_mtime" != "$last_mtime" ] && [ "$last_mtime" != 0 ]; then
            echo ""
            echo -e "${CYAN}[▶]${NC}Cambio detectado - recompilando..."

            if compile; then
                # Reiniciar servidor
                pkill -f "hodei-server-bin" 2>/dev/null || true
                sleep 2

                HODEI_DATABASE_URL="$HODEI_DATABASE_URL" \
                HODEI_NATS_URL="$HODEI_NATS_URL" \
                HODEI_GRPC_ADDRESS="$HODEI_GRPC_ADDRESS" \
                HODEI_WORKER_IMAGE="$HODEI_WORKER_IMAGE" \
                HODEI_WORKER_ADDRESS="$HODEI_WORKER_ADDRESS" \
                HODEI_K8S_SERVICE_NAME="$HODEI_K8S_SERVICE_NAME" \
                RUST_LOG="$RUST_LOG" \
                nohup ./target/release/hodei-server-bin > /tmp/hodei-server.log 2>&1 &

                sleep 5

                if curl -s --connect-timeout 5 "http://localhost:9090" &>/dev/null; then
                    print_success "✅ Servidor reiniciado en localhost:9090"
                else
                    print_warning "⚠️  Servidor reiniciando..."
                    tail -10 /tmp/hodei-server.log
                fi
            else
                print_error "❌ Error en compilación, esperando cambios..."
            fi
        fi

        last_mtime=$current_mtime
    done
}

# ============================================================
# CLEANUP
# ============================================================

cleanup() {
    echo ""
    print_status "Cerrando hot-reload..."
    pkill -f "hodei-server-bin" 2>/dev/null || true
    print_success "Cleanup completado"
}

trap cleanup EXIT

# ============================================================
# INICIO
# ============================================================

cd "$SCRIPT_DIR/.."

check_telepresence
setup_env
print_env_summary
check_dns_services
check_database_schema
hot_reload

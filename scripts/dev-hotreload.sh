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

    # Verificar DNS
    echo ""
    echo "Verificando DNS..."
    if getent hosts postgresql &>/dev/null; then
        print_success "postgresql resuelve: $(getent hosts postgresql | awk '{print $1}')"
    else
        print_warning "postgresql no resuelve"
    fi

    if getent hosts nats &>/dev/null; then
        print_success "nats resuelve: $(getent hosts nats | awk '{print $1}')"
    else
        print_warning "nats no resuelve"
    fi
}

# Configurar variables de entorno
setup_env() {
    # Usar nombre completo del servicio para PostgreSQL (requerido por Telepresence sin intercept)
    export DATABASE_URL="${DATABASE_URL:-postgres://postgres:postgres@hodei-hodei-jobs-platform-postgresql.hodei-jobs.svc.cluster.local:5432/hodei_jobs}"
    export HODEI_NATS_URL="${HODEI_NATS_URL:-nats://nats:4222}"
    export RUST_LOG="${RUST_LOG:-info}"
    export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"

    # Configurar path de migraciones (incluye core/ e infra/)
    export HODEI_MIGRATIONS_PATH="${HODEI_MIGRATIONS_PATH:-crates/server/infrastructure/src/persistence/postgres/migrations/core,crates/server/infrastructure/src/persistence/postgres/migrations/infra}"

    # ⚠️ IMPORTANTE: Usar nombre completo del servicio para que los workers puedan conectarse
    # Esto es crítico para Telepresence: los workers en k8s deben usar el Service, no localhost
    export HODEI_GRPC_ADDRESS="${HODEI_GRPC_ADDRESS:-http://hodei-hodei-jobs-platform.hodei-jobs.svc.cluster.local:9090}"

    echo ""
    echo -e "${CYAN}Variables de entorno:${NC}"
    echo "   DATABASE_URL: $DATABASE_URL"
    echo "   HODEI_NATS_URL: $HODEI_NATS_URL"
    echo "   HODEI_MIGRATIONS_PATH: $HODEI_MIGRATIONS_PATH"
    echo "   HODEI_GRPC_ADDRESS: $HODEI_GRPC_ADDRESS"
    echo "   RUST_LOG: $RUST_LOG"
}

# Verificar schema de base de datos
check_database_schema() {
    echo ""
    echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
    echo -e "${MAGENTA}║${NC}  🗄️  Verificando schema de BD                  ${MAGENTA}║${NC}"
    echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
    echo ""

    if psql "$DATABASE_URL" -t -c "SELECT 1 FROM information_schema.columns WHERE table_name = 'jobs' AND column_name = 'max_attempts';" 2>/dev/null | grep -q 1; then
        print_success "Columna max_attempts existe"
    else
        print_warning "Falta columna max_attempts, agregándola..."
        psql "$DATABASE_URL" -c "ALTER TABLE jobs ADD COLUMN IF NOT EXISTS max_attempts INTEGER NOT NULL DEFAULT 3;" 2>/dev/null && \
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
    DATABASE_URL="$DATABASE_URL" \
    HODEI_NATS_URL="$HODEI_NATS_URL" \
    HODEI_GRPC_ADDRESS="$HODEI_GRPC_ADDRESS" \
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

                DATABASE_URL="$DATABASE_URL" \
                HODEI_NATS_URL="$HODEI_NATS_URL" \
                HODEI_GRPC_ADDRESS="$HODEI_GRPC_ADDRESS" \
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
check_database_schema
hot_reload

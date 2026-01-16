#!/bin/bash
# DevSpace startup script with hot reload
# Flow: Code sync → cargo watch → compile → restart server

set -eo pipefail

# Colores
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
WHITE='\033[1;37m'
NC='\033[0m'

print_status() { echo -e "${BLUE}[ℹ]${NC} $1"; }
print_success() { echo -e "${GREEN}[✓]${NC} $1"; }
print_error() { echo -e "${RED}[✗]${NC} $1"; }

# Configuración paralela - use incremental compilation
export CARGO_BUILD_JOBS=4
export CARGO_INCREMENTAL=1

# Paths
SOURCE_DIR="${SOURCE_DIR:-/app}"
SERVER_BIN="/tmp/hodei-server-bin"
PID_FILE="/tmp/server.pid"
LOG_FILE="/tmp/cargo_build.log"

# Cambiar al directorio fuente
cd "$SOURCE_DIR"

# ============================================================
# Compilación con output visible
# ============================================================
compile_server() {
    print_status "Iniciando compilación..."
    echo ""

    local start_time=$(date +%s)

    # Compilar con output visible, capturando el exit status
    set +e
    cargo build --release -p hodei-server-bin 2>&1 | tee "$LOG_FILE"
    local build_status=${PIPESTATUS[0]}
    set -e

    local end_time=$(date +%s)
    local duration=$((end_time - start_time))

    echo ""

    if [ $build_status -eq 0 ]; then
        if [ $duration -lt 60 ]; then
            print_success "Compilación completada en ${duration}s"
        else
            local mins=$((duration / 60))
            local secs=$((duration % 60))
            print_success "Compilación completada en ${mins}m ${secs}s"
        fi

        # Mostrar información del binario
        if [ -f "$SOURCE_DIR/target/release/hodei-server-bin" ]; then
            local size=$(du -h "$SOURCE_DIR/target/release/hodei-server-bin" | cut -f1)
            print_status "Binario: ${size}"
        fi

        return 0
    else
        print_error "Compilación fallida (exit code: $build_status)"
        echo ""
        return 1
    fi
}

# ============================================================
# Arrancar/reiniciar servidor
# ============================================================
start_server() {
    # Matar servidor anterior si existe
    if [ -f "$PID_FILE" ]; then
        local old_pid=$(cat "$PID_FILE" 2>/dev/null)
        if [ -n "$old_pid" ] && kill -0 "$old_pid" 2>/dev/null; then
            print_status "Deteniendo servidor anterior (PID: $old_pid)..."
            kill "$old_pid" 2>/dev/null || true
            sleep 1
            # Force kill if still running
            kill -9 "$old_pid" 2>/dev/null || true
        fi
        rm -f "$PID_FILE"
    fi

    # Copiar binario
    cp "$SOURCE_DIR/target/release/hodei-server-bin" "$SERVER_BIN"
    chmod +x "$SERVER_BIN"

    # Arrancar servidor
    $SERVER_BIN &
    local server_pid=$!
    echo $server_pid > "$PID_FILE"

    print_success "Servidor arrancado (PID: $server_pid)"
}

# ============================================================
# Script de hot reload para cargo watch
# ============================================================
create_reload_script() {
    cat > /tmp/reload.sh << 'RELOAD_EOF'
#!/bin/bash
set -e

SOURCE_DIR="${SOURCE_DIR:-/app}"
SERVER_BIN="/tmp/hodei-server-bin"
PID_FILE="/tmp/server.pid"

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo ""
echo -e "${BLUE}[ℹ]${NC} Cambios detectados, recompilando..."
echo ""

cd "$SOURCE_DIR"

start_time=$(date +%s)

if cargo build --release -p hodei-server-bin; then
    end_time=$(date +%s)
    duration=$((end_time - start_time))

    echo ""
    echo -e "${GREEN}[✓]${NC} Compilación completada en ${duration}s"

    # Matar servidor anterior
    if [ -f "$PID_FILE" ]; then
        old_pid=$(cat "$PID_FILE" 2>/dev/null)
        if [ -n "$old_pid" ] && kill -0 "$old_pid" 2>/dev/null; then
            echo -e "${BLUE}[ℹ]${NC} Deteniendo servidor anterior..."
            kill "$old_pid" 2>/dev/null || true
            sleep 1
            kill -9 "$old_pid" 2>/dev/null || true
        fi
    fi

    # Copiar y arrancar nuevo servidor
    cp "$SOURCE_DIR/target/release/hodei-server-bin" "$SERVER_BIN"
    chmod +x "$SERVER_BIN"

    $SERVER_BIN &
    new_pid=$!
    echo $new_pid > "$PID_FILE"

    echo -e "${GREEN}[✓]${NC} Servidor reiniciado (PID: $new_pid)"
    echo ""
    echo -e "${YELLOW}  💡 Listo para más cambios...${NC}"
    echo ""
else
    echo ""
    echo -e "${RED}[✗]${NC} Compilación fallida - servidor anterior sigue corriendo"
    echo ""
fi
RELOAD_EOF
    chmod +x /tmp/reload.sh
}

# ============================================================
# Cleanup on exit
# ============================================================
cleanup() {
    echo ""
    print_status "Cerrando..."

    if [ -f "$PID_FILE" ]; then
        local pid=$(cat "$PID_FILE" 2>/dev/null)
        if [ -n "$pid" ]; then
            kill "$pid" 2>/dev/null || true
            kill -9 "$pid" 2>/dev/null || true
        fi
        rm -f "$PID_FILE"
    fi

    # Kill cargo watch if running
    pkill -f "cargo watch" 2>/dev/null || true

    print_status "Limpieza completada"
    exit 0
}

trap cleanup SIGINT SIGTERM

# ============================================================
# INICIO
# ============================================================

echo ""
echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
echo -e "${MAGENTA}║${NC}  🔥 Hodei Jobs - DevSpace Hot Reload           ${MAGENTA}║${NC}"
echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
echo ""

# Primera compilación
print_status "Primera compilación - esto puede tomar unos minutos..."
echo ""

if compile_server; then
    start_server
else
    print_error "No se pudo compilar el servidor. Revisa los errores arriba."
    exit 1
fi

echo ""
echo -e "${GREEN}============================================${NC}"
echo -e "${GREEN}  ✅ Servidor listo para desarrollo${NC}"
echo -e "${GREEN}============================================${NC}"
echo ""

# Crear script de reload
create_reload_script

# Hot reload con cargo watch
print_status "Iniciando hot reload con cargo watch..."
echo ""
echo "  📝 Flow de desarrollo:"
echo "     1. DevSpace sincroniza código → pod"
echo "     2. cargo watch detecta cambios en crates/, proto/"
echo "     3. Recompila automáticamente"
echo "     4. Reinicia el servidor"
echo ""
echo -e "${YELLOW}  💡 Edita código localmente y guarda para recargar${NC}"
echo -e "${YELLOW}  🛑 Ctrl+C para salir${NC}"
echo ""

echo -e "${GREEN}🔥 Modo desarrollo activo!${NC}"
echo ""

# Usar cargo watch para detectar cambios
# --postpone: no ejecutar al inicio (ya compilamos arriba)
# --watch: directorios a observar (no existe src/ en la raíz, es un workspace)
cargo watch \
    --postpone \
    --watch crates \
    --watch proto \
    --shell /tmp/reload.sh &

WATCH_PID=$!

# Esperar a cargo watch (mantiene el script corriendo)
wait $WATCH_PID

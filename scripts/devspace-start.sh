#!/bin/bash
# DevSpace startup script with hot reload and anti-spam protection
# Features:
# - Debouncing: Wait for idle period before recompiling
# - Mutex: Prevent concurrent compilations
# - Change coalescing: Only recompile latest state

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
print_warning() { echo -e "${YELLOW}[!]${NC} $1"; }

# ============================================================
# CONFIGURACIÓN ANTI-SPAM
# ============================================================

# Mutex para evitar compilaciones concurrentes
COMPILE_LOCK="/tmp/compile.lock"
export COMPILE_LOCK

# Tiempo de espera después del último cambio antes de compilar (milisegundos)
DEBOUNCE_MS="${DEBOUNCE_MS:-2000}"

# Tiempo mínimo entre compilaciones exitosas (segundos)
MIN_BUILD_INTERVAL="${MIN_BUILD_INTERVAL:-10}"

# Timestamp de última compilación exitosa
LAST_BUILD_TIME=0

# Contador de cambios pendientes
PENDING_CHANGES=0
PENDING_CHANGES_LOCK="/tmp/pending_changes.lock"

# ============================================================
# FUNCIONES DE UTILIDAD
# ============================================================

# Adquirir mutex de compilación
acquire_compile_lock() {
    local max_wait=30  # Max seconds to wait for lock
    local waited=0

    while [ -f "$COMPILE_LOCK" ]; do
        if [ $waited -ge $max_wait ]; then
            print_warning "Timeout esperando lock de compilación"
            return 1
        fi
        sleep 1
        waited=$((waited + 1))
    done

    echo $$ > "$COMPILE_LOCK"
    return 0
}

# Liberar mutex de compilación
release_compile_lock() {
    rm -f "$COMPILE_LOCK"
}

# Incrementar contador de cambios pendientes
add_pending_change() {
    (flock -n "$PENDING_CHANGES_LOCK" echo $(($(cat "$PENDING_CHANGES_LOCK" 2>/dev/null || echo 0) + 1)) > "$PENDING_CHANGES_LOCK") 2>/dev/null || true
}

# Resetear contador de cambios pendientes
reset_pending_changes() {
    echo 0 > "$PENDING_CHANGES_LOCK"
}

# Obtener número de cambios pendientes
get_pending_changes() {
    cat "$PENDING_CHANGES_LOCK" 2>/dev/null || echo 0
}

# Verificar si hay cambios pendientes
has_pending_changes() {
    [ "$(get_pending_changes)" -gt 0 ]
}

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
# Compilación con protección anti-spam
# ============================================================
compile_server() {
    # Verificar mutex antes de compilar
    if ! acquire_compile_lock; then
        print_warning "Otra compilación en progreso, omitiendo..."
        return 1
    fi

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

        # Actualizar timestamp de última compilación exitosa
        LAST_BUILD_TIME=$(date +%s)
        release_compile_lock
        return 0
    else
        print_error "Compilación fallida (exit code: $build_status)"
        echo ""
        release_compile_lock
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
# Script de hot reload inteligente con debouncing
# ============================================================
create_reload_script() {
    cat > /tmp/reload.sh << 'RELOAD_EOF'
#!/bin/bash
# Hot reload script con debouncing y coalescing de cambios

set -e

SOURCE_DIR="${SOURCE_DIR:-/app}"
SERVER_BIN="/tmp/hodei-server-bin"
PID_FILE="/tmp/server.pid"
COMPILE_LOCK="/tmp/compile.lock"
PENDING_CHANGES_LOCK="/tmp/pending_changes.lock"

# Configuración
DEBOUNCE_MS="${DEBOUNCE_MS:-2000}"
MIN_BUILD_INTERVAL="${MIN_BUILD_INTERVAL:-5}"

# Colores
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[ℹ]${NC} $1"; }
log_success() { echo -e "${GREEN}[✓]${NC} $1"; }
log_error() { echo -e "${RED}[✗]${NC} $1"; }
log_wait() { echo -e "${CYAN}[⏳]${NC} $1"; }

# ============================================================
# FUNCIONES DE CONTROL
# ============================================================

# Adquirir lock de compilación
try_acquire_lock() {
    if [ -f "$COMPILE_LOCK" ]; then
        return 1
    fi
    echo $$ > "$COMPILE_LOCK"
    return 0
}

# Liberar lock
release_lock() {
    rm -f "$COMPILE_LOCK"
}

# Obtener tiempo desde última compilación
time_since_last_build() {
    local last_build="${LAST_BUILD_TIME:-0}"
    local now=$(date +%s)
    echo $((now - last_build))
}

# ============================================================
# LÓGICA DE DEBOUNCING
# ============================================================

# Marcar cambio pendiente
add_pending_change() {
    local count=$(cat "$PENDING_CHANGES_LOCK" 2>/dev/null || echo 0)
    echo $((count + 1)) > "$PENDING_CHANGES_LOCK"
}

# Resetear cambios pendientes
reset_changes() {
    echo 0 > "$PENDING_CHANGES_LOCK"
}

# Obtener cambios pendientes
get_pending_count() {
    cat "$PENDING_CHANGES_LOCK" 2>/dev/null || echo 0
}

# ============================================================
# PROCESAR CAMBIOS CON DEBOUNCING (se ejecuta como script independiente)
# ============================================================

# Este bloque se genera en /tmp/reload.sh y se ejecuta directamente
# No usamos 'local' porque no está dentro de una función

cd "$SOURCE_DIR"

# Incrementar contador de cambios
add_pending_change
pending=$(get_pending_count)

echo ""
echo -e "${BLUE}[ℹ]${NC} Cambios detectados (cola: $pending)"
log_info "Esperando ${DEBOUNCE_MS}ms para coalescing..."

# Debouncing: esperar antes de compilar
sleep_ms=$((DEBOUNCE_MS / 1000))
sleep $sleep_ms

# Verificar si hay más cambios acumulados durante la espera
sleep 0.5  # Espera adicional para nuevos cambios
final_count=$(get_pending_count)

if [ "$final_count" -gt 1 ]; then
    echo -e "${YELLOW}[!]${NC} $final_count cambios acumulados, continuando espera..."
    sleep 1
fi

# Resetear contador ANTES de compilar
reset_changes

# Verificar si otra instancia está compilando
if ! try_acquire_lock; then
    echo -e "${YELLOW}[!]${NC} Compilación en progreso por otro proceso, saltando..."
    exit 0
fi

# Verificar intervalo mínimo entre compilaciones
elapsed=$(time_since_last_build)
if [ "$elapsed" -lt "$MIN_BUILD_INTERVAL" ]; then
    echo -e "${YELLOW}[!]${NC} Compilación reciente hace ${elapsed}s, esperando..."
    release_lock
    sleep $((MIN_BUILD_INTERVAL - elapsed))
fi

start_time=$(date +%s)

echo ""
echo -e "${CYAN}[▶]${NC} Compilando..."
echo ""

if cargo build --release -p hodei-server-bin; then
    end_time=$(date +%s)
    duration=$((end_time - start_time))

    echo ""
    log_success "Compilación completada en ${duration}s"

    # Reiniciar servidor
    if [ -f "$PID_FILE" ]; then
        old_pid=$(cat "$PID_FILE" 2>/dev/null)
        if [ -n "$old_pid" ] && kill -0 "$old_pid" 2>/dev/null; then
            echo -e "${BLUE}[ℹ]${NC} Deteniendo servidor anterior..."
            kill "$old_pid" 2>/dev/null || true
            sleep 1
            kill -9 "$old_pid" 2>/dev/null || true
        fi
    fi

    cp "$SOURCE_DIR/target/release/hodei-server-bin" "$SERVER_BIN"
    chmod +x "$SERVER_BIN"

    $SERVER_BIN &
    new_pid=$!
    echo $new_pid > "$PID_FILE"

    log_success "Servidor reiniciado (PID: $new_pid)"
    export LAST_BUILD_TIME=$(date +%s)

    echo ""
    echo -e "${YELLOW}  💡 Esperando más cambios...${NC}"
    echo ""

    release_lock
else
    echo ""
    log_error "Compilación fallida - revisando errores arriba"
    echo -e "${YELLOW}  💡 El servidor anterior sigue corriendo (si existía)${NC}"
    release_lock
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

    # Limpiar locks
    rm -f "$COMPILE_LOCK"
    rm -f "$PENDING_CHANGES_LOCK"

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
print_status "Iniciando hot reload con protección anti-spam..."
echo ""
echo "  📝 Flow de desarrollo:"
echo "     1. DevSpace sincroniza código → pod"
echo "     2. cargo watch detecta cambios en crates/, proto/"
echo "     3. Espera ${DEBOUNCE_MS}ms (debouncing) antes de compilar"
echo "     4. Usa mutex para evitar compilaciones concurrentes"
echo "     5. Coalesce múltiples cambios en una sola compilación"
echo ""
echo -e "${YELLOW}  💡 Edita código localmente y guarda para recargar${NC}"
echo -e "${YELLOW}  🛑 Ctrl+C para salir${NC}"
echo ""
echo "  🔧 Variables configurables:"
echo "     DEBOUNCE_MS=${DEBOUNCE_MS}      # Tiempo de espera antes de compilar"
echo "     MIN_BUILD_INTERVAL=${MIN_BUILD_INTERVAL}  # Segundos entre compilaciones"
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

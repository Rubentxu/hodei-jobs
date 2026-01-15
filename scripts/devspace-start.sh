#!/bin/bash
# DevSpace startup script with hot reload and progress bar
# Flow: Code sync → cargo watch → compile with progress → reload (SIGUSR1)

set -e

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

# Configuración paralela
export CARGO_BUILD_JOBS=4
export CARGO_INCREMENTAL=1

# Paths
SERVER_BIN="/usr/local/bin/hodei-jobs-server"
PID_FILE="/tmp/server.pid"
LOG_FILE="/tmp/cargo_build.log"

# Spinner animado
SPINNER_CHARS=('⠋' '⠙' '⠹' '⠸' '⠼' '⠴' '⠦' '⠧' '⠇' '⠏')

# ============================================================
# Obtener lista de crates del workspace
# ============================================================
get_crate_list() {
    cargo metadata --format-version=1 2>/dev/null | \
        jq -r '.packages[].name' | \
        grep -v '\.' | \
        sort -u
}

# ============================================================
# Compilación con barra de progreso REAL
# ============================================================
compile_with_progress() {
    print_status "Analizando dependencias..."

    # Obtener todos los crates del workspace
    local crates=$(get_crate_list)
    local total_crates=$(echo "$crates" | wc -l)
    local compiled_crates=0
    local spin_idx=0
    local start_time=$(date +%s)
    local last_update=0

    # Limpiar log
    > "$LOG_FILE"

    echo ""
    echo -e "${WHITE}[░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░]${NC} 0% (0/${total_crates})"
    echo -ne "${CYAN}${SPINNER_CHARS[0]}${NC} Inicializando compilación..."

    # Compilar en background con captura de output
    cargo build --release -p hodei-server-bin 2>&1 | tee "$LOG_FILE" &
    BUILD_PID=$!

    # Procesar output en tiempo real
    while kill -0 $BUILD_PID 2>/dev/null; do
        # Leer nuevas líneas del log
        if [ -f "$LOG_FILE" ]; then
            # Contar crates compilados (Finished) - sanitizar salida
            finished=$(grep -c "Finished" "$LOG_FILE" 2>/dev/null | tr -d '[:space:]' || echo "0")
            running=$(grep -cE "^   Compiling" "$LOG_FILE" 2>/dev/null | tr -d '[:space:]' || echo "0")

            # Calcular progreso real
            compiled_crates=${finished:-0}

            # Limitar al total de crates conocidos
            if [ "$compiled_crates" -gt "$total_crates" ] 2>/dev/null; then
                compiled_crates=$total_crates
            fi

            # Calcular porcentaje
            if [ "$total_crates" -gt 0 ] 2>/dev/null; then
                pct=$((compiled_crates * 100 / total_crates))
            else
                pct=0
            fi

            # Limitar entre 1-99% mientras compila
            if [ "$pct" -eq 0 ] 2>/dev/null; then pct=1; fi
            if [ "$pct" -ge 100 ] 2>/dev/null; then pct=99; fi

            # Spinner
            spin_idx=$(( (spin_idx + 1) % 10 ))

            # Mostrar progreso
            current_crate=$(tail -5 "$LOG_FILE" 2>/dev/null | grep -oE "Compiling [a-z0-9_-]+" | tail -1 | sed 's/Compiling //' || echo "")

            if [ -n "$current_crate" ] && [ ${#current_crate} -lt 30 ]; then
                echo -ne "\r\033[K${CYAN}${SPINNER_CHARS[$spin_idx]}${NC} Compilando: ${YELLOW}${current_crate}${NC} ${WHITE}[${compiled_crates}/${total_crates}]${NC} ${pct}%"
            else
                echo -ne "\r\033[K${CYAN}${SPINNER_CHARS[$spin_idx]}${NC} Compilando... ${WHITE}[${compiled_crates}/${total_crates}]${NC} ${pct}%"
            fi
        fi

        sleep 0.2
    done

    # Esperar finalización
    wait $BUILD_PID
    BUILD_STATUS=$?

    echo ""

    # Verificar resultado
    if [ $BUILD_STATUS -eq 0 ]; then
        # Obtener tiempo
        end_time=$(date +%s)
        duration=$((end_time - start_time))

        # Mostrar barra completa
        local bar_len=50
        local filled=$((bar_len))
        local bar=""
        for ((i=0; i<filled; i++)); do bar+="█"; done
        for ((i=filled; i<bar_len; i++)); done bar+="░"; done

        echo -e "${GREEN}[${bar}]${NC} 100% (${total_crates}/${total_crates})"

        if [ $duration -lt 60 ]; then
            print_success "Compilación completada en ${duration}s"
        else
            mins=$((duration / 60))
            secs=$((duration % 60))
            print_success "Compilación completada en ${mins}m ${secs}s"
        fi
        return 0
    else
        print_error "Errores de compilación:"
        echo ""
        tail -30 "$LOG_FILE"
        return 1
    fi
}

# ============================================================
# INICIO
# ============================================================

echo ""
echo -e "${MAGENTA}╔════════════════════════════════════════════════════╗${NC}"
echo -e "${MAGENTA}║${NC}  🔥 Hodei Jobs - DevSpace Hot Reload           ${MAGENTA}║${NC}"
echo -e "${MAGENTA}╚════════════════════════════════════════════════════╝${NC}"
echo ""

# Compilar si no existe el binario
if [ ! -f "$SERVER_BIN" ]; then
    echo -e "${BLUE}[ℹ]${NC} Primera compilación - esto puede tomar unos minutos..."
    echo ""

    if compile_with_progress; then
        cp target/release/hodei-server-bin "$SERVER_BIN"
        chmod +x "$SERVER_BIN"
    else
        exit 1
    fi
else
    # Verificar si hay cambios
    echo -e "${BLUE}[ℹ]${NC} Binario existente encontrado"
    echo ""

    print_status "Verificando cambios..."

    if cargo build --release -p hodei-server-bin -j 4 2>&1 | grep -q "Finished"; then
        print_success "No hay cambios - binario actualizado"
    else
        echo ""
        print_status "Hay cambios, recompilando..."
        echo ""
        if compile_with_progress; then
            cp target/release/hodei-server-bin "$SERVER_BIN"
            chmod +x "$SERVER_BIN"
        fi
    fi
fi

# Copiar binario a ubicación final
if [ -f "target/release/hodei-server-bin" ]; then
    cp target/release/hodei-server-bin "$SERVER_BIN"
    chmod +x "$SERVER_BIN"
fi

echo ""
echo -e "${BLUE}[ℹ]${NC} Arrancando servidor..."
echo ""

# Arrancar servidor
$SERVER_BIN &
SERVER_PID=$!
echo $SERVER_PID > "$PID_FILE"

echo -e "${GREEN}============================================${NC}"
echo -e "${GREEN}  ✅ Servidor arrancado (PID: $SERVER_PID)${NC}"
echo -e "${GREEN}============================================${NC}"
echo ""

# Hot reload con cargo watch
echo -e "${BLUE}[ℹ]${NC} Iniciando cargo watch para hot reload..."
echo ""
echo "  📝 Flow de desarrollo:"
echo "     1. DevSpace sincroniza código → pod"
echo "     2. cargo watch detecta cambios"
echo "     3. Recompila automáticamente"
echo "     4. Envía SIGUSR1 para reload"
echo ""
echo -e "${YELLOW}  💡 Edita código y guarda para recargar${NC}"
echo ""

cargo watch --release -p hodei-server-bin -s "kill -USR1 $SERVER_PID 2>/dev/null || true" &

echo -e "${GREEN}🔥 Modo desarrollo activo!${NC}"
echo ""

wait

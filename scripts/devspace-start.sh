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

# ============================================================
# Spinner animado para compilación
# ============================================================
SPINNER_CHARS=('⠋' '⠙' '⠹' '⠸' '⠼' '⠴' '⠦' '⠧' '⠇' '⠏')

compile_with_progress() {
    print_status "Compilando servidor..."

    # Contador de archivos compilados
    local count=0
    local total=50  # Estimado
    local start_time=$(date +%s)
    local spin_idx=0

    echo -e "${WHITE}[░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░]${NC} 0%"
    echo -ne "${CYAN}${SPINNER_CHARS[0]}${NC} Compilando..."

    # Compilar en background con log
    cargo build --release -p hodei-server-bin -j 4 > /tmp/cargo_build.log 2>&1 &
    BUILD_PID=$!

    # Actualizar spinner mientras compila
    while kill -0 $BUILD_PID 2>/dev/null; do
        # Leer log para obtener progreso
        if [ -f /tmp/cargo_build.log ]; then
            compiling=$(grep -c "Compiling" /tmp/cargo_build.log 2>/dev/null || echo 0)
            errors=$(grep -c "error:" /tmp/cargo_build.log 2>/dev/null || echo 0)

            # Calcular porcentaje estimado
            count=$((count + 1))
            if [ $count -gt 40 ]; then count=40; fi
            pct=$((count * 2))

            # Spinner
            spin_idx=$(( (spin_idx + 1) % 10 ))

            # Mostrar estado
            if [ "$errors" -gt 0 ]; then
                echo -ne "\r\033[K${RED}${SPINNER_CHARS[$spin_idx]}${NC} Compilando... ${YELLOW}${pct}%${NC} (${errors} errores)"
            else
                echo -ne "\r\033[K${CYAN}${SPINNER_CHARS[$spin_idx]}${NC} Compilando... ${WHITE}${pct}%${NC}"
            fi
        fi
        sleep 0.3
    done

    # Esperar a que termine
    wait $BUILD_PID
    BUILD_STATUS=$?

    echo ""

    # Verificar resultado
    if [ $BUILD_STATUS -eq 0 ]; then
        # Obtener tiempo
        end_time=$(date +%s)
        duration=$((end_time - start_time))

        # Mostrar barra completa
        echo -e "${GREEN}[██████████████████████████████████████████████████]${NC} 100%"

        if [ $duration -lt 60 ]; then
            print_success "Compilación completada en ${duration}s"
        else
            mins=$((duration / 60))
            secs=$((duration % 60))
            print_success "Compilación completada en ${mins}m ${secs}s"
        fi
        return 0
    else
        echo ""
        print_error "Errores de compilación:"
        echo ""
        tail -20 /tmp/cargo_build.log
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

#!/bin/bash
# =============================================================================
# Hodei Build & Push with Content Hash
# =============================================================================
# Compila y sube imágenes Docker solo si el código cambió
# Usa hash del binario + archivos fuente para determinar si hay cambios
#
# Usage:
#   ./scripts/build-and-push.sh server    # Solo servidor
#   ./scripts/build-and-push.sh worker    # Solo worker
#   ./scripts/build-and-push.sh all       # Servidor + Worker
#   ./scripts/build-and-push.sh check     # Solo verificar si hay cambios
# =============================================================================

set -e

# Configuración
REGISTRY="${REGISTRY:-ghcr.io/rubentxu/hodei-jobs}"
SERVER_IMAGE="${REGISTRY}/server"
WORKER_IMAGE="${REGISTRY}/worker"
NAMESPACE="${REGISTRY##*/}"  # Extrae "rubentxu" de "ghcr.io/rubentxu"

# Directorios
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
HASH_FILE="${PROJECT_ROOT}/.image-hash"

# Colores
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log_info() { echo -e "${BLUE}ℹ️  $1${NC}"; }
log_success() { echo -e "${GREEN}✅ $1${NC}"; }
log_warn() { echo -e "${YELLOW}⚠️  $1${NC}"; }
log_error() { echo -e "${RED}❌ $1${NC}"; }
log_build() { echo -e "${CYAN}🔨 $1${NC}"; }

# Calcular hash del código fuente
calculate_source_hash() {
    log_info "Calculando hash del código fuente..."

    # Hash de archivos Rust clave
    local src_hash=$(find \
        crates/server \
        Cargo.toml \
        Cargo.lock \
        -type f \( -name "*.rs" -o -name "*.toml" \) \
        -exec sha256sum {} + 2>/dev/null | sha256sum | cut -d' ' -f1)

    echo "$src_hash"
}

# Calcular hash del binario compilado
calculate_binary_hash() {
    local binary="$1"
    if [ -f "$binary" ]; then
        sha256sum "$binary" | cut -d' ' -f1
    else
        echo ""
    fi
}

# Obtener hash de imagen existente
get_image_hash() {
    local image="$1"
    skopeo inspect --format "{{.Digest}}" "docker://${image}:latest" 2>/dev/null || echo ""
}

# Guardar hash actual
save_hash() {
    local component="$1"
    local hash="$2"
    echo "${component}=${hash}" >> "$HASH_FILE"
}

# Cargar hash guardado
load_hash() {
    local component="$1"
    grep "^${component}=" "$HASH_FILE" 2>/dev/null | cut -d'=' -f2- || echo ""
}

# Verificar si hay cambios para un componente
check_changes() {
    local component="$1"
    local binary="$2"

    log_info "Verificando ${component}..."

    # Calcular hash actual
    local src_hash=$(calculate_source_hash)
    local bin_hash=$(calculate_binary_hash "$binary")

    # Hash combinado
    local current_hash="${src_hash:0:16}-${bin_hash:0:16}"

    # Hash anterior
    local prev_hash=$(load_hash "$component")

    if [ "$current_hash" = "$prev_hash" ]; then
        log_warn "${component}: Sin cambios, saltando compilación"
        return 1  # No hay cambios
    else
        log_build "${component}: Cambios detectados, compilando..."
        if [ -n "$prev_hash" ]; then
            log_info "Hash anterior: ${prev_hash:0:20}..."
            log_info "Hash actual:   ${current_hash:0:20}..."
        fi
        return 0  # Hay cambios
    fi
}

# Compilar binario Rust
build_binary() {
    local package="$1"
    local binary="$2"

    log_build "Compilando ${package}..."
    cd "$PROJECT_ROOT"

    cargo build --package "$package" --release 2>&1 | tail -5

    if [ ! -f "target/release/$binary" ]; then
        log_error "Compilación de ${package} falló"
        return 1
    fi

    log_success "${package} compilado"
}

# Compilar imagen Docker
build_docker_image() {
    local component="$1"
    local dockerfile="$2"
    local image="$3"
    local binary="$4"

    log_build "Construyendo imagen Docker para ${component}..."

    # Copiar binario a contexto de Docker
    mkdir -p "${PROJECT_ROOT}/.docker-${component}"
    cp "${PROJECT_ROOT}/target/release/${binary}" "${PROJECT_ROOT}/.docker-${component}/"

    cd "${PROJECT_ROOT}"

    # Construir imagen
    docker build \
        -f "$dockerfile" \
        -t "${image}:latest" \
        -t "${image}:$(git rev-parse --short HEAD 2>/dev/null || echo 'local')" \
        ".docker-${component}" 2>&1 | tail -10

    # Limpiar directorio temporal
    rm -rf "${PROJECT_ROOT}/.docker-${component}"

    log_success "Imagen ${image}:latest creada"
}

# Subir imagen al registry
push_image() {
    local image="$1"

    log_info "Subiendo ${image}..."

    # Login si es necesario (para GHCR)
    if [[ "$image" =~ ^ghcr\.io ]]; then
        if ! crane auth whoami "$REGISTRY" &>/dev/null; then
            log_info "Haciendo login en GHCR..."
            echo "$GITHUB_TOKEN" | crane auth login "$REGISTRY" -u rubentxu --password-stdin 2>/dev/null || \
            echo "$CR_PAT" | docker login "$REGISTRY" -u rubentxu --password-stdin 2>/dev/null || true
        fi
    fi

    docker push "${image}:latest" 2>&1 | tail -5
    log_success "${image} subida"
}

# Compilar y desplegar servidor
deploy_server() {
    local binary="hodei-server-bin"
    local dockerfile="${PROJECT_ROOT}/Dockerfile.server"
    local image="$SERVER_IMAGE"

    # Verificar cambios
    if ! check_changes "server" "target/release/${binary}"; then
        return 0
    fi

    # Compilar
    build_binary "hodei-server-bin" "$binary" || return 1

    # Si no existe Dockerfile.server, usar el genérico
    if [ ! -f "$dockerfile" ]; then
        dockerfile="${PROJECT_ROOT}/Dockerfile"
    fi

    # Construir imagen
    build_docker_image "server" "$dockerfile" "$image" "$binary"

    # Subir
    push_image "$image"

    # Guardar hash
    local src_hash=$(calculate_source_hash)
    save_hash "server" "${src_hash:0:16}"
}

# Compilar y desplegar worker
deploy_worker() {
    local binary="hodei-worker-bin"
    local dockerfile="${PROJECT_ROOT}/Dockerfile.worker"
    local image="$WORKER_IMAGE"

    # Verificar cambios
    if ! check_changes "worker" "target/release/${binary}"; then
        return 0
    fi

    # Compilar
    build_binary "hodei-worker-bin" "$binary" || return 1

    # Si no existe Dockerfile.worker, usar el genérico
    if [ ! -f "$dockerfile" ]; then
        dockerfile="${PROJECT_ROOT}/Dockerfile"
    fi

    # Construir imagen
    build_docker_image "worker" "$dockerfile" "$image" "$binary"

    # Subir
    push_image "$image"

    # Guardar hash
    local src_hash=$(calculate_source_hash)
    save_hash "worker" "${src_hash:0:16}"
}

# Solo verificar estado
check_status() {
    log_info "Estado de cambios:"

    local server_hash=$(load_hash "server")
    local worker_hash=$(load_hash "worker")

    echo ""
    echo "Servidor:"
    if [ -n "$server_hash" ]; then
        echo "  Hash: ${server_hash:0:20}..."
    else
        echo "  Sin registro (nunca compilado)"
    fi

    echo ""
    echo "Worker:"
    if [ -n "$worker_hash" ]; then
        echo "  Hash: ${worker_hash:0:20}..."
    else
        echo "  Sin registro (nunca compilado)"
    fi

    echo ""
    echo "Imágenes:"
    echo "  ${SERVER_IMAGE}:$(get_image_hash "$SERVER_IMAGE" | cut -d':' -f2 | cut -c1-12)"
    echo "  ${WORKER_IMAGE}:$(get_image_hash "$WORKER_IMAGE" | cut -d':' -f2 | cut -c1-12)"
}

# Help
help() {
    echo "Hodei Build & Push with Content Hash"
    echo ""
    echo "Este script compila y sube imágenes Docker solo si el código cambió."
    echo "Usa hash SHA256 del código fuente + binario para determinar cambios."
    echo ""
    echo "Uso: $0 <comando>"
    echo ""
    echo "Comandos:"
    echo "  server   - Compilar y subir solo el servidor"
    echo "  worker   - Compilar y subir solo el worker"
    echo "  all      - Compilar y subir servidor + worker"
    echo "  check    - Verificar estado de cambios"
    echo "  clean    - Limpiar hashes guardados"
    echo "  help     - Mostrar esta ayuda"
    echo ""
    echo "Variables de entorno:"
    echo "  REGISTRY - Registry de imágenes (default: ghcr.io/rubentxu/hodei-jobs)"
    echo ""
    echo "Archivos:"
    echo "  ${HASH_FILE} - Guarda los hashes de la última compilación"
}

# Limpiar hashes
clean_hashes() {
    rm -f "$HASH_FILE"
    log_success "Hashes limpiados"
}

# Main
case "${1:-help}" in
    server) deploy_server ;;
    worker) deploy_worker ;;
    all)
        deploy_server
        deploy_worker
        ;;
    check) check_status ;;
    clean) clean_hashes ;;
    help|--help|-h) help ;;
    *) help ;;
esac

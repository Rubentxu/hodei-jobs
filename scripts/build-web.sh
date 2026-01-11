#!/bin/bash
# Build script for Hodei Operator Web Dashboard
# Builds WASM bundle and prepares for deployment

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="${SCRIPT_DIR}/../.."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Default values
VERSION="${VERSION:-latest}"
RELEASE=false
DOCKER=false
TARGET="web"
OUT_DIR="dist"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --version)
            VERSION="$2"
            shift 2
            ;;
        --release)
            RELEASE=true
            shift
            ;;
        --docker)
            DOCKER=true
            shift
            ;;
        --target)
            TARGET="$2"
            shift 2
            ;;
        --out-dir)
            OUT_DIR="$2"
            shift 2
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --version VERSION    Set version (default: latest)"
            echo "  --release           Build in release mode"
            echo "  --docker            Also build Docker image"
            echo "  --target TARGET     WASM target (web, browser, nodejs)"
            echo "  --out-dir DIR       Output directory"
            echo "  --help              Show this help message"
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

cd "$PROJECT_ROOT"

log_info "Building Hodei Operator Web Dashboard v${VERSION}"
log_info "Target: ${TARGET}, Release: ${RELEASE}, Output: ${OUT_DIR}"

# Check prerequisites
log_info "Checking prerequisites..."

if ! command -v rustc &> /dev/null; then
    log_error "Rust not found. Please install Rust first."
    exit 1
fi

if ! command -v wasm-pack &> /dev/null; then
    log_warn "wasm-pack not found. Installing..."
    cargo install wasm-pack
fi

# Check for wasm32 target
if ! rustup target list | grep -q "wasm32-unknown-unknown"; then
    log_info "Adding wasm32-unknown-unknown target..."
    rustup target add wasm32-unknown-unknown
fi

# Build WASM bundle
log_info "Building WASM bundle with wasm-pack..."

if [ "$RELEASE" = true ]; then
    BUILD_MODE="--release"
else
    BUILD_MODE=""
fi

wasm-pack build \
    --target "$TARGET" \
    --out-dir "$OUT_DIR" \
    $BUILD_MODE \
    -- \
    -p hodei-console-web

if [ $? -ne 0 ]; then
    log_error "WASM build failed!"
    exit 1
fi

log_success "WASM bundle built successfully!"

# Copy assets
log_info "Copying static assets..."

mkdir -p "$OUT_DIR"
cp -r crates/operator-web/assets/index.html "$OUT_DIR/"
cp -r crates/operator-web/assets/style.css "$OUT_DIR/"

if [ "$DOCKER" = true ]; then
    log_info "Building Docker image..."

    docker build \
        -t "ghcr.io/rubentxu/hodei-jobs/operator-web:${VERSION}" \
        -f crates/operator-web/Dockerfile.prod \
        .

    if [ $? -ne 0 ]; then
        log_error "Docker build failed!"
        exit 1
    fi

    log_success "Docker image built: ghcr.io/rubentxu/hodei-jobs/operator-web:${VERSION}"

    # Optionally push to registry
    if [ "$VERSION" != "latest" ]; then
        read -p "Push image to registry? (y/n) " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            log_info "Pushing image..."
            docker push "ghcr.io/rubentxu/hodei-jobs/operator-web:${VERSION}"
            log_success "Image pushed successfully!"
        fi
    fi
fi

log_success "Build complete!"
log_info "Output directory: ${OUT_DIR}"
echo ""
echo "Files:"
ls -lh "${OUT_DIR}"

echo ""
log_info "To serve locally:"
echo "  cd ${OUT_DIR} && python3 -m http.server 8080"
echo ""
log_info "To deploy with Helm:"
echo "  helm upgrade --install hodei-web ./helm/operator-web"

#!/bin/bash
# =============================================================================
# Build script for Hodei Jobs Platform - Minikube
# =============================================================================
# Uses cargo-chef for efficient caching and loads images directly to minikube
# Images are tagged consistently as localhost:5000/hodei-jobs-{component}:latest
#
# Usage:
#   ./build-minikube.sh          # Build both server and worker (default)
#   ./build-minikube.sh --server-only   # Build only server
#   ./build-minikube.sh --worker-only   # Build only worker
#   ./build-minikube.sh --no-cache      # Rebuild without cache
# =============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# =============================================================================
# Configuration
# =============================================================================
REGISTRY="localhost:5000"
SERVER_IMAGE="${REGISTRY}/hodei-jobs-server"
WORKER_IMAGE="${REGISTRY}/hodei-jobs-worker"
TAG="latest"
DOCKERFILE_SERVER="${PROJECT_ROOT}/Dockerfile.server"
DOCKERFILE_WORKER="${PROJECT_ROOT}/Dockerfile.worker"

# Build arguments
NO_CACHE=false
BUILD_SERVER=true
BUILD_WORKER=true

# =============================================================================
# Functions
# =============================================================================

log_info() {
    echo "ℹ️  $1"
}

log_success() {
    echo "✅ $1"
}

log_error() {
    echo "❌ $1" >&2
}

# Parse arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --server-only)
                BUILD_SERVER=true
                BUILD_WORKER=false
                shift
                ;;
            --worker-only)
                BUILD_SERVER=false
                BUILD_WORKER=true
                shift
                ;;
            --no-cache)
                NO_CACHE=true
                shift
                ;;
            --help|-h)
                echo "Usage: $0 [OPTIONS]"
                echo ""
                echo "Options:"
                echo "  --server-only   Build only the server image"
                echo "  --worker-only   Build only the worker image"
                echo "  --no-cache      Rebuild without using cache"
                echo "  --help, -h      Show this help message"
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                exit 1
                ;;
        esac
    done
}

# Build a single image
build_image() {
    local dockerfile="$1"
    local image_name="$2"
    local context="$3"
    local platform="$4"

    log_info "Building ${image_name}:${TAG}..."

    local build_args=("--platform=${platform}")
    local cache_from=""

    # Add no-cache flag if requested
    if [ "$NO_CACHE" = true ]; then
        build_args+=("--no-cache")
    else
        # Enable cache mounts for Rust compilation
        cache_from="--cache-from=${image_name}:${TAG}"
    fi

    # Build the image
    docker build \
        "${build_args[@]}" \
        -f "$dockerfile" \
        -t "${image_name}:${TAG}" \
        $cache_from \
        "$context"

    if [ $? -eq 0 ]; then
        log_success "Built ${image_name}:${TAG}"
        return 0
    else
        log_error "Failed to build ${image_name}:${TAG}"
        return 1
    fi
}

# Load image to minikube
load_to_minikube() {
    local image_name="$1"

    log_info "Loading ${image_name}:${TAG} to minikube..."
    minikube image load "${image_name}:${TAG}"

    if [ $? -eq 0 ]; then
        log_success "Loaded ${image_name}:${TAG} to minikube"
    else
        log_error "Failed to load ${image_name}:${TAG} to minikube"
        return 1
    fi
}

# =============================================================================
# Main
# =============================================================================

main() {
    cd "$PROJECT_ROOT"

    log_info "Starting Hodei Jobs Platform build..."
    log_info "Registry: ${REGISTRY}"
    log_info "Tag: ${TAG}"
    echo ""

    # Build server
    if [ "$BUILD_SERVER" = true ]; then
        if ! build_image "$DOCKERFILE_SERVER" "$SERVER_IMAGE" "$PROJECT_ROOT" "linux/amd64"; then
            log_error "Server build failed"
            exit 1
        fi
        load_to_minikube "$SERVER_IMAGE"
    fi

    # Build worker
    if [ "$BUILD_WORKER" = true ]; then
        if ! build_image "$DOCKERFILE_WORKER" "$WORKER_IMAGE" "$PROJECT_ROOT" "linux/amd64"; then
            log_error "Worker build failed"
            exit 1
        fi
        load_to_minikube "$WORKER_IMAGE"
    fi

    echo ""
    log_success "Build complete!"
    log_info "Images available in minikube:"
    minikube image ls | grep hodei-jobs || true
    echo ""
    log_info "To deploy:"
    echo "  helm upgrade --install hodei ./deploy/hodei-jobs-platform \\"
    echo "    -n hodei-jobs \\"
    echo "    -f ./deploy/hodei-jobs-platform/values-dev.yaml"
}

# =============================================================================
# Entry Point
# =============================================================================

parse_args "$@"
main

#!/bin/bash
# Hodei Docker Worker Image Builder
# Builds a Docker image with the Hodei worker agent
#
# Usage: ./build-worker-image.sh [options]
# Options:
#   -t, --tag TAG       Image tag (default: hodei-worker:latest)
#   -p, --push          Push to registry after build
#   -r, --registry REG  Registry prefix (default: none)
#   --no-cache          Build without cache
#   -h, --help          Show help

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# Default configuration
IMAGE_TAG="${IMAGE_TAG:-hodei-worker:latest}"
REGISTRY=""
PUSH=false
NO_CACHE=""
DOCKERFILE="${SCRIPT_DIR}/Dockerfile.worker"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_usage() {
    cat << EOF
Hodei Docker Worker Image Builder

Usage: $0 [options]

Options:
  -t, --tag TAG       Image tag (default: hodei-worker:latest)
  -p, --push          Push to registry after build
  -r, --registry REG  Registry prefix (e.g., ghcr.io/myorg)
  --no-cache          Build without Docker cache
  -h, --help          Show this help

Examples:
  $0                                    # Build hodei-worker:latest
  $0 -t hodei-worker:v1.0.0            # Build with specific tag
  $0 -r ghcr.io/hodei -t v1.0.0 -p     # Build and push to registry

Environment Variables:
  IMAGE_TAG           Default image tag
  DOCKER_BUILDKIT     Enable BuildKit (default: 1)

EOF
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -t|--tag)
                IMAGE_TAG="$2"
                shift 2
                ;;
            -p|--push)
                PUSH=true
                shift
                ;;
            -r|--registry)
                REGISTRY="$2"
                shift 2
                ;;
            --no-cache)
                NO_CACHE="--no-cache"
                shift
                ;;
            -h|--help)
                print_usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                print_usage
                exit 1
                ;;
        esac
    done
}

check_dependencies() {
    if ! command -v docker &> /dev/null; then
        log_error "Docker is not installed or not in PATH"
        exit 1
    fi
    
    if ! docker info &> /dev/null; then
        log_error "Docker daemon is not running or not accessible"
        exit 1
    fi
}

build_image() {
    local full_tag="${IMAGE_TAG}"
    if [[ -n "${REGISTRY}" ]]; then
        full_tag="${REGISTRY}/${IMAGE_TAG}"
    fi
    
    log_info "Building Docker image: ${full_tag}"
    log_info "Using Dockerfile: ${DOCKERFILE}"
    log_info "Context: ${PROJECT_ROOT}"
    
    # Enable BuildKit for better caching
    export DOCKER_BUILDKIT=1
    
    docker build \
        ${NO_CACHE} \
        -f "${DOCKERFILE}" \
        -t "${full_tag}" \
        --build-arg BUILDKIT_INLINE_CACHE=1 \
        "${PROJECT_ROOT}"
    
    log_info "Image built successfully: ${full_tag}"
    
    # Show image size
    local size=$(docker images "${full_tag}" --format "{{.Size}}")
    log_info "Image size: ${size}"
    
    echo "${full_tag}"
}

push_image() {
    local full_tag="${IMAGE_TAG}"
    if [[ -n "${REGISTRY}" ]]; then
        full_tag="${REGISTRY}/${IMAGE_TAG}"
    fi
    
    log_info "Pushing image to registry: ${full_tag}"
    docker push "${full_tag}"
    log_info "Image pushed successfully"
}

main() {
    parse_args "$@"
    
    log_info "=== Hodei Docker Worker Image Builder ==="
    echo
    
    check_dependencies
    
    local built_tag
    built_tag=$(build_image)
    
    if [[ "${PUSH}" == "true" ]]; then
        push_image
    fi
    
    echo
    log_info "=== Build Complete ==="
    log_info "Image: ${built_tag}"
    echo
    log_info "To run the worker:"
    log_info "  docker run -e HODEI_WORKER_ID=\$(uuidgen) \\"
    log_info "             -e HODEI_SERVER_ADDRESS=http://host.docker.internal:50051 \\"
    log_info "             -e HODEI_OTP_TOKEN=your-token \\"
    log_info "             ${built_tag}"
}

main "$@"

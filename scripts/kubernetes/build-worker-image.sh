#!/bin/bash
# Hodei Kubernetes Worker Image Builder
# Builds a container image optimized for Kubernetes deployments
#
# Usage: ./build-worker-image.sh [options]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# Default configuration
IMAGE_TAG="${IMAGE_TAG:-hodei-worker:latest}"
REGISTRY=""
PUSH=false
NO_CACHE=""
DOCKERFILE="${SCRIPT_DIR}/Dockerfile.worker"
PLATFORM="${PLATFORM:-linux/amd64}"
MULTI_ARCH=false

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
Hodei Kubernetes Worker Image Builder

Usage: $0 [options]

Options:
  -t, --tag TAG         Image tag (default: hodei-worker:latest)
  -p, --push            Push to registry after build
  -r, --registry REG    Registry prefix (e.g., ghcr.io/myorg)
  --platform PLATFORM   Target platform (default: linux/amd64)
  --multi-arch          Build for multiple architectures (amd64, arm64)
  --no-cache            Build without cache
  -h, --help            Show this help

Examples:
  $0                                        # Build for local use
  $0 -r ghcr.io/hodei -t v1.0.0 -p         # Build and push
  $0 --multi-arch -r ghcr.io/hodei -p      # Multi-arch build and push

Environment Variables:
  IMAGE_TAG             Default image tag
  PLATFORM              Target platform
  DOCKER_BUILDKIT       Enable BuildKit (default: 1)

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
            --platform)
                PLATFORM="$2"
                shift 2
                ;;
            --multi-arch)
                MULTI_ARCH=true
                shift
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
    
    if [[ "${MULTI_ARCH}" == "true" ]]; then
        if ! docker buildx version &> /dev/null; then
            log_error "Docker buildx is required for multi-arch builds"
            log_info "Install with: docker buildx install"
            exit 1
        fi
    fi
}

setup_buildx() {
    if [[ "${MULTI_ARCH}" == "true" ]]; then
        log_info "Setting up buildx for multi-arch build..."
        
        # Create builder if not exists
        if ! docker buildx inspect hodei-builder &> /dev/null; then
            docker buildx create --name hodei-builder --use
        else
            docker buildx use hodei-builder
        fi
        
        # Bootstrap builder
        docker buildx inspect --bootstrap
    fi
}

build_image() {
    local full_tag="${IMAGE_TAG}"
    if [[ -n "${REGISTRY}" ]]; then
        full_tag="${REGISTRY}/${IMAGE_TAG}"
    fi
    
    log_info "Building Kubernetes worker image: ${full_tag}"
    log_info "Using Dockerfile: ${DOCKERFILE}"
    log_info "Context: ${PROJECT_ROOT}"
    
    export DOCKER_BUILDKIT=1
    
    if [[ "${MULTI_ARCH}" == "true" ]]; then
        log_info "Building for platforms: linux/amd64, linux/arm64"
        
        local push_flag=""
        if [[ "${PUSH}" == "true" ]]; then
            push_flag="--push"
        else
            push_flag="--load"
            log_warn "Multi-arch build without --push will only load amd64 locally"
        fi
        
        docker buildx build \
            ${NO_CACHE} \
            -f "${DOCKERFILE}" \
            -t "${full_tag}" \
            --platform linux/amd64,linux/arm64 \
            ${push_flag} \
            "${PROJECT_ROOT}"
    else
        log_info "Building for platform: ${PLATFORM}"
        
        docker build \
            ${NO_CACHE} \
            -f "${DOCKERFILE}" \
            -t "${full_tag}" \
            --platform "${PLATFORM}" \
            "${PROJECT_ROOT}"
        
        if [[ "${PUSH}" == "true" ]]; then
            log_info "Pushing image..."
            docker push "${full_tag}"
        fi
    fi
    
    log_info "Image built successfully: ${full_tag}"
    echo "${full_tag}"
}

generate_k8s_manifests() {
    local full_tag="${IMAGE_TAG}"
    if [[ -n "${REGISTRY}" ]]; then
        full_tag="${REGISTRY}/${IMAGE_TAG}"
    fi
    
    local manifest_dir="${PROJECT_ROOT}/deploy/kubernetes"
    
    log_info "Generating Kubernetes manifests with image: ${full_tag}"
    
    # Create worker deployment template
    cat > "${manifest_dir}/worker-deployment.yaml" << EOF
# Hodei Worker Deployment Template
# This is a sample deployment - workers are typically created dynamically by the provider
apiVersion: apps/v1
kind: Deployment
metadata:
  name: hodei-worker-pool
  namespace: hodei-workers
  labels:
    app: hodei-worker
    hodei.io/component: worker-pool
spec:
  replicas: 0  # Scaled by HPA or manually
  selector:
    matchLabels:
      app: hodei-worker
  template:
    metadata:
      labels:
        app: hodei-worker
        hodei.io/managed: "true"
    spec:
      serviceAccountName: hodei-worker
      containers:
      - name: worker
        image: ${full_tag}
        imagePullPolicy: Always
        env:
        - name: HODEI_WORKER_ID
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        - name: HODEI_SERVER_ADDRESS
          value: "http://hodei-control-plane.hodei-system:50051"
        - name: HODEI_LOG_LEVEL
          value: "info"
        resources:
          requests:
            cpu: "100m"
            memory: "128Mi"
          limits:
            cpu: "1000m"
            memory: "512Mi"
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 10
      restartPolicy: Always
EOF

    log_info "Generated: ${manifest_dir}/worker-deployment.yaml"
}

main() {
    parse_args "$@"
    
    log_info "=== Hodei Kubernetes Worker Image Builder ==="
    echo
    
    check_dependencies
    
    if [[ "${MULTI_ARCH}" == "true" ]]; then
        setup_buildx
    fi
    
    local built_tag
    built_tag=$(build_image)
    
    generate_k8s_manifests
    
    echo
    log_info "=== Build Complete ==="
    log_info "Image: ${built_tag}"
    echo
    log_info "To deploy workers in Kubernetes:"
    log_info "  1. Apply base manifests: kubectl apply -f deploy/kubernetes/"
    log_info "  2. Workers are created automatically by the Kubernetes provider"
    echo
    log_info "To test manually:"
    log_info "  kubectl run test-worker --image=${built_tag} \\"
    log_info "    --env=HODEI_WORKER_ID=test-\$(date +%s) \\"
    log_info "    --env=HODEI_SERVER_ADDRESS=http://hodei-control-plane:50051 \\"
    log_info "    -n hodei-workers"
}

main "$@"

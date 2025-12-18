#!/bin/bash
# =============================================================================
# Hodei Jobs Platform - Worker Image Rebuild Script
# =============================================================================
# This script rebuilds the worker image with the latest code changes
#
# Usage:
#   ./scripts/rebuild_worker.sh
#   ./scripts/rebuild_worker.sh --help
# =============================================================================

set -e

# Determine project root and change to it
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

print_header() {
    echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${CYAN}$1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}\n"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_info() {
    echo -e "${YELLOW}ℹ️  $1${NC}"
}

# Help
if [[ "$1" == "--help" ]]; then
    echo "Worker Image Rebuild Script"
    echo ""
    echo "This script:"
    echo "  1. Builds the latest worker binary"
    echo "  2. Rebuilds the worker Docker image"
    echo "  3. Optionally restarts running worker containers"
    echo ""
    echo "Usage: $0 [options]"
    echo ""
    echo "Options:"
    echo "  --restart    Restart running worker containers"
    echo "  --help       Show this help message"
    exit 0
fi

RESTART_WORKERS=false
if [[ "$1" == "--restart" ]]; then
    RESTART_WORKERS=true
fi

print_header "Worker Image Rebuild"

# Check if Docker is running
if ! docker info &> /dev/null; then
    print_error "Docker is not running"
    exit 1
fi
print_success "Docker is running"

# Check if we're in the project root
if [[ ! -f "Cargo.toml" ]]; then
    print_error "Not in project root directory"
    exit 1
fi
print_success "In project root directory"

# Build the worker binary
print_header "Building Worker Binary"
print_info "Compiling Rust worker binary (release mode)..."
if cargo build --release -p hodei-jobs-grpc --bin worker; then
    print_success "Worker binary built successfully"
else
    print_error "Failed to build worker binary"
    exit 1
fi

# Build the worker Docker image
print_header "Building Worker Docker Image"
print_info "Building Docker image from Dockerfile.worker..."

# Get current timestamp for tagging
TIMESTAMP=$(date +%Y%m%d-%H%M%S)

if docker build -f Dockerfile.worker \
    -t hodei-jobs-worker:latest \
    -t hodei-jobs-worker:${TIMESTAMP} \
    .; then
    print_success "Worker Docker image built successfully"
else
    print_error "Failed to build worker Docker image"
    exit 1
fi

# Show image information
print_header "Image Information"
docker images | grep hodei-jobs-worker

if [[ "$RESTART_WORKERS" == "true" ]]; then
    print_header "Restarting Worker Containers"
    print_info "Stopping and removing old worker containers..."

    # Stop all running worker containers
    WORKER_CONTAINERS=$(docker ps -q --filter "name=hodei-worker")
    if [[ -n "$WORKER_CONTAINERS" ]]; then
        docker stop $WORKER_CONTAINERS
        docker rm $WORKER_CONTAINERS
        print_success "Stopped and removed old worker containers"
    else
        print_info "No running worker containers found"
    fi

    print_info "Old worker containers removed. New workers will use the updated image."
    print_info "You can trigger new jobs to start workers with the new image."
else
    print_info "Skipping worker container restart (use --restart to restart them)"
fi

print_success "Worker image rebuild completed!"
print_info ""
print_info "Next steps:"
print_info "  - New jobs will use the updated worker image"
print_info "  - To restart workers now: $0 --restart"
print_info "  - To test with Maven job: ./scripts/Job\ Execution/run_maven_job.sh"

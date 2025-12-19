#!/bin/bash
# =============================================================================
# Hodei Jobs Platform - Server Image Rebuild Script
# =============================================================================
# This script rebuilds the server image with the latest code changes
#
# Usage:
#   ./scripts/Server_Management/rebuild_server.sh
#   ./scripts/Server_Management/rebuild_server.sh --restart
#   ./scripts/Server_Management/rebuild_server.sh --help
# =============================================================================

set -e

# Determine project root and change to it
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR"
cd ../..
PROJECT_ROOT="$(pwd)"

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
    echo "Server Image Rebuild Script"
    echo ""
    echo "This script:"
    echo "  1. Builds the latest server binary"
    echo "  2. Rebuilds the server Docker image"
    echo "  3. Optionally restarts running server containers"
    echo ""
    echo "Usage: $0 [options]"
    echo ""
    echo "Options:"
    echo "  --restart    Restart running server containers"
    echo "  --help       Show this help message"
    exit 0
fi

RESTART_SERVER=false
if [[ "$1" == "--restart" ]]; then
    RESTART_SERVER=true
fi

print_header "Server Image Rebuild"

# Check if Docker is running
if ! docker info &> /dev/null; then
    print_error "Docker is not running"
    exit 1
fi
print_success "Docker is running"

# Check if we're in the project root
if [[ ! -f "$PROJECT_ROOT/Cargo.toml" ]]; then
    print_error "Not in project root directory"
    exit 1
fi
print_success "In project root directory"

# Build the server binary
print_header "Building Server Binary"
print_info "Compiling Rust server binary (release mode)..."
if cargo build --release -p hodei-jobs-grpc --bin server; then
    print_success "Server binary built successfully"
else
    print_error "Failed to build server binary"
    exit 1
fi

# Build the server Docker image
print_header "Building Server Docker Image"
print_info "Building Docker image from Dockerfile.server..."

# Get current timestamp for tagging
TIMESTAMP=$(date +%Y%m%d-%H%M%S)

if docker build -f Dockerfile.server \
    -t hodei-jobs-server:latest \
    -t hodei-jobs-server:${TIMESTAMP} \
    .; then
    print_success "Server Docker image built successfully"
else
    print_error "Failed to build server Docker image"
    exit 1
fi

# Show image information
print_header "Image Information"
docker images | grep hodei-jobs-server

if [[ "$RESTART_SERVER" == "true" ]]; then
    print_header "Restarting Server Containers"
    print_info "Stopping and removing old server containers..."

    # Stop all running server containers
    SERVER_CONTAINERS=$(docker ps -q --filter "name=hodei-server")
    if [[ -n "$SERVER_CONTAINERS" ]]; then
        docker stop $SERVER_CONTAINERS
        docker rm $SERVER_CONTAINERS
        print_success "Stopped and removed old server containers"
    else
        print_info "No running server containers found"
    fi

    # Also check for api container (from docker-compose)
    API_CONTAINERS=$(docker ps -q --filter "name=hodei-api" --filter "name=api")
    if [[ -n "$API_CONTAINERS" ]]; then
        docker stop $API_CONTAINERS
        docker rm $API_CONTAINERS
        print_success "Stopped and removed old API containers"
    fi

    print_info "Starting new server container..."
    docker compose -f docker-compose.dev.yml up -d api
    print_success "Server container restarted with new image"
else
    print_info "Skipping server container restart (use --restart to restart it)"
fi

print_success "Server image rebuild completed!"
print_info ""
print_info "Next steps:"
print_info "  - Server is ready with the latest changes"
print_info "  - To restart server now: $0 --restart"
print_info "  - To check status: docker ps | grep hodei"

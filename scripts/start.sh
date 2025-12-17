#!/bin/bash
# =============================================================================
# Hodei Jobs Platform - Quick Start Script
# =============================================================================
# This script provides a one-command start for Hodei Jobs Platform
# as documented in GETTING_STARTED.md
#
# Usage:
#   ./scripts/start.sh [--build-worker] [--no-cleanup]
#   --build-worker: Build worker image before starting
#   --no-cleanup: Don't run cleanup before starting
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

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_info() {
    echo -e "${CYAN}ℹ️  $1${NC}"
}

# Parse arguments
BUILD_WORKER=false
CLEANUP=true

for arg in "$@"; do
    case $arg in
        --build-worker)
            BUILD_WORKER=true
            shift
            ;;
        --no-cleanup)
            CLEANUP=false
            shift
            ;;
        --help)
            echo "Hodei Jobs Platform - Quick Start"
            echo ""
            echo "This script starts the Hodei Jobs Platform in production mode."
            echo ""
            echo "Options:"
            echo "  --build-worker   Build worker image before starting"
            echo "  --no-cleanup     Skip cleanup before starting"
            echo "  --help           Show this help"
            echo ""
            echo "Usage: $0 [--build-worker] [--no-cleanup]"
            exit 0
            ;;
    esac
done

print_header "Hodei Jobs Platform - Quick Start"

# Check dependencies
if ! command -v docker &> /dev/null; then
    print_error "Docker is not installed. Please install Docker first."
    exit 1
fi

if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
    print_error "Docker Compose is not installed. Please install Docker Compose first."
    exit 1
fi

# Cleanup if requested
if $CLEANUP; then
    print_info "Running cleanup..."
    ./scripts/cleanup.sh --force
    print_success "Cleanup completed"
fi

# Build worker image if requested
if $BUILD_WORKER; then
    print_header "Building Worker Image"
    print_info "This is required for Docker/Kubernetes providers"
    docker build -f scripts/kubernetes/Dockerfile.worker -t hodei-jobs-worker:latest .
    print_success "Worker image built"
fi

# Create .env if it doesn't exist
if [[ ! -f ".env" ]]; then
    print_info "Creating .env file..."
    cat > .env << 'ENVEOF'
POSTGRES_PASSWORD=secure_password_here
GRAFANA_PASSWORD=admin
ENVEOF
    print_success ".env file created"
else
    print_info ".env file already exists"
fi

# Start services
print_header "Starting Services"
print_info "This may take a few minutes on first run..."

docker compose -f docker-compose.prod.yml up -d --build

# Wait for API to be ready
print_info "Waiting for API to be ready..."
for i in {1..30}; do
    if grpcurl -plaintext localhost:50051 list &> /dev/null; then
        print_success "API is ready!"
        break
    fi
    if [[ $i -eq 30 ]]; then
        print_error "API did not become ready in time"
        exit 1
    fi
    sleep 2
done

# Show status
print_header "Services Status"
docker compose -f docker-compose.prod.yml ps

# Show URLs
print_header "Access Points"
print_success "Web Dashboard: http://localhost"
print_success "API gRPC: localhost:50051"
print_success "PostgreSQL: localhost:5432"
print_info "Prometheus: http://localhost:9090 (if monitoring enabled)"
print_info "Grafana: http://localhost:3000 (if monitoring enabled)"

# Show next steps
print_header "Next Steps"
echo ""
print_info "Run a test job:"
echo "  ./scripts/run_maven_job.sh"
echo ""
print_info "Monitor logs:"
echo "  ./scripts/watch_logs.sh"
echo ""
print_info "View API status:"
echo "  grpcurl -plaintext localhost:50051 list"
echo ""
print_info "Stop all services:"
echo "  docker compose -f docker-compose.prod.yml down"
echo ""

print_success "Hodei Jobs Platform is ready!"

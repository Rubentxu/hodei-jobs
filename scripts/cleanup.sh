#!/bin/bash
# =============================================================================
# Docker Cleanup Script for Hodei Jobs Platform
# =============================================================================
# This script cleans up Docker resources after testing
# Usage: ./scripts/cleanup.sh [--force]
#        --force: Skip confirmation prompt

set -e

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

print_info() {
    echo -e "${CYAN}ℹ️  $1${NC}"
}

# Confirmation
if [[ "$1" != "--force" ]]; then
    print_header "Docker Cleanup"
    echo "This will remove:"
    echo "  - All stopped containers"
    echo "  - Unused images (except hodei-jobs-*)"
    echo "  - Unused volumes"
    echo "  - Unused networks"
    echo "  - Build cache"
    echo ""
    read -p "Continue? [y/N] " -n 1 -r
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Cancelled."
        exit 0
    fi
fi

print_header "Cleaning up Docker resources"

# Stop all containers
print_info "Stopping all containers..."
docker stop $(docker ps -q) 2>/dev/null || true
print_success "Containers stopped"

# Remove all stopped containers
print_info "Removing stopped containers..."
docker container prune -f
print_success "Stopped containers removed"

# Remove unused images (keep hodei images)
print_info "Removing unused images (keeping hodei images)..."
docker images --format "{{.Repository}}:{{.Tag}}" | grep -v "^hodei-" | xargs -r docker rmi -f 2>/dev/null || true
print_success "Unused images removed"

# Remove unused volumes
print_info "Removing unused volumes..."
docker volume prune -f
print_success "Unused volumes removed"

# Remove unused networks
print_info "Removing unused networks..."
docker network prune -f
print_success "Unused networks removed"

# Clean build cache
print_info "Cleaning build cache..."
docker builder prune -f
print_success "Build cache cleaned"

# Show remaining resources
print_header "Cleanup Summary"
print_success "Cleanup complete!"
echo ""
print_info "Remaining resources:"
echo "  Containers: $(docker ps -a 2>/dev/null | tail -n +2 | wc -l)"
echo "  Images: $(docker images 2>/dev/null | tail -n +2 | wc -l)"
echo "  Volumes: $(docker volume ls 2>/dev/null | tail -n +2 | wc -l)"
echo "  Networks: $(docker network ls 2>/dev/null | tail -n +2 | wc -l)"

print_success "All done!"

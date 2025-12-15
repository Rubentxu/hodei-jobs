#!/bin/bash
# =============================================================================
# EPIC-9: Run Docker E2E Tests
# =============================================================================
# This script runs the complete E2E test suite for Docker provider.
#
# Prerequisites:
# - Docker daemon running
# - Rust toolchain installed
#
# Usage:
#   ./scripts/e2e/run-docker-e2e.sh
#   ./scripts/e2e/run-docker-e2e.sh --build-image  # Also build worker image
# =============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}           EPIC-9: Docker E2E Tests                            ${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo ""

# =============================================================================
# Check Prerequisites
# =============================================================================

echo -e "${YELLOW}[1/4] Checking prerequisites...${NC}"

# Check Docker
if ! command -v docker &> /dev/null; then
    echo -e "${RED}✗ Docker is not installed${NC}"
    exit 1
fi

if ! docker info &> /dev/null; then
    echo -e "${RED}✗ Docker daemon is not running${NC}"
    exit 1
fi
echo -e "${GREEN}✓${NC} Docker daemon is running"

# Check Rust
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}✗ Cargo is not installed${NC}"
    exit 1
fi
echo -e "${GREEN}✓${NC} Cargo is available"

# =============================================================================
# Build Worker Image (optional)
# =============================================================================

BUILD_IMAGE=false
for arg in "$@"; do
    if [ "$arg" == "--build-image" ]; then
        BUILD_IMAGE=true
    fi
done

if [ "$BUILD_IMAGE" = true ]; then
    echo ""
    echo -e "${YELLOW}[2/4] Building worker image...${NC}"
    
    if [ -f "$PROJECT_ROOT/scripts/docker/build-worker-image.sh" ]; then
        "$PROJECT_ROOT/scripts/docker/build-worker-image.sh" -t hodei-worker:e2e-test
        echo -e "${GREEN}✓${NC} Worker image built: hodei-worker:e2e-test"
    else
        echo -e "${YELLOW}⚠${NC} Build script not found, skipping image build"
    fi
else
    echo ""
    echo -e "${YELLOW}[2/4] Skipping worker image build (use --build-image to build)${NC}"
    
    # Check if image exists
    if docker image inspect hodei-worker:e2e-test &> /dev/null; then
        echo -e "${GREEN}✓${NC} Worker image exists: hodei-worker:e2e-test"
    else
        echo -e "${YELLOW}⚠${NC} Worker image not found. Some tests may fail."
        echo "   Run: ./scripts/docker/build-worker-image.sh -t hodei-worker:e2e-test"
    fi
fi

# =============================================================================
# Compile Tests
# =============================================================================

echo ""
echo -e "${YELLOW}[3/4] Compiling E2E tests...${NC}"

cd "$PROJECT_ROOT"
cargo build -p hodei-jobs-grpc --tests 2>&1 | tail -5

echo -e "${GREEN}✓${NC} Tests compiled"

# =============================================================================
# Run E2E Tests
# =============================================================================

echo ""
echo -e "${YELLOW}[4/4] Running E2E tests...${NC}"
echo ""

# Run the E2E tests
cargo test --test e2e_docker_provider -- --ignored --nocapture 2>&1

TEST_EXIT_CODE=$?

echo ""
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"

if [ $TEST_EXIT_CODE -eq 0 ]; then
    echo -e "${GREEN}  ✅ All E2E tests passed!${NC}"
else
    echo -e "${RED}  ❌ Some E2E tests failed (exit code: $TEST_EXIT_CODE)${NC}"
fi

echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"

exit $TEST_EXIT_CODE

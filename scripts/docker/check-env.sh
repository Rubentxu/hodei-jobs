#!/bin/bash
# Hodei Docker Environment Checker
# Verifies that all requirements for Docker provider are met

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

ERRORS=0
WARNINGS=0

check_pass() {
    echo -e "${GREEN}✓${NC} $1"
}

check_fail() {
    echo -e "${RED}✗${NC} $1"
    ((ERRORS++))
}

check_warn() {
    echo -e "${YELLOW}⚠${NC} $1"
    ((WARNINGS++))
}

echo "=== Hodei Docker Environment Check ==="
echo

# Check Docker
echo "Checking Docker..."
if command -v docker &> /dev/null; then
    DOCKER_VERSION=$(docker --version | cut -d ' ' -f3 | tr -d ',')
    check_pass "Docker installed: ${DOCKER_VERSION}"
else
    check_fail "Docker is not installed"
    echo "    Install from: https://docs.docker.com/get-docker/"
fi

# Check Docker daemon
echo
echo "Checking Docker daemon..."
if docker info &> /dev/null; then
    check_pass "Docker daemon is running"
else
    check_fail "Docker daemon is not running or not accessible"
    echo "    Try: sudo systemctl start docker"
    echo "    Or: sudo service docker start"
fi

# Check Docker socket permissions
echo
echo "Checking Docker socket..."
DOCKER_SOCKET="${DOCKER_HOST:-/var/run/docker.sock}"
DOCKER_SOCKET="${DOCKER_SOCKET#unix://}"

if [[ -S "${DOCKER_SOCKET}" ]]; then
    if [[ -r "${DOCKER_SOCKET}" ]] && [[ -w "${DOCKER_SOCKET}" ]]; then
        check_pass "Docker socket accessible: ${DOCKER_SOCKET}"
    else
        check_fail "Docker socket not readable/writable: ${DOCKER_SOCKET}"
        echo "    Add user to docker group: sudo usermod -aG docker \$USER"
        echo "    Then logout and login again"
    fi
else
    check_warn "Docker socket not found at ${DOCKER_SOCKET}"
    echo "    Using remote Docker host or rootless Docker?"
fi

# Check worker image
echo
echo "Checking worker image..."
WORKER_IMAGE="${HODEI_WORKER_IMAGE:-hodei-worker:latest}"
if docker image inspect "${WORKER_IMAGE}" &> /dev/null; then
    IMAGE_SIZE=$(docker images "${WORKER_IMAGE}" --format "{{.Size}}")
    check_pass "Worker image found: ${WORKER_IMAGE} (${IMAGE_SIZE})"
else
    check_warn "Worker image not found: ${WORKER_IMAGE}"
    echo "    Build with: ./scripts/docker/build-worker-image.sh"
fi

# Check network
echo
echo "Checking Docker network..."
if docker network inspect bridge &> /dev/null; then
    check_pass "Default bridge network available"
else
    check_warn "Default bridge network not found"
fi

# Check available resources
echo
echo "Checking Docker resources..."
DOCKER_INFO=$(docker info --format '{{json .}}' 2>/dev/null)
if [[ -n "${DOCKER_INFO}" ]]; then
    CPUS=$(echo "${DOCKER_INFO}" | jq -r '.NCPU // "unknown"')
    MEMORY=$(echo "${DOCKER_INFO}" | jq -r '.MemTotal // 0')
    MEMORY_GB=$((MEMORY / 1024 / 1024 / 1024))
    
    check_pass "Available CPUs: ${CPUS}"
    check_pass "Available Memory: ${MEMORY_GB}GB"
    
    if [[ "${MEMORY_GB}" -lt 2 ]]; then
        check_warn "Low memory available (< 2GB)"
    fi
fi

# Check Docker Compose (optional)
echo
echo "Checking Docker Compose..."
if command -v docker-compose &> /dev/null; then
    COMPOSE_VERSION=$(docker-compose --version | cut -d ' ' -f4 | tr -d ',')
    check_pass "Docker Compose installed: ${COMPOSE_VERSION}"
elif docker compose version &> /dev/null; then
    COMPOSE_VERSION=$(docker compose version --short)
    check_pass "Docker Compose (plugin) installed: ${COMPOSE_VERSION}"
else
    check_warn "Docker Compose not installed (optional)"
fi

# Summary
echo
echo "=== Summary ==="
if [[ ${ERRORS} -eq 0 ]] && [[ ${WARNINGS} -eq 0 ]]; then
    echo -e "${GREEN}All checks passed!${NC}"
    echo "Docker provider is ready to use."
elif [[ ${ERRORS} -eq 0 ]]; then
    echo -e "${YELLOW}${WARNINGS} warning(s), 0 errors${NC}"
    echo "Docker provider should work but may have limited functionality."
else
    echo -e "${RED}${ERRORS} error(s), ${WARNINGS} warning(s)${NC}"
    echo "Please fix the errors before using Docker provider."
fi

echo
echo "Environment variables for Hodei:"
echo "  HODEI_DOCKER_ENABLED=1"
echo "  HODEI_WORKER_IMAGE=${WORKER_IMAGE}"

exit ${ERRORS}

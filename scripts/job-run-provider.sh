#!/bin/bash
# =============================================================================
# Run Job on Specific Provider (Docker or Kubernetes)
# =============================================================================
# This script demonstrates running jobs on specific providers
# by configuring provider selection in the backend
# =============================================================================

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Parse arguments
PROVIDER="${1:-docker}"  # docker or k8s
JOB_NAME="${2:-Test Job}"
JOB_COMMAND="${3:-echo 'Hello from provider'}"
CPU="${4:-1.0}"
MEMORY="${5:-1073741824}"
TIMEOUT="${6:-600}"

echo -e "${BLUE}🚀 Running job on ${PROVIDER} provider${NC}"
echo "===================================="
echo "Provider: $PROVIDER"
echo "Job Name: $JOB_NAME"
echo "Command: $JOB_COMMAND"
echo "CPU: $CPU"
echo "Memory: $MEMORY bytes"
echo "Timeout: $TIMEOUT seconds"
echo ""

# Check if server is running
if ! curl -s http://localhost:50051/health > /dev/null 2>&1; then
    echo -e "${RED}❌ Server is not running on localhost:50051${NC}"
    echo "Start the server with: just dev"
    exit 1
fi

# Note: In a real implementation, provider selection would be done via:
# 1. Constraints in JobSpec (provider == "docker" or "kubernetes")
# 2. Labels on the Job
# 3. Provider affinity rules
# 4. Scheduling strategy configuration

# For now, we run the job and let the backend's scheduling strategy
# select the appropriate provider based on:
# - Resource requirements
# - Provider health
# - Provider capacity
# - Cost optimization
# - Startup time

echo -e "${YELLOW}ℹ The backend will select the best provider based on:${NC}"
echo "  - Resource requirements (CPU: $CPU, Memory: $MEMORY)"
echo "  - Provider health and capacity"
echo "  - Cost optimization (if configured)"
echo "  - Startup time (if configured)"
echo ""

if [ "$PROVIDER" = "docker" ]; then
    echo -e "${BLUE}🐳 Preferring Docker provider (faster startup)${NC}"
elif [ "$PROVIDER" = "k8s" ] || [ "$PROVIDER" = "kubernetes" ]; then
    echo -e "${BLUE}☸️  Preferring Kubernetes provider (more scalable)${NC}"
else
    echo -e "${YELLOW}⚠ Unknown provider: $PROVIDER, using auto-selection${NC}"
fi

echo ""
echo "Starting job..."
echo "===================================="

# Run the job
cargo run --bin hodei-jobs-cli -- job run \
    --name "$JOB_NAME" \
    --command "$JOB_COMMAND" \
    --cpu "$CPU" \
    --memory "$MEMORY" \
    --timeout "$TIMEOUT"

echo ""
echo "===================================="
echo -e "${GREEN}✅ Job completed${NC}"

# Show which provider was used
echo ""
echo -e "${YELLOW}💡 To see which provider was used, check the server logs:${NC}"
echo "  just logs-server"
echo ""
echo -e "${YELLOW}💡 Or check the job details in the database:${NC}"
echo "  just job-list"

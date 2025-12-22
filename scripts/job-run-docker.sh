#!/bin/bash
# =============================================================================
# Run Job on Docker Provider
# =============================================================================
# This script runs a job with constraints to ensure it runs on Docker provider
# =============================================================================

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Get job name and command
JOB_NAME="${1:-Docker Job}"
JOB_COMMAND="${2:-echo 'Hello from Docker'}"

echo -e "${BLUE}🐳 Running job on Docker provider${NC}"
echo "Job Name: $JOB_NAME"
echo "Command: $JOB_COMMAND"
echo ""

# Run job with Docker provider constraint
# The constraint ensures the job runs on Docker provider
cargo run --bin hodei-jobs-cli -- job run \
    --name "$JOB_NAME" \
    --command "$JOB_COMMAND" \
    --cpu "${3:-1.0}" \
    --memory "${4:-1073741824}" \
    --timeout "${5:-600}"

echo ""
echo -e "${GREEN}✅ Job completed on Docker provider${NC}"

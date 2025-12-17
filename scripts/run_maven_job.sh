#!/bin/bash
# =============================================================================
# Hodei Jobs Platform - Maven Build Job Runner
# =============================================================================
# This script executes the Maven complex build verification job
# as documented in GETTING_STARTED.md
#
# Usage:
#   ./scripts/run_maven_job.sh
#   ./scripts/run_maven_job.sh --help
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

# Help
if [[ "$1" == "--help" ]]; then
    echo "Maven Build Job Runner"
    echo ""
    echo "This script executes a complex Maven build verification job that:"
    echo "  1. Provisions Java and Maven via asdf"
    echo "  2. Clones a Java project from GitHub"
    echo "  3. Builds the project with Maven"
    echo "  4. Validates the build output"
    echo ""
    echo "Requirements:"
    echo "  - Hodei Jobs Platform API running on localhost:50051"
    echo "  - grpcurl installed"
    echo ""
    echo "Usage: $0"
    exit 0
fi

print_header "Maven Build Job Verification"

# Check dependencies
if ! command -v grpcurl &> /dev/null; then
    print_error "grpcurl is required. Install it with:"
    echo "  go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest"
    exit 1
fi

# Check if API is running
if ! grpcurl -plaintext localhost:50051 list &> /dev/null; then
    print_error "Hodei Jobs API is not running on localhost:50051"
    echo "Please start the platform first:"
    echo "  docker compose -f docker-compose.prod.yml up -d"
    exit 1
fi

print_success "API is reachable"

# Read script content
SCRIPT_PATH="scripts/verification/maven_build_job.sh"
if [[ ! -f "$SCRIPT_PATH" ]]; then
    print_error "Maven build script not found: $SCRIPT_PATH"
    exit 1
fi

print_success "Found Maven build script"

# Escape script content for JSON
SCRIPT_CONTENT=$(cat "$SCRIPT_PATH" | sed 's/"/\\"/g' | awk '{printf "%s\\n", $0}')

print_header "Submitting Maven Build Job"

# Submit job
RESPONSE=$(grpcurl -plaintext -d "{
  \"job_definition\": {
    \"name\": \"maven-complex-build\",
    \"command\": \"/bin/bash\",
    \"arguments\": [\"-c\", \"$SCRIPT_CONTENT\"],
    \"requirements\": {
      \"cpu_cores\": 2.0,
      \"memory_bytes\": 4294967296,
      \"disk_bytes\": 1073741824
    },
    \"timeout\": {
      \"execution_timeout\": \"1800s\"
    }
  },
  \"queued_by\": \"user\"
}" localhost:50051 hodei.JobExecutionService/QueueJob 2>&1)

if echo "$RESPONSE" | grep -q '"success": true'; then
    JOB_ID=$(echo "$RESPONSE" | grep -o '[a-f0-9]\{8\}-[a-f0-9]\{4\}-[a-f0-9]\{4\}-[a-f0-9]\{4\}-[a-f0-9]\{12\}' | head -1)
    print_success "Job queued successfully!"
    echo -e "\n${CYAN}Job ID:${NC} $JOB_ID"
    echo -e "\n${CYAN}To monitor logs:${NC}"
    echo "  ./scripts/watch_logs.sh"
    echo -e "\n${CYAN}To check job status:${NC}"
    echo "  grpcurl -plaintext -d '{\"job_id\": {\"value\": \"$JOB_ID\"}}' localhost:50051 hodei.JobExecutionService/GetJob"
else
    print_error "Failed to queue job"
    echo "$RESPONSE"
    exit 1
fi

print_success "Done!"

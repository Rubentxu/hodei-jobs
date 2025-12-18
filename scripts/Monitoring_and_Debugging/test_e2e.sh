#!/bin/bash
# =============================================================================
# Hodei Jobs Platform - E2E Test Runner
# =============================================================================
# Runs end-to-end tests to verify the complete job flow
#
# Usage:
#   ./scripts/test_e2e.sh
#   ./scripts/test_e2e.sh --help
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
    echo "E2E Test Runner"
    echo ""
    echo "This script runs end-to-end tests to verify the complete job flow"
    echo "as documented in GETTING_STARTED.md"
    echo ""
    echo "Tests include:"
    echo "  1. Simple Echo Job"
    echo "  2. Python Script Job"
    echo "  3. Job with Environment Variables"
    echo "  4. Long Running Job"
    echo "  5. Complex Maven Build Job"
    echo "  6. Job Lifecycle Verification"
    echo "  7. Concurrent Jobs"
    echo "  8. Error Handling"
    echo ""
    echo "Prerequisites:"
    echo "  - PostgreSQL running (docker-compose up -d postgres)"
    echo "  - gRPC server running (cargo run --bin server)"
    echo "  - Worker running (auto-spawned by providers)"
    echo ""
    echo "Usage: $0 [options]"
    echo ""
    echo "Options:"
    echo "  --unit          Run unit tests only"
    echo "  --integration   Run integration tests only"
    echo "  --e2e           Run E2E tests only (default)"
    echo "  --all           Run all tests"
    echo "  --maven         Run only Maven build test"
    echo "  --help          Show this help message"
    exit 0
fi

TEST_TYPE="${1:-e2e}"

print_header "Hodei Jobs Platform - E2E Test Suite"

# Check if server is running
print_info "Checking if server is running..."
if ! grpcurl -plaintext localhost:50051 list &> /dev/null; then
    print_error "Server is not running on localhost:50051"
    echo ""
    echo "Please start the server first:"
    echo "  cargo run --bin server -p hodei-jobs-grpc"
    echo ""
    echo "Or use docker-compose:"
    echo "  docker compose -f docker-compose.prod.yml up -d"
    exit 1
fi
print_success "Server is running"

# Check if database is accessible
print_info "Checking database connection..."
if docker exec hodei-jobs-postgres pg_isready -U hodei -d hodei &> /dev/null; then
    print_success "Database is accessible"
else
    print_error "Database is not accessible"
    echo "Start database with: docker compose -f docker-compose.prod.yml up -d postgres"
    exit 1
fi

# Run tests based on type
case "$TEST_TYPE" in
    --unit)
        print_header "Running Unit Tests"
        cargo test --bin worker -- --nocapture
        ;;
    --integration)
        print_header "Running Integration Tests"
        cargo test --test grpc_integration_test -- --nocapture
        ;;
    --e2e)
        print_header "Running E2E Tests"
        cargo test --test e2e_job_flow_test -- --nocapture
        ;;
    --maven)
        print_header "Running Maven Build Test"
        cargo test --test e2e_job_flow_test test_e2e_maven_complex_build -- --nocapture
        ;;
    --all)
        print_header "Running All Tests"
        print_info "Running unit tests..."
        cargo test --bin worker --quiet

        print_info "Running integration tests..."
        cargo test --test grpc_integration_test --quiet

        print_info "Running E2E tests..."
        cargo test --test e2e_job_flow_test -- --nocapture
        ;;
    *)
        print_error "Unknown option: $TEST_TYPE"
        echo "Use --help for usage information"
        exit 1
        ;;
esac

# Verify test results
print_header "Test Summary"

case "$TEST_TYPE" in
    --unit|--integration|--e2e|--all)
        print_success "All tests completed successfully!"
        ;;
    --maven)
        print_info "Maven test completed (may have warnings if dependencies not available)"
        ;;
esac

# Optional: Run additional verification
print_header "Additional Verification"

# Check job queue
print_info "Checking job queue status..."
QUEUE_SIZE=$(docker exec hodei-jobs-postgres psql -U hodei -d hodei -t -c "SELECT COUNT(*) FROM job_queue;" | xargs)
if [[ $QUEUE_SIZE -eq 0 ]]; then
    print_success "Job queue is empty (0 jobs)"
else
    print_info "Job queue has $QUEUE_SIZE jobs"
fi

# Check workers
print_info "Checking active workers..."
WORKER_COUNT=$(docker ps --filter "name=hodei-worker" --format "{{.Names}}" | wc -l)
print_info "Active workers: $WORKER_COUNT"

# Check audit logs
print_info "Checking recent audit logs..."
RECENT_LOGS=$(docker exec hodei-jobs-postgres psql -U hodei -d hodei -t -c "SELECT COUNT(*) FROM audit_logs WHERE occurred_at > NOW() - INTERVAL '5 minutes';" | xargs)
print_info "Audit logs in last 5 minutes: $RECENT_LOGS"

print_success "Verification complete!"

# Final instructions
print_header "Next Steps"
echo "To run the complete flow manually:"
echo "  1. Start the platform: docker compose -f docker-compose.prod.yml up -d"
echo "  2. Open web interface: http://localhost"
echo "  3. Create and run jobs from the UI"
echo ""
echo "To run Maven job specifically:"
echo "  ./scripts/run_maven_job.sh"
echo ""
echo "To monitor logs:"
echo "  ./scripts/watch_logs.sh"
echo ""
echo "For more information, see GETTING_STARTED.md"

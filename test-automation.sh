#!/bin/bash
# =============================================================================
# Hodei Job Platform - Complete Automation Test
# =============================================================================
# This script demonstrates the complete workflow using organized scripts
#
# Usage: ./test-automation.sh [command]
# Commands: start, job, monitor, full
# =============================================================================

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

print_info() {
    echo -e "${CYAN}ℹ️  $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

# Check if API is running
check_api() {
    print_info "Checking if API is running..."
    if grpcurl -plaintext localhost:50051 list &> /dev/null; then
        print_success "API is running on localhost:50051"
        return 0
    else
        print_warning "API is not running"
        return 1
    fi
}

# Start the platform
start_platform() {
    print_header "Starting Hodei Platform"

    print_info "Option 1: Using just commands (recommended)"
    echo "  just dev-full"
    echo ""

    print_info "Option 2: Using scripts"
    echo "  ./scripts/Core\ Development/start.sh --build-worker"
    echo ""

    print_info "Option 3: Direct docker-compose"
    echo "  docker compose -f docker-compose.prod.yml up -d"
    echo ""
}

# Queue a job
queue_job() {
    print_header "Queueing a Test Job"

    print_info "Using CLI directly:"
    echo '  cargo run --bin hodei-jobs-cli -- job queue \'
    echo '    --name "Test Job" \'
    echo '    --command "echo '\''Hello from Worker!'\'' && sleep 2"'
    echo ""

    print_info "Or use the organized script:"
    echo "  ./scripts/Job_Execution/maven_job_with_logs.sh --simple"
    echo ""

    # Actually queue a job
    print_info "Queueing job now..."
    RESPONSE=$(cargo run --bin hodei-jobs-cli -- job queue \
      --name "Automation Test - $(date +%H:%M:%S)" \
      --command "echo 'Worker v8.0 - LogBatching Active!' && echo 'Timestamp: '$(date) && sleep 3 && echo 'Job completed!'" 2>&1 | grep "Job queued:")

    if [[ -n "$RESPONSE" ]]; then
        JOB_ID=$(echo "$RESPONSE" | grep -oP '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}')
        print_success "Job queued successfully!"
        print_info "Job ID: $JOB_ID"
        echo "$JOB_ID" > /tmp/last_job_id.txt
    else
        print_warning "Failed to queue job or already handled by script"
    fi
}

# Monitor logs
monitor_logs() {
    print_header "Monitoring Job Logs"

    if [[ -f /tmp/last_job_id.txt ]]; then
        JOB_ID=$(cat /tmp/last_job_id.txt)
        print_info "Monitoring Job ID: $JOB_ID"
        echo ""

        print_info "Option 1: Using just command (recommended)"
        echo "  just watch-logs"
        echo ""

        print_info "Option 2: Using organized scripts"
        echo "  ./scripts/Monitoring_and_Debugging/watch_logs.sh"
        echo "  ./scripts/Monitoring_and_Debugging/watch_logs.sh $JOB_ID"
        echo ""

        print_info "Option 3: Trace specific job"
        echo "  ./scripts/Job_Execution/trace-job.sh $JOB_ID"
        echo ""

        # Actually monitor
        print_info "Starting log monitor for 10 seconds..."
        timeout 10 "./scripts/Monitoring_and_Debugging/watch_logs.sh" "$JOB_ID" 2>&1 || true
    else
        print_warning "No job ID found. Queue a job first."
        print_info "Use: $0 job"
    fi
}

# Run full test
run_full_test() {
    print_header "Running Complete Automation Test"

    # Check API
    if ! check_api; then
        print_info "Starting platform..."
        start_platform
        print_info "Waiting for API to be ready..."
        sleep 10
    fi

    # Queue job
    print_info "Step 1: Queue job"
    queue_job
    sleep 2

    # Show job status
    print_info "Step 2: Check job status"
    if [[ -f /tmp/last_job_id.txt ]]; then
        JOB_ID=$(cat /tmp/last_job_id.txt)
        grpcurl -plaintext -d "{\"job_id\": {\"value\": \"$JOB_ID\"}}" localhost:50051 hodei.JobExecutionService/GetJob 2>/dev/null | \
            jq -r '.job | "Status: \(.status // "UNKNOWN")"'
    fi
    echo ""

    # Monitor logs
    print_info "Step 3: Monitor logs"
    monitor_logs

    print_success "Test complete!"
}

# Main execution
case "${1:-full}" in
    "start")
        start_platform
        ;;
    "job")
        check_api || exit 1
        queue_job
        ;;
    "monitor")
        monitor_logs
        ;;
    "full"|"test")
        run_full_test
        ;;
    "help"|"-h"|"--help")
        echo "Hodei Platform - Automation Test"
        echo ""
        echo "Usage: $0 [command]"
        echo ""
        echo "Commands:"
        echo "  start    - Show how to start the platform"
        echo "  job      - Queue a test job"
        echo "  monitor  - Monitor job logs"
        echo "  full     - Run complete test (default)"
        echo "  help     - Show this help"
        echo ""
        echo "Using Just Commands:"
        echo "  just dev-full              - Start platform"
        echo "  just maven-job             - Run Maven job"
        echo "  just watch-logs            - Monitor logs"
        echo "  just test-auto-provision   - Test worker provisioning"
        echo ""
        echo "Using Organized Scripts:"
        echo "  ./scripts/Core_Development/start.sh --build-worker"
        echo "  ./scripts/Job_Execution/maven_job_with_logs.sh --simple"
        echo "  ./scripts/Monitoring_and_Debugging/watch_logs.sh"
        ;;
    *)
        echo "Unknown command: $1"
        echo "Use: $0 help"
        exit 1
        ;;
esac

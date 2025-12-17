#!/bin/bash
# =============================================================================
# Hodei Jobs Platform - Job Tracer
# =============================================================================
# This script traces a job from submission to completion, showing:
# 1. Job queueing
# 2. Assignment to worker
# 3. Execution progress
# 4. Logs in real-time
# 5. Final result
#
# Usage:
#   ./scripts/trace-job.sh JOB-ID
#   ./scripts/trace-job.sh JOB-ID --no-logs
#   ./scripts/trace-job.sh --help
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

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

# Help
if [[ "$1" == "--help" ]]; then
    echo "Job Tracer - Trace job execution from start to finish"
    echo ""
    echo "Usage: $0 <job-id> [options]"
    echo ""
    echo "Options:"
    echo "  --no-logs    Don't stream logs"
    echo "  --interval   Check interval in seconds (default: 2)"
    echo "  --help       Show this help"
    echo ""
    echo "Example:"
    echo "  $0 550e8400-e29b-41d4-a716-446655440000"
    exit 0
fi

# Parse arguments
JOB_ID="$1"
NO_LOGS=false
INTERVAL=2

shift || true
while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-logs)
            NO_LOGS=true
            shift
            ;;
        --interval)
            INTERVAL="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

if [[ -z "$JOB_ID" ]]; then
    print_error "Job ID is required"
    echo "Usage: $0 <job-id>"
    exit 1
fi

# Configuration
URL="${HODEI_GRPC_URL:-"localhost:50051"}"
GRPCURL_CMD="grpcurl -plaintext"

# Check dependencies
if ! command -v grpcurl &> /dev/null; then
    print_error "grpcurl is required. Install it with:"
    echo "  go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest"
    exit 1
fi

if ! command -v jq &> /dev/null; then
    print_error "jq is required. Please install it."
    exit 1
fi

# Check if API is running
if ! grpcurl -plaintext localhost:50051 list &> /dev/null; then
    print_error "Hodei Jobs API is not running on localhost:50051"
    exit 1
fi

print_header "Job Tracer"
print_info "Job ID: $JOB_ID"
print_info "API URL: $URL"
print_info "Check Interval: ${INTERVAL}s"
[[ "$NO_LOGS" == true ]] && print_info "Log streaming: DISABLED"
echo ""

# Function to get job status
get_job_status() {
    local job_id="$1"
    $GRPCURL_CMD -d "{\"job_id\": {\"value\": \"$job_id\"}}" "$URL" hodei.JobExecutionService/GetJob 2>/dev/null
}

# Function to extract status
extract_status() {
    jq -r '.job.status // "UNKNOWN"' 2>/dev/null
}

# Function to extract execution info
extract_execution() {
    jq -r '
        if .latestExecution then
            "Execution ID: " + (.latestExecution.executionId.value // "N/A") + "\n" +
            "State: " + (.latestExecution.state // "N/A") + "\n" +
            "Worker: " + (.latestExecution.workerId.value // "N/A") + "\n" +
            "Progress: " + (.latestExecution.progressPercentage // "0") + "%\n" +
            "Exit Code: " + (.latestExecution.exitCode // "N/A")
        else
            "No execution yet"
        end
    ' 2>/dev/null
}

# Function to extract job name
extract_name() {
    jq -r '.job.name // "Unknown"' 2>/dev/null
}

# Function to stream logs
stream_logs() {
    local job_id="$1"
    $GRPCURL_CMD -d "{\"job_id\": \"$job_id\", \"include_history\": true}" "$URL" hodei.LogStreamService/SubscribeLogs 2>/dev/null | \
        jq -r --unbuffered '
            select(.line != null) |
            (.isStderr as $err | .timestamp as $ts | .line as $line) |
            if $err then "[\($ts)] [ERROR] " + $line else "[\($ts)] [INFO]  " + $line end
        ' &
    LOG_PID=$!
}

# Main tracing loop
print_header "Starting Trace"

JOB_NAME=$(get_job_status "$JOB_ID" | extract_name)
print_info "Job Name: $JOB_NAME"
echo ""

# Start log streaming if enabled
if [[ "$NO_LOGS" != true ]]; then
    print_info "Starting log stream..."
    stream_logs "$JOB_ID"
    LOG_STREAM_PID=$LOG_PID
    echo ""
fi

# Tracing loop
START_TIME=$(date +%s)
PREV_STATUS=""
PREV_PROGRESS=0

while true; do
    # Get current status
    RESPONSE=$(get_job_status "$JOB_ID")
    STATUS=$(echo "$RESPONSE" | extract_status)
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))

    # Show periodic status update
    echo -e "${CYAN}[$(date +'%H:%M:%S')] [Elapsed: ${ELAPSED}s]${NC} Job Status: $STATUS"

    # Show execution details if available
    EXEC_INFO=$(echo "$RESPONSE" | extract_execution)
    if [[ "$EXEC_INFO" != "No execution yet" ]]; then
        echo "$EXEC_INFO" | while read -r line; do
            echo "  $line"
        done
    fi

    # Check if job is in terminal state
    case "$STATUS" in
        "JOB_STATUS_COMPLETED"|"JOB_STATUS_FAILED"|"JOB_STATUS_CANCELLED"|"JOB_STATUS_TIMEOUT")
            print_success "Job reached terminal state: $STATUS"
            echo ""

            # Show final execution details
            echo "$EXEC_INFO" | while read -r line; do
                echo -e "  ${GREEN}$line${NC}"
            done
            echo ""

            # Break loop
            break
            ;;
    esac

    # Wait for next check
    sleep "$INTERVAL"
done

# Stop log streaming if enabled
if [[ "$NO_LOGS" != true ]] && [[ -n "$LOG_STREAM_PID" ]]; then
    print_info "Stopping log stream..."
    kill "$LOG_STREAM_PID" 2>/dev/null || true
    wait "$LOG_STREAM_PID" 2>/dev/null || true
fi

# Show final summary
print_header "Trace Summary"
echo "Job ID: $JOB_ID"
echo "Job Name: $JOB_NAME"
echo "Duration: ${ELAPSED}s"
echo "Final Status: $STATUS"

END_TIME=$(date +%s)
TOTAL_DURATION=$((END_TIME - START_TIME))
print_success "Trace completed in ${TOTAL_DURATION}s"

exit 0

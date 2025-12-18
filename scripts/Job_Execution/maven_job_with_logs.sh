#!/bin/bash
# =============================================================================
# Hodei Jobs Platform - Maven Job with Live Log Streaming
# =============================================================================
# This script submits a Maven build job and automatically subscribes to its
# logs in real-time using the job ID from the response.
#
# Usage:
#   ./scripts/maven_job_with_logs.sh
#   ./scripts/maven_job_with_logs.sh --simple   # Simple Maven job
#   ./scripts/maven_job_with_logs.sh --complex  # Complex job with more steps
#   ./scripts/maven_job_with_logs.sh --help
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
GRAY='\033[0;90m'
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

# Configuration
URL="${HODEI_GRPC_URL:-localhost:50051}"
GRPCURL_CMD="grpcurl -plaintext"
JOB_TYPE="simple"

# Help
if [[ "$1" == "--help" || "$1" == "-h" ]]; then
    echo "Maven Job with Live Log Streaming"
    echo ""
    echo "This script submits a Maven build job and automatically subscribes"
    echo "to its logs in real-time."
    echo ""
    echo "Usage: $0 [options]"
    echo ""
    echo "Options:"
    echo "  --simple     Simple Maven job (default) - clones and builds"
    echo "  --complex    Complex job with more build steps and tests"
    echo "  --help       Show this help"
    echo ""
    echo "Requirements:"
    echo "  - Hodei Jobs Platform API running on localhost:50051"
    echo "  - grpcurl and jq installed"
    echo ""
    echo "Example:"
    echo "  $0 --simple"
    exit 0
fi

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --simple)
            JOB_TYPE="simple"
            shift
            ;;
        --complex)
            JOB_TYPE="complex"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

print_header "Maven Job with Live Log Streaming"

# Check dependencies
for cmd in grpcurl jq; do
    if ! command -v "$cmd" &> /dev/null; then
        print_error "$cmd is required. Please install it."
        exit 1
    fi
done

# Check if API is running
if ! $GRPCURL_CMD "$URL" list &> /dev/null; then
    print_error "Hodei Jobs API is not running on $URL"
    echo "Please start the platform first:"
    echo "  just dev-server"
    exit 1
fi

print_success "API is reachable at $URL"

# Define job based on type
if [[ "$JOB_TYPE" == "simple" ]]; then
    JOB_NAME="maven-build-simple"
    # Simple: clone and build
    JOB_COMMAND="cd /tmp && rm -rf simple-java-maven-app && git clone https://github.com/jenkins-docs/simple-java-maven-app.git && cd simple-java-maven-app && mvn clean install -B"
    TIMEOUT="600s"
    CPU_CORES=2.0
    MEMORY_BYTES=2147483648
else
    JOB_NAME="maven-build-complex"
    # Complex: Multi-module Maven build with asdf-managed tools
    JOB_COMMAND="cd /tmp && rm -rf spring-boot-sample && git clone https://github.com/spring-projects/spring-boot.git --depth 1 -b v3.2.0 2>/dev/null || echo 'Clone may have failed, trying alternative...' && cd /tmp && git clone https://github.com/jenkins-docs/simple-java-maven-app.git complex-app && cd complex-app && mvn clean package -DskipTests -B && echo 'Build completed successfully!'"
    TIMEOUT="1800s"
    CPU_CORES=2.0
    MEMORY_BYTES=4294967296
fi

print_info "Job Type: $JOB_TYPE"
print_info "Job Name: $JOB_NAME"
echo ""

print_header "Submitting Maven Build Job"

# Create JSON payload using jq for proper escaping
JSON_PAYLOAD=$(jq -n \
    --arg name "$JOB_NAME" \
    --arg cmd "$JOB_COMMAND" \
    --arg timeout "$TIMEOUT" \
    --argjson cpu "$CPU_CORES" \
    --argjson mem "$MEMORY_BYTES" \
    '{
        job_definition: {
            name: $name,
            command: "/bin/bash",
            arguments: ["-c", $cmd],
            requirements: {
                cpu_cores: $cpu,
                memory_bytes: $mem
            },
            timeout: {
                execution_timeout: $timeout
            }
        },
        queued_by: "user"
    }')

# Submit job and capture response
RESPONSE=$($GRPCURL_CMD -d "$JSON_PAYLOAD" "$URL" hodei.JobExecutionService/QueueJob 2>&1)

# Extract job ID from response
JOB_ID=$(echo "$RESPONSE" | grep -oE '[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}' | head -1)

if [[ -z "$JOB_ID" ]]; then
    print_error "Failed to queue job or extract job ID"
    echo "Response: $RESPONSE"
    exit 1
fi

print_success "Job queued successfully!"
echo -e "${CYAN}Job ID:${NC} $JOB_ID"
echo ""

# Function to get job status
get_job_status() {
    $GRPCURL_CMD -d "{\"job_id\": {\"value\": \"$JOB_ID\"}}" "$URL" hodei.JobExecutionService/GetJob 2>/dev/null | \
        jq -r '.job.status // "UNKNOWN"'
}

# Function to get job error message
get_job_error() {
    $GRPCURL_CMD -d "{\"job_id\": {\"value\": \"$JOB_ID\"}}" "$URL" hodei.JobExecutionService/GetJob 2>/dev/null | \
        jq -r '.executions[0].errorMessage // empty'
}

print_header "Subscribing to Live Logs"
print_info "Waiting for job execution..."
print_info "Press Ctrl+C to stop watching (job will continue running)"
echo ""

# Start time tracking
START_TIME=$(date +%s)

# Trap to handle Ctrl+C gracefully
cleanup() {
    echo ""
    print_warning "Log streaming stopped by user"
    # Kill background processes
    jobs -p | xargs -r kill 2>/dev/null || true
    # Always exit with 0 for graceful termination
    exit 0
}
# Use EXIT to catch all termination signals
trap cleanup EXIT INT TERM

# Check initial status - job might have already failed
sleep 1
INITIAL_STATUS=$(get_job_status 2>/dev/null || echo "UNKNOWN")

if [[ "$INITIAL_STATUS" == "JOB_STATUS_FAILED" ]]; then
    print_error "Job failed before execution!"
    ERROR_MSG=$(get_job_error)
    if [[ -n "$ERROR_MSG" ]]; then
        echo -e "${RED}Error: $ERROR_MSG${NC}"
    fi
    END_TIME=$(date +%s)
    DURATION=$((END_TIME - START_TIME))
    FINAL_STATUS="JOB_STATUS_FAILED"
else
    # Stream logs with formatted output
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ LIVE LOGS ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

    # Start log streaming in background
    $GRPCURL_CMD -d "{\"job_id\": \"$JOB_ID\", \"include_history\": true}" "$URL" hodei.LogStreamService/SubscribeLogs 2>/dev/null | \
        while IFS= read -r json_line; do
            LINE=$(echo "$json_line" | jq -r '.line // empty' 2>/dev/null)
            IS_STDERR=$(echo "$json_line" | jq -r '.isStderr // false' 2>/dev/null)
            TIMESTAMP=$(echo "$json_line" | jq -r '.timestamp // empty' 2>/dev/null | cut -d'.' -f1 | sed 's/T/ /')

            if [[ -n "$LINE" ]]; then
                if [[ "$IS_STDERR" == "true" ]]; then
                    echo -e "${RED}[ERR]${NC} ${GRAY}$TIMESTAMP${NC} $LINE"
                else
                    echo -e "${GREEN}[OUT]${NC} ${GRAY}$TIMESTAMP${NC} $LINE"
                fi
            fi
        done &

    LOG_PID=$!

    # Monitor job status while streaming logs
    CHECK_COUNT=0
    while kill -0 "$LOG_PID" 2>/dev/null; do
        sleep 3
        ((CHECK_COUNT++))
        STATUS=$(get_job_status 2>/dev/null || echo "UNKNOWN")

        case "$STATUS" in
            "JOB_STATUS_COMPLETED"|"JOB_STATUS_CANCELLED"|"JOB_STATUS_TIMEOUT")
                sleep 2
                kill "$LOG_PID" 2>/dev/null || true
                break
                ;;
            "JOB_STATUS_FAILED")
                sleep 2
                kill "$LOG_PID" 2>/dev/null || true
                ERROR_MSG=$(get_job_error)
                if [[ -n "$ERROR_MSG" ]]; then
                    echo -e "\n${RED}[ERROR] $ERROR_MSG${NC}"
                fi
                break
                ;;
        esac

        # Show waiting message every 30s if still queued
        if [[ $((CHECK_COUNT % 10)) -eq 0 ]]; then
            if [[ "$STATUS" == "JOB_STATUS_QUEUED" || "$STATUS" == "JOB_STATUS_PENDING" ]]; then
                echo -e "${YELLOW}[WAIT]${NC} Job still queued, waiting for worker... (${CHECK_COUNT}0s)"
            fi
        fi
    done

    # Wait for log process to finish
    wait "$LOG_PID" 2>/dev/null || true

    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

    END_TIME=$(date +%s)
    DURATION=$((END_TIME - START_TIME))
    FINAL_STATUS=$(get_job_status 2>/dev/null || echo "UNKNOWN")
fi

print_header "Job Execution Summary"

echo -e "Job ID:     ${CYAN}$JOB_ID${NC}"
echo -e "Job Name:   ${CYAN}$JOB_NAME${NC}"
echo -e "Duration:   ${CYAN}${DURATION}s${NC}"

case "$FINAL_STATUS" in
    "JOB_STATUS_COMPLETED")
        echo -e "Status:     ${GREEN}$FINAL_STATUS${NC}"
        print_success "Job completed successfully!"
        ;;
    "JOB_STATUS_FAILED")
        echo -e "Status:     ${RED}$FINAL_STATUS${NC}"
        print_error "Job failed!"
        ;;
    "JOB_STATUS_CANCELLED")
        echo -e "Status:     ${YELLOW}$FINAL_STATUS${NC}"
        print_warning "Job was cancelled"
        ;;
    "JOB_STATUS_TIMEOUT")
        echo -e "Status:     ${YELLOW}$FINAL_STATUS${NC}"
        print_warning "Job timed out"
        ;;
    *)
        echo -e "Status:     ${BLUE}$FINAL_STATUS${NC}"
        print_info "Job may still be running"
        ;;
esac

echo ""
print_info "To check job details:"
echo "  grpcurl -plaintext -d '{\"job_id\": {\"value\": \"$JOB_ID\"}}' $URL hodei.JobExecutionService/GetJob"

exit 0

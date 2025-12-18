#!/bin/bash
# =============================================================================
# Hodei Jobs Platform - List Jobs
# =============================================================================
# Lists jobs with various filters and formats
#
# Usage:
#   ./scripts/list-jobs.sh                    # List all jobs
#   ./scripts/list-jobs.sh --running          # List running jobs only
#   ./scripts/list-jobs.sh --status COMPLETED # List completed jobs
#   ./scripts/list-jobs.sh --json             # Output as JSON
#   ./scripts/list-jobs.sh --search maven     # Search by name
#   ./scripts/list-jobs.sh --limit 20         # Limit results
# =============================================================================

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# Configuration
URL="${HODEI_GRPC_URL:-"localhost:50051"}"
GRPCURL_CMD="grpcurl -plaintext"

# Default values
FORMAT="table"
LIMIT=50
STATUS=""
SEARCH=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --running)
            STATUS="JOB_STATUS_RUNNING"
            shift
            ;;
        --queued)
            STATUS="JOB_STATUS_QUEUED"
            shift
            ;;
        --completed)
            STATUS="JOB_STATUS_COMPLETED"
            shift
            ;;
        --failed)
            STATUS="JOB_STATUS_FAILED"
            shift
            ;;
        --json)
            FORMAT="json"
            shift
            ;;
        --table)
            FORMAT="table"
            shift
            ;;
        --search)
            SEARCH="$2"
            shift 2
            ;;
        --limit)
            LIMIT="$2"
            shift 2
            ;;
        --help)
            echo "List Jobs - List jobs with various filters"
            echo ""
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --running       List running jobs only"
            echo "  --queued        List queued jobs only"
            echo "  --completed     List completed jobs only"
            echo "  --failed        List failed jobs only"
            echo "  --json          Output as JSON"
            echo "  --table         Output as table (default)"
            echo "  --search TEXT   Search by job name"
            echo "  --limit N       Limit results (default: 50)"
            echo "  --help          Show this help"
            echo ""
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Check dependencies
if ! command -v grpcurl &> /dev/null; then
    echo "Error: grpcurl is required. Install it with:"
    echo "  go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest"
    exit 1
fi

if ! command -v jq &> /dev/null; then
    echo "Error: jq is required. Please install it."
    exit 1
fi

# Check if API is running
if ! grpcurl -plaintext localhost:50051 list &> /dev/null; then
    echo "Error: Hodei Jobs API is not running on localhost:50051"
    exit 1
fi

# Build request
REQUEST='{"limit": '$LIMIT

if [[ -n "$STATUS" ]]; then
    REQUEST+=', "status": "'$STATUS'"'
fi

if [[ -n "$SEARCH" ]]; then
    REQUEST+=', "search_term": "'$SEARCH'"'
fi

REQUEST+='}'

# Fetch jobs
JOBS_JSON=$(grpcurl -plaintext -d "$REQUEST" "$URL" hodei.JobExecutionService/ListJobs 2>/dev/null)

# Check for errors
if echo "$JOBS_JSON" | jq -e '.jobs == null' > /dev/null 2>&1; then
    echo "Error: Failed to fetch jobs"
    exit 1
fi

# Output based on format
if [[ "$FORMAT" == "json" ]]; then
    echo "$JOBS_JSON" | jq '.'
else
    # Table format
    TOTAL=$(echo "$JOBS_JSON" | jq -r '.totalCount')
    COUNT=$(echo "$JOBS_JSON" | jq -r '.jobs | length')

    echo ""
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${CYAN}Jobs List${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
    echo -e "Total: $TOTAL | Showing: $COUNT"
    echo ""

    # Table header
    printf "%-36s %-30s %-12s %-12s %-12s\n" "ID" "NAME" "STATUS" "PROGRESS" "DURATION"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

    # Table rows
    echo "$JOBS_JSON" | jq -r '
        .jobs[] |
        .jobId.value as $id |
        .name as $name |
        .status as $status |
        .progressPercentage as $progress |
        .duration as $duration |
        [
            $id,
            ($name | if length > 28 then .[0:25] + "..." else . end),
            ($status | gsub("JOB_STATUS_"; "")),
            ($progress | tostring + "%"),
            ($duration // "N/A")
        ] | @tsv
    ' | while IFS=$'\t' read -r id name status progress duration; do
        # Color based on status
        case "$status" in
            RUNNING)
                echo -ne "  ${GREEN}"
                ;;
            QUEUED|ASSIGNED)
                echo -ne "  ${YELLOW}"
                ;;
            COMPLETED)
                echo -ne "  ${CYAN}"
                ;;
            FAILED|CANCELLED|TIMEOUT)
                echo -ne "  ${RED}"
                ;;
            *)
                echo -ne "  ${NC}"
                ;;
        esac

        printf "%-36s %-30s %-12s %-12s %-12s\n" "$id" "$name" "$status" "$progress" "$duration"
        echo -ne "${NC}"
    done

    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
fi

exit 0

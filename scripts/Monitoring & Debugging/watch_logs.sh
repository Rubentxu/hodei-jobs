#!/bin/bash
set -e
# =============================================================================

# Determine project root and change to it
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

# Configuration
URL="${HODEI_GRPC_URL:-"localhost:50051"}"
LOG_DIR="build/logs"
GRPCURL_CMD="grpcurl -plaintext"

# Ensure dependencies
if ! command -v jq &> /dev/null; then
    echo "Error: jq is required. Please install it."
    exit 1
fi

if ! command -v grpcurl &> /dev/null; then
    echo "Error: grpcurl is required. Please install it (e.g., go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest)"
    exit 1
fi

mkdir -p "$LOG_DIR"

echo "=== Hodei Jobs Log Watcher ==="
echo "Target: $URL"
echo "Output: $LOG_DIR"
echo "Listening for running jobs..."
echo "Press Ctrl+C to stop."

# Store PIDs of log followers: keys are job_ids, values are PIDs
declare -A WATCHED_JOBS

cleanup() {
    echo -e "\nStopping log watchers..."
    for pid in "${WATCHED_JOBS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null
        fi
    done
    exit 0
}
trap cleanup SIGINT SIGTERM

while true; do
    # Fetch running jobs
    # We query for RUNNING (3) jobs. We also check for ASSIGNED (2) to catch them early.
    # JobStatus enum: PENDING=0, QUEUED=1, ASSIGNED=2, RUNNING=3

    # Query for RUNNING
    RUNNING_JOBS=$($GRPCURL_CMD -d '{"limit": 50, "status": "JOB_STATUS_RUNNING"}' $URL hodei.JobExecutionService/ListJobs 2>/dev/null || echo "{}")

    # Query for ASSIGNED (catch early logs)
    ASSIGNED_JOBS=$($GRPCURL_CMD -d '{"limit": 50, "status": "JOB_STATUS_ASSIGNED"}' $URL hodei.JobExecutionService/ListJobs 2>/dev/null || echo "{}")

    # If a specific ID is provided as argument, use only that one; otherwise find active jobs
    TARGET_ID="$1"

    if [[ -n "$TARGET_ID" ]]; then
        IDS="$TARGET_ID"
        echo "Targeting specific Job ID: $TARGET_ID"
    else
        # Combine IDs (extract from JSON)
        # Expected JSON: { "jobs": [ { "jobId": { "value": "..." } }, ... ] }
        # Only pick jobs that are ACTUALLY running or assigned.
        IDS=$(echo "$RUNNING_JOBS $ASSIGNED_JOBS" | jq -r -s '.[].jobs[]? | select(.status == "JOB_STATUS_RUNNING" or .status == "JOB_STATUS_ASSIGNED") | .jobId.value // empty' | sort | uniq)
    fi

    for id in $IDS; do
        if [[ -z "${WATCHED_JOBS[$id]}" ]]; then
            echo "Found new job: $id. Streaming logs..."

            # Spawn follower in background
            # We use `buf` or plain text streaming
            # Output format of grpcurl for stream is JSON by default.
            # We filter for .line using jq to get raw text.
            (
                $GRPCURL_CMD -d "{\"job_id\": \"$id\", \"include_history\": true}" $URL hodei.LogStreamService/SubscribeLogs \
                | jq -r --unbuffered '
                    select(.line != null) |
                    (.isStderr // .is_stderr // false) as $err |
                    if $err then "[ERR] " + .line else "[OUT] " + .line end
                ' >> "$LOG_DIR/$id.log"
            ) &
            PID=$!
            WATCHED_JOBS[$id]=$PID
        fi
    done

    # Cleanup finished watchers
    for id in "${!WATCHED_JOBS[@]}"; do
        pid=${WATCHED_JOBS[$id]}
        if ! kill -0 "$pid" 2>/dev/null; then
            # Process dead
            echo "Log streaming finished for job: $id"
            unset WATCHED_JOBS[$id]
        fi
    done

    sleep 2
done

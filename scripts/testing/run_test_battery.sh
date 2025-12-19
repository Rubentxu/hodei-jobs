#!/bin/bash
# =============================================================================
# Hodei Jobs Platform - Manual Test Battery & Log Evidence Capture
# =============================================================================
set -e

URL="${HODEI_GRPC_URL:-localhost:50051}"
CMD="grpcurl -plaintext"
EVIDENCE_DIR="build/logs"

mkdir -p "$EVIDENCE_DIR"

run_test() {
    local NAME=$1
    local COMMAND=$2
    local OUTPUT_FILE="$EVIDENCE_DIR/${NAME// /-}_$(date +%s).json"

    echo "------------------------------------------------------------"
    echo "🧪 Running Test: $NAME"

    # 1. Queue Job
    JSON_PAYLOAD=$(jq -n --arg name "$NAME" --arg cmd "$COMMAND" \
        '{job_definition: {name: $name, command: "/bin/bash", arguments: ["-c", $cmd]}}')

    RESPONSE=$($CMD -d "$JSON_PAYLOAD" "$URL" hodei.JobExecutionService/QueueJob)
    JOB_ID=$(echo "$RESPONSE" | grep -oE '[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}' | head -1)

    if [[ -z "$JOB_ID" ]]; then
        echo "❌ Failed to queue job $NAME"
        return 1
    fi
    echo "✅ Job Queued: $JOB_ID"

    # 2. Subscribe and capture logs to evidence file
    echo "📹 Subscribing to logs... saving to $OUTPUT_FILE"
    # Note: Using include_history=true to ensure we don't miss anything
    $CMD -d "{\"job_id\": \"$JOB_ID\", \"include_history\": true}" "$URL" hodei.LogStreamService/SubscribeLogs > "$OUTPUT_FILE" &
    STREAM_PID=$!

    # 3. Monitor for completion (simple poll)
    while true; do
        STATUS=$($CMD -d "{\"job_id\": {\"value\": \"$JOB_ID\"}}" "$URL" hodei.JobExecutionService/GetJob 2>/dev/null | jq -r '.job.status')
        if [[ "$STATUS" == "JOB_STATUS_COMPLETED" || "$STATUS" == "JOB_STATUS_FAILED" || "$STATUS" == "JOB_STATUS_CANCELLED" ]]; then
            echo "🏁 Job finished with status: $STATUS"
            sleep 2 # Let final logs arrive
            kill $STREAM_PID 2>/dev/null || true
            break
        fi
        sleep 2
    done

    echo "📄 Evidence captured: $(wc -l < "$OUTPUT_FILE") lines"
}

echo "🚀 Starting Manual Test Battery..."

# Test 1: Simple Job
run_test "Simple Echo" "echo 'Hello from Hodei Worker Agent!'; sleep 2; echo 'Job finished.'"

# Test 2: High Volume (smaller version for battery)
run_test "High Volume Burst" "for i in {1..1000}; do echo \"Log line \$i: testing high frequency output handling\"; done"

# Test 3: System Context & Metrics
run_test "System Context" "echo 'Host OS:'; uname -a; echo 'Memory info:'; free -m; sleep 5; echo 'Done.'"

echo "============================================================"
echo "✅ Test Battery Completed!"
echo "📂 All evidences are in $EVIDENCE_DIR"
ls -lh "$EVIDENCE_DIR"

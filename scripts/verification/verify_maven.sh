#!/bin/bash
set -e

# =============================================================================
# Verification Script: Maven Build Job on Hodei Platform
# =============================================================================

# 1. Define Payload Script
# Using the robust ASDF-compatible script
PAYLOAD_SCRIPT="scripts/verification/maven_build_job.sh"

if [ ! -f "$PAYLOAD_SCRIPT" ]; then
    echo "Error: Payload script $PAYLOAD_SCRIPT not found!"
    exit 1
fi

echo ">>> Reading payload from $PAYLOAD_SCRIPT"
SCRIPT_CONTENT=$(cat "$PAYLOAD_SCRIPT" | sed 's/"/\\"/g' | awk '{printf "%s\\n", $0}')

# 2. Submit Job via grpcurl
echo ">>> Submitting Job..."
# Note: We specify 'container_image' in metadata to ensure DockerProvider uses the correct image.
# We use 'hodei-jobs-worker:latest' which contains asdf.
RESPONSE=$(grpcurl -plaintext -d "{
  \"job_definition\": {
    \"name\": \"verify-maven-asdf-flow\",
    \"command\": \"/bin/bash\",
    \"arguments\": [\"-c\", \"$SCRIPT_CONTENT\"],
    \"requirements\": { \"cpu_cores\": 1.0, \"memory_bytes\": 1073741824 },
    \"timeout\": { \"execution_timeout\": \"600s\" }
  },
  \"queued_by\": \"admin\"
}" localhost:50051 hodei.JobExecutionService/QueueJob)

echo ">>> Submission Response:"
echo "$RESPONSE"

# 3. Extract Job ID
# The response message format is "Job queued: <job_id>"
JOB_ID=$(echo "$RESPONSE" | jq -r '.message' | sed 's/Job queued: //')

if [[ -n "$JOB_ID" && "$JOB_ID" != "null" && "$JOB_ID" != "Job queued:"* ]]; then
    echo ">>> Job ID: $JOB_ID"

    # 4. Start Log Watcher
    echo ">>> Starting log watcher..."
    mkdir -p build/logs
    # Kill any existing watcher to avoid noise
    pkill -f "watch_logs.sh" || true

    # Run watcher in background but we will tailor it to just this job if possible
    # My watch_logs.sh supports an argument!
    nohup ./scripts/watch_logs.sh "$JOB_ID" > build/logs/verification_watcher.out 2>&1 &
    WATCHER_PID=$!
    echo ">>> Watcher PID: $WATCHER_PID"

    # 5. Wait for Log File
    LOG_FILE="build/logs/${JOB_ID}.log"
    echo ">>> Waiting for log file: $LOG_FILE"

    COUNT=0
    while [ ! -f "$LOG_FILE" ]; do
        sleep 2
        COUNT=$((COUNT+1))
        if [ $COUNT -gt 30 ]; then
            echo "timeout waiting for log file creation."
            break
        fi
        echo -n "."
    done
    echo ""

    if [ -f "$LOG_FILE" ]; then
        echo ">>> Log file found. Tailing..."
        tail -f "$LOG_FILE"
    else
        echo ">>> Checking server logs for potential provisioning errors..."
        docker logs hodei-jobs-api --tail 20
    fi
else
    echo ">>> FAILED to submit job."
    exit 1
fi

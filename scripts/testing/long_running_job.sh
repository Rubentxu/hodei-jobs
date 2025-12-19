#!/bin/bash
# =============================================================================
# Hodei Jobs Platform - Long Running Job Stability Test
# =============================================================================
set -e

URL="${HODEI_GRPC_URL:-localhost:50051}"
CMD="grpcurl -plaintext"

echo "🚀 Submitting long-running stability job (120s)..."

# Job that runs for 2 minutes and logs every 5 seconds
JOB_COMMAND="for i in {1..24}; do echo \"[\$i/24] Stability check at \$(date). Worker should be sending heartbeats with real metrics...\"; sleep 5; done; echo \"✅ Stability test completed successfully.\""

JSON_PAYLOAD=$(jq -n \
    --arg cmd "$JOB_COMMAND" \
    '{
        job_definition: {
            name: "stability-test-long-running",
            command: "/bin/bash",
            arguments: ["-c", $cmd],
            requirements: {
                cpu_cores: 0.5,
                memory_bytes: 268435456
            },
            timeout: {
                execution_timeout: "300s"
            }
        },
        queued_by: "stability-test"
    }')

RESPONSE=$($CMD -d "$JSON_PAYLOAD" "$URL" hodei.JobExecutionService/QueueJob)
JOB_ID=$(echo "$RESPONSE" | grep -oE '[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}' | head -1)

echo "✅ Job queued: $JOB_ID"
echo "👀 You can watch logs with: just watch-logs"
echo "📂 Worker local log will be at: /tmp/hodei-logs/job-$JOB_ID.log"

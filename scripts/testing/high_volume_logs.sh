#!/bin/bash
# =============================================================================
# Hodei Jobs Platform - High Volume Logging Stress Test
# =============================================================================
set -e

URL="${HODEI_GRPC_URL:-localhost:50051}"
CMD="grpcurl -plaintext"

echo "🚀 Submitting high-volume logging job..."

# Job that produces 50,000 lines
JOB_COMMAND="for i in {1..50000}; do echo \"Stress test log line \$i producing some volume of data to fill buffers and test batching efficiency and disk I/O performance of the file logger implementation in the worker agent\"; done"

JSON_PAYLOAD=$(jq -n \
    --arg cmd "$JOB_COMMAND" \
    '{
        job_definition: {
            name: "stress-test-high-volume",
            command: "/bin/bash",
            arguments: ["-c", $cmd],
            requirements: {
                cpu_cores: 1.0,
                memory_bytes: 536870912
            }
        },
        queued_by: "stress-test"
    }')

RESPONSE=$($CMD -d "$JSON_PAYLOAD" "$URL" hodei.JobExecutionService/QueueJob)
JOB_ID=$(echo "$RESPONSE" | grep -oE '[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}' | head -1)

echo "✅ Job queued: $JOB_ID"
echo "👀 You can watch logs with: just watch-logs"
echo "📂 Worker local log will be at: /tmp/hodei-logs/job-$JOB_ID.log"

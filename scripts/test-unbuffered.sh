#!/bin/bash
# =============================================================================
# Test with script command to force unbuffered output
# =============================================================================

set -e

echo "=== Testing with 'script' command for unbuffered output ==="
echo ""

# Submit a test job
echo "Submitting job..."
RESPONSE=$(grpcurl -plaintext -d '{
    "job_definition": {
        "name": "Unbuffered-Test",
        "command": "/bin/bash",
        "arguments": ["-c", "for i in {1..5}; do echo \"Line \$i at \$(date +%T)\"; sleep 1; done"]
    },
    "queued_by": "test"
}' "localhost:50051" hodei.JobExecutionService/QueueJob 2>&1)

JOB_ID=$(echo "$RESPONSE" | grep -oP '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)

if [[ -z "$JOB_ID" ]]; then
    echo "❌ Failed to get job ID"
    exit 1
fi

echo "✅ Job ID: $JOB_ID"
echo "Waiting 2 seconds..."
sleep 2
echo ""

echo "Method 1: Using 'script' command to force unbuffered output"
echo "Command: script -q -c '/home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs $JOB_ID' /dev/null"
echo ""
script -q -c "/home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs $JOB_ID" /dev/null

echo ""
echo "Test complete!"

#!/bin/bash
# =============================================================================
# Simple streaming test - No buffering at all
# =============================================================================

set -e

echo "=== SIMPLE STREAMING TEST ==="
echo ""

# Submit job
echo "Submitting job..."
RESPONSE=$(grpcurl -plaintext -d '{
    "job_definition": {
        "name": "Simple-Test",
        "command": "/bin/bash",
        "arguments": ["-c", "for i in 1 2 3 4 5; do echo \"Line \$i\"; sleep 1; done"]
    },
    "queued_by": "test"
}' "localhost:50051" hodei.JobExecutionService/QueueJob 2>&1)

JOB_ID=$(echo "$RESPONSE" | grep -oP '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)

if [[ -z "$JOB_ID" ]]; then
    echo "❌ Failed"
    exit 1
fi

echo "✅ Job ID: $JOB_ID"
sleep 2

echo ""
echo "Method 1: Direct Rust client (no wrapper)"
echo "---"
/home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs "$JOB_ID"

echo ""
echo "Test complete!"

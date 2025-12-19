#!/bin/bash
# =============================================================================
# Direct test of the SubscribeLogs stream
# =============================================================================

set -e

echo "=== Testing SubscribeLogs Stream ==="
echo ""

# Get a real job ID from a running job
echo "Step 1: Submitting a simple test job..."
RESPONSE=$(grpcurl -plaintext -d '{
    "job_definition": {
        "name": "Stream-Test",
        "command": "/bin/bash",
        "arguments": ["-c", "for i in {1..5}; do echo \"Log \$i\"; sleep 1; done; echo \"Done\""]
    },
    "queued_by": "test"
}' "localhost:50051" hodei.JobExecutionService/QueueJob 2>&1)

JOB_ID=$(echo "$RESPONSE" | grep -oP '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)

if [[ -z "$JOB_ID" ]]; then
    echo "❌ Failed to get job ID"
    echo "Response: $RESPONSE"
    exit 1
fi

echo "✅ Job submitted: $JOB_ID"
echo ""

echo "Step 2: Waiting 3 seconds for job to start..."
sleep 3
echo ""

echo "Step 3: Subscribing to SubscribeLogs stream..."
echo "Using: /home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs $JOB_ID"
echo ""
echo "=== BEGIN STREAM ==="
echo ""

# Test the Rust client directly
/home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs "$JOB_ID"

echo ""
echo "=== END STREAM ==="
echo ""
echo "✅ Test complete!"

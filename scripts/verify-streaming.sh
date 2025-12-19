#!/bin/bash
# =============================================================================
# Final verification test - Force real-time streaming
# =============================================================================

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║         REAL-TIME STREAMING VERIFICATION TEST                  ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Submit a test job with very clear timing
echo "📤 Submitting test job with timed output..."
RESPONSE=$(grpcurl -plaintext -d '{
    "job_definition": {
        "name": "Streaming-Verification",
        "command": "/bin/bash",
        "arguments": ["-c", "for i in {1..10}; do echo \"[\$(date +%T)] Iteration \$i of 10\"; sleep 1; done; echo \"DONE at \$(date +%T)\""]
    },
    "queued_by": "verification"
}' "localhost:50051" hodei.JobExecutionService/QueueJob 2>&1)

JOB_ID=$(echo "$RESPONSE" | grep -oP '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)

if [[ -z "$JOB_ID" ]]; then
    echo "❌ Failed to submit job"
    exit 1
fi

echo "✅ Job ID: $JOB_ID"
echo ""
echo "⏳ Waiting 2 seconds for job to start..."
sleep 2
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "🧪 TESTING WITH 'script' COMMAND"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "You should see 10 lines appearing, each 1 second apart."
echo "If all appear at once, there's a buffering issue."
echo ""
echo "Starting stream in 1 second..."
sleep 1
echo ""

# Use script command for unbuffered output
script -q -c "/home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs $JOB_ID" /dev/null

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "✅ Test complete!"
echo ""
echo "ANALYSIS:"
echo "  • Did lines appear progressively (1s apart)? ✅ WORKING!"
echo "  • Did all lines appear at once at the end? ❌ BUFFERING"
echo ""
echo "If buffering was detected, the issue is at the terminal/shell level."
echo "The 'script' command should have fixed it. Try checking your terminal"
echo "settings or try running this test from a different terminal."

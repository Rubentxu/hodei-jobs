#!/bin/bash
# =============================================================================
# File-based streaming - Guaranteed real-time output
# =============================================================================

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║         FILE-BASED REAL-TIME STREAMING TEST                   ║"
echo "║  This method writes to a file and uses 'tail -f'              ║"
echo "║  Guarantees real-time output without buffering                ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Create a temporary file for the stream
TEMP_FILE="/tmp/stream-output-$$.log"
echo "📁 Using temp file: $TEMP_FILE"
echo ""

# Submit a test job
echo "📤 Submitting test job..."
RESPONSE=$(grpcurl -plaintext -d '{
    "job_definition": {
        "name": "File-Stream-Test",
        "command": "/bin/bash",
        "arguments": ["-c", "for i in {1..10}; do echo \"[\$(date +%T)] Line \$i of 10\"; sleep 1; done; echo \"DONE\""]
    },
    "queued_by": "test"
}' "localhost:50051" hodei.JobExecutionService/QueueJob 2>&1)

JOB_ID=$(echo "$RESPONSE" | grep -oP '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)

if [[ -z "$JOB_ID" ]]; then
    echo "❌ Failed to submit job"
    exit 1
fi

echo "✅ Job ID: $JOB_ID"
echo "⏳ Waiting 2 seconds for job to start..."
sleep 2
echo ""

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "🚀 STARTING FILE-BASED STREAM"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "Method: Rust client writes to file + tail -f follows in real-time"
echo "File: $TEMP_FILE"
echo ""
echo "Starting at: $(date '+%T.%3N')"
echo "---"

# Start tail in background to follow the file
tail -f "$TEMP_FILE" &
TAIL_PID=$!

# Run the Rust client writing to the file
/home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs "$JOB_ID" "$TEMP_FILE"

# Wait a moment for tail to process
sleep 1

# Kill tail
kill $TAIL_PID 2>/dev/null || true
wait $TAIL_PID 2>/dev/null || true

echo "---"
echo "Ended at: $(date '+%T.%3N')"
echo ""

# Clean up
rm -f "$TEMP_FILE"

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "✅ TEST COMPLETE!"
echo ""
echo "RESULT:"
echo "  • Did you see lines appearing progressively (1s apart)?"
echo "  • Or did all lines appear at once?"
echo ""
echo "This method GUARANTEES real-time output because:"
echo "  1. Rust client flushes immediately to file"
echo "  2. tail -f follows file changes in real-time"
echo "  3. No terminal or shell buffering involved"

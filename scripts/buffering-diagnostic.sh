#!/bin/bash
# =============================================================================
# Deep buffering diagnostic - Find exactly where buffering occurs
# =============================================================================

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║              BUFFERING DIAGNOSTIC TOOL                         ║"
echo "║  This will help identify where buffering is occurring         ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Test 1: Check terminal settings
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "TEST 1: Terminal Settings"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "stdout is a tty: $([ -t 1 ] && echo "YES" || echo "NO")"
echo "stderr is a tty: $([ -t 2 ] && echo "YES" || echo "NO")"
echo "stty size: $(stty size 2>/dev/null || echo "unknown")"
echo "stty -icanon time: $(stty -a 2>/dev/null | grep -o 'icanon time = [0-9]*' || echo "unknown")"
echo ""

# Test 2: Submit a job
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "TEST 2: Submitting Test Job"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

RESPONSE=$(grpcurl -plaintext -d '{
    "job_definition": {
        "name": "Buffering-Diagnostic",
        "command": "/bin/bash",
        "arguments": ["-c", "for i in {1..5}; do echo \"[\$(date +%T.%3N)] Line \$i\"; sleep 1; done"]
    },
    "queued_by": "diagnostic"
}' "localhost:50051" hodei.JobExecutionService/QueueJob 2>&1)

JOB_ID=$(echo "$RESPONSE" | grep -oP '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)

if [[ -z "$JOB_ID" ]]; then
    echo "❌ Failed to submit job"
    exit 1
fi

echo "✅ Job ID: $JOB_ID"
sleep 2
echo ""

# Test 3: Check what the client sees
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "TEST 3: Direct Client Output (with timestamps)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Starting at: $(date '+%T.%3N')"
echo ""

# Capture timestamps from the actual output
{
    script -q -c "/home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs $JOB_ID" /dev/null 2>&1
} | while IFS= read -r line; do
    echo "[$(date '+%T.%3N')] $line"
done

echo ""
echo "Ended at: $(date '+%T.%3N')"
echo ""

# Test 4: Check server-side buffering
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "TEST 4: Checking Server Logs"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Look for LogStreamService messages in the server logs"
echo "Server should be receiving logs from worker in real-time"
echo ""

# Test 5: Manual verification
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "TEST 5: Manual Line-by-Line Test"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "If you saw lines appearing one by one above, streaming works!"
echo "If all lines appeared at once, buffering is occurring."
echo ""
echo "Possible causes:"
echo "  1. Terminal TTY settings (stty -icanon, etc.)"
echo "  2. Shell output buffering"
echo "  3. gRPC stream buffering"
echo "  4. Server-side batching"
echo ""
echo "Try these fixes:"
echo "  • Run: stty -icanon time 0"
echo "  • Run: stty -echo"
echo "  • Use: stdbuf -o0 -e0 script -c '...' /dev/null"
echo ""

# Test 6: Background test
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "TEST 6: Background Process Test"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Submitting another job and running in background..."
echo ""

RESPONSE=$(grpcurl -plaintext -d '{
    "job_definition": {
        "name": "Background-Diagnostic",
        "command": "/bin/bash",
        "arguments": ["-c", "for i in {1..5}; do echo \"BG Line \$i\"; sleep 1; done"]
    },
    "queued_by": "diagnostic"
}' "localhost:50051" hodei.JobExecutionService/QueueJob 2>&1)

JOB_ID2=$(echo "$RESPONSE" | grep -oP '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)
sleep 2

echo "Job ID: $JOB_ID2"
echo "Starting background stream at: $(date '+%T.%3N')"

# Run in background and capture
script -q -c "/home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs $JOB_ID2" /dev/null | while IFS= read -r line; do
    echo "[$(date '+%T.%3N')] $line"
done &

BG_PID=$!
sleep 8
kill $BG_PID 2>/dev/null || true
wait $BG_PID 2>/dev/null || true

echo "Background test ended at: $(date '+%T.%3N')"
echo ""

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║              DIAGNOSTIC COMPLETE                               ║"
echo "╚════════════════════════════════════════════════════════════════╝"

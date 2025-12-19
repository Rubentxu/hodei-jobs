#!/bin/bash
# =============================================================================
# Diagnostic script to test different buffering scenarios
# =============================================================================

set -e

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║         RUST LOG STREAMER - DIAGNOSTIC TEST                    ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Configuration
JOB_NAME="Diagnostic-Test-$$"
URL="localhost:50051"

# Submit a quick test job
echo "📤 Submitting test job: $JOB_NAME"
COMMAND="echo 'Line 1 - T+0s'; sleep 1; echo 'Line 2 - T+1s'; sleep 1; echo 'Line 3 - T+2s'; sleep 1; echo 'Line 4 - T+3s'; sleep 1; echo 'Line 5 - T+4s'; echo 'DONE'"

RESPONSE=$(grpcurl -plaintext -d "{
    \"job_definition\": {
        \"name\": \"$JOB_NAME\",
        \"command\": \"/bin/bash\",
        \"arguments\": [\"-c\", \"$COMMAND\"]
    },
    \"queued_by\": \"diagnostic\"
}" "$URL" hodei.JobExecutionService/QueueJob 2>&1)

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

# Test 1: Basic execution
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "TEST 1: Basic Rust Client (foreground, stderr)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
/home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs "$JOB_ID"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Check job status
STATUS=$(grpcurl -plaintext -d "{\"job_id\": {\"value\": \"$JOB_ID\"}}" "$URL" hodei.JobExecutionService/GetJob 2>/dev/null | jq -r '.job.status // "UNKNOWN"')
echo "Job Status: $STATUS"
echo ""

if [[ "$STATUS" != "JOB_STATUS_COMPLETED" ]]; then
    echo "⚠️  Job not completed yet, waiting..."
    sleep 3
fi

# Test 2: With stdbuf
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "TEST 2: With stdbuf (line buffering)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Submit another job for Test 2
RESPONSE=$(grpcurl -plaintext -d "{
    \"job_definition\": {
        \"name\": \"Diagnostic-Test-2-$$\",
        \"command\": \"/bin/bash\",
        \"arguments\": [\"-c\", \"for i in {1..5}; do echo \\\"Test2 Line \$i\\\"; sleep 1; done; echo \\\"DONE\\\"\"]
    },
    \"queued_by\": \"diagnostic\"
}" "$URL" hodei.JobExecutionService/QueueJob 2>&1)

JOB_ID2=$(echo "$RESPONSE" | grep -oP '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)
sleep 2

echo "Job ID: $JOB_ID2"
echo ""
stdbuf -oL -eL /home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs "$JOB_ID2" 2>&1
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Test 3: Background with pipe
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "TEST 3: Background with unbuffered pipe"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

RESPONSE=$(grpcurl -plaintext -d "{
    \"job_definition\": {
        \"name\": \"Diagnostic-Test-3-$$\",
        \"command\": \"/bin/bash\",
        \"arguments\": [\"-c\", \"for i in {1..5}; do echo \\\"Test3 Line \$i\\\"; sleep 1; done; echo \\\"DONE\\\"\"]
    },
    \"queued_by\": \"diagnostic\"
}" "$URL" hodei.JobExecutionService/QueueJob 2>&1)

JOB_ID3=$(echo "$RESPONSE" | grep -oP '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)
sleep 2

echo "Job ID: $JOB_ID3"
echo ""
stdbuf -o0 -e0 /home/rubentxu/Proyectos/rust/package/hodei-job-platform/scripts/stream-logs "$JOB_ID3" 2>&1 | cat &
PID=$!
echo "Started background process (PID: $PID)"
echo "Waiting 8 seconds..."
sleep 8
kill $PID 2>/dev/null || true
wait $PID 2>/dev/null || true
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                    DIAGNOSTIC COMPLETE                         ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "RESULTS:"
echo "  • If lines appeared progressively (1s apart) → Streaming works!"
echo "  • If all lines appeared at once → Buffering issue detected"
echo ""
echo "RECOMMENDATIONS:"
echo "  • Use TEST 2 (stdbuf) if it showed progressive output"
echo "  • Check terminal settings for output buffering"
echo "  • Try: script -c 'command' /dev/null to force unbuffered output"

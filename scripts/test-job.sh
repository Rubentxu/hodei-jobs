#!/bin/bash
# Quick test job script with automatic trace subscription

set -e

echo "⚡ Queuing test job..."
JOB_ID=$(cargo run --bin hodei-jobs-cli -- job queue \
    --name "Automation Test" \
    --command "echo 'Worker v8.0 - LogBatching Active'; echo 'Timestamp: $(date)'; sleep 3; echo 'Job completed'" \
    2>&1 | grep -oP '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)

if [ -n "$JOB_ID" ]; then
    echo ""
    echo "🎬 Job ID: $JOB_ID"
    echo "📡 Subscribing to live logs..."
    echo ""
    # NOTE: grpcurl buffers the gRPC stream internally
    # Logs are sent in real-time from worker (capacity=1, 10ms flush)
    # but grpcurl batches them for display
    grpcurl -plaintext -d "{\"job_id\": \"$JOB_ID\", \"include_history\": true}" \
        localhost:50051 hodei.LogStreamService/SubscribeLogs 2>&1 | grep -v "{" | head -50
    echo ""
    echo -e "${GREEN}✅ Stream Finished${NC}"
else
    echo "❌ Failed to extract job ID"
    exit 1
fi

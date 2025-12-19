#!/bin/bash
# Maven job script with automatic trace subscription

set -e

echo "🔨 Queuing Maven job..."
JOB_ID=$(cargo run --bin hodei-jobs-cli -- job queue \
    --name "Maven Build" \
    --command "echo 'Building Maven project...'; echo 'Timestamp: $(date)'; sleep 3; echo '✓ Build completed successfully!'" \
    2>&1 | grep -oP '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)

if [ -n "$JOB_ID" ]; then
    echo ""
    echo "🎬 Job ID: $JOB_ID"
    echo "📡 Subscribing to live logs..."
    echo ""
    # Use stdbuf to disable buffering for real-time streaming
    stdbuf -oL grpcurl -plaintext -d "{\"job_id\": \"$JOB_ID\", \"include_history\": true}" \
        localhost:50051 hodei.LogStreamService/SubscribeLogs
    echo ""
    echo -e "${GREEN}✅ Stream Finished${NC}"
else
    echo "❌ Failed to extract job ID"
    exit 1
fi

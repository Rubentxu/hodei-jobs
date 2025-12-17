#!/bin/bash
set -e

# Read the script content safely
SCRIPT_CONTENT=$(cat scripts/verification/maven_job_optimized.sh | sed 's/"/\\"/g' | awk '{printf "%s\\n", $0}')

# Submit using grpcurl
RESPONSE=$(grpcurl -plaintext -d "{
  \"job_definition\": {
    \"name\": \"maven-optimized-build-v2\",
    \"command\": \"/bin/bash\",
    \"arguments\": [\"-c\", \"$SCRIPT_CONTENT\"],
    \"metadata\": {
        \"container_image\": \"maven:3.9-eclipse-temurin-17\"
    },
    \"requirements\": { \"cpu_cores\": 2.0, \"memory_bytes\": 2147483648 },
    \"timeout\": { \"execution_timeout\": \"600s\" }
  },
  \"queued_by\": \"user\"
}" localhost:50051 hodei.JobExecutionService/QueueJob)

echo "Response: $RESPONSE"

JOB_ID=$(echo "$RESPONSE" | jq -r '.jobId.value')

if [[ -n "$JOB_ID" && "$JOB_ID" != "null" ]]; then
    echo "SUCCESS: Job submitted with ID: $JOB_ID"
    # Restart watcher for this ID
    pkill -f "scripts/watch_logs.sh" || true
    rm -rf build/logs/*
    nohup ./scripts/watch_logs.sh "$JOB_ID" > build/logs/watcher.out 2>&1 &
    echo "Watcher started for $JOB_ID"
else
    echo "FAILED: Could not populate JOB_ID"
    exit 1
fi

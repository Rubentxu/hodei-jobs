#!/bin/bash
CONTAINER_ID=$(docker ps -q --filter "ancestor=hodei-jobs-worker:latest" | head -n 1)
if [ -z "$CONTAINER_ID" ]; then
    CONTAINER_ID=$(docker ps -a --filter "name=hodei-worker" --format "{{.ID}}" | head -n 1)
fi

if [ -n "$CONTAINER_ID" ]; then
    echo "📜 Tailing logs for worker $CONTAINER_ID"
    docker logs -f $CONTAINER_ID
else
    echo "❌ No running worker containers found"
fi

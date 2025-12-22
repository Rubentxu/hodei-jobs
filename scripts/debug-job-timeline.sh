#!/bin/bash

# check if job id is provided
if [ -z "$1" ]; then
    echo "❌ Missing Job ID"
    echo "Usage: ./debug-job-timeline.sh <JOB_ID>"
    exit 1
fi

JOB_ID="$1"

echo "🔍 Fetching timeline for Job ID: $JOB_ID"
echo "────────────────────────────────────────"

# Execute SQL query via Docker
docker exec hodei-jobs-postgres psql -U postgres -c "
SELECT
    to_char(occurred_at, 'HH24:MI:SS.MS') as time,
    event_type as event,
    substring(aggregate_id from 1 for 8) as entity_id,
    COALESCE(actor, 'unknown') as actor,
    COALESCE(substring(correlation_id from 1 for 8), '-') as corr_id,
    left(payload::text, 50) || '...' as summary
FROM domain_events
WHERE correlation_id = '$JOB_ID' OR aggregate_id = '$JOB_ID'
ORDER BY occurred_at ASC;
"

echo "────────────────────────────────────────"
echo "✅ Timeline end"

#!/bin/bash
set -e

# Config
HTTP_URL="http://localhost:3000/api/v1/jobs"
CORRELATION_ID="timeout-test-$(date +%s)"

echo "---------------------------------------------------"
echo "Testing Job Timeout Auditing"
echo "Correlation ID: $CORRELATION_ID"
echo "---------------------------------------------------"

# 1. Create a job that sleeps for 10s but times out in 2s
echo "1. Creating Job (Sleep 10s, Timeout 2s)..."
RESPONSE=$(curl -s -X POST "$HTTP_URL" \
  -H "Content-Type: application/json" \
  -d "{
    \"spec\": {
      \"command\": [\"sleep\", \"10\"],
      \"timeout_ms\": 2000
    },
    \"correlation_id\": \"$CORRELATION_ID\"
  }")

echo "Response: $RESPONSE"

# Extract Job ID
JOB_ID=$(echo "$RESPONSE" | jq -r '.data.job_id')

if [ "$JOB_ID" == "null" ] || [ -z "$JOB_ID" ]; then
    echo "Failed to create job. Aborting."
    exit 1
fi

echo "Job Created. ID: $JOB_ID"

# 2. Wait for timeout (2s + 5s buffer = 7s to be safe)
echo "2. Waiting 7s for job to timeout..."
sleep 7

# 3. Check Audit Trail via DB
echo "3. Verifying Audit Trail (DB Query)..."
# We query specific fields to verify: Event Type, New State (from payload), and Correlation ID
docker exec hodei-jobs-postgres psql -U postgres -d hodei -c "
SELECT
    event_type,
    payload::json->>'new_state' as new_state,
    correlation_id,
    actor
FROM domain_events
WHERE aggregate_id = '${JOB_ID}'
ORDER BY occurred_at ASC;
" > timeout_audit_db.log 2>&1

cat timeout_audit_db.log

# 4. Assertions
echo "---------------------------------------------------"
echo "Assertions:"

# Assert Status FAILED. The payload has new_state="FAILED" (or case variant)
if grep -i "Failed" timeout_audit_db.log; then
    echo "✅ SUCCESS: Job Failed event detected."
else
    echo "❌ FAILURE: Job Failed event NOT found."
    exit 1
fi

# Assert Correlation ID persistence
if grep -q "$CORRELATION_ID" timeout_audit_db.log; then
     echo "✅ SUCCESS: Correlation ID '$CORRELATION_ID' found in audit log."
else
     echo "❌ FAILURE: Correlation ID '$CORRELATION_ID' NOT found."
     exit 1
fi

echo "---------------------------------------------------"
echo "Timeout Verification Complete"

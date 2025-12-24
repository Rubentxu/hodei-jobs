# Hodei Job Platform - Troubleshooting Guide

## Table of Contents
1. [Common Issues and Solutions](#common-issues-and-solutions)
2. [Debugging Strategies](#debugging-strategies)
3. [Event-Driven System Monitoring](#event-driven-system-monitoring)
4. [Step-by-Step Troubleshooting](#step-by-step-troubleshooting)
5. [Log Analysis](#log-analysis)
6. [Database Issues](#database-issues)
7. [Worker Management](#worker-management)

---

## Common Issues and Solutions

### 1. UUID Type Mismatch Errors

**Symptom:**
```
ERROR: operator does not exist: text = uuid
```

**Root Cause:**
The `worker_bootstrap_token_store` was creating tables with `TEXT` columns for UUID fields but attempting to compare with UUID types.

**Solution:**
Ensure database schema uses UUID type consistently:

```sql
-- Correct schema
CREATE TABLE IF NOT EXISTS worker_bootstrap_tokens (
    token UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    worker_id UUID NOT NULL,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL
);
```

**Code Fix:**
```rust
// Use UUID directly, not string conversion
.bind(token.0)  // Direct UUID binding
```

### 2. UnexpectedNullError in Worker Registry

**Symptom:**
```
Error: UnexpectedNullError
```

**Root Cause:**
Worker registry expected non-optional `DateTime<Utc>` for `last_heartbeat`, but database contained NULL values.

**Solution:**
Make `last_heartbeat` field optional throughout the stack:

```rust
// In database query
let last_heartbeat: Option<chrono::DateTime<chrono::Utc>> = row.get("last_heartbeat");

// In aggregate constructor
pub fn from_database(
    id: WorkerId,
    worker_type: WorkerType,
    capabilities: Vec<Capability>,
    last_heartbeat: Option<DateTime<Utc>>,
    // ...
) -> Self {
    let heartbeat = last_heartbeat.unwrap_or_else(chrono::Utc::now);
    // ...
}
```

### 3. Port Conflict Errors

**Symptom:**
```
ERROR: Address already in use (port 50051)
```

**Root Cause:**
Previous server instance still running.

**Solution:**
```bash
# Find process using port
lsof -i :50051

# Kill the process
kill -9 <PID>

# Or use pkill
pkill -f hodei-server

# Restart services cleanly
just down
just up
```

### 4. Missing PostgreSQL Extension

**Symptom:**
```
ERROR: function gen_random_uuid() does not exist
```

**Root Cause:**
pgcrypto extension not enabled in PostgreSQL.

**Solution:**
Enable extension in migration or SQL script:

```sql
CREATE EXTENSION IF NOT EXISTS pgcrypto;
```

### 5. Workers Not Registering

**Symptom:**
- Jobs created but not executed
- No workers available for scheduling

**Root Cause:**
Worker registration issues due to token consumption failures or database constraints.

**Debugging:**
```bash
# Check registered workers
docker exec -t hodei-jobs-postgres psql -U postgres -d hodei_dev -c \
  "SELECT id, worker_type, status, last_heartbeat FROM workers;"

# Check bootstrap tokens
docker exec -t hodei-jobs-postgres psql -U postgres -d hodei_dev -c \
  "SELECT token, worker_id, expires_at FROM worker_bootstrap_tokens;"
```

---

## Debugging Strategies

### 1. Service Status Verification

Always start by checking service status:

```bash
# Check running services
just status

# Check Docker containers
docker ps

# Check specific service logs
just logs-server
just logs-worker
```

### 2. Database Inspection

Query key tables to understand system state:

```sql
-- Check jobs and their states
SELECT id, name, status, created_at FROM jobs ORDER BY created_at DESC LIMIT 10;

-- Check workers
SELECT id, worker_type, status, last_heartbeat FROM workers;

-- Check audit logs (event history)
SELECT occurred_at, event_type, payload FROM audit_logs
ORDER BY occurred_at DESC LIMIT 20;

-- Check for stuck events
SELECT event_type, COUNT(*) FROM audit_logs
GROUP BY event_type;
```

### 3. Real-Time Event Monitoring

Monitor events as they occur:

```bash
# Listen to all events
docker exec -it hodei-jobs-postgres psql -U postgres -d hodei_dev -c "LISTEN hodei_events;"

# Create a job and watch events in real-time
./target/release/hodei-jobs-cli job run --name "Test Job" --command "echo 'Hello'"
```

---

## Event-Driven System Monitoring

### Understanding the Event Flow

The system uses PostgreSQL LISTEN/NOTIFY for event-driven communication. Typical event sequence:

```
1. JobCreated      → Job queued for processing
2. JobScheduled    → Worker assigned to job
3. JobStarted      → Worker began execution
4. JobCompleted    → Job finished successfully
5. JobFailed       → Job encountered error
```

### Monitoring Commands

#### 1. Real-Time Event Stream

```bash
# Terminal 1: Monitor events
docker exec -it hodei-jobs-postgres psql -U postgres -d hodei_dev -c "LISTEN hodei_events;"

# Terminal 2: Create job
./target/release/hodei-jobs-cli job run --name "Monitor Test" --command "sleep 5"
```

#### 2. Event Analysis

```sql
-- Find event gaps (where expected sequence breaks)
WITH event_sequence AS (
  SELECT
    job_id,
    event_type,
    occurred_at,
    LAG(event_type) OVER (PARTITION BY job_id ORDER BY occurred_at) as prev_event
  FROM audit_logs
)
SELECT
  job_id,
  event_type,
  prev_event,
  occurred_at
FROM event_sequence
WHERE prev_event IS NOT NULL
  AND NOT (
    (prev_event = 'JobCreated' AND event_type = 'JobScheduled') OR
    (prev_event = 'JobScheduled' AND event_type = 'JobStarted') OR
    (prev_event = 'JobStarted' AND event_type IN ('JobCompleted', 'JobFailed'))
  );

-- Check job lifecycle completion
SELECT
  j.id,
  j.name,
  j.status,
  MAX(CASE WHEN al.event_type = 'JobCreated' THEN al.occurred_at END) as created_at,
  MAX(CASE WHEN al.event_type = 'JobScheduled' THEN al.occurred_at END) as scheduled_at,
  MAX(CASE WHEN al.event_type = 'JobStarted' THEN al.occurred_at END) as started_at,
  MAX(CASE WHEN al.event_type IN ('JobCompleted', 'JobFailed') THEN al.occurred_at END) as finished_at
FROM jobs j
LEFT JOIN audit_logs al ON j.id::text = (al.payload->>'job_id')
GROUP BY j.id, j.name, j.status
ORDER BY j.created_at DESC;
```

---

## Step-by-Step Troubleshooting

### Problem: Job Created But Never Executed

**Step 1: Verify Service Status**
```bash
just status
docker ps | grep hodei
```

**Step 2: Check Job State**
```sql
SELECT id, name, status, created_at FROM jobs WHERE name = 'Test Job';
```

**Step 3: Check Worker Availability**
```sql
SELECT id, worker_type, status, last_heartbeat
FROM workers
WHERE status = 'available';
```

**Step 4: Monitor Event Stream**
```bash
docker exec -it hodei-jobs-postgres psql -U postgres -d hodei_dev -c "LISTEN hodei_events;"
```

**Step 5: Check Audit Logs**
```sql
SELECT occurred_at, event_type, payload
FROM audit_logs
WHERE payload->>'job_id' = '<JOB_ID>'
ORDER BY occurred_at;
```

**Step 6: Examine Scheduler Logs**
```bash
just logs-server | grep -i scheduler
just logs-server | grep -i "job_id=<JOB_ID>"
```

### Problem: Worker Registration Failures

**Step 1: Check Bootstrap Tokens**
```sql
SELECT token, worker_id, expires_at
FROM worker_bootstrap_tokens
ORDER BY expires_at DESC;
```

**Step 2: Verify Token Consumption**
```sql
SELECT * FROM worker_bootstrap_tokens WHERE worker_id IS NOT NULL;
```

**Step 3: Check for Token Expiration**
```sql
SELECT token, expires_at, expires_at < NOW() as expired
FROM worker_bootstrap_tokens;
```

**Step 4: Examine Worker Logs**
```bash
just logs-worker | grep -i "register\|bootstrap\|token"
```

---

## Log Analysis

### Server Logs

#### Important Log Patterns

**Scheduler Decision Making:**
```
Evaluating workers for job scheduling
job_id=<ID> total_workers=<N> available_workers=<M> strategy=<STRATEGY>
```

**Worker Assignment:**
```
Successfully scheduled job
job_id=<ID> worker_id=<ID>
```

**Event Publishing:**
```
Published event JobScheduled
payload=<JSON>
```

#### Filtering Logs

```bash
# Filter by job ID
just logs-server | grep "job_id=<JOB_ID>"

# Filter by worker ID
just logs-server | grep "worker_id=<WORKER_ID>"

# Filter by event type
just logs-server | grep "JobScheduled\|JobStarted\|JobCompleted"

# Filter errors only
just logs-server | grep -i "error\|warn\|fail"
```

### Worker Logs

#### Important Patterns

**Registration:**
```
Registering worker
Bootstrap token consumed
Worker registered successfully
```

**Job Execution:**
```
Received job command
Executing job
Job completed
```

#### Monitoring Worker Activity

```bash
# All worker logs
just logs-worker

# Follow specific worker
docker logs -f hodei-worker-<ID>
```

---

## Database Issues

### Connection Problems

**Check Connection:**
```bash
docker exec -it hodei-jobs-postgres psql -U postgres -d hodei_dev -c "SELECT version();"
```

**Check Connection Pool:**
```sql
SELECT * FROM pg_stat_activity WHERE datname = 'hodei_dev';
```

### Migration Issues

**Run Migrations Manually:**
```bash
# Check migration status
docker exec -it hodei-jobs-postgres psql -U postgres -d hodei_dev -c \
  "SELECT * FROM schema_migrations;"

# Apply pending migrations
cargo run --bin hodei-server -- migrate
```

### Data Consistency

**Verify Foreign Keys:**
```sql
SELECT
  tc.table_name,
  kcu.column_name,
  ccu.table_name AS foreign_table_name,
  ccu.column_name AS foreign_column_name
FROM information_schema.table_constraints AS tc
JOIN information_schema.key_column_usage AS kcu
  ON tc.constraint_name = kcu.constraint_name
JOIN information_schema.constraint_column_usage AS ccu
  ON ccu.constraint_name = tc.constraint_name
WHERE constraint_type = 'FOREIGN KEY';
```

---

## Worker Management

### Viewing Workers

```sql
-- All workers
SELECT id, worker_type, status, last_heartbeat, created_at
FROM workers
ORDER BY created_at DESC;

-- Available workers
SELECT id, worker_type, capabilities
FROM workers
WHERE status = 'available';

-- Worker history
SELECT id, status, last_heartbeat
FROM workers
WHERE last_heartbeat < NOW() - INTERVAL '5 minutes';
```

### Cleaning Up Stale Workers

```sql
-- Mark workers as failed if no heartbeat in 5 minutes
UPDATE workers
SET status = 'failed'
WHERE last_heartbeat < NOW() - INTERVAL '5 minutes'
  AND status = 'available';
```

### Worker Provisioning

**Check Provisioning Logs:**
```bash
just logs-server | grep -i "provision"
```

**Verify Ephemeral Worker Creation:**
```sql
-- Workers created in last hour
SELECT id, worker_type, created_at, status
FROM workers
WHERE created_at > NOW() - INTERVAL '1 hour';
```

---

## Performance Monitoring

### Query Performance

```sql
-- Slow queries
SELECT query, mean_time, calls
FROM pg_stat_statements
ORDER BY mean_time DESC
LIMIT 10;

-- Active connections
SELECT pid, usename, application_name, client_addr, state
FROM pg_stat_activity
WHERE datname = 'hodei_dev';
```

### Event Queue Backlog

```sql
-- Check for events without consumers
SELECT event_type, COUNT(*)
FROM audit_logs
WHERE occurred_at > NOW() - INTERVAL '1 hour'
GROUP BY event_type;
```

---

## Quick Reference Commands

### Service Management

```bash
# Start all services
just up

# Stop all services
just down

# Restart services
just restart

# View status
just status

# View logs
just logs          # All services
just logs-server   # Server only
just logs-worker   # Worker only
```

### Testing Workflows

```bash
# Create and run a job
./target/release/hodei-jobs-cli job run \
  --name "Test Job" \
  --command "echo 'Hello from Hodei!'" \
  --worker-type "default"

# List all jobs
./target/release/hodei-jobs-cli job list

# Get job details
./target/release/hodei-jobs-cli job show <JOB_ID>
```

### Database Queries

```bash
# Access PostgreSQL
docker exec -it hodei-jobs-postgres psql -U postgres -d hodei_dev

# Quick check
docker exec -t hodei-jobs-postgres psql -U postgres -d hodei_dev -c \
  "SELECT COUNT(*) as job_count FROM jobs;"

# Monitor events
docker exec -t hodei-jobs-postgres psql -U postgres -d hodei_dev -c \
  "SELECT occurred_at, event_type FROM audit_logs ORDER BY occurred_at DESC LIMIT 10;"
```

---

## Advanced Debugging

### Correlation ID Tracking

All events include a correlation_id for end-to-end tracing:

```sql
-- Trace all events for a specific job
SELECT occurred_at, event_type, correlation_id, payload
FROM audit_logs
WHERE payload->>'job_id' = '<JOB_ID>'
  OR correlation_id = '<CORRELATION_ID>'
ORDER BY occurred_at;
```

### Event Gap Analysis

```sql
-- Identify where event sequences break
WITH ordered_events AS (
  SELECT
    job_id,
    event_type,
    occurred_at,
    ROW_NUMBER() OVER (PARTITION BY job_id ORDER BY occurred_at) as event_order
  FROM audit_logs
)
SELECT
  oe1.job_id,
  oe1.event_type as current_event,
  oe2.event_type as next_event,
  oe1.occurred_at
FROM ordered_events oe1
LEFT JOIN ordered_events oe2
  ON oe1.job_id = oe2.job_id AND oe1.event_order + 1 = oe2.event_order
WHERE oe2.event_type IS NULL
  AND oe1.event_type NOT IN ('JobCompleted', 'JobFailed');
```

---

## Best Practices

1. **Always monitor real-time events** when testing job execution
2. **Check audit logs first** when investigating issues
3. **Use correlation IDs** for end-to-end tracing
4. **Clean up stale workers** regularly to avoid scheduling issues
5. **Verify database schema consistency** after migrations
6. **Monitor PostgreSQL logs** for database-level issues
7. **Use structured logging** with job_id and worker_id for filtering

---

## Emergency Procedures

### Complete System Reset

If system is in inconsistent state:

```bash
# Stop all services
just down

# Remove all containers and volumes
docker-compose down -v

# Clean database
docker system prune -af

# Restart fresh
just up
```

### Selective Data Cleanup

```sql
-- Clear jobs (keeps workers and tokens)
DELETE FROM jobs;

-- Clear audit logs
DELETE FROM audit_logs;

-- Reset all workers
UPDATE workers SET status = 'stopped';

-- Clear bootstrap tokens
DELETE FROM worker_bootstrap_tokens;
```

---

## Support and Resources

- **Architecture Documentation**: `docs/architecture.md`
- **API Documentation**: `docs/asyncapi.md`
- **User Manual**: `docs/USER_MANUAL.md`
- **Getting Started**: `docs/GETTING_STARTED.md`

---

*This guide is based on real debugging sessions and common issues encountered in the Hodei Job Platform. Keep it updated as new issues are discovered and resolved.*

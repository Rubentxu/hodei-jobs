-- Migration: Add indexes for efficient reconciliation queries
-- Part of EPIC-20: Auditable Event Architecture

-- Index for finding unhealthy workers (stale heartbeat detection)
-- Covers: find_unhealthy() query filtering by last_heartbeat and state
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_workers_heartbeat_state
ON workers (last_heartbeat, state)
WHERE state NOT IN ('TERMINATED', 'TERMINATING');

-- Index for workers with assigned jobs (job reassignment detection)
-- Covers: reconciliation queries that check current_job_id
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_workers_current_job
ON workers (current_job_id)
WHERE current_job_id IS NOT NULL;

-- Composite index for worker lifecycle queries
-- Covers: find_for_termination() and general lifecycle management
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_workers_lifecycle
ON workers (state, updated_at)
WHERE state IN ('READY', 'BUSY', 'CONNECTING', 'CREATING');

-- Index for outbox event processing
-- Covers: get_pending_events() in OutboxRelay
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_outbox_events_pending
ON outbox_events (status, created_at)
WHERE status = 'PENDING';

-- Index for outbox idempotency key lookups
-- Covers: exists_by_idempotency_key() duplicate detection
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_outbox_events_idempotency
ON outbox_events (idempotency_key)
WHERE idempotency_key IS NOT NULL;

-- Comment for documentation
COMMENT ON INDEX idx_workers_heartbeat_state IS 'Optimizes heartbeat reconciliation queries - EPIC-20 US-20.8';
COMMENT ON INDEX idx_workers_current_job IS 'Optimizes job reassignment detection during reconciliation';
COMMENT ON INDEX idx_workers_lifecycle IS 'Optimizes worker lifecycle management queries';
COMMENT ON INDEX idx_outbox_events_pending IS 'Optimizes outbox relay pending event retrieval';
COMMENT ON INDEX idx_outbox_events_idempotency IS 'Optimizes idempotency key duplicate detection';

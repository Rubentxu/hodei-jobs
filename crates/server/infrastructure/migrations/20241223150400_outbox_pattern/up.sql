-- Migration: Outbox Pattern - Transactional Outbox for reliable event publishing
-- Version: 20241223150400
-- Purpose: Solve dual-write problem between database and event bus

CREATE TABLE IF NOT EXISTS outbox_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Event identity
    aggregate_id UUID NOT NULL,
    aggregate_type VARCHAR(20) NOT NULL CHECK (aggregate_type IN ('JOB', 'WORKER', 'PROVIDER', 'SAGA')),
    event_type VARCHAR(50) NOT NULL,
    event_version INTEGER DEFAULT 1,

    -- Event content
    payload JSONB NOT NULL,
    metadata JSONB,

    -- Idempotency
    idempotency_key VARCHAR(100),

    -- Audit trail
    created_at TIMESTAMPTZ DEFAULT NOW(),
    published_at TIMESTAMPTZ,

    -- Processing state
    status VARCHAR(20) DEFAULT 'PENDING' CHECK (status IN ('PENDING', 'PUBLISHED', 'FAILED')),
    retry_count INTEGER DEFAULT 0,
    last_error TEXT,

    -- Constraints
    UNIQUE(idempotency_key)
);

-- Indexes for outbox_events
CREATE INDEX IF NOT EXISTS idx_outbox_status_created
ON outbox_events(status, created_at)
WHERE status = 'PENDING';

CREATE INDEX IF NOT EXISTS idx_outbox_aggregate
ON outbox_events(aggregate_type, aggregate_id);

CREATE INDEX IF NOT EXISTS idx_outbox_event_type
ON outbox_events(event_type, created_at);

CREATE INDEX IF NOT EXISTS idx_outbox_pending_retries
ON outbox_events(status, retry_count)
WHERE status = 'PENDING';

-- Comments
COMMENT ON TABLE outbox_events IS 'Transactional Outbox Pattern - stores domain events for reliable publishing';
COMMENT ON COLUMN outbox_events.idempotency_key IS 'Unique key to prevent duplicate event processing';
COMMENT ON COLUMN outbox_events.status IS 'PENDING: not published, PUBLISHED: sent to event bus, FAILED: retried exceeded';

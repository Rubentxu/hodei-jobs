-- Migration: Add EPIC-32 reactive system tables
-- Date: 2026-01-01

-- Create subscription_offsets table for event sourcing
CREATE TABLE IF NOT EXISTS subscription_offsets (
    subscription_id VARCHAR(255) PRIMARY KEY,
    topic VARCHAR(255) NOT NULL,
    consumer_group VARCHAR(255) NOT NULL,
    last_event_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    last_event_occurred_at TIMESTAMPTZ NOT NULL DEFAULT '-infinity',
    last_processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    event_count BIGINT NOT NULL DEFAULT 0,
    gap_detected_at TIMESTAMPTZ,
    gap_resolved_at TIMESTAMPTZ,
    metadata JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create event_processing_dlq table for failed events
CREATE TABLE IF NOT EXISTS event_processing_dlq (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id UUID NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    aggregate_id VARCHAR(255) NOT NULL,
    payload JSONB NOT NULL,
    error_message TEXT NOT NULL,
    error_count SMALLINT NOT NULL DEFAULT 1,
    first_failure_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_failure_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    resolution_action VARCHAR(50),
    resolution_metadata JSONB,
    subscription_id VARCHAR(255) NOT NULL,
    retry_count SMALLINT NOT NULL DEFAULT 0,
    max_retries SMALLINT NOT NULL DEFAULT 3,
    metadata JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(event_id, subscription_id)
);

-- Create indexes for performance
CREATE INDEX IF NOT EXISTS idx_subscription_offsets_topic ON subscription_offsets(topic);
CREATE INDEX IF NOT EXISTS idx_subscription_offsets_consumer_group ON subscription_offsets(consumer_group);
CREATE INDEX IF NOT EXISTS idx_dlq_subscription_id ON event_processing_dlq(subscription_id);
CREATE INDEX IF NOT EXISTS idx_dlq_event_type ON event_processing_dlq(event_type);
CREATE INDEX IF NOT EXISTS idx_dlq_first_failure ON event_processing_dlq(first_failure_at);

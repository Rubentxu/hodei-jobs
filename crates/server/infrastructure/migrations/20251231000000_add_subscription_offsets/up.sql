-- Migration: EPIC-32 Add subscription offsets table
-- Purpose: Track last processed event offset for persistent subscriptions

-- UP migration
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

-- Index for topic and consumer group lookups
CREATE INDEX IF NOT EXISTS idx_subscription_offsets_topic_group
ON subscription_offsets(topic, consumer_group);

-- Index for detecting gaps (most recent events first)
CREATE INDEX IF NOT EXISTS idx_subscription_offsets_last_event
ON subscription_offsets(last_event_occurred_at DESC);

-- Index for gap detection queries
CREATE INDEX IF NOT EXISTS idx_subscription_offsets_gap
ON subscription_offsets(gap_detected_at)
WHERE gap_detected_at IS NOT NULL;

-- DOWN migration
DROP TABLE IF EXISTS subscription_offsets;

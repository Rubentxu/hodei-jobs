#!/bin/bash
set -e

echo "Running manual migration for domain_events..."

docker exec hodei-jobs-postgres psql -U postgres -d hodei -c "
CREATE TABLE IF NOT EXISTS domain_events (
    id UUID PRIMARY KEY,
    occurred_at TIMESTAMPTZ NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    aggregate_id VARCHAR(255) NOT NULL,
    correlation_id VARCHAR(255),
    actor VARCHAR(255),
    payload JSONB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_domain_events_occurred_at ON domain_events(occurred_at);
CREATE INDEX IF NOT EXISTS idx_domain_events_aggregate_id ON domain_events(aggregate_id);
CREATE INDEX IF NOT EXISTS idx_domain_events_correlation_id ON domain_events(correlation_id);
"

echo "Migration complete."

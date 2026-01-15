-- Migration: Add provider filtering columns (US-86.7)
-- Purpose: Add columns for region, labels, and annotations filtering

-- Add new columns for provider selection filtering
ALTER TABLE provider_configs ADD COLUMN IF NOT EXISTS preferred_region VARCHAR(100);
ALTER TABLE provider_configs ADD COLUMN IF NOT EXISTS allowed_regions JSONB NOT NULL DEFAULT '[]';
ALTER TABLE provider_configs ADD COLUMN IF NOT EXISTS required_labels JSONB NOT NULL DEFAULT '{}';
ALTER TABLE provider_configs ADD COLUMN IF NOT EXISTS annotations JSONB NOT NULL DEFAULT '{}';

-- Update comments
COMMENT ON COLUMN provider_configs.preferred_region IS 'Preferred region for this provider';
COMMENT ON COLUMN provider_configs.allowed_regions IS 'List of regions where this provider can run jobs';
COMMENT ON COLUMN provider_configs.required_labels IS 'Labels that must be present on the provider';
COMMENT ON COLUMN provider_configs.annotations IS 'Annotations for provider metadata and compliance';

-- Update default providers with new fields
UPDATE provider_configs SET
    allowed_regions = '["us-east-1", "us-west-2", "eu-west-1"]'::jsonb,
    required_labels = '{}'::jsonb,
    annotations = '{}'::jsonb
WHERE name = 'Docker';

UPDATE provider_configs SET
    preferred_region = 'us-east-1',
    allowed_regions = '["us-east-1", "us-west-2", "eu-west-1"]'::jsonb,
    required_labels = '{"infrastructure": "kubernetes"}'::jsonb,
    annotations = '{"compliance": "soc2"}'::jsonb
WHERE name = 'Kubernetes';

-- Create indexes for filtering queries
CREATE INDEX IF NOT EXISTS idx_provider_configs_preferred_region ON provider_configs(preferred_region);
CREATE INDEX IF NOT EXISTS idx_provider_configs_allowed_regions ON provider_configs USING GIN (allowed_regions);
CREATE INDEX IF NOT EXISTS idx_provider_configs_required_labels ON provider_configs USING GIN (required_labels);
CREATE INDEX IF NOT EXISTS idx_provider_configs_annotations ON provider_configs USING GIN (annotations);

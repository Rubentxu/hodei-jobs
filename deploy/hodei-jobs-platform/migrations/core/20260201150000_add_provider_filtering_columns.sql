-- =============================================================================
-- Migration: Add provider filtering columns
-- Description: Adds columns for region/label/annotation filtering to provider_configs
-- Author: Hodei Jobs Platform
-- Date: 2025-02-01
-- =============================================================================

-- Add filtering columns to provider_configs
ALTER TABLE provider_configs
ADD COLUMN IF NOT EXISTS preferred_region VARCHAR(100),
ADD COLUMN IF NOT EXISTS allowed_regions JSONB DEFAULT '[]'::jsonb,
ADD COLUMN IF NOT EXISTS required_labels JSONB DEFAULT '{}'::jsonb,
ADD COLUMN IF NOT EXISTS annotations JSONB DEFAULT '{}'::jsonb;

-- Index for region queries
CREATE INDEX IF NOT EXISTS idx_provider_configs_preferred_region
ON provider_configs(preferred_region) WHERE preferred_region IS NOT NULL;

-- GIN index for JSONB queries on allowed_regions
CREATE INDEX IF NOT EXISTS idx_provider_configs_allowed_regions
ON provider_configs USING GIN (allowed_regions);

-- GIN index for JSONB queries on required_labels
CREATE INDEX IF NOT EXISTS idx_provider_configs_required_labels
ON provider_configs USING GIN (required_labels);

-- GIN index for JSONB queries on annotations
CREATE INDEX IF NOT EXISTS idx_provider_configs_annotations
ON provider_configs USING GIN (annotations);

-- =============================================================================
-- Rollback Script (commented out for reference)
-- =============================================================================
-- ALTER TABLE provider_configs
-- DROP COLUMN IF EXISTS preferred_region,
-- DROP COLUMN IF EXISTS allowed_regions,
-- DROP COLUMN IF EXISTS required_labels,
-- DROP COLUMN IF EXISTS annotations;

-- DROP INDEX IF EXISTS idx_provider_configs_preferred_region;
-- DROP INDEX IF EXISTS idx_provider_configs_allowed_regions;
-- DROP INDEX IF EXISTS idx_provider_configs_required_labels;
-- DROP INDEX IF EXISTS idx_provider_configs_annotations;

-- Migration: Create provider_configs table
-- Purpose: Store provider configurations for worker provisioners (Docker, Kubernetes, etc.)

-- Create provider_configs table
CREATE TABLE IF NOT EXISTS provider_configs (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    provider_type VARCHAR(100) NOT NULL,
    config JSONB NOT NULL DEFAULT '{}',
    status VARCHAR(50) NOT NULL DEFAULT 'INACTIVE',
    priority INTEGER NOT NULL DEFAULT 0,
    max_workers INTEGER NOT NULL DEFAULT 10,
    tags JSONB NOT NULL DEFAULT '[]',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Index for finding enabled providers
CREATE INDEX IF NOT EXISTS idx_provider_configs_status ON provider_configs(status) WHERE status = 'ACTIVE';

-- Index for finding providers by type
CREATE INDEX IF NOT EXISTS idx_provider_configs_type ON provider_configs(provider_type);

-- Index for priority-based provider selection
CREATE INDEX IF NOT EXISTS idx_provider_configs_priority ON provider_configs(priority DESC);

-- Trigger function to update updated_at
CREATE OR REPLACE FUNCTION update_provider_configs_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create trigger for updated_at
DROP TRIGGER IF EXISTS update_provider_configs_updated_at ON provider_configs;
CREATE TRIGGER update_provider_configs_updated_at
    BEFORE UPDATE ON provider_configs
    FOR EACH ROW
    EXECUTE FUNCTION update_provider_configs_updated_at();

-- Insert default providers for development
INSERT INTO provider_configs (id, name, provider_type, config, status, priority, max_workers, tags)
VALUES
    ('515e2c35-997e-4bdc-8647-2e0f75db9f93', 'Docker', 'docker',
     '{
       "type": "docker",
       "network": null,
       "networkMode": "host",
       "socket_path": "/var/run/docker.sock",
       "default_image": "hodei-jobs-worker:v3",
       "registry_auth": null,
       "default_resources": null
     }'::jsonb,
     'INACTIVE', 0, 10, '[]'::jsonb),
    ('1f5359ce-ca99-4cdd-ad18-2c6287a07b5e', 'Kubernetes', 'kubernetes',
     '{
       "type": "kubernetes",
       "namespace": "hodei-jobs-workers",
       "tolerations": [],
       "default_image": "hodei-jobs-worker:latest",
       "node_selector": {},
       "kubeconfig_path": null,
       "service_account": "hodei-jobs-worker",
       "default_resources": null,
       "image_pull_secrets": []
     }'::jsonb,
     'ACTIVE', 0, 10, '[]'::jsonb)
ON CONFLICT (name) DO NOTHING;

-- Comments for documentation
COMMENT ON TABLE provider_configs IS 'Provider configurations for worker provisioners (Docker, Kubernetes, etc.)';
COMMENT ON COLUMN provider_configs.id IS 'Unique provider identifier';
COMMENT ON COLUMN provider_configs.name IS 'Provider name (unique)';
COMMENT ON COLUMN provider_configs.provider_type IS 'Provider type: docker, kubernetes, lambda, etc.';
COMMENT ON COLUMN provider_configs.config IS 'Provider-specific configuration as JSON';
COMMENT ON COLUMN provider_configs.status IS 'Provider status: ACTIVE, INACTIVE, MAINTENANCE';
COMMENT ON COLUMN provider_configs.priority IS 'Provider priority for scheduling (higher = preferred)';
COMMENT ON COLUMN provider_configs.max_workers IS 'Maximum concurrent workers for this provider';
COMMENT ON COLUMN provider_configs.tags IS 'Tags for provider selection/filtering';
COMMENT ON COLUMN provider_configs.metadata IS 'Additional metadata';

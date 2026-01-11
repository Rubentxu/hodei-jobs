-- Migration: Add provider_resource_id to worker_bootstrap_tokens
-- This enables JIT registration to recover the correct provider resource ID
-- (container ID for Docker, pod name for Kubernetes, VM ID for Firecracker)
-- without hardcoding provider-specific logic.

-- Add provider_resource_id column (nullable for backwards compatibility)
ALTER TABLE worker_bootstrap_tokens
ADD COLUMN provider_resource_id TEXT;

-- Add index for faster lookups during JIT registration
CREATE INDEX IF NOT EXISTS idx_worker_bootstrap_tokens_provider_resource_id
ON worker_bootstrap_tokens(provider_resource_id)
WHERE provider_resource_id IS NOT NULL;

-- Add comment explaining the column
COMMENT ON COLUMN worker_bootstrap_tokens.provider_resource_id IS
'Provider-specific resource identifier (container ID, pod name, VM ID) used for worker cleanup. Stored at OTP generation to enable correct JIT registration.';

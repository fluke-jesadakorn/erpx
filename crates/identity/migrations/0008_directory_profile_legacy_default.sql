-- Preserve server-side compatibility for legacy/internal connector writers
-- while all public administration paths require an approved profile key.
ALTER TABLE erpx_identity.directory_connectors
    ALTER COLUMN credential_profile_key SET DEFAULT 'legacy-unconfigured';

-- Development persona access is a local-only capability.  Bearer context
-- values never enter SQL; only SHA-256 digests are stored.

CREATE TABLE IF NOT EXISTS erpx_identity.development_cluster_identity (
    cluster_id UUID PRIMARY KEY,
    marker TEXT NOT NULL UNIQUE CHECK (marker = 'local-ucs-lab'),
    authority_fingerprint TEXT NOT NULL CHECK (authority_fingerprint ~ '^[0-9a-f]{64}$'),
    profile TEXT NOT NULL CHECK (profile = 'local'),
    created_at TIMESTAMPTZ NOT NULL,
    reconciled_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS erpx_identity.development_persona_manifests (
    persona_key TEXT PRIMARY KEY,
    principal_ref TEXT NOT NULL UNIQUE,
    directory_username TEXT NOT NULL,
    tenant_id TEXT,
    organization_scope TEXT NOT NULL,
    role_key TEXT NOT NULL,
    feature TEXT NOT NULL,
    permissions TEXT[] NOT NULL,
    entitled BOOLEAN NOT NULL,
    directory_enabled BOOLEAN NOT NULL,
    requires_totp_for_high_risk BOOLEAN NOT NULL,
    manifest_version BIGINT NOT NULL CHECK (manifest_version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS development_persona_manifests_tenant_idx
    ON erpx_identity.development_persona_manifests (tenant_id, organization_scope, role_key);

CREATE TABLE IF NOT EXISTS erpx_identity.development_persona_contexts (
    id UUID PRIMARY KEY,
    token_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    controller_principal_ref TEXT NOT NULL,
    controller_session_id UUID NOT NULL,
    controller_device_id UUID NOT NULL,
    persona_key TEXT NOT NULL REFERENCES erpx_identity.development_persona_manifests(persona_key),
    role_assignment_ref TEXT NOT NULL,
    tenant_id TEXT,
    organization_scope TEXT NOT NULL,
    feature TEXT NOT NULL,
    reason TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    revoked_reason TEXT,
    CHECK (controller_principal_ref = 'local:dev-controller'),
    CHECK (expires_at > issued_at),
    CHECK (expires_at <= issued_at + interval '30 minutes'),
    CHECK ((revoked_at IS NULL) = (revoked_reason IS NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS development_persona_contexts_active_session_idx
    ON erpx_identity.development_persona_contexts (controller_session_id)
    WHERE revoked_at IS NULL;
CREATE INDEX IF NOT EXISTS development_persona_contexts_expiry_idx
    ON erpx_identity.development_persona_contexts (expires_at, revoked_at);

CREATE TABLE IF NOT EXISTS erpx_identity.development_audit (
    id UUID PRIMARY KEY,
    controller_principal_ref TEXT NOT NULL,
    persona_principal_ref TEXT NOT NULL,
    persona_key TEXT NOT NULL,
    role_key TEXT NOT NULL,
    tenant_id TEXT,
    organization_scope TEXT NOT NULL,
    feature TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('issued', 'ended', 'authorized', 'denied', 'expired')),
    recorded_at TIMESTAMPTZ NOT NULL,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    CHECK (jsonb_typeof(details) = 'object')
);

CREATE INDEX IF NOT EXISTS development_audit_recorded_idx
    ON erpx_identity.development_audit (recorded_at DESC, id DESC);

CREATE OR REPLACE FUNCTION erpx_identity.reject_development_audit_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'development audit is append-only';
END;
$$;

DROP TRIGGER IF EXISTS development_audit_append_only ON erpx_identity.development_audit;
CREATE TRIGGER development_audit_append_only
    BEFORE UPDATE OR DELETE ON erpx_identity.development_audit
    FOR EACH ROW EXECUTE FUNCTION erpx_identity.reject_development_audit_mutation();

CREATE TABLE IF NOT EXISTS erpx_identity.development_seed_receipts (
    receipt_key TEXT PRIMARY KEY,
    fixture_version TEXT NOT NULL,
    tenant_id TEXT,
    manifest_digest TEXT NOT NULL CHECK (manifest_digest ~ '^[0-9a-f]{64}$'),
    applied_at TIMESTAMPTZ NOT NULL,
    counts JSONB NOT NULL CHECK (jsonb_typeof(counts) = 'object')
);

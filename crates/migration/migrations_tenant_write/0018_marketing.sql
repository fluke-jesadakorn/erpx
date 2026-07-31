-- Marketing write-side state. These rows are authoritative only in the
-- tenant write database; read-side tables with the same logical names are
-- disposable projections rebuilt from this state and its outbox.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_profiles (
    profile_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    scope JSONB NOT NULL,
    external_ref TEXT NOT NULL CHECK (btrim(external_ref) <> ''),
    dedupe_key TEXT NOT NULL CHECK (length(dedupe_key) = 64),
    email TEXT,
    phone TEXT,
    locale TEXT NOT NULL CHECK (btrim(locale) <> ''),
    attributes JSONB NOT NULL,
    payload JSONB NOT NULL,
    payload_hash TEXT NOT NULL CHECK (length(payload_hash) = 64),
    version BIGINT NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (holding_ref, dedupe_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_consents (
    consent_id UUID PRIMARY KEY,
    profile_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    scope JSONB NOT NULL,
    purpose TEXT NOT NULL CHECK (purpose IN ('newsletter', 'product_updates', 'promotional', 'transactional', 'lead_nurture')),
    channel TEXT NOT NULL CHECK (channel IN ('email', 'sms', 'social', 'push', 'webhook')),
    state TEXT NOT NULL CHECK (state IN ('pending_double_opt_in', 'granted', 'revoked')),
    source TEXT NOT NULL CHECK (btrim(source) <> ''),
    jurisdiction TEXT NOT NULL CHECK (btrim(jurisdiction) <> ''),
    policy_version TEXT NOT NULL CHECK (btrim(policy_version) <> ''),
    proof_ref TEXT,
    recorded_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (expires_at IS NULL OR expires_at > recorded_at)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_suppressions (
    suppression_id UUID PRIMARY KEY,
    profile_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    scope JSONB NOT NULL,
    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    source TEXT NOT NULL CHECK (btrim(source) <> ''),
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ,
    payload JSONB NOT NULL,
    payload_hash TEXT NOT NULL CHECK (length(payload_hash) = 64),
    version BIGINT NOT NULL CHECK (version > 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (expires_at IS NULL OR expires_at > created_at)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_audiences (
    audience_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    scope JSONB NOT NULL,
    materialized_at TIMESTAMPTZ NOT NULL,
    profile_count BIGINT NOT NULL CHECK (profile_count >= 0),
    membership_hash TEXT NOT NULL CHECK (length(membership_hash) = 64),
    version BIGINT NOT NULL CHECK (version > 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_audience_memberships (
    audience_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    membership_version BIGINT NOT NULL CHECK (membership_version > 0),
    materialized_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (audience_id, profile_id)
);

-- This ledger is reserved and completed in the same transaction as each
-- mutation. A request hash cannot be rebound to another operation or payload.
CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_command_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    operation TEXT NOT NULL CHECK (operation IN ('append_consent', 'revoke_consent', 'upsert_suppression', 'upsert_audience_membership')),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    resource_id UUID NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('reserved', 'completed')),
    response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    CHECK ((status = 'reserved' AND response IS NULL AND completed_at IS NULL)
        OR (status = 'completed' AND response IS NOT NULL AND completed_at IS NOT NULL))
);

CREATE OR REPLACE FUNCTION erpx_tenant.reject_marketing_consent_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'marketing consent records are append-only';
END;
$$;

DROP TRIGGER IF EXISTS marketing_consent_append_only
    ON erpx_tenant.marketing_consents;
CREATE TRIGGER marketing_consent_append_only
BEFORE UPDATE OR DELETE ON erpx_tenant.marketing_consents
FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_marketing_consent_mutation();

CREATE INDEX IF NOT EXISTS marketing_profiles_external_ref_idx
    ON erpx_tenant.marketing_profiles (holding_ref, external_ref);
CREATE INDEX IF NOT EXISTS marketing_consents_profile_idx
    ON erpx_tenant.marketing_consents (holding_ref, profile_id, recorded_at DESC);
CREATE INDEX IF NOT EXISTS marketing_suppressions_profile_idx
    ON erpx_tenant.marketing_suppressions (holding_ref, profile_id, created_at DESC);
CREATE INDEX IF NOT EXISTS marketing_audiences_scope_idx
    ON erpx_tenant.marketing_audiences (holding_ref, updated_at DESC);
CREATE INDEX IF NOT EXISTS marketing_memberships_page_idx
    ON erpx_tenant.marketing_audience_memberships
       (holding_ref, audience_id, profile_id);

ALTER TABLE erpx_tenant.marketing_profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_profiles FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_consents ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_consents FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_suppressions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_suppressions FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_audiences ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_audiences FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_audience_memberships ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_audience_memberships FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_marketing_profiles ON erpx_tenant.marketing_profiles;
CREATE POLICY tenant_scope_marketing_profiles ON erpx_tenant.marketing_profiles
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_consents ON erpx_tenant.marketing_consents;
CREATE POLICY tenant_scope_marketing_consents ON erpx_tenant.marketing_consents
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_suppressions ON erpx_tenant.marketing_suppressions;
CREATE POLICY tenant_scope_marketing_suppressions ON erpx_tenant.marketing_suppressions
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_audiences ON erpx_tenant.marketing_audiences;
CREATE POLICY tenant_scope_marketing_audiences ON erpx_tenant.marketing_audiences
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_memberships ON erpx_tenant.marketing_audience_memberships;
CREATE POLICY tenant_scope_marketing_memberships ON erpx_tenant.marketing_audience_memberships
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_idempotency ON erpx_tenant.marketing_command_idempotency;
CREATE POLICY tenant_scope_marketing_idempotency ON erpx_tenant.marketing_command_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

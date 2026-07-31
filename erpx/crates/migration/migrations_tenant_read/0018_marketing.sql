-- Marketing read-side projection. It is disposable and must be rebuilt from
-- tenant write state; no command may use these rows as its source of truth.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_profiles (
    profile_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    scope JSONB NOT NULL,
    external_ref TEXT NOT NULL,
    dedupe_key TEXT NOT NULL,
    email TEXT,
    phone TEXT,
    locale TEXT NOT NULL,
    attributes JSONB NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_consents (
    consent_id UUID PRIMARY KEY,
    profile_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    scope JSONB NOT NULL,
    purpose TEXT NOT NULL,
    channel TEXT NOT NULL,
    state TEXT NOT NULL,
    source TEXT NOT NULL,
    jurisdiction TEXT NOT NULL,
    policy_version TEXT NOT NULL,
    proof_ref TEXT,
    recorded_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_suppressions (
    suppression_id UUID PRIMARY KEY,
    profile_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    scope JSONB NOT NULL,
    reason TEXT NOT NULL,
    source TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_audiences (
    audience_id UUID PRIMARY KEY,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    scope JSONB NOT NULL,
    materialized_at TIMESTAMPTZ NOT NULL,
    profile_count BIGINT NOT NULL CHECK (profile_count >= 0),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_audience_memberships (
    audience_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    materialized_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (audience_id, profile_id)
);

CREATE INDEX IF NOT EXISTS marketing_read_profiles_external_ref_idx
    ON erpx_tenant.marketing_profiles (holding_ref, external_ref);
CREATE INDEX IF NOT EXISTS marketing_read_consents_profile_idx
    ON erpx_tenant.marketing_consents (holding_ref, profile_id, recorded_at DESC);
CREATE INDEX IF NOT EXISTS marketing_read_suppressions_profile_idx
    ON erpx_tenant.marketing_suppressions (holding_ref, profile_id, created_at DESC);
CREATE INDEX IF NOT EXISTS marketing_read_memberships_page_idx
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

DROP POLICY IF EXISTS tenant_scope_marketing_read_profiles ON erpx_tenant.marketing_profiles;
CREATE POLICY tenant_scope_marketing_read_profiles ON erpx_tenant.marketing_profiles
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_read_consents ON erpx_tenant.marketing_consents;
CREATE POLICY tenant_scope_marketing_read_consents ON erpx_tenant.marketing_consents
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_read_suppressions ON erpx_tenant.marketing_suppressions;
CREATE POLICY tenant_scope_marketing_read_suppressions ON erpx_tenant.marketing_suppressions
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_read_audiences ON erpx_tenant.marketing_audiences;
CREATE POLICY tenant_scope_marketing_read_audiences ON erpx_tenant.marketing_audiences
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_read_memberships ON erpx_tenant.marketing_audience_memberships;
CREATE POLICY tenant_scope_marketing_read_memberships ON erpx_tenant.marketing_audience_memberships
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

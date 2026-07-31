-- Provider-account routing metadata. Secrets stay in the deployment secret
-- manager; this table stores only opaque references and authoritative tenant
-- binding data used before a callback reaches a tenant database.
CREATE TABLE IF NOT EXISTS erpx_control.marketing_provider_accounts (
    provider_account_id UUID PRIMARY KEY,
    provider_account_ref TEXT NOT NULL CHECK (btrim(provider_account_ref) <> ''),
    tenant_id TEXT NOT NULL REFERENCES erpx_control.tenant_registry(tenant_id) ON DELETE RESTRICT,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    holding_id UUID NOT NULL,
    verifier_key_ref TEXT NOT NULL CHECK (btrim(verifier_key_ref) <> ''),
    state TEXT NOT NULL CHECK (state IN ('active', 'suspended')),
    version BIGINT NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_ref)
);

CREATE INDEX IF NOT EXISTS marketing_provider_accounts_route_idx
    ON erpx_control.marketing_provider_accounts
       (provider_account_ref, tenant_id, holding_ref, state);

-- Control-plane-only routing metadata for Finance bank-provider accounts.
-- Provider SDK payloads, access tokens, and credential bytes remain outside
-- PostgreSQL in the deployment credential manager. This table stores only
-- opaque references needed to route a validated provider operation.
CREATE TABLE IF NOT EXISTS erpx_control.finance_bank_provider_accounts (
    finance_bank_provider_account_id UUID PRIMARY KEY,
    provider_account_ref TEXT NOT NULL
        CHECK (length(btrim(provider_account_ref)) BETWEEN 1 AND 255),
    provider_key TEXT NOT NULL
        CHECK (length(btrim(provider_key)) BETWEEN 1 AND 255),
    tenant_id TEXT NOT NULL
        REFERENCES erpx_control.tenant_registry(tenant_id) ON DELETE RESTRICT,
    holding_ref TEXT NOT NULL
        CHECK (length(btrim(holding_ref)) BETWEEN 1 AND 255),
    holding_id UUID NOT NULL
        CHECK (holding_id <> '00000000-0000-0000-0000-000000000000'::uuid),
    credential_key_ref TEXT NOT NULL
        CHECK (length(btrim(credential_key_ref)) BETWEEN 1 AND 255),
    state TEXT NOT NULL CHECK (state IN ('active', 'suspended')),
    version BIGINT NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_ref)
);

CREATE INDEX IF NOT EXISTS finance_bank_provider_accounts_route_idx
    ON erpx_control.finance_bank_provider_accounts
       (provider_account_ref, provider_key, tenant_id, holding_ref,
        credential_key_ref, state);

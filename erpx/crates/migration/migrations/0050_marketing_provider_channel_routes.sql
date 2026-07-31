-- Per-account, provider-neutral channel routes. These URLs contain no
-- credentials; secret values remain in the deployment secret manager.
ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS channel_routes JSONB NOT NULL DEFAULT '[]'::jsonb
        CHECK (jsonb_typeof(channel_routes) = 'array');

CREATE INDEX IF NOT EXISTS marketing_provider_accounts_channel_routes_idx
    ON erpx_control.marketing_provider_accounts (tenant_id, holding_ref, state);

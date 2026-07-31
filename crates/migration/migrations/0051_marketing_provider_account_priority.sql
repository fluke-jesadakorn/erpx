-- Lower priority values are selected first for a channel. The provider
-- reference remains the deterministic tie-breaker; this is routing metadata,
-- not a secret or a payment authority.
ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS provider_priority INTEGER NOT NULL DEFAULT 100
        CHECK (provider_priority BETWEEN 1 AND 10_000);

CREATE INDEX IF NOT EXISTS marketing_provider_accounts_priority_idx
    ON erpx_control.marketing_provider_accounts
       (tenant_id, holding_ref, state, provider_priority, provider_account_ref);

-- Marketing provider onboarding is control-plane configuration only. Secret
-- values remain in the deployment secret manager; these columns contain only
-- opaque references and non-secret routing metadata.
ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS onboarding_state TEXT NOT NULL DEFAULT 'verified'
        CHECK (onboarding_state IN ('pending', 'verified'));

ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS channels TEXT[] NOT NULL DEFAULT ARRAY['email']::TEXT[]
        CHECK (
            cardinality(channels) BETWEEN 1 AND 5
            AND channels <@ ARRAY['email', 'sms', 'social', 'push', 'webhook']::TEXT[]
        );

ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS last_verified_at TIMESTAMPTZ;

ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS verification_reference_hash TEXT
        CHECK (verification_reference_hash IS NULL OR length(verification_reference_hash) = 64);

ALTER TABLE erpx_control.marketing_provider_accounts
    ADD CONSTRAINT marketing_provider_accounts_active_requires_verification
    CHECK (state <> 'active' OR onboarding_state = 'verified');

CREATE INDEX IF NOT EXISTS marketing_provider_accounts_onboarding_idx
    ON erpx_control.marketing_provider_accounts (tenant_id, holding_ref, onboarding_state, state);

-- Profile erasure tombstones scrub direct contact fields while retaining the
-- profile identity and append-only consent/audit facts.
ALTER TABLE erpx_tenant.marketing_profiles
    ADD COLUMN IF NOT EXISTS erased_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS erasure_reason TEXT;

ALTER TABLE erpx_tenant.marketing_profiles
    ADD CONSTRAINT marketing_profile_erasure_reason_bound
        CHECK (erasure_reason IS NULL OR (btrim(erasure_reason) <> '' AND octet_length(erasure_reason) <= 512)),
    ADD CONSTRAINT marketing_profile_erased_at_pair
        CHECK ((erased_at IS NULL AND erasure_reason IS NULL)
            OR (erased_at IS NOT NULL AND erasure_reason IS NOT NULL));

ALTER TABLE erpx_tenant.marketing_command_idempotency
    DROP CONSTRAINT IF EXISTS marketing_command_idempotency_operation_check,
    ADD CONSTRAINT marketing_command_idempotency_operation_check
        CHECK (operation IN ('append_consent', 'revoke_consent',
                             'upsert_suppression', 'upsert_audience_membership',
                             'erase_profile'));

CREATE INDEX IF NOT EXISTS marketing_profiles_erasure_idx
    ON erpx_tenant.marketing_profiles (holding_ref, erased_at, profile_id);

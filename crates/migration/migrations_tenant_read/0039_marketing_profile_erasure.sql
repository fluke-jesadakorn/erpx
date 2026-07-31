-- Disposable read profile rows carry only the redacted erasure marker; old
-- outbox profile snapshots must not be able to repopulate contact values.
ALTER TABLE erpx_tenant.marketing_profiles
    ADD COLUMN IF NOT EXISTS erased_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS erasure_reason TEXT;

ALTER TABLE erpx_tenant.marketing_profiles
    ADD CONSTRAINT marketing_read_profile_erasure_reason_bound
        CHECK (erasure_reason IS NULL OR (btrim(erasure_reason) <> '' AND octet_length(erasure_reason) <= 512)),
    ADD CONSTRAINT marketing_read_profile_erased_at_pair
        CHECK ((erased_at IS NULL AND erasure_reason IS NULL)
            OR (erased_at IS NOT NULL AND erasure_reason IS NOT NULL));

CREATE INDEX IF NOT EXISTS marketing_read_profiles_erasure_idx
    ON erpx_tenant.marketing_profiles (holding_ref, erased_at, profile_id);

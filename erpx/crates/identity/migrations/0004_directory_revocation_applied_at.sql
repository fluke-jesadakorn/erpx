-- Durable evidence for the revocation SLA. A pending row is the source-side
-- obligation; an applied row must retain the timestamp at which the identity
-- projection committed its inactive state. Breached rows remain evidence of a
-- missed obligation and intentionally have no applied timestamp.
ALTER TABLE erpx_identity.directory_revocation_sla
    ADD COLUMN IF NOT EXISTS applied_at TIMESTAMPTZ;

UPDATE erpx_identity.directory_revocation_sla
SET applied_at = observed_at
WHERE status = 'applied'
  AND applied_at IS NULL;

ALTER TABLE erpx_identity.directory_revocation_sla
    DROP CONSTRAINT IF EXISTS directory_revocation_sla_applied_at_check;
ALTER TABLE erpx_identity.directory_revocation_sla
    ADD CONSTRAINT directory_revocation_sla_applied_at_check
    CHECK (status <> 'applied' OR applied_at IS NOT NULL);

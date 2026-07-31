-- Bind support SLA delivery acknowledgements to the exact worker lease.
-- Existing pending/delivered rows remain valid; newly claimed rows carry both
-- the worker identity and a unique durable claim token.
ALTER TABLE erpx_control.support_sla_notifications
    ADD COLUMN IF NOT EXISTS claimed_by UUID,
    ADD COLUMN IF NOT EXISTS claim_token UUID;

CREATE INDEX IF NOT EXISTS support_sla_notifications_claim_idx
    ON erpx_control.support_sla_notifications (notification_key, claimed_by, claim_token);

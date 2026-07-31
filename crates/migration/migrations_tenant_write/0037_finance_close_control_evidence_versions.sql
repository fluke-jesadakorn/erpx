-- Ordered source publication for close-control evidence. Existing rows are
-- the first observed source version; later source events must advance it.
ALTER TABLE erpx_tenant.finance_close_control_evidence
    ADD COLUMN IF NOT EXISTS source_version BIGINT NOT NULL DEFAULT 1,
    ADD CONSTRAINT finance_close_control_evidence_source_version_check
        CHECK (source_version > 0);

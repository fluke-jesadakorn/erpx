ALTER TABLE erpx_identity.directory_administration_audit
    ADD COLUMN IF NOT EXISTS decision_session_id UUID,
    ADD COLUMN IF NOT EXISTS decision_authority_scope JSONB,
    ADD COLUMN IF NOT EXISTS decision_ref TEXT;

ALTER TABLE erpx_identity.directory_administration_audit
    ADD CONSTRAINT directory_administration_audit_decision_evidence_ck
    CHECK (
        (decision_session_id IS NULL AND decision_authority_scope IS NULL AND decision_ref IS NULL)
        OR
        (decision_session_id IS NOT NULL AND decision_authority_scope IS NOT NULL AND decision_ref IS NOT NULL)
    ) NOT VALID;

ALTER TABLE erpx_identity.directory_administration_audit
    VALIDATE CONSTRAINT directory_administration_audit_decision_evidence_ck;

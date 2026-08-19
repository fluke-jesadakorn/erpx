-- Development persona contexts are available only behind the complete local
-- development gate. Keep production holding-administration contexts at their
-- existing 30-minute lifetime while allowing a selected local QA persona to
-- remain active for up to 30 days.

ALTER TABLE erpx_identity.development_persona_contexts
    DROP CONSTRAINT IF EXISTS development_persona_contexts_check1,
    DROP CONSTRAINT IF EXISTS development_persona_contexts_expiry_window_check;

ALTER TABLE erpx_identity.development_persona_contexts
    ADD CONSTRAINT development_persona_contexts_expiry_window_check
    CHECK (expires_at <= issued_at + INTERVAL '30 days');

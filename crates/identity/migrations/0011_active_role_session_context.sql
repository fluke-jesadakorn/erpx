-- One exact, server-validated authorization role is bound to each session.
-- Existing sessions deliberately remain unselected and may use only auth APIs
-- until the user selects a currently effective role.
ALTER TABLE erpx_identity.sessions
    ADD COLUMN IF NOT EXISTS active_role_source TEXT;

ALTER TABLE erpx_identity.sessions
    ADD COLUMN IF NOT EXISTS active_role_selection_id TEXT;

ALTER TABLE erpx_identity.sessions
    ADD COLUMN IF NOT EXISTS authorization_context_version BIGINT NOT NULL DEFAULT 1;

ALTER TABLE erpx_identity.sessions
    DROP CONSTRAINT IF EXISTS sessions_active_role_source_check;
ALTER TABLE erpx_identity.sessions
    ADD CONSTRAINT sessions_active_role_source_check
    CHECK (active_role_source IS NULL OR active_role_source IN ('local', 'directory'));

ALTER TABLE erpx_identity.sessions
    DROP CONSTRAINT IF EXISTS sessions_active_role_pair_check;
ALTER TABLE erpx_identity.sessions
    ADD CONSTRAINT sessions_active_role_pair_check
    CHECK ((active_role_source IS NULL) = (active_role_selection_id IS NULL));

ALTER TABLE erpx_identity.sessions
    DROP CONSTRAINT IF EXISTS sessions_authorization_context_version_check;
ALTER TABLE erpx_identity.sessions
    ADD CONSTRAINT sessions_authorization_context_version_check
    CHECK (authorization_context_version > 0);

CREATE INDEX IF NOT EXISTS sessions_active_role_idx
    ON erpx_identity.sessions
       (principal_id, active_role_source, active_role_selection_id)
    WHERE revoked_at IS NULL AND active_role_selection_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS erpx_identity.session_authorization_audit (
    event_id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES erpx_identity.sessions(id),
    principal_ref TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK (event_type IN ('role_selected', 'role_changed', 'role_invalidated')),
    role_source TEXT NOT NULL CHECK (role_source IN ('local', 'directory')),
    role_selection_id TEXT NOT NULL,
    authorization_context_version BIGINT NOT NULL CHECK (authorization_context_version > 0),
    occurred_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS session_authorization_audit_session_idx
    ON erpx_identity.session_authorization_audit (session_id, occurred_at DESC);

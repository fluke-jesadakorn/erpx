ALTER TABLE erpx_web.session_accounts
    ADD COLUMN active_role_source TEXT,
    ADD COLUMN active_role_selection_id TEXT,
    ADD COLUMN active_role_key TEXT,
    ADD COLUMN active_scope_key TEXT,
    ADD COLUMN authorization_context_version BIGINT;

ALTER TABLE erpx_web.session_accounts
    ADD CONSTRAINT web_session_role_pair_check CHECK (
        (active_role_source IS NULL AND active_role_selection_id IS NULL)
        OR (
            active_role_source IN ('local', 'directory')
            AND length(active_role_selection_id) BETWEEN 1 AND 256
        )
    ),
    ADD CONSTRAINT web_session_role_labels_pair_check CHECK (
        (active_role_key IS NULL AND active_scope_key IS NULL)
        OR (
            length(active_role_key) BETWEEN 1 AND 256
            AND length(active_scope_key) BETWEEN 1 AND 256
        )
    ),
    ADD CONSTRAINT web_session_auth_context_version_check CHECK (
        authorization_context_version IS NULL OR authorization_context_version > 0
    );

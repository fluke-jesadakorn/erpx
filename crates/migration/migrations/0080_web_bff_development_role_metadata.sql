ALTER TABLE erpx_web.session_accounts
    ADD COLUMN development_role_source TEXT,
    ADD COLUMN development_role_selection_id TEXT,
    ADD COLUMN development_role_key TEXT,
    ADD COLUMN development_scope_key TEXT;

ALTER TABLE erpx_web.session_accounts
    ADD CONSTRAINT web_session_development_role_pair_check CHECK (
        (development_role_source IS NULL AND development_role_selection_id IS NULL)
        OR (
            development_role_source IN ('local', 'directory')
            AND length(development_role_selection_id) BETWEEN 1 AND 256
        )
    ),
    ADD CONSTRAINT web_session_development_role_labels_pair_check CHECK (
        (development_role_key IS NULL AND development_scope_key IS NULL)
        OR (
            length(development_role_key) BETWEEN 1 AND 256
            AND length(development_scope_key) BETWEEN 1 AND 256
        )
    ),
    ADD CONSTRAINT web_session_development_role_requires_context_check CHECK (
        development_role_source IS NULL
        OR development_context_nonce IS NOT NULL
    );

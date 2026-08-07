ALTER TABLE erpx_web.session_accounts
    ADD COLUMN development_context_nonce BYTEA,
    ADD COLUMN development_context_ciphertext BYTEA;

ALTER TABLE erpx_web.session_accounts
    ADD CONSTRAINT web_session_development_context_pair_check CHECK (
        (development_context_nonce IS NULL AND development_context_ciphertext IS NULL)
        OR (
            octet_length(development_context_nonce) = 12
            AND octet_length(development_context_ciphertext) > 16
        )
    );

CREATE SCHEMA IF NOT EXISTS erpx_web;

CREATE TABLE erpx_web.session_bundles (
    id UUID PRIMARY KEY,
    selector_digest BYTEA NOT NULL UNIQUE CHECK (octet_length(selector_digest) = 32),
    active_session_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL CHECK (updated_at >= created_at),
    expires_at TIMESTAMPTZ NOT NULL CHECK (expires_at > updated_at)
);

CREATE INDEX web_session_bundles_expiry_idx
    ON erpx_web.session_bundles (expires_at);

CREATE TABLE erpx_web.session_accounts (
    bundle_id UUID NOT NULL REFERENCES erpx_web.session_bundles(id) ON DELETE CASCADE,
    session_id UUID NOT NULL,
    principal_ref TEXT NOT NULL CHECK (
        principal_ref = btrim(principal_ref)
        AND length(principal_ref) BETWEEN 1 AND 256
    ),
    key_reference TEXT NOT NULL CHECK (length(key_reference) BETWEEN 1 AND 128),
    access_nonce BYTEA NOT NULL CHECK (octet_length(access_nonce) = 12),
    access_ciphertext BYTEA NOT NULL CHECK (octet_length(access_ciphertext) > 16),
    refresh_nonce BYTEA NOT NULL CHECK (octet_length(refresh_nonce) = 12),
    refresh_ciphertext BYTEA NOT NULL CHECK (octet_length(refresh_ciphertext) > 16),
    access_expires_at TIMESTAMPTZ NOT NULL,
    refresh_expires_at TIMESTAMPTZ NOT NULL CHECK (refresh_expires_at > access_expires_at),
    last_used_at TIMESTAMPTZ NOT NULL CHECK (last_used_at <= refresh_expires_at),
    mru_position SMALLINT NOT NULL CHECK (mru_position BETWEEN 0 AND 4),
    PRIMARY KEY (bundle_id, session_id),
    UNIQUE (bundle_id, principal_ref),
    UNIQUE (bundle_id, mru_position)
);

ALTER TABLE erpx_web.session_bundles
    ADD CONSTRAINT web_session_active_account_fk
    FOREIGN KEY (id, active_session_id)
    REFERENCES erpx_web.session_accounts(bundle_id, session_id)
    DEFERRABLE INITIALLY DEFERRED;

REVOKE ALL ON SCHEMA erpx_web FROM PUBLIC;
REVOKE ALL ON ALL TABLES IN SCHEMA erpx_web FROM PUBLIC;

-- Serialize first-use bootstrap when multiple identity workers start together.
-- The lock is transaction-scoped and does not affect normal identity queries.
SELECT pg_advisory_xact_lock(hashtextextended('erpx_identity_schema_v1', 0));

CREATE SCHEMA IF NOT EXISTS erpx_identity;

CREATE TABLE IF NOT EXISTS erpx_identity.principals (
    id UUID PRIMARY KEY,
    principal_ref TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL CHECK (status IN ('active', 'suspended', 'recovery_required')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS erpx_identity.password_credentials (
    principal_id UUID PRIMARY KEY REFERENCES erpx_identity.principals(id),
    argon2id_digest TEXT NOT NULL,
    changed_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS erpx_identity.recovery_codes (
    id UUID PRIMARY KEY,
    principal_id UUID NOT NULL REFERENCES erpx_identity.principals(id),
    code_hash TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS recovery_codes_principal_idx
    ON erpx_identity.recovery_codes (principal_id, consumed_at);

CREATE TABLE IF NOT EXISTS erpx_identity.totp_secrets (
    principal_id UUID PRIMARY KEY REFERENCES erpx_identity.principals(id),
    key_reference TEXT NOT NULL,
    nonce BYTEA NOT NULL,
    ciphertext BYTEA NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS erpx_identity.devices (
    id UUID PRIMARY KEY,
    principal_id UUID NOT NULL REFERENCES erpx_identity.principals(id),
    label TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS devices_principal_idx
    ON erpx_identity.devices (principal_id, revoked_at, created_at);

CREATE TABLE IF NOT EXISTS erpx_identity.sessions (
    id UUID PRIMARY KEY,
    principal_id UUID NOT NULL REFERENCES erpx_identity.principals(id),
    device_id UUID NOT NULL REFERENCES erpx_identity.devices(id),
    refresh_token_hash BYTEA NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    refresh_expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS sessions_principal_idx
    ON erpx_identity.sessions (principal_id, revoked_at, refresh_expires_at);

CREATE TABLE IF NOT EXISTS erpx_identity.passkeys (
    id UUID PRIMARY KEY,
    principal_id UUID NOT NULL REFERENCES erpx_identity.principals(id),
    credential_id BYTEA NOT NULL UNIQUE,
    credential_id_hash BYTEA NOT NULL CHECK (octet_length(credential_id_hash) = 32),
    public_key BYTEA NOT NULL,
    rp_id TEXT NOT NULL,
    sign_count INTEGER NOT NULL CHECK (sign_count >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ
);

-- Existing installations may have the original passkeys shape. These
-- additive columns remain nullable only for legacy rows that cannot be
-- trusted until explicitly re-registered with RP and digest bindings.
ALTER TABLE erpx_identity.passkeys
    ADD COLUMN IF NOT EXISTS credential_id_hash BYTEA;

ALTER TABLE erpx_identity.passkeys
    ADD COLUMN IF NOT EXISTS rp_id TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS passkeys_credential_id_hash_idx
    ON erpx_identity.passkeys (credential_id_hash)
    WHERE credential_id_hash IS NOT NULL;

CREATE TABLE IF NOT EXISTS erpx_identity.webauthn_challenges (
    id UUID PRIMARY KEY,
    principal_id UUID NOT NULL REFERENCES erpx_identity.principals(id),
    ceremony TEXT NOT NULL CHECK (ceremony IN ('registration', 'assertion')),
    challenge_digest BYTEA NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ
);

ALTER TABLE erpx_identity.webauthn_challenges
    ADD COLUMN IF NOT EXISTS provider_state BYTEA;

CREATE INDEX IF NOT EXISTS webauthn_challenges_expiry_idx
    ON erpx_identity.webauthn_challenges (principal_id, expires_at, consumed_at);

CREATE TABLE IF NOT EXISTS erpx_identity.auth_rate_limits (
    bucket_digest BYTEA PRIMARY KEY CHECK (octet_length(bucket_digest) = 32),
    window_started_at TIMESTAMPTZ NOT NULL,
    attempts INTEGER NOT NULL CHECK (attempts >= 0),
    blocked_until TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS auth_rate_limits_updated_idx
    ON erpx_identity.auth_rate_limits (updated_at);

-- TOTP enrollment material is encrypted before it reaches this write-side
-- schema. Pending enrollment rows are short-lived and are never exposed to a
-- read database; the final secret has a replay watermark for one-time codes.
ALTER TABLE erpx_identity.totp_secrets
    ADD COLUMN IF NOT EXISTS algorithm TEXT NOT NULL DEFAULT 'SHA256';

ALTER TABLE erpx_identity.totp_secrets
    ADD COLUMN IF NOT EXISTS digits SMALLINT NOT NULL DEFAULT 6;

ALTER TABLE erpx_identity.totp_secrets
    ADD COLUMN IF NOT EXISTS period_seconds INTEGER NOT NULL DEFAULT 30;

ALTER TABLE erpx_identity.totp_secrets
    ADD COLUMN IF NOT EXISTS enabled_at TIMESTAMPTZ NOT NULL DEFAULT now();

ALTER TABLE erpx_identity.totp_secrets
    ADD COLUMN IF NOT EXISTS last_verified_step BIGINT;

CREATE TABLE IF NOT EXISTS erpx_identity.totp_enrollments (
    id UUID PRIMARY KEY,
    principal_id UUID NOT NULL REFERENCES erpx_identity.principals(id),
    key_reference TEXT NOT NULL,
    nonce BYTEA NOT NULL CHECK (octet_length(nonce) = 12),
    ciphertext BYTEA NOT NULL CHECK (octet_length(ciphertext) > 16),
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    verified_at TIMESTAMPTZ,
    CHECK (expires_at > issued_at)
);

CREATE INDEX IF NOT EXISTS totp_enrollments_principal_idx
    ON erpx_identity.totp_enrollments (principal_id, expires_at, verified_at);

-- Recovery handoff tokens are opaque, short-lived, and one-time. Only the
-- digest is persisted; the bearer token is returned once by the HTTP route.
CREATE TABLE IF NOT EXISTS erpx_identity.recovery_sessions (
    id UUID PRIMARY KEY,
    principal_id UUID NOT NULL REFERENCES erpx_identity.principals(id),
    token_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    CHECK (expires_at > issued_at)
);

CREATE INDEX IF NOT EXISTS recovery_sessions_principal_idx
    ON erpx_identity.recovery_sessions (principal_id, expires_at, consumed_at);

-- Directory identity is write-side authoritative.  The connector stores only
-- a credential-manager reference; secret material and raw directory payloads
-- are never copied into ERPX control/read projections.
CREATE TABLE IF NOT EXISTS erpx_identity.directory_connectors (
    id UUID PRIMARY KEY,
    provider TEXT NOT NULL CHECK (provider IN ('generic_ldap', 'active_directory_domain_services', 'univention_corporate_server')),
    vendor TEXT NOT NULL DEFAULT 'generic_ldap' CHECK (vendor IN ('generic_ldap', 'active_directory_domain_services', 'univention_corporate_server')),
    scope_key TEXT NOT NULL,
    scope_kind TEXT NOT NULL DEFAULT 'platform' CHECK (scope_kind IN ('platform', 'holding')),
    scope_id UUID,
    holding_ref TEXT,
    endpoint TEXT NOT NULL CHECK (endpoint LIKE 'ldaps://%' OR endpoint LIKE 'ldap://%' OR endpoint LIKE 'ldap+starttls://%' OR endpoint LIKE 'https://%'),
    base_dn TEXT NOT NULL,
    tls_mode TEXT NOT NULL CHECK (tls_mode IN ('ldaps', 'start_tls', 'https')),
    security TEXT NOT NULL DEFAULT 'ldaps' CHECK (security IN ('ldaps', 'start_tls', 'https')),
    page_size INTEGER NOT NULL DEFAULT 500 CHECK (page_size > 0 AND page_size <= 5000),
    max_in_flight_pages INTEGER NOT NULL DEFAULT 4 CHECK (max_in_flight_pages > 0 AND max_in_flight_pages <= 64),
    ca_certificate_sha256 TEXT NOT NULL CHECK (length(ca_certificate_sha256) = 64),
    server_name TEXT NOT NULL,
    minimum_tls_version TEXT NOT NULL CHECK (minimum_tls_version IN ('1.2', '1.3')),
    feed_kind TEXT NOT NULL CHECK (feed_kind IN ('polling', 'ad_dirsync', 'rfc4533', 'ucs_provisioning_service')),
    credential_ref TEXT NOT NULL,
    enabled BOOLEAN NOT NULL,
    max_import_users BIGINT NOT NULL CHECK (max_import_users > 0 AND max_import_users <= 1000000),
    revision BIGINT NOT NULL CHECK (revision > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS directory_connectors_scope_idx
    ON erpx_identity.directory_connectors (scope_key, provider);

-- Forward-compatible columns for the native full-import/change-feed adapter.
ALTER TABLE erpx_identity.directory_connectors
    ADD COLUMN IF NOT EXISTS vendor TEXT NOT NULL DEFAULT 'generic_ldap';
ALTER TABLE erpx_identity.directory_connectors
    ADD COLUMN IF NOT EXISTS scope_kind TEXT NOT NULL DEFAULT 'platform';
ALTER TABLE erpx_identity.directory_connectors
    ADD COLUMN IF NOT EXISTS scope_id UUID;
ALTER TABLE erpx_identity.directory_connectors
    ADD COLUMN IF NOT EXISTS security TEXT NOT NULL DEFAULT 'ldaps';
ALTER TABLE erpx_identity.directory_connectors
    ADD COLUMN IF NOT EXISTS page_size INTEGER NOT NULL DEFAULT 500;
ALTER TABLE erpx_identity.directory_connectors
    ADD COLUMN IF NOT EXISTS max_in_flight_pages INTEGER NOT NULL DEFAULT 4;
ALTER TABLE erpx_identity.directory_connectors
    DROP CONSTRAINT IF EXISTS directory_connectors_endpoint_check;
ALTER TABLE erpx_identity.directory_connectors
    ADD CONSTRAINT directory_connectors_endpoint_check
    CHECK (endpoint LIKE 'ldaps://%' OR endpoint LIKE 'ldap://%' OR endpoint LIKE 'ldap+starttls://%' OR endpoint LIKE 'https://%');
ALTER TABLE erpx_identity.directory_connectors
    DROP CONSTRAINT IF EXISTS directory_connectors_tls_mode_check;
ALTER TABLE erpx_identity.directory_connectors
    ADD CONSTRAINT directory_connectors_tls_mode_check
    CHECK (tls_mode IN ('ldaps', 'start_tls', 'https'));

CREATE TABLE IF NOT EXISTS erpx_identity.directory_sync_checkpoints (
    connector_id UUID PRIMARY KEY REFERENCES erpx_identity.directory_connectors(id),
    feed_kind TEXT NOT NULL,
    sequence BIGINT NOT NULL CHECK (sequence >= 0),
    opaque_cookie TEXT,
    last_success_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('healthy', 'stale', 'failed')),
    last_error_redacted TEXT
);

CREATE TABLE IF NOT EXISTS erpx_identity.directory_identity_links (
    connector_id UUID NOT NULL REFERENCES erpx_identity.directory_connectors(id),
    external_object_id TEXT NOT NULL,
    identity_link TEXT UNIQUE,
    principal_ref TEXT,
    active BOOLEAN NOT NULL DEFAULT true,
    source_revision BIGINT NOT NULL DEFAULT 0 CHECK (source_revision >= 0),
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ,
    PRIMARY KEY (connector_id, external_object_id)
);

CREATE INDEX IF NOT EXISTS directory_identity_links_active_idx
    ON erpx_identity.directory_identity_links (principal_ref, active, observed_at);

CREATE TABLE IF NOT EXISTS erpx_identity.directory_role_mappings (
    connector_id UUID NOT NULL REFERENCES erpx_identity.directory_connectors(id),
    group_ref TEXT NOT NULL,
    role_ref TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    requires_mfa BOOLEAN NOT NULL,
    active BOOLEAN NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (connector_id, group_ref, role_ref, scope_key)
);

CREATE TABLE IF NOT EXISTS erpx_identity.directory_role_facts (
    id UUID PRIMARY KEY,
    connector_id UUID NOT NULL REFERENCES erpx_identity.directory_connectors(id),
    identity_link TEXT NOT NULL,
    role_ref TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('grant', 'revoke')),
    sequence BIGINT NOT NULL CHECK (sequence >= 0),
    previous_hash TEXT,
    fact_hash TEXT NOT NULL UNIQUE,
    occurred_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS directory_role_facts_identity_idx
    ON erpx_identity.directory_role_facts (identity_link, sequence, occurred_at);

CREATE TABLE IF NOT EXISTS erpx_identity.directory_identity_projection (
    identity_link TEXT PRIMARY KEY,
    connector_id UUID NOT NULL REFERENCES erpx_identity.directory_connectors(id),
    principal_ref TEXT,
    active BOOLEAN NOT NULL,
    groups_json JSONB NOT NULL,
    source_revision BIGINT NOT NULL CHECK (source_revision >= 0),
    last_feed_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS directory_identity_projection_principal_idx
    ON erpx_identity.directory_identity_projection (principal_ref, active, last_feed_at);

CREATE TABLE IF NOT EXISTS erpx_identity.directory_outbox (
    event_id UUID PRIMARY KEY,
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL,
    payload_json JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ,
    delivery_attempts INTEGER NOT NULL DEFAULT 0 CHECK (delivery_attempts >= 0)
);

CREATE INDEX IF NOT EXISTS directory_outbox_pending_idx
    ON erpx_identity.directory_outbox (published_at, occurred_at);

CREATE TABLE IF NOT EXISTS erpx_identity.directory_import_jobs (
    id UUID PRIMARY KEY,
    connector_id UUID NOT NULL REFERENCES erpx_identity.directory_connectors(id),
    total_users BIGINT NOT NULL CHECK (total_users > 0 AND total_users <= 1000000),
    page_size INTEGER NOT NULL CHECK (page_size > 0 AND page_size <= 10000),
    page_count BIGINT NOT NULL CHECK (page_count > 0),
    state TEXT NOT NULL CHECK (state IN ('queued', 'running', 'completed', 'failed', 'rolled_back')),
    next_page BIGINT NOT NULL CHECK (next_page >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    error_redacted TEXT
);

CREATE INDEX IF NOT EXISTS directory_import_jobs_claim_idx
    ON erpx_identity.directory_import_jobs (state, created_at, connector_id);

-- Native-feed/full-import persistence used by the directory synchronization
-- runner. These rows are write-side only; the shared control outbox remains
-- the durable publication boundary.
CREATE TABLE IF NOT EXISTS erpx_identity.directory_imports (
    id UUID PRIMARY KEY,
    connector_id UUID NOT NULL REFERENCES erpx_identity.directory_connectors(id),
    scope_kind TEXT NOT NULL,
    scope_id UUID,
    state TEXT NOT NULL CHECK (state IN ('running', 'finalizing', 'completed', 'failed', 'rolled_back')),
    page_size INTEGER NOT NULL CHECK (page_size > 0 AND page_size <= 5000),
    max_users BIGINT NOT NULL CHECK (max_users > 0 AND max_users <= 1000000),
    cursor JSONB NOT NULL,
    users_seen BIGINT NOT NULL CHECK (users_seen >= 0),
    pages_applied BIGINT NOT NULL CHECK (pages_applied >= 0),
    last_external_id TEXT,
    snapshot_digest BYTEA NOT NULL CHECK (octet_length(snapshot_digest) = 32),
    started_at TIMESTAMPTZ NOT NULL,
    finalized_at TIMESTAMPTZ,
    failure_reason TEXT,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS directory_imports_connector_state_idx
    ON erpx_identity.directory_imports (connector_id, state, updated_at);

CREATE TABLE IF NOT EXISTS erpx_identity.directory_import_pages (
    import_id UUID NOT NULL REFERENCES erpx_identity.directory_imports(id),
    idempotency_key TEXT NOT NULL,
    page_digest BYTEA NOT NULL CHECK (octet_length(page_digest) = 32),
    cursor JSONB NOT NULL,
    next_checkpoint JSONB NOT NULL,
    records_count INTEGER NOT NULL CHECK (records_count >= 0),
    applied_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (import_id, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_identity.directory_import_members (
    import_id UUID NOT NULL REFERENCES erpx_identity.directory_imports(id),
    identity_id UUID NOT NULL,
    connector_id UUID NOT NULL REFERENCES erpx_identity.directory_connectors(id),
    external_id TEXT NOT NULL,
    immutable_authority TEXT NOT NULL,
    immutable_value TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('active', 'disabled', 'deleted')),
    record_digest BYTEA NOT NULL CHECK (octet_length(record_digest) = 32),
    record JSONB NOT NULL,
    event_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (import_id, external_id)
);

CREATE INDEX IF NOT EXISTS directory_import_members_identity_idx
    ON erpx_identity.directory_import_members (connector_id, immutable_authority, immutable_value);

CREATE TABLE IF NOT EXISTS erpx_identity.directory_identities (
    id UUID PRIMARY KEY,
    connector_id UUID NOT NULL REFERENCES erpx_identity.directory_connectors(id),
    scope_kind TEXT,
    scope_id UUID,
    external_id TEXT NOT NULL,
    immutable_authority TEXT NOT NULL,
    immutable_value TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('active', 'disabled', 'deleted')),
    record JSONB NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    last_import_id UUID,
    updated_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE (connector_id, external_id)
);

CREATE UNIQUE INDEX IF NOT EXISTS directory_identities_immutable_idx
    ON erpx_identity.directory_identities (immutable_authority, immutable_value);

ALTER TABLE erpx_identity.directory_identity_links
    ADD COLUMN IF NOT EXISTS immutable_authority TEXT;
ALTER TABLE erpx_identity.directory_identity_links
    ADD COLUMN IF NOT EXISTS immutable_value TEXT;
ALTER TABLE erpx_identity.directory_identity_links
    ADD COLUMN IF NOT EXISTS identity_id UUID;
ALTER TABLE erpx_identity.directory_identity_links
    ADD COLUMN IF NOT EXISTS linked_at TIMESTAMPTZ;
ALTER TABLE erpx_identity.directory_identity_links
    ALTER COLUMN identity_link DROP NOT NULL;
ALTER TABLE erpx_identity.directory_identity_links
    ALTER COLUMN active SET DEFAULT true;
ALTER TABLE erpx_identity.directory_identity_links
    ALTER COLUMN source_revision SET DEFAULT 0;
ALTER TABLE erpx_identity.directory_identity_links
    ALTER COLUMN observed_at SET DEFAULT now();

CREATE UNIQUE INDEX IF NOT EXISTS directory_identity_links_immutable_idx
    ON erpx_identity.directory_identity_links (immutable_authority, immutable_value)
    WHERE immutable_authority IS NOT NULL AND immutable_value IS NOT NULL;

CREATE TABLE IF NOT EXISTS erpx_identity.directory_revocation_sla (
    event_id UUID PRIMARY KEY,
    connector_id UUID NOT NULL REFERENCES erpx_identity.directory_connectors(id),
    external_id TEXT NOT NULL,
    immutable_authority TEXT NOT NULL,
    immutable_value TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    deadline TIMESTAMPTZ NOT NULL,
    reason TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'applied', 'breached'))
);

ALTER TABLE erpx_identity.directory_revocation_sla
    ALTER COLUMN reason TYPE TEXT USING reason::text;

CREATE INDEX IF NOT EXISTS directory_revocation_sla_pending_idx
    ON erpx_identity.directory_revocation_sla (status, deadline, connector_id);

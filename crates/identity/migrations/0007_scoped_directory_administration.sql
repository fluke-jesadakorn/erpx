-- Holding/branch directory views, secret-profile references, verified fallback,
-- and two-person local-account supersession. Raw directory entries and secret
-- values remain outside these tables.

ALTER TABLE erpx_identity.directory_connectors
    ADD COLUMN IF NOT EXISTS company_id UUID,
    ADD COLUMN IF NOT EXISTS branch_id UUID,
    ADD COLUMN IF NOT EXISTS parent_connector_id UUID,
    ADD COLUMN IF NOT EXISTS directory_authority_fingerprint CHAR(64),
    ADD COLUMN IF NOT EXISTS credential_profile_key TEXT,
    ADD COLUMN IF NOT EXISTS fallback_to_holding BOOLEAN NOT NULL DEFAULT FALSE;

UPDATE erpx_identity.directory_connectors
SET credential_profile_key = 'legacy-' || replace(id::text, '-', '')
WHERE credential_profile_key IS NULL;

ALTER TABLE erpx_identity.directory_connectors
    ALTER COLUMN credential_profile_key SET NOT NULL,
    DROP CONSTRAINT IF EXISTS directory_connectors_scope_kind_check,
    DROP CONSTRAINT IF EXISTS directory_connectors_branch_scope_check,
    DROP CONSTRAINT IF EXISTS directory_connectors_authority_fingerprint_check,
    DROP CONSTRAINT IF EXISTS directory_connectors_credential_profile_check,
    DROP CONSTRAINT IF EXISTS directory_connectors_parent_fk;

ALTER TABLE erpx_identity.directory_connectors
    ADD CONSTRAINT directory_connectors_scope_kind_check
        CHECK (scope_kind IN ('platform', 'holding', 'branch')),
    ADD CONSTRAINT directory_connectors_branch_scope_check CHECK (
        (scope_kind = 'branch'
            AND data_profile_key IS NOT NULL
            AND scope_id IS NOT NULL
            AND company_id IS NOT NULL
            AND branch_id IS NOT NULL
            AND parent_connector_id IS NOT NULL)
        OR
        (scope_kind <> 'branch'
            AND company_id IS NULL
            AND branch_id IS NULL
            AND parent_connector_id IS NULL
            AND NOT fallback_to_holding)
    ),
    ADD CONSTRAINT directory_connectors_authority_fingerprint_check CHECK (
        directory_authority_fingerprint IS NULL
        OR directory_authority_fingerprint ~ '^[0-9A-Fa-f]{64}$'
    ),
    ADD CONSTRAINT directory_connectors_credential_profile_check CHECK (
        credential_profile_key ~ '^[a-z][a-z0-9_-]{0,63}$'
    ),
    ADD CONSTRAINT directory_connectors_parent_fk FOREIGN KEY (parent_connector_id)
        REFERENCES erpx_identity.directory_connectors(id) ON DELETE RESTRICT;

CREATE UNIQUE INDEX IF NOT EXISTS directory_connectors_active_holding_idx
    ON erpx_identity.directory_connectors (scope_key)
    WHERE scope_kind = 'holding' AND lifecycle = 'active';

CREATE UNIQUE INDEX IF NOT EXISTS directory_connectors_active_branch_idx
    ON erpx_identity.directory_connectors (data_profile_key, company_id, branch_id)
    WHERE scope_kind = 'branch' AND lifecycle = 'active';

CREATE TABLE IF NOT EXISTS erpx_identity.directory_connector_hostnames (
    hostname TEXT PRIMARY KEY,
    connector_id UUID NOT NULL
        REFERENCES erpx_identity.directory_connectors(id) ON DELETE RESTRICT,
    verified_by TEXT NOT NULL,
    verified_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    CHECK (hostname = lower(hostname)),
    CHECK (hostname ~ '^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?(\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)+$'),
    CHECK (length(btrim(verified_by)) BETWEEN 1 AND 255)
);

CREATE INDEX IF NOT EXISTS directory_connector_hostnames_connector_idx
    ON erpx_identity.directory_connector_hostnames (connector_id, verified_at DESC)
    WHERE revoked_at IS NULL;

-- Holding and branch views of the same authority intentionally share one
-- canonical identity_link. Connector/object remains the row identity.
ALTER TABLE erpx_identity.directory_identity_links
    DROP CONSTRAINT IF EXISTS directory_identity_links_identity_link_key;
DROP INDEX IF EXISTS erpx_identity.directory_identity_links_immutable_idx;
DROP INDEX IF EXISTS erpx_identity.directory_identities_immutable_idx;
CREATE INDEX IF NOT EXISTS directory_identity_links_canonical_idx
    ON erpx_identity.directory_identity_links (identity_link)
    WHERE identity_link IS NOT NULL;
CREATE INDEX IF NOT EXISTS directory_identity_links_authority_idx
    ON erpx_identity.directory_identity_links (immutable_authority, immutable_value)
    WHERE immutable_authority IS NOT NULL AND immutable_value IS NOT NULL;
CREATE INDEX IF NOT EXISTS directory_identities_authority_idx
    ON erpx_identity.directory_identities (immutable_authority, immutable_value);

CREATE TABLE IF NOT EXISTS erpx_identity.directory_fallback_audit (
    id UUID PRIMARY KEY,
    branch_connector_id UUID NOT NULL
        REFERENCES erpx_identity.directory_connectors(id) ON DELETE RESTRICT,
    holding_connector_id UUID NOT NULL
        REFERENCES erpx_identity.directory_connectors(id) ON DELETE RESTRICT,
    branch_id UUID NOT NULL,
    authority_fingerprint CHAR(64) NOT NULL
        CHECK (authority_fingerprint ~ '^[0-9A-Fa-f]{64}$'),
    outcome TEXT NOT NULL CHECK (outcome IN ('accepted', 'rejected')),
    failure_category TEXT,
    correlation_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (branch_connector_id <> holding_connector_id),
    CHECK ((outcome = 'accepted' AND failure_category IS NULL) OR outcome = 'rejected'),
    CHECK (failure_category IS NULL OR lower(failure_category)
        !~ '(password|secret|token|credential|ldap://|ldaps://)')
);

CREATE INDEX IF NOT EXISTS directory_fallback_audit_branch_idx
    ON erpx_identity.directory_fallback_audit (branch_id, occurred_at DESC, id DESC);

ALTER TABLE erpx_identity.directory_identity_conflicts
    ADD COLUMN IF NOT EXISTS match_reasons TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS proposed_by TEXT,
    ADD COLUMN IF NOT EXISTS proposed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS proposed_principal_ref TEXT,
    ADD COLUMN IF NOT EXISTS replacement_principal_ref TEXT,
    ADD COLUMN IF NOT EXISTS conflict_version BIGINT NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS correlation_id UUID;

ALTER TABLE erpx_identity.directory_identity_conflicts
    DROP CONSTRAINT IF EXISTS directory_identity_conflicts_version_check,
    DROP CONSTRAINT IF EXISTS directory_identity_conflicts_proposal_check,
    DROP CONSTRAINT IF EXISTS directory_identity_conflicts_conflict_kind_check;
ALTER TABLE erpx_identity.directory_identity_conflicts
    ADD CONSTRAINT directory_identity_conflicts_conflict_kind_check CHECK (
        conflict_kind IN (
            'immutable_identity', 'canonical_principal', 'duplicate_source',
            'local_identity_collision'
        )
    ),
    ADD CONSTRAINT directory_identity_conflicts_version_check
        CHECK (conflict_version > 0),
    ADD CONSTRAINT directory_identity_conflicts_proposal_check CHECK (
        (proposed_by IS NULL AND proposed_at IS NULL AND proposed_principal_ref IS NULL)
        OR
        (proposed_by IS NOT NULL AND proposed_at IS NOT NULL AND proposed_principal_ref IS NOT NULL)
    );

CREATE UNIQUE INDEX IF NOT EXISTS directory_identity_conflicts_open_identity_idx
    ON erpx_identity.directory_identity_conflicts
        (connector_id, external_object_id, principal_id, conflict_kind)
    WHERE state = 'unresolved';

CREATE TABLE IF NOT EXISTS erpx_identity.directory_identity_replacement_approvals (
    conflict_id UUID PRIMARY KEY
        REFERENCES erpx_identity.directory_identity_conflicts(id) ON DELETE RESTRICT,
    initiator_ref TEXT NOT NULL,
    approver_ref TEXT,
    state TEXT NOT NULL CHECK (state IN ('awaiting_approval', 'approved', 'rejected')),
    expected_conflict_version BIGINT NOT NULL CHECK (expected_conflict_version > 0),
    initiated_at TIMESTAMPTZ NOT NULL,
    approved_at TIMESTAMPTZ,
    rejected_at TIMESTAMPTZ,
    correlation_id UUID NOT NULL,
    CHECK (approver_ref IS NULL OR approver_ref <> initiator_ref),
    CHECK (
        (state = 'awaiting_approval' AND approver_ref IS NULL
            AND approved_at IS NULL AND rejected_at IS NULL)
        OR
        (state = 'approved' AND approver_ref IS NOT NULL
            AND approved_at IS NOT NULL AND rejected_at IS NULL)
        OR
        (state = 'rejected' AND approver_ref IS NOT NULL
            AND approved_at IS NULL AND rejected_at IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS erpx_identity.principal_supersession_aliases (
    local_principal_id UUID PRIMARY KEY
        REFERENCES erpx_identity.principals(id) ON DELETE RESTRICT,
    directory_principal_id UUID NOT NULL UNIQUE
        REFERENCES erpx_identity.principals(id) ON DELETE RESTRICT,
    conflict_id UUID NOT NULL UNIQUE
        REFERENCES erpx_identity.directory_identity_conflicts(id) ON DELETE RESTRICT,
    approved_by TEXT NOT NULL,
    superseded_at TIMESTAMPTZ NOT NULL,
    correlation_id UUID NOT NULL,
    CHECK (local_principal_id <> directory_principal_id)
);

CREATE TABLE IF NOT EXISTS erpx_identity.protected_local_principals (
    principal_id UUID PRIMARY KEY
        REFERENCES erpx_identity.principals(id) ON DELETE RESTRICT,
    protection_kind TEXT NOT NULL CHECK (protection_kind IN ('platform_root', 'customer_break_glass')),
    designated_by TEXT NOT NULL,
    designated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO erpx_identity.protected_local_principals
    (principal_id, protection_kind, designated_by)
SELECT id, 'platform_root', 'migration:0007'
FROM erpx_identity.principals
WHERE principal_ref = 'local:platform-root'
ON CONFLICT (principal_id) DO NOTHING;

CREATE OR REPLACE FUNCTION erpx_identity.reject_protected_principal_supersession()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM erpx_identity.protected_local_principals
        WHERE principal_id = NEW.local_principal_id
    ) THEN
        RAISE EXCEPTION 'protected local principal cannot be superseded';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS principal_supersession_reject_protected
    ON erpx_identity.principal_supersession_aliases;
CREATE TRIGGER principal_supersession_reject_protected
    BEFORE INSERT OR UPDATE OF local_principal_id
    ON erpx_identity.principal_supersession_aliases
    FOR EACH ROW
    EXECUTE FUNCTION erpx_identity.reject_protected_principal_supersession();

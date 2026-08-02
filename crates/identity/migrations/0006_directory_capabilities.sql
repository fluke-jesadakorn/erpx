-- Canonical multi-provider directory binding, connector lifecycle, and
-- supervisor coordination. Secret values and raw directory payloads never
-- enter these relations.

ALTER TABLE erpx_identity.directory_connectors
    ADD COLUMN IF NOT EXISTS lifecycle TEXT NOT NULL DEFAULT 'draft',
    ADD COLUMN IF NOT EXISTS user_filter TEXT NOT NULL DEFAULT '(objectClass=person)',
    ADD COLUMN IF NOT EXISTS attribute_mapping JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS ca_reference TEXT NOT NULL DEFAULT 'system-trust',
    ADD COLUMN IF NOT EXISTS feed_configuration JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS last_preflight_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_import_completed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_reconciled_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_heartbeat_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS failure_category TEXT;

UPDATE erpx_identity.directory_connectors
SET lifecycle = CASE WHEN enabled THEN 'active' ELSE 'draft' END
WHERE lifecycle = 'draft' AND enabled;

ALTER TABLE erpx_identity.directory_connectors
    DROP CONSTRAINT IF EXISTS directory_connectors_lifecycle_check,
    DROP CONSTRAINT IF EXISTS directory_connectors_attribute_mapping_object_check,
    DROP CONSTRAINT IF EXISTS directory_connectors_feed_configuration_object_check,
    DROP CONSTRAINT IF EXISTS directory_connectors_activation_shape_check;
ALTER TABLE erpx_identity.directory_connectors
    ADD CONSTRAINT directory_connectors_lifecycle_check CHECK (
        lifecycle IN ('draft', 'verified', 'importing', 'ready', 'active', 'suspended', 'failed')
    ),
    ADD CONSTRAINT directory_connectors_attribute_mapping_object_check CHECK (
        jsonb_typeof(attribute_mapping) = 'object'
    ),
    ADD CONSTRAINT directory_connectors_feed_configuration_object_check CHECK (
        jsonb_typeof(feed_configuration) = 'object'
    ),
    ADD CONSTRAINT directory_connectors_activation_shape_check CHECK (
        lifecycle <> 'active' OR enabled
    );

-- A directory source identity may be joined only by an explicit
-- administrator operation to a pre-existing canonical ERPX principal. The
-- principal reference copied by feed projections is intentionally not the
-- binding authority.
CREATE TABLE IF NOT EXISTS erpx_identity.directory_principal_bindings (
    id UUID PRIMARY KEY,
    connector_id UUID NOT NULL,
    external_object_id TEXT NOT NULL,
    principal_id UUID NOT NULL REFERENCES erpx_identity.principals(id),
    active BOOLEAN NOT NULL DEFAULT true,
    revision BIGINT NOT NULL CHECK (revision > 0),
    linked_by TEXT NOT NULL,
    linked_at TIMESTAMPTZ NOT NULL,
    unlinked_by TEXT,
    unlinked_at TIMESTAMPTZ,
    correlation_id UUID NOT NULL,
    FOREIGN KEY (connector_id, external_object_id)
        REFERENCES erpx_identity.directory_identity_links(connector_id, external_object_id),
    UNIQUE (connector_id, external_object_id),
    CHECK (
        (active AND unlinked_by IS NULL AND unlinked_at IS NULL)
        OR (NOT active AND unlinked_by IS NOT NULL AND unlinked_at IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS directory_principal_bindings_principal_idx
    ON erpx_identity.directory_principal_bindings (principal_id, active, connector_id);

CREATE OR REPLACE FUNCTION erpx_identity.reject_platform_root_directory_binding()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM erpx_identity.principals
        WHERE id = NEW.principal_id AND principal_ref = 'local:platform-root'
    ) THEN
        RAISE EXCEPTION 'Platform Root cannot be joined to a directory';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS directory_principal_bindings_reject_platform_root
    ON erpx_identity.directory_principal_bindings;
CREATE TRIGGER directory_principal_bindings_reject_platform_root
    BEFORE INSERT OR UPDATE OF principal_id, active
    ON erpx_identity.directory_principal_bindings
    FOR EACH ROW
    WHEN (NEW.active)
    EXECUTE FUNCTION erpx_identity.reject_platform_root_directory_binding();

CREATE TABLE IF NOT EXISTS erpx_identity.directory_identity_conflicts (
    id UUID PRIMARY KEY,
    connector_id UUID NOT NULL REFERENCES erpx_identity.directory_connectors(id),
    external_object_id TEXT NOT NULL,
    principal_id UUID REFERENCES erpx_identity.principals(id),
    conflict_kind TEXT NOT NULL CHECK (
        conflict_kind IN ('immutable_identity', 'canonical_principal', 'duplicate_source')
    ),
    state TEXT NOT NULL CHECK (state IN ('unresolved', 'resolved')),
    detected_at TIMESTAMPTZ NOT NULL,
    resolved_at TIMESTAMPTZ,
    resolution_ref TEXT,
    CHECK (
        (state = 'unresolved' AND resolved_at IS NULL AND resolution_ref IS NULL)
        OR (state = 'resolved' AND resolved_at IS NOT NULL AND resolution_ref IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS directory_identity_conflicts_open_idx
    ON erpx_identity.directory_identity_conflicts (connector_id, state, detected_at)
    WHERE state = 'unresolved';

CREATE TABLE IF NOT EXISTS erpx_identity.directory_connector_probes (
    id UUID PRIMARY KEY,
    connector_id UUID NOT NULL REFERENCES erpx_identity.directory_connectors(id),
    probe_kind TEXT NOT NULL CHECK (probe_kind IN ('tls', 'bind', 'feed')),
    succeeded BOOLEAN NOT NULL,
    failure_category TEXT,
    observed_at TIMESTAMPTZ NOT NULL,
    correlation_id UUID NOT NULL,
    CHECK ((succeeded AND failure_category IS NULL) OR NOT succeeded)
);

CREATE INDEX IF NOT EXISTS directory_connector_probes_recent_idx
    ON erpx_identity.directory_connector_probes (connector_id, observed_at DESC);

CREATE TABLE IF NOT EXISTS erpx_identity.directory_supervisor_leases (
    connector_id UUID PRIMARY KEY REFERENCES erpx_identity.directory_connectors(id),
    owner_ref UUID NOT NULL,
    lease_expires_at TIMESTAMPTZ NOT NULL,
    heartbeat_at TIMESTAMPTZ NOT NULL,
    consecutive_failures INTEGER NOT NULL DEFAULT 0 CHECK (consecutive_failures >= 0),
    next_attempt_at TIMESTAMPTZ NOT NULL,
    last_error_category TEXT,
    CHECK (lease_expires_at > heartbeat_at)
);

CREATE INDEX IF NOT EXISTS directory_supervisor_leases_claim_idx
    ON erpx_identity.directory_supervisor_leases (next_attempt_at, lease_expires_at);

CREATE TABLE IF NOT EXISTS erpx_identity.directory_administration_audit (
    id UUID PRIMARY KEY,
    connector_id UUID REFERENCES erpx_identity.directory_connectors(id),
    principal_id UUID REFERENCES erpx_identity.principals(id),
    action TEXT NOT NULL,
    actor_ref TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('accepted', 'rejected')),
    failure_category TEXT,
    occurred_at TIMESTAMPTZ NOT NULL,
    CHECK ((outcome = 'accepted' AND failure_category IS NULL) OR outcome = 'rejected')
);

CREATE INDEX IF NOT EXISTS directory_administration_audit_connector_idx
    ON erpx_identity.directory_administration_audit (connector_id, occurred_at DESC);

-- Disposable, typed authority evidence used only to calculate finite Item
-- Directory grants. Physical candidate databases provide the rebuild fence.

CREATE SCHEMA IF NOT EXISTS erpx_authority_read;

CREATE OR REPLACE FUNCTION erpx_authority_read.current_tenant_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.tenant_id', true), '')::uuid
$$;

CREATE TABLE erpx_authority_read.principals (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_read.current_tenant_id(),
    principal_id UUID NOT NULL,
    principal_kind TEXT NOT NULL CHECK (
        principal_kind IN ('human','service','agent','human_group','agent_pool')
    ),
    realm TEXT NOT NULL CHECK (
        realm IN ('internal_workforce','platform_workforce','customer_portal')
    ),
    principal_scope JSONB NOT NULL,
    active BOOLEAN NOT NULL,
    principal_version BIGINT NOT NULL CHECK (principal_version > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    changed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, principal_id)
);

CREATE TABLE erpx_authority_read.roles (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_read.current_tenant_id(),
    role_id UUID NOT NULL,
    role_scope JSONB NOT NULL,
    role_version BIGINT NOT NULL CHECK (role_version > 0),
    permissions TEXT[] NOT NULL CHECK (cardinality(permissions) BETWEEN 1 AND 256),
    classification_ceiling TEXT NOT NULL CHECK (
        classification_ceiling IN ('public','internal','confidential','restricted')
    ),
    active BOOLEAN NOT NULL,
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    changed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, role_id)
);

CREATE TABLE erpx_authority_read.assignments (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_read.current_tenant_id(),
    assignment_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    role_id UUID NOT NULL,
    assignment_scope JSONB NOT NULL,
    assignment_version BIGINT NOT NULL CHECK (assignment_version > 0),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    changed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, assignment_id),
    FOREIGN KEY (tenant_id, principal_id)
        REFERENCES erpx_authority_read.principals(tenant_id, principal_id)
        ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, role_id)
        REFERENCES erpx_authority_read.roles(tenant_id, role_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (valid_until > valid_from)
);

CREATE TABLE erpx_authority_read.group_memberships (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_read.current_tenant_id(),
    group_id UUID NOT NULL,
    member_principal_id UUID NOT NULL,
    group_kind TEXT NOT NULL CHECK (group_kind IN ('human_group','agent_pool')),
    group_version BIGINT NOT NULL CHECK (group_version > 0),
    membership_version BIGINT NOT NULL CHECK (membership_version > 0),
    membership_scope JSONB NOT NULL,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    changed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, group_id, member_principal_id),
    FOREIGN KEY (tenant_id, group_id)
        REFERENCES erpx_authority_read.principals(tenant_id, principal_id)
        ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, member_principal_id)
        REFERENCES erpx_authority_read.principals(tenant_id, principal_id)
        ON DELETE CASCADE,
    CHECK (valid_until > valid_from)
);

CREATE TABLE erpx_authority_read.organization_sharing_agreements (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_read.current_tenant_id(),
    sharing_agreement_id UUID NOT NULL,
    agreement_version BIGINT NOT NULL CHECK (agreement_version > 0),
    source_scope JSONB NOT NULL,
    target_scope JSONB NOT NULL,
    direction TEXT NOT NULL CHECK (direction IN ('source_to_target','mutual')),
    permits_open_record BOOLEAN NOT NULL,
    source_approved BOOLEAN NOT NULL,
    target_approved BOOLEAN NOT NULL,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    sharing_status TEXT NOT NULL CHECK (
        sharing_status IN ('proposed','active','rejected','revoked','expired')
    ),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    changed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, sharing_agreement_id),
    CHECK (valid_until > valid_from)
);

CREATE TABLE erpx_authority_read.customer_portal_principal_bindings (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_read.current_tenant_id(),
    binding_id UUID NOT NULL,
    binding_version BIGINT NOT NULL CHECK (binding_version > 0),
    customer_record_id UUID NOT NULL,
    customer_record_version BIGINT NOT NULL CHECK (customer_record_version > 0),
    portal_principal_id UUID NOT NULL,
    principal_kind TEXT NOT NULL CHECK (principal_kind IN ('human','service')),
    portal_realm TEXT NOT NULL CHECK (portal_realm = 'customer_portal'),
    binding_scope JSONB NOT NULL,
    verified_identity_digest TEXT NOT NULL CHECK (
        verified_identity_digest ~ '^[0-9A-Fa-f]{64}$'
    ),
    verified_identity_version BIGINT NOT NULL CHECK (verified_identity_version > 0),
    permitted_use_cases TEXT[] NOT NULL CHECK (
        cardinality(permitted_use_cases) = 1
        AND permitted_use_cases <@ ARRAY['field_service_record_access']::TEXT[]
    ),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    proposed_by UUID NOT NULL,
    approved_by UUID,
    ended_at TIMESTAMPTZ,
    binding_status TEXT NOT NULL CHECK (
        binding_status IN ('proposed','active','rejected','revoked','expired')
    ),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    changed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, binding_id),
    FOREIGN KEY (tenant_id, portal_principal_id)
        REFERENCES erpx_authority_read.principals(tenant_id, principal_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (valid_until > valid_from),
    CHECK (binding_status <> 'active' OR (
        approved_by IS NOT NULL AND approved_by <> proposed_by AND ended_at IS NULL
    ))
);

CREATE TABLE erpx_authority_read.policy_decision_checkpoint (
    tenant_id UUID PRIMARY KEY DEFAULT erpx_authority_read.current_tenant_id(),
    source_event_position BIGINT NOT NULL CHECK (source_event_position >= 0),
    decision_version BIGINT NOT NULL CHECK (decision_version > 0),
    last_success_at TIMESTAMPTZ,
    last_error_code TEXT,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX authority_read_assignments_effective
    ON erpx_authority_read.assignments
        (tenant_id, principal_id, role_id, valid_from, valid_until)
    WHERE revoked_at IS NULL;
CREATE INDEX authority_read_memberships_effective
    ON erpx_authority_read.group_memberships
        (tenant_id, group_id, member_principal_id, valid_from, valid_until)
    WHERE revoked_at IS NULL;
CREATE INDEX authority_read_sharing_effective
    ON erpx_authority_read.organization_sharing_agreements
        (tenant_id, sharing_status, valid_from, valid_until)
    WHERE sharing_status = 'active';
CREATE INDEX authority_read_portal_binding_effective
    ON erpx_authority_read.customer_portal_principal_bindings
        (tenant_id, portal_principal_id, customer_record_id, valid_from, valid_until)
    WHERE binding_status = 'active';

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'principals','roles','assignments','group_memberships',
        'organization_sharing_agreements','customer_portal_principal_bindings',
        'policy_decision_checkpoint'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_authority_read.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_authority_read.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY authority_read_tenant_isolation ON erpx_authority_read.%I '
            'USING (tenant_id = erpx_authority_read.current_tenant_id()) '
            'WITH CHECK (tenant_id = erpx_authority_read.current_tenant_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- Final generation-two role marker: erpx_meta.schema_manifest,
-- schema_generation=2, database_role=tenant_read, migration_set=tenant-read-v2.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_generation = 2
  AND database_role = 'tenant_read' AND migration_set = 'tenant-read-v2';

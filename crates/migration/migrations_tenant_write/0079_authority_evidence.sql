-- Normalized authority evidence for IAM and Organization. These tables are
-- command-side guards/evidence, not serialized aggregate snapshots. Every
-- mutation occurs inside the owning context command transaction.

CREATE SCHEMA IF NOT EXISTS erpx_authority_write;

CREATE OR REPLACE FUNCTION erpx_authority_write.current_tenant_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.tenant_id', true), '')::uuid
$$;

CREATE TABLE erpx_authority_write.principals (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_write.current_tenant_id(),
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
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, principal_id),
    UNIQUE (tenant_id, principal_id, principal_version)
);

CREATE TABLE erpx_authority_write.roles (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_write.current_tenant_id(),
    role_id UUID NOT NULL,
    role_scope JSONB NOT NULL,
    role_version BIGINT NOT NULL CHECK (role_version > 0),
    permissions TEXT[] NOT NULL CHECK (cardinality(permissions) BETWEEN 1 AND 256),
    classification_ceiling TEXT NOT NULL CHECK (
        classification_ceiling IN ('public','internal','confidential','restricted')
    ),
    active BOOLEAN NOT NULL,
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, role_id),
    UNIQUE (tenant_id, role_id, role_version)
);

CREATE TABLE erpx_authority_write.assignments (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_write.current_tenant_id(),
    assignment_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    role_id UUID NOT NULL,
    assignment_scope JSONB NOT NULL,
    assignment_version BIGINT NOT NULL CHECK (assignment_version > 0),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, assignment_id),
    UNIQUE (tenant_id, assignment_id, assignment_version),
    FOREIGN KEY (tenant_id, principal_id)
        REFERENCES erpx_authority_write.principals(tenant_id, principal_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, role_id)
        REFERENCES erpx_authority_write.roles(tenant_id, role_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (valid_until > valid_from)
);

CREATE TABLE erpx_authority_write.group_memberships (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_write.current_tenant_id(),
    group_id UUID NOT NULL,
    member_principal_id UUID NOT NULL,
    group_kind TEXT NOT NULL CHECK (group_kind IN ('human_group','agent_pool')),
    group_version BIGINT NOT NULL CHECK (group_version > 0),
    membership_version BIGINT NOT NULL CHECK (membership_version > 0),
    membership_scope JSONB NOT NULL,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, group_id, member_principal_id),
    UNIQUE (tenant_id, group_id, member_principal_id, membership_version),
    FOREIGN KEY (tenant_id, group_id)
        REFERENCES erpx_authority_write.principals(tenant_id, principal_id)
        ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, member_principal_id)
        REFERENCES erpx_authority_write.principals(tenant_id, principal_id)
        ON DELETE CASCADE,
    CHECK (group_id <> member_principal_id),
    CHECK (valid_until > valid_from)
);

CREATE TABLE erpx_authority_write.organization_sharing_agreements (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_write.current_tenant_id(),
    sharing_agreement_id UUID NOT NULL,
    agreement_version BIGINT NOT NULL CHECK (agreement_version > 0),
    source_scope JSONB NOT NULL,
    target_scope JSONB NOT NULL,
    direction TEXT NOT NULL CHECK (direction IN ('source_to_target','mutual')),
    capabilities TEXT[] NOT NULL CHECK (cardinality(capabilities) BETWEEN 1 AND 4),
    reason TEXT NOT NULL CHECK (octet_length(reason) BETWEEN 1 AND 256),
    proposed_by TEXT NOT NULL CHECK (octet_length(proposed_by) BETWEEN 1 AND 256),
    source_approved_by TEXT,
    target_approved_by TEXT,
    rejected_by TEXT,
    rejected_at TIMESTAMPTZ,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    sharing_status TEXT NOT NULL CHECK (
        sharing_status IN ('proposed','active','rejected','revoked','expired')
    ),
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, sharing_agreement_id),
    UNIQUE (tenant_id, sharing_agreement_id, agreement_version),
    CHECK (valid_until > valid_from),
    CHECK ((rejected_by IS NULL) = (rejected_at IS NULL)),
    CHECK (sharing_status <> 'active' OR (
        source_approved_by IS NOT NULL AND target_approved_by IS NOT NULL
        AND 'open_record' = ANY(capabilities)
    ))
);

CREATE TABLE erpx_authority_write.sharing_command_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) BETWEEN 1 AND 256),
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash) = 32),
    sharing_agreement_id UUID NOT NULL,
    agreement_version BIGINT NOT NULL CHECK (agreement_version > 0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, idempotency_key),
    UNIQUE (tenant_id, command_id),
    FOREIGN KEY (tenant_id, sharing_agreement_id)
        REFERENCES erpx_authority_write.organization_sharing_agreements
            (tenant_id, sharing_agreement_id)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE erpx_authority_write.sharing_audit_log (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_write.current_tenant_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    actor JSONB NOT NULL,
    authority_scope JSONB NOT NULL,
    sharing_agreement_id UUID NOT NULL,
    agreement_version BIGINT NOT NULL CHECK (agreement_version > 0),
    action TEXT NOT NULL CHECK (action IN (
        'propose','approve_source','approve_target','reject','revoke','expire'
    )),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, audit_id),
    UNIQUE (tenant_id, command_id),
    FOREIGN KEY (tenant_id, sharing_agreement_id)
        REFERENCES erpx_authority_write.organization_sharing_agreements
            (tenant_id, sharing_agreement_id)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE erpx_authority_write.customer_portal_principal_bindings (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_write.current_tenant_id(),
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
    verified_at TIMESTAMPTZ NOT NULL,
    permitted_use_cases TEXT[] NOT NULL CHECK (
        cardinality(permitted_use_cases) = 1
        AND permitted_use_cases <@ ARRAY['field_service_record_access']::TEXT[]
    ),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    proposed_by UUID NOT NULL,
    approved_by UUID,
    rejected_by UUID,
    revoked_by UUID,
    ended_at TIMESTAMPTZ,
    decision_evidence_digest TEXT CHECK (
        decision_evidence_digest IS NULL
        OR decision_evidence_digest ~ '^[0-9A-Fa-f]{64}$'
    ),
    binding_status TEXT NOT NULL CHECK (
        binding_status IN ('proposed','active','rejected','revoked','expired')
    ),
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, binding_id),
    UNIQUE (tenant_id, binding_id, binding_version),
    FOREIGN KEY (tenant_id, portal_principal_id)
        REFERENCES erpx_authority_write.principals(tenant_id, principal_id)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (tenant_id, proposed_by)
        REFERENCES erpx_authority_write.principals(tenant_id, principal_id)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (tenant_id, approved_by)
        REFERENCES erpx_authority_write.principals(tenant_id, principal_id)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (tenant_id, rejected_by)
        REFERENCES erpx_authority_write.principals(tenant_id, principal_id)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (tenant_id, revoked_by)
        REFERENCES erpx_authority_write.principals(tenant_id, principal_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (valid_until > valid_from),
    CHECK (binding_status <> 'active' OR (
        approved_by IS NOT NULL AND approved_by <> proposed_by
        AND ended_at IS NULL AND decision_evidence_digest IS NOT NULL
    )),
    CHECK (binding_status <> 'rejected' OR (
        rejected_by IS NOT NULL AND rejected_by <> proposed_by
        AND ended_at IS NOT NULL AND decision_evidence_digest IS NOT NULL
    )),
    CHECK (binding_status <> 'revoked' OR (
        approved_by IS NOT NULL AND revoked_by IS NOT NULL
        AND ended_at IS NOT NULL AND decision_evidence_digest IS NOT NULL
    )),
    CHECK (binding_status <> 'expired' OR ended_at IS NOT NULL)
);

CREATE TABLE erpx_authority_write.portal_binding_command_receipts (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_write.current_tenant_id(),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) BETWEEN 1 AND 256),
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash) = 32),
    binding_id UUID NOT NULL,
    binding_version BIGINT NOT NULL CHECK (binding_version > 0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, idempotency_key),
    UNIQUE (tenant_id, command_id),
    FOREIGN KEY (tenant_id, binding_id)
        REFERENCES erpx_authority_write.customer_portal_principal_bindings
            (tenant_id, binding_id)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE erpx_authority_write.portal_binding_audit_log (
    tenant_id UUID NOT NULL DEFAULT erpx_authority_write.current_tenant_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    actor JSONB NOT NULL,
    authority_scope JSONB NOT NULL,
    binding_id UUID NOT NULL,
    binding_version BIGINT NOT NULL CHECK (binding_version > 0),
    customer_record_id UUID NOT NULL,
    portal_principal_id UUID NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('propose','approve','reject','revoke','expire')),
    evidence_digest TEXT,
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, audit_id),
    UNIQUE (tenant_id, command_id),
    FOREIGN KEY (tenant_id, binding_id)
        REFERENCES erpx_authority_write.customer_portal_principal_bindings
            (tenant_id, binding_id)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE INDEX authority_assignments_effective
    ON erpx_authority_write.assignments
        (tenant_id, principal_id, role_id, valid_from, valid_until)
    WHERE revoked_at IS NULL;
CREATE INDEX authority_memberships_effective
    ON erpx_authority_write.group_memberships
        (tenant_id, group_id, member_principal_id, valid_from, valid_until)
    WHERE revoked_at IS NULL;
CREATE INDEX authority_sharing_effective
    ON erpx_authority_write.organization_sharing_agreements
        (tenant_id, sharing_status, valid_from, valid_until)
    WHERE sharing_status = 'active';
CREATE UNIQUE INDEX authority_portal_binding_one_current
    ON erpx_authority_write.customer_portal_principal_bindings
        (tenant_id, customer_record_id, portal_principal_id)
    WHERE binding_status IN ('proposed','active');
CREATE INDEX authority_portal_binding_effective
    ON erpx_authority_write.customer_portal_principal_bindings
        (tenant_id, portal_principal_id, customer_record_id, valid_from, valid_until)
    WHERE binding_status = 'active';

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'principals','roles','assignments','group_memberships',
        'organization_sharing_agreements','sharing_command_receipts','sharing_audit_log',
        'customer_portal_principal_bindings','portal_binding_command_receipts',
        'portal_binding_audit_log'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_authority_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_authority_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY authority_write_tenant_isolation ON erpx_authority_write.%I '
            'USING (tenant_id = erpx_authority_write.current_tenant_id()) '
            'WITH CHECK (tenant_id = erpx_authority_write.current_tenant_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- Final generation-two role marker: erpx_meta.schema_manifest,
-- schema_generation=2, database_role=tenant_write, migration_set=tenant-write-v2.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_generation = 2
  AND database_role = 'tenant_write' AND migration_set = 'tenant-write-v2';

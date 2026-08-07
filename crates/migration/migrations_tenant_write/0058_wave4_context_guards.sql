-- Context-specific uniqueness and hierarchy guards maintained in the same
-- transaction as erpx_cqrs_write snapshots, receipts, audit, and outbox rows.
-- Aggregate state remains authoritative in the shared snapshot table.

CREATE SCHEMA IF NOT EXISTS erpx_identity_access_write;

CREATE TABLE erpx_identity_access_write.role_code_guards (
    role_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    role_code TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, role_code)
);

CREATE TABLE erpx_identity_access_write.assignment_guards (
    assignment_id UUID PRIMARY KEY,
    principal_id UUID NOT NULL,
    role_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    risk_tier TEXT NOT NULL CHECK (risk_tier IN (
        'platform_root', 'administrator', 'high', 'standard', 'contractor',
        'support_diagnostic', 'support_mutation', 'emergency'
    )),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    principal_version BIGINT NOT NULL CHECK (principal_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (valid_until > valid_from)
);

CREATE UNIQUE INDEX identity_access_one_live_assignment
    ON erpx_identity_access_write.assignment_guards
        (principal_id, role_id, scope_key)
    WHERE revoked_at IS NULL;

CREATE INDEX identity_access_assignment_validity
    ON erpx_identity_access_write.assignment_guards
        (principal_id, valid_from, valid_until)
    WHERE revoked_at IS NULL;

CREATE SCHEMA IF NOT EXISTS erpx_organization_write;

CREATE TABLE erpx_organization_write.hierarchy_guards (
    node_id UUID PRIMARY KEY,
    node_kind TEXT NOT NULL CHECK (node_kind IN (
        'holding', 'company', 'headquarters', 'branch', 'department', 'team'
    )),
    parent_node_id UUID,
    lineage_node_ids UUID[] NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    department_id UUID,
    node_code TEXT NOT NULL,
    node_status TEXT NOT NULL CHECK (node_status IN ('active', 'suspended', 'archived')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (NOT node_id = ANY(lineage_node_ids)),
    CHECK (
        (node_kind = 'holding' AND parent_node_id IS NULL AND company_id IS NULL)
        OR (node_kind <> 'holding' AND parent_node_id IS NOT NULL)
    )
);

CREATE UNIQUE INDEX organization_sibling_code
    ON erpx_organization_write.hierarchy_guards
        (holding_id, COALESCE(parent_node_id, holding_id), node_code)
    WHERE node_status <> 'archived';

CREATE INDEX organization_active_children
    ON erpx_organization_write.hierarchy_guards (parent_node_id, node_id)
    WHERE node_status = 'active';

CREATE INDEX organization_lineage_lookup
    ON erpx_organization_write.hierarchy_guards USING GIN (lineage_node_ids);

CREATE SCHEMA IF NOT EXISTS erpx_commercial_write;

CREATE TABLE erpx_commercial_write.catalog_plan_guards (
    catalog_plan_id UUID PRIMARY KEY,
    plan_key TEXT NOT NULL UNIQUE,
    plan_status TEXT NOT NULL CHECK (plan_status IN ('draft', 'published')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE erpx_commercial_write.subscription_guards (
    subscription_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    catalog_plan_id UUID NOT NULL,
    subscription_status TEXT NOT NULL CHECK (subscription_status IN (
        'trial', 'active', 'past_due', 'suspended', 'ended'
    )),
    last_provider_event_at TIMESTAMPTZ,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX commercial_customer_subscription
    ON erpx_commercial_write.subscription_guards
        (scope_key, customer_id, subscription_status);

CREATE TABLE erpx_commercial_write.entitlement_guards (
    entitlement_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    subscription_id UUID NOT NULL,
    capability TEXT NOT NULL,
    enabled BOOLEAN NOT NULL,
    seat_capacity BIGINT NOT NULL CHECK (seat_capacity > 0),
    company_capacity BIGINT NOT NULL CHECK (company_capacity > 0),
    branch_capacity BIGINT NOT NULL CHECK (branch_capacity > 0),
    allocated_seats BIGINT NOT NULL CHECK (allocated_seats >= 0 AND allocated_seats <= seat_capacity),
    allocated_companies BIGINT NOT NULL CHECK (allocated_companies >= 0 AND allocated_companies <= company_capacity),
    allocated_branches BIGINT NOT NULL CHECK (allocated_branches >= 0 AND allocated_branches <= branch_capacity),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, subscription_id, capability)
);

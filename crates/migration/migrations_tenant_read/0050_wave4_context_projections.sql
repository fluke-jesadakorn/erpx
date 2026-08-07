-- Rebuildable Wave 4 read models. Every row is generation-scoped so a shadow
-- rebuild can be activated without mutating the currently active projection.

CREATE SCHEMA IF NOT EXISTS erpx_identity_access_read;

CREATE TABLE erpx_identity_access_read.principal_projection (
    projection_name TEXT NOT NULL DEFAULT 'identity_access.principals'
        CHECK (projection_name = 'identity_access.principals'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    principal_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    principal_status TEXT NOT NULL CHECK (principal_status IN (
        'invited', 'active', 'suspended', 'disabled'
    )),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, principal_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX identity_access_principal_scope_status
    ON erpx_identity_access_read.principal_projection
        (generation, scope_key, principal_status, aggregate_updated_at DESC, principal_id DESC);

CREATE TABLE erpx_identity_access_read.role_projection (
    projection_name TEXT NOT NULL DEFAULT 'identity_access.roles'
        CHECK (projection_name = 'identity_access.roles'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    role_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    role_code TEXT NOT NULL,
    display_name TEXT NOT NULL,
    risk_tier TEXT NOT NULL CHECK (risk_tier IN (
        'platform_root', 'administrator', 'high', 'standard', 'contractor',
        'support_diagnostic', 'support_mutation', 'emergency'
    )),
    permissions JSONB NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, role_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, role_code)
);

CREATE INDEX identity_access_role_scope_risk
    ON erpx_identity_access_read.role_projection
        (generation, scope_key, risk_tier, aggregate_updated_at DESC, role_id DESC);

CREATE TABLE erpx_identity_access_read.assignment_projection (
    projection_name TEXT NOT NULL DEFAULT 'identity_access.principals'
        CHECK (projection_name = 'identity_access.principals'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    principal_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    role_id UUID NOT NULL,
    assignment_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    risk_tier TEXT NOT NULL,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, assignment_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    CHECK (valid_until > valid_from)
);

CREATE INDEX identity_access_assignment_principal_validity
    ON erpx_identity_access_read.assignment_projection
        (generation, principal_id, valid_from, valid_until)
    WHERE revoked_at IS NULL;

CREATE SCHEMA IF NOT EXISTS erpx_organization_read;

CREATE TABLE erpx_organization_read.node_projection (
    projection_name TEXT NOT NULL DEFAULT 'organization.nodes'
        CHECK (projection_name = 'organization.nodes'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    node_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    node_kind TEXT NOT NULL CHECK (node_kind IN (
        'holding', 'company', 'headquarters', 'branch', 'department', 'team'
    )),
    node_status TEXT NOT NULL CHECK (node_status IN ('active', 'suspended', 'archived')),
    parent_node_id UUID,
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    placement JSONB NOT NULL,
    node_code TEXT NOT NULL,
    display_name TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, node_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX organization_node_scope_kind_status
    ON erpx_organization_read.node_projection
        (generation, scope_key, node_kind, node_status, aggregate_updated_at DESC, node_id DESC);

CREATE INDEX organization_node_parent
    ON erpx_organization_read.node_projection
        (generation, parent_node_id, aggregate_updated_at DESC, node_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_commercial_read;

CREATE TABLE erpx_commercial_read.catalog_plan_projection (
    projection_name TEXT NOT NULL DEFAULT 'commercial.catalog_plans'
        CHECK (projection_name = 'commercial.catalog_plans'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    catalog_plan_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    plan_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    plan_status TEXT NOT NULL CHECK (plan_status IN ('draft', 'published')),
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, catalog_plan_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, plan_key)
);

CREATE INDEX commercial_catalog_status
    ON erpx_commercial_read.catalog_plan_projection
        (generation, plan_status, aggregate_updated_at DESC, catalog_plan_id DESC);

CREATE TABLE erpx_commercial_read.subscription_projection (
    projection_name TEXT NOT NULL DEFAULT 'commercial.subscriptions'
        CHECK (projection_name = 'commercial.subscriptions'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    subscription_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    catalog_plan_id UUID NOT NULL,
    subscription_status TEXT NOT NULL CHECK (subscription_status IN (
        'trial', 'active', 'past_due', 'suspended', 'ended'
    )),
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, subscription_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX commercial_subscription_customer_status
    ON erpx_commercial_read.subscription_projection
        (generation, scope_key, customer_id, subscription_status,
         aggregate_updated_at DESC, subscription_id DESC);

CREATE TABLE erpx_commercial_read.entitlement_projection (
    projection_name TEXT NOT NULL DEFAULT 'commercial.entitlements'
        CHECK (projection_name = 'commercial.entitlements'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    entitlement_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
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
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, entitlement_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, subscription_id, capability)
);

CREATE INDEX commercial_entitlement_subscription
    ON erpx_commercial_read.entitlement_projection
        (generation, scope_key, subscription_id, enabled,
         aggregate_updated_at DESC, entitlement_id DESC);


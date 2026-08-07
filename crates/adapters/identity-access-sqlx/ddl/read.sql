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

-- Disposable Wave 5 control-plane projections. Every table is generation
-- scoped so rebuilds can switch atomically without stopping writes.

CREATE SCHEMA IF NOT EXISTS erpx_platform_administration_read;

CREATE TABLE erpx_platform_administration_read.tenant_projection (
    projection_name TEXT NOT NULL DEFAULT 'platform_administration.tenants'
        CHECK (projection_name = 'platform_administration.tenants'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    tenant_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_code TEXT NOT NULL,
    tenant_name TEXT NOT NULL,
    environment_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    root_holding_id UUID NOT NULL,
    holding_ref TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL CHECK (lifecycle_state IN (
        'provisioning', 'active', 'suspended', 'recovery', 'retired'
    )),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, tenant_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, tenant_code),
    UNIQUE (generation, customer_id),
    UNIQUE (generation, root_holding_id),
    UNIQUE (generation, holding_ref)
);

CREATE INDEX platform_admin_read_tenant_state
    ON erpx_platform_administration_read.tenant_projection
        (generation, lifecycle_state, aggregate_updated_at DESC, tenant_id DESC);

CREATE TABLE erpx_platform_administration_read.connector_projection (
    projection_name TEXT NOT NULL DEFAULT 'platform_administration.connectors'
        CHECK (projection_name = 'platform_administration.connectors'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    connector_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    connector_kind TEXT NOT NULL,
    connector_key TEXT NOT NULL,
    configuration_digest TEXT NOT NULL,
    capabilities JSONB NOT NULL,
    connector_state TEXT NOT NULL CHECK (connector_state IN (
        'pending', 'active', 'suspended', 'retired'
    )),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, connector_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, connector_key)
);

CREATE INDEX platform_admin_read_connector_scope
    ON erpx_platform_administration_read.connector_projection
        (generation, scope_key, connector_state, aggregate_updated_at DESC, connector_id DESC);

CREATE TABLE erpx_platform_administration_read.admin_context_projection (
    projection_name TEXT NOT NULL DEFAULT 'platform_administration.admin_contexts'
        CHECK (projection_name = 'platform_administration.admin_contexts'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    context_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_id UUID NOT NULL,
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    principal_ref TEXT NOT NULL,
    reason TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    context_status TEXT NOT NULL CHECK (context_status IN ('active', 'revoked', 'expired')),
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, context_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    CHECK (expires_at > issued_at)
);

CREATE INDEX platform_admin_read_context_expiry
    ON erpx_platform_administration_read.admin_context_projection
        (generation, tenant_id, scope_key, context_status, expires_at, context_id);

CREATE SCHEMA IF NOT EXISTS erpx_brand_read;

CREATE TABLE erpx_brand_read.profile_projection (
    projection_name TEXT NOT NULL DEFAULT 'brand.profiles'
        CHECK (projection_name = 'brand.profiles'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    profile_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    draft_revision BIGINT CHECK (draft_revision > 0),
    published_revision BIGINT CHECK (published_revision > 0),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, profile_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key)
);

CREATE INDEX brand_read_scope_revision
    ON erpx_brand_read.profile_projection
        (generation, scope_key, published_revision DESC, profile_id DESC);

CREATE TABLE erpx_brand_read.resolved_projection (
    projection_name TEXT NOT NULL DEFAULT 'brand.resolved'
        CHECK (projection_name = 'brand.resolved'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    published_revision BIGINT NOT NULL CHECK (published_revision > 0),
    definition JSONB NOT NULL,
    inherited_categories JSONB NOT NULL,
    effective_locked_categories JSONB NOT NULL,
    digest TEXT NOT NULL,
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, scope_key),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_brand_read.history_projection (
    projection_name TEXT NOT NULL DEFAULT 'brand.history'
        CHECK (projection_name = 'brand.history'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    profile_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    lifecycle TEXT NOT NULL CHECK (lifecycle IN ('draft', 'published')),
    definition JSONB NOT NULL,
    parent_published_revision BIGINT CHECK (parent_published_revision > 0),
    updated_by TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ,
    archived_at TIMESTAMPTZ,
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, profile_id, revision),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX brand_read_history
    ON erpx_brand_read.history_projection
        (generation, scope_key, revision DESC, profile_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_customer_authority_read;

CREATE TABLE erpx_customer_authority_read.customer_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer_authority.customers'
        CHECK (projection_name = 'customer_authority.customers'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    customer_code TEXT NOT NULL,
    trading_name TEXT NOT NULL,
    legal_name TEXT,
    country TEXT NOT NULL CHECK (country ~ '^[A-Z]{2}$'),
    customer_status TEXT NOT NULL CHECK (customer_status IN (
        'prospect', 'onboarding', 'active', 'suspended', 'churned'
    )),
    primary_authority JSONB NOT NULL,
    primary_scope_key TEXT NOT NULL,
    merged_into_customer_id UUID,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, customer_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, customer_code)
);

CREATE INDEX customer_authority_read_search
    ON erpx_customer_authority_read.customer_projection
        (generation, customer_status, trading_name, customer_code, customer_id);

CREATE TABLE erpx_customer_authority_read.authority_link_projection (
    projection_name TEXT NOT NULL DEFAULT 'customer_authority.customers'
        CHECK (projection_name = 'customer_authority.customers'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    authority_scope JSONB NOT NULL,
    is_primary BOOLEAN NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, customer_id, scope_key),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    FOREIGN KEY (projection_name, generation, partition_key, customer_id)
        REFERENCES erpx_customer_authority_read.customer_projection
            (projection_name, generation, partition_key, customer_id)
        ON DELETE CASCADE
);

CREATE UNIQUE INDEX customer_authority_read_one_primary
    ON erpx_customer_authority_read.authority_link_projection
        (generation, partition_key, customer_id)
    WHERE is_primary;

CREATE INDEX customer_authority_read_scope
    ON erpx_customer_authority_read.authority_link_projection
        (generation, scope_key, customer_id);

-- D13: disposable active-generation Platform governance read model.
CREATE TABLE erpx_platform_administration_read.governance_record_projection (
    projection_name TEXT NOT NULL DEFAULT 'platform_administration.governance-records'
        CHECK (projection_name = 'platform_administration.governance-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'platform.data-plane-operation', 'platform.migration', 'platform.release'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, generation, partition_key, record_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX platform_governance_read_state
    ON erpx_platform_administration_read.governance_record_projection
        (generation, record_type, lifecycle_state, aggregate_updated_at DESC, record_id DESC);

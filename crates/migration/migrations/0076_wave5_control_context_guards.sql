-- Wave 5 control-plane guards. Aggregate snapshots, receipts, audit, and
-- outbox continue to commit through erpx_cqrs_write in the same transaction.

CREATE SCHEMA IF NOT EXISTS erpx_platform_administration_write;

CREATE TABLE erpx_platform_administration_write.tenant_guards (
    tenant_id UUID PRIMARY KEY,
    tenant_code TEXT NOT NULL UNIQUE,
    tenant_name TEXT NOT NULL,
    environment_id UUID NOT NULL,
    customer_id UUID NOT NULL UNIQUE,
    root_holding_id UUID NOT NULL UNIQUE,
    holding_ref TEXT NOT NULL UNIQUE,
    lifecycle_state TEXT NOT NULL CHECK (lifecycle_state IN (
        'provisioning', 'active', 'suspended', 'recovery', 'retired'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX platform_admin_tenant_state
    ON erpx_platform_administration_write.tenant_guards
        (lifecycle_state, updated_at DESC, tenant_id DESC);

CREATE TABLE erpx_platform_administration_write.connector_guards (
    connector_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    connector_kind TEXT NOT NULL,
    connector_key TEXT NOT NULL,
    configuration_digest BYTEA NOT NULL CHECK (octet_length(configuration_digest) = 32),
    capabilities JSONB NOT NULL,
    connector_state TEXT NOT NULL CHECK (connector_state IN (
        'pending', 'active', 'suspended', 'retired'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, connector_key)
);

CREATE INDEX platform_admin_connector_scope_state
    ON erpx_platform_administration_write.connector_guards
        (scope_key, connector_state, updated_at DESC, connector_id DESC);

CREATE TABLE erpx_platform_administration_write.admin_context_guards (
    context_id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    principal_ref TEXT NOT NULL,
    device_id UUID NOT NULL,
    session_id UUID NOT NULL,
    reason TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    CHECK (expires_at > issued_at),
    CHECK (expires_at <= issued_at + INTERVAL '30 minutes')
);

CREATE INDEX platform_admin_context_live
    ON erpx_platform_administration_write.admin_context_guards
        (tenant_id, scope_key, expires_at, context_id)
    WHERE revoked_at IS NULL;

CREATE SCHEMA IF NOT EXISTS erpx_brand_write;

CREATE TABLE erpx_brand_write.profile_guards (
    profile_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL UNIQUE,
    draft_revision BIGINT CHECK (draft_revision > 0),
    published_revision BIGINT CHECK (published_revision > 0),
    parent_published_revision BIGINT CHECK (parent_published_revision > 0),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE erpx_brand_write.profile_revisions (
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
    PRIMARY KEY (profile_id, revision),
    FOREIGN KEY (profile_id)
        REFERENCES erpx_brand_write.profile_guards (profile_id)
        ON DELETE CASCADE
);

CREATE INDEX brand_revision_history
    ON erpx_brand_write.profile_revisions
        (profile_id, revision DESC, created_at DESC);

CREATE SCHEMA IF NOT EXISTS erpx_customer_authority_write;

CREATE TABLE erpx_customer_authority_write.account_guards (
    customer_id UUID PRIMARY KEY,
    customer_code TEXT NOT NULL UNIQUE,
    primary_scope_key TEXT NOT NULL,
    primary_holding_id UUID NOT NULL,
    primary_company_id UUID NOT NULL,
    customer_status TEXT NOT NULL CHECK (customer_status IN (
        'prospect', 'onboarding', 'active', 'suspended', 'churned'
    )),
    merged_into_customer_id UUID,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (merged_into_customer_id IS NULL OR merged_into_customer_id <> customer_id)
);

CREATE INDEX customer_authority_status
    ON erpx_customer_authority_write.account_guards
        (customer_status, updated_at DESC, customer_id DESC);

CREATE TABLE erpx_customer_authority_write.authority_link_guards (
    customer_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('holding', 'company', 'branch')),
    holding_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    is_primary BOOLEAN NOT NULL DEFAULT FALSE,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (customer_id, scope_key),
    FOREIGN KEY (customer_id)
        REFERENCES erpx_customer_authority_write.account_guards (customer_id)
        ON DELETE CASCADE,
    CHECK (
        (scope_kind = 'holding' AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_kind = 'company' AND company_id IS NOT NULL AND branch_id IS NULL)
        OR (scope_kind = 'branch' AND company_id IS NOT NULL AND branch_id IS NOT NULL)
    )
);

CREATE UNIQUE INDEX customer_authority_one_primary
    ON erpx_customer_authority_write.authority_link_guards (customer_id)
    WHERE is_primary;

CREATE INDEX customer_authority_scope
    ON erpx_customer_authority_write.authority_link_guards
        (scope_key, customer_id);

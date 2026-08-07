-- Branch-scoped physical database routing and resumable first-install state.
-- All credential material remains in server-side profile resolvers; these
-- relations contain only bounded profile keys, opaque physical identifiers,
-- lifecycle state, and redacted observations.

ALTER TABLE erpx_control.tenant_registry
    ADD COLUMN IF NOT EXISTS isolation_mode TEXT NOT NULL DEFAULT 'holding_pair';

ALTER TABLE erpx_control.tenant_registry
    DROP CONSTRAINT IF EXISTS tenant_registry_isolation_mode_check;
ALTER TABLE erpx_control.tenant_registry
    ADD CONSTRAINT tenant_registry_isolation_mode_check
    CHECK (isolation_mode IN ('holding_pair', 'holding_and_branch_pairs'));

CREATE TABLE IF NOT EXISTS erpx_control.database_profiles (
    profile_key TEXT PRIMARY KEY,
    database_prefix TEXT NOT NULL,
    deployment_mode TEXT NOT NULL
        CHECK (deployment_mode IN ('erpx_managed', 'customer_data_plane')),
    credential_profile_key TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('active', 'suspended')),
    profile_version BIGINT NOT NULL CHECK (profile_version > 0),
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (profile_key ~ '^[a-z][a-z0-9_-]{0,63}$'),
    CHECK (database_prefix ~ '^[a-z][a-z0-9_]{1,15}$'),
    CHECK (length(btrim(credential_profile_key)) BETWEEN 1 AND 255),
    CHECK (length(btrim(created_by)) BETWEEN 1 AND 255)
);

CREATE TABLE IF NOT EXISTS erpx_control.tenant_database_profile_bindings (
    tenant_id TEXT NOT NULL
        REFERENCES erpx_control.tenant_registry(tenant_id) ON DELETE RESTRICT,
    profile_key TEXT NOT NULL
        REFERENCES erpx_control.database_profiles(profile_key) ON DELETE RESTRICT,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    bound_by TEXT NOT NULL,
    bound_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, profile_key),
    CHECK (length(btrim(bound_by)) BETWEEN 1 AND 255)
);

CREATE TABLE IF NOT EXISTS erpx_control.branch_database_routes (
    tenant_id TEXT NOT NULL,
    holding_ref TEXT NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID NOT NULL,
    profile_key TEXT NOT NULL,
    write_database_key TEXT NOT NULL,
    read_database_key TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN (
        'queued', 'provisioning', 'verifying', 'active', 'failed', 'suspended'
    )),
    route_version BIGINT NOT NULL CHECK (route_version > 0),
    failure_category TEXT,
    requested_by TEXT NOT NULL,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    activated_at TIMESTAMPTZ,
    suspended_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, branch_id),
    FOREIGN KEY (tenant_id, holding_ref)
        REFERENCES erpx_control.tenant_registry(tenant_id, holding_ref)
        ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, profile_key)
        REFERENCES erpx_control.tenant_database_profile_bindings(tenant_id, profile_key)
        ON DELETE RESTRICT,
    UNIQUE (write_database_key),
    UNIQUE (read_database_key),
    CHECK (write_database_key <> read_database_key),
    CHECK (length(btrim(write_database_key)) BETWEEN 1 AND 63),
    CHECK (length(btrim(read_database_key)) BETWEEN 1 AND 63),
    CHECK (write_database_key ~ '^[A-Za-z0-9_-]+$'),
    CHECK (read_database_key ~ '^[A-Za-z0-9_-]+$'),
    CHECK (length(btrim(requested_by)) BETWEEN 1 AND 255),
    CHECK (failure_category IS NULL OR (
        length(btrim(failure_category)) BETWEEN 1 AND 128
        AND lower(failure_category) !~ '(password|secret|token|credential|postgres://|postgresql://)'
    )),
    CHECK ((state = 'active') = (activated_at IS NOT NULL)),
    CHECK ((state = 'suspended') = (suspended_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS branch_database_routes_parent_idx
    ON erpx_control.branch_database_routes
        (tenant_id, holding_id, company_id, state, branch_id);

CREATE TABLE IF NOT EXISTS erpx_control.branch_provisioning_jobs (
    job_id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    branch_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL,
    branch_code TEXT NOT NULL,
    branch_name TEXT NOT NULL,
    registration_number TEXT NOT NULL DEFAULT '',
    enabled_business_units TEXT[] NOT NULL,
    state TEXT NOT NULL CHECK (state IN (
        'queued', 'provisioning', 'verifying', 'active', 'failed', 'suspended'
    )),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count BETWEEN 0 AND 10),
    claimed_by UUID,
    claimed_at TIMESTAMPTZ,
    lease_expires_at TIMESTAMPTZ,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    failure_category TEXT,
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    FOREIGN KEY (tenant_id, branch_id)
        REFERENCES erpx_control.branch_database_routes(tenant_id, branch_id)
        ON DELETE RESTRICT,
    UNIQUE (tenant_id, idempotency_key),
    CHECK (length(btrim(idempotency_key)) BETWEEN 1 AND 255),
    CHECK (length(btrim(branch_code)) BETWEEN 1 AND 128),
    CHECK (length(btrim(branch_name)) BETWEEN 1 AND 255),
    CHECK (length(registration_number) <= 255),
    CHECK (cardinality(enabled_business_units) BETWEEN 1 AND 12),
    CHECK (enabled_business_units <@ ARRAY[
        'operations', 'finance', 'sales', 'procurement', 'inventory', 'assets',
        'maintenance', 'quality', 'marketing-crm', 'hr-payroll',
        'point-of-sale', 'legal'
    ]::TEXT[]),
    CHECK ((claimed_by IS NULL) = (claimed_at IS NULL)),
    CHECK ((claimed_by IS NULL) = (lease_expires_at IS NULL)),
    CHECK (lease_expires_at IS NULL OR lease_expires_at > claimed_at),
    CHECK (failure_category IS NULL OR (
        length(btrim(failure_category)) BETWEEN 1 AND 128
        AND lower(failure_category) !~ '(password|secret|token|credential|postgres://|postgresql://)'
    )),
    CHECK ((state = 'active') = (completed_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS branch_provisioning_jobs_claim_idx
    ON erpx_control.branch_provisioning_jobs (next_attempt_at, created_at, job_id)
    WHERE state = 'queued' AND completed_at IS NULL;

CREATE TABLE IF NOT EXISTS erpx_control.branch_database_health (
    tenant_id TEXT NOT NULL,
    branch_id UUID NOT NULL,
    side TEXT NOT NULL CHECK (side IN ('write', 'read')),
    database_key TEXT NOT NULL,
    schema_version TEXT NOT NULL,
    reachable BOOLEAN NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL,
    failure_category TEXT,
    PRIMARY KEY (tenant_id, branch_id, side),
    FOREIGN KEY (tenant_id, branch_id)
        REFERENCES erpx_control.branch_database_routes(tenant_id, branch_id)
        ON DELETE RESTRICT,
    CHECK (length(btrim(schema_version)) BETWEEN 1 AND 128),
    CHECK (failure_category IS NULL OR (
        length(btrim(failure_category)) BETWEEN 1 AND 128
        AND lower(failure_category) !~ '(password|secret|token|credential|postgres://|postgresql://)'
    ))
);

CREATE TABLE IF NOT EXISTS erpx_control.branch_business_unit_configurations (
    tenant_id TEXT NOT NULL,
    branch_id UUID NOT NULL,
    business_unit TEXT NOT NULL CHECK (business_unit IN (
        'operations', 'finance', 'sales', 'procurement', 'inventory', 'assets',
        'maintenance', 'quality', 'marketing-crm', 'hr-payroll',
        'point-of-sale', 'legal'
    )),
    active BOOLEAN NOT NULL,
    configuration_version BIGINT NOT NULL CHECK (configuration_version > 0),
    updated_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, branch_id, business_unit),
    FOREIGN KEY (tenant_id, branch_id)
        REFERENCES erpx_control.branch_database_routes(tenant_id, branch_id)
        ON DELETE RESTRICT,
    CHECK (length(btrim(updated_by)) BETWEEN 1 AND 255)
);

-- Existing holding-pair tenants opt in one branch at a time. The job retains
-- only database keys, high-watermarks, counts, digests and rollback fences;
-- aggregate payloads continue to live in tenant databases and their outboxes.
CREATE TABLE IF NOT EXISTS erpx_control.branch_route_migration_jobs (
    migration_id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    branch_id UUID NOT NULL,
    state TEXT NOT NULL CHECK (state IN (
        'queued', 'preflighting', 'provisioning', 'copying', 'write_fenced',
        'verifying', 'active', 'rollback_queued', 'rolling_back',
        'rolled_back', 'failed'
    )),
    source_write_database_key TEXT NOT NULL,
    source_read_database_key TEXT NOT NULL,
    candidate_write_database_key TEXT NOT NULL,
    candidate_read_database_key TEXT NOT NULL,
    source_high_watermark BIGINT NOT NULL DEFAULT 0 CHECK (source_high_watermark >= 0),
    final_high_watermark BIGINT CHECK (final_high_watermark >= source_high_watermark),
    aggregate_count BIGINT CHECK (aggregate_count IS NULL OR aggregate_count >= 0),
    receipt_count BIGINT CHECK (receipt_count IS NULL OR receipt_count >= 0),
    outbox_count BIGINT CHECK (outbox_count IS NULL OR outbox_count >= 0),
    source_digest CHAR(64),
    candidate_digest CHAR(64),
    requested_by TEXT NOT NULL,
    approved_by TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    failure_category TEXT,
    rollback_until TIMESTAMPTZ,
    activated_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (tenant_id, branch_id)
        REFERENCES erpx_control.branch_database_routes(tenant_id, branch_id)
        ON DELETE RESTRICT,
    UNIQUE (tenant_id, branch_id, migration_id),
    CHECK (requested_by <> approved_by),
    CHECK (length(btrim(requested_by)) BETWEEN 1 AND 255),
    CHECK (length(btrim(approved_by)) BETWEEN 1 AND 255),
    CHECK (source_write_database_key <> candidate_write_database_key),
    CHECK (source_read_database_key <> candidate_read_database_key),
    CHECK (source_digest IS NULL OR source_digest ~ '^[0-9A-Fa-f]{64}$'),
    CHECK (candidate_digest IS NULL OR candidate_digest ~ '^[0-9A-Fa-f]{64}$'),
    CHECK ((state = 'active') = (activated_at IS NOT NULL AND rollback_until IS NOT NULL)),
    CHECK (rollback_until IS NULL OR rollback_until > activated_at),
    CHECK (failure_category IS NULL OR (
        length(btrim(failure_category)) BETWEEN 1 AND 128
        AND lower(failure_category) !~ '(password|secret|token|credential|postgres://|postgresql://)'
    ))
);

CREATE UNIQUE INDEX IF NOT EXISTS branch_route_migration_jobs_open_idx
    ON erpx_control.branch_route_migration_jobs (tenant_id, branch_id)
    WHERE state NOT IN ('active', 'rolled_back', 'failed');

CREATE INDEX IF NOT EXISTS branch_route_migration_jobs_claim_idx
    ON erpx_control.branch_route_migration_jobs (state, updated_at, migration_id)
    WHERE state IN ('queued', 'rollback_queued');

CREATE TABLE IF NOT EXISTS erpx_control.platform_bootstrap_receipts (
    installation_prefix TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL UNIQUE
        REFERENCES erpx_control.tenant_registry(tenant_id) ON DELETE RESTRICT,
    holding_ref TEXT NOT NULL,
    root_principal_ref TEXT NOT NULL,
    owner_principal_ref TEXT NOT NULL,
    profile_key TEXT NOT NULL
        REFERENCES erpx_control.database_profiles(profile_key) ON DELETE RESTRICT,
    state TEXT NOT NULL CHECK (state IN ('provisioning', 'complete', 'failed')),
    completed_steps TEXT[] NOT NULL DEFAULT '{}',
    failure_category TEXT,
    bootstrap_version BIGINT NOT NULL CHECK (bootstrap_version > 0),
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (installation_prefix ~ '^[a-z][a-z0-9_]{1,15}$'),
    CHECK (root_principal_ref <> owner_principal_ref),
    CHECK (length(btrim(root_principal_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(owner_principal_ref)) BETWEEN 1 AND 255),
    CHECK ((state = 'complete') = (completed_at IS NOT NULL)),
    CHECK (failure_category IS NULL OR (
        length(btrim(failure_category)) BETWEEN 1 AND 128
        AND lower(failure_category) !~ '(password|secret|token|credential|postgres://|postgresql://)'
    ))
);

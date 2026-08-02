-- Customer-hosted data-plane control state. This migration is additive and
-- defaults only already-bound tenants to ERPX-managed execution. It stores
-- secret-manager references, hashes, fingerprints, and redacted observations;
-- raw database, storage, directory, bootstrap, or capability secrets are not
-- part of this schema.

-- PostgreSQL requires an exact unique key for the composite tenant/holding
-- foreign keys below. Migration 0069 makes both columns individually unique,
-- but that does not make the pair a valid referenced key.
ALTER TABLE erpx_control.customer_tenant_bindings
    ADD CONSTRAINT customer_tenant_bindings_tenant_holding_unique
    UNIQUE (tenant_id, root_holding_id);

CREATE TABLE IF NOT EXISTS erpx_control.tenant_infrastructure_profiles (
    tenant_id TEXT PRIMARY KEY
        REFERENCES erpx_control.customer_tenant_bindings(tenant_id) ON DELETE RESTRICT,
    root_holding_id UUID NOT NULL,
    deployment_mode TEXT NOT NULL
        CHECK (deployment_mode IN ('erpx_managed', 'customer_data_plane')),
    write_database_ref TEXT NOT NULL,
    read_database_ref TEXT NOT NULL,
    storage_ref TEXT NOT NULL,
    directory_credential_ref TEXT,
    backup_profile_ref TEXT NOT NULL,
    profile_version BIGINT NOT NULL CHECK (profile_version > 0),
    updated_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (tenant_id, root_holding_id)
        REFERENCES erpx_control.customer_tenant_bindings (tenant_id, root_holding_id)
        ON DELETE RESTRICT,
    CHECK (write_database_ref <> read_database_ref),
    CHECK (length(btrim(write_database_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(read_database_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(storage_ref)) BETWEEN 1 AND 255),
    CHECK (directory_credential_ref IS NULL
        OR length(btrim(directory_credential_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(backup_profile_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(updated_by)) BETWEEN 1 AND 255),
    CHECK (
        (deployment_mode = 'erpx_managed'
            AND write_database_ref LIKE 'erpx-managed:%'
            AND read_database_ref LIKE 'erpx-managed:%'
            AND storage_ref LIKE 'erpx-managed:%'
            AND backup_profile_ref LIKE 'erpx-managed:%')
        OR
        (deployment_mode = 'customer_data_plane'
            AND write_database_ref LIKE 'customer-secret:%'
            AND read_database_ref LIKE 'customer-secret:%'
            AND storage_ref LIKE 'customer-secret:%'
            AND backup_profile_ref LIKE 'customer-secret:%')
    )
);

INSERT INTO erpx_control.tenant_infrastructure_profiles
    (tenant_id, root_holding_id, deployment_mode, write_database_ref,
     read_database_ref, storage_ref, backup_profile_ref, profile_version, updated_by)
SELECT binding.tenant_id,
       binding.root_holding_id,
       'erpx_managed',
       'erpx-managed:registry/' || md5(binding.tenant_id || ':write'),
       'erpx-managed:registry/' || md5(binding.tenant_id || ':read'),
       'erpx-managed:registry/' || md5(binding.tenant_id || ':storage'),
       'erpx-managed:registry/' || md5(binding.tenant_id || ':backup'),
       1,
       'migration:0070'
FROM erpx_control.customer_tenant_bindings AS binding
ON CONFLICT (tenant_id) DO NOTHING;

CREATE TABLE IF NOT EXISTS erpx_control.data_plane_agent_enrollments (
    enrollment_id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    root_holding_id UUID NOT NULL,
    token_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    manifest_hash BYTEA NOT NULL CHECK (octet_length(manifest_hash) = 32),
    issued_by TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    consumed_by_agent_id UUID,
    revoked_at TIMESTAMPTZ,
    correlation_id UUID NOT NULL,
    FOREIGN KEY (tenant_id, root_holding_id)
        REFERENCES erpx_control.customer_tenant_bindings (tenant_id, root_holding_id)
        ON DELETE RESTRICT,
    CHECK (expires_at > issued_at AND expires_at <= issued_at + INTERVAL '30 minutes'),
    CHECK (length(btrim(issued_by)) BETWEEN 1 AND 255),
    CHECK (consumed_at IS NULL OR consumed_by_agent_id IS NOT NULL),
    CHECK (NOT (consumed_at IS NOT NULL AND revoked_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS data_plane_agent_enrollment_active_idx
    ON erpx_control.data_plane_agent_enrollments (tenant_id, expires_at)
    WHERE consumed_at IS NULL AND revoked_at IS NULL;

CREATE TABLE IF NOT EXISTS erpx_control.data_plane_agents (
    agent_id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL UNIQUE,
    root_holding_id UUID NOT NULL,
    certificate_fingerprint CHAR(64) NOT NULL UNIQUE
        CHECK (certificate_fingerprint ~ '^[0-9A-Fa-f]{64}$'),
    capabilities TEXT[] NOT NULL CHECK (cardinality(capabilities) > 0),
    state TEXT NOT NULL CHECK (state IN (
        'unenrolled', 'enrolling', 'connected', 'healthy', 'drifted',
        'degraded', 'offline', 'suspended', 'upgrade_required'
    )),
    observed_version BIGINT NOT NULL DEFAULT 0 CHECK (observed_version >= 0),
    agent_version TEXT NOT NULL,
    certificate_expires_at TIMESTAMPTZ NOT NULL,
    enrolled_at TIMESTAMPTZ NOT NULL,
    last_heartbeat_at TIMESTAMPTZ,
    suspended_at TIMESTAMPTZ,
    FOREIGN KEY (tenant_id, root_holding_id)
        REFERENCES erpx_control.customer_tenant_bindings (tenant_id, root_holding_id)
        ON DELETE RESTRICT,
    CHECK (length(btrim(agent_version)) BETWEEN 1 AND 128),
    CHECK (certificate_expires_at > enrolled_at),
    CHECK ((state = 'suspended') = (suspended_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS data_plane_agents_health_idx
    ON erpx_control.data_plane_agents (state, last_heartbeat_at, tenant_id);

CREATE TABLE IF NOT EXISTS erpx_control.tenant_desired_configurations (
    configuration_id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    root_holding_id UUID NOT NULL,
    target_agent_id UUID,
    deployment_mode TEXT NOT NULL
        CHECK (deployment_mode IN ('erpx_managed', 'customer_data_plane')),
    configuration_layer TEXT NOT NULL
        CHECK (configuration_layer IN ('platform', 'tenant', 'business_unit')),
    business_unit TEXT,
    expected_observed_version BIGINT NOT NULL CHECK (expected_observed_version >= 0),
    desired_version BIGINT NOT NULL CHECK (desired_version > 0),
    lifecycle TEXT NOT NULL CHECK (lifecycle IN (
        'draft', 'awaiting_approval', 'approved', 'applying', 'verified',
        'active', 'rejected', 'drifted', 'failed', 'suspended'
    )),
    configuration_hash BYTEA NOT NULL CHECK (octet_length(configuration_hash) = 32),
    desired_state JSONB NOT NULL CHECK (jsonb_typeof(desired_state) = 'object'),
    secret_references TEXT[] NOT NULL DEFAULT '{}',
    required_capabilities TEXT[] NOT NULL DEFAULT '{}',
    requester TEXT NOT NULL,
    reason TEXT NOT NULL,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    customer_approver TEXT,
    customer_approved_at TIMESTAMPTZ,
    rejected_at TIMESTAMPTZ,
    applied_at TIMESTAMPTZ,
    correlation_id UUID NOT NULL,
    FOREIGN KEY (tenant_id, root_holding_id)
        REFERENCES erpx_control.customer_tenant_bindings (tenant_id, root_holding_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (target_agent_id)
        REFERENCES erpx_control.data_plane_agents(agent_id) ON DELETE RESTRICT,
    UNIQUE (tenant_id, desired_version),
    CHECK (desired_version = expected_observed_version + 1),
    CHECK ((configuration_layer = 'business_unit') = (business_unit IS NOT NULL)),
    CHECK (business_unit IS NULL OR business_unit IN (
        'operations', 'finance', 'sales', 'procurement', 'inventory', 'assets',
        'maintenance', 'quality', 'marketing-crm', 'hr-payroll', 'point-of-sale', 'legal'
    )),
    CHECK ((deployment_mode = 'customer_data_plane') = (target_agent_id IS NOT NULL)),
    CHECK (expires_at > requested_at),
    CHECK (length(btrim(requester)) BETWEEN 1 AND 255),
    CHECK (length(btrim(reason)) BETWEEN 1 AND 512),
    CHECK ((customer_approver IS NULL) = (customer_approved_at IS NULL)),
    CHECK (NOT (customer_approved_at IS NOT NULL AND rejected_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS tenant_desired_configuration_queue_idx
    ON erpx_control.tenant_desired_configurations
        (tenant_id, lifecycle, desired_version, requested_at);

CREATE TABLE IF NOT EXISTS erpx_control.data_plane_observations (
    agent_id UUID PRIMARY KEY
        REFERENCES erpx_control.data_plane_agents(agent_id) ON DELETE RESTRICT,
    tenant_id TEXT NOT NULL UNIQUE,
    observed_version BIGINT NOT NULL CHECK (observed_version >= 0),
    desired_version BIGINT NOT NULL CHECK (desired_version >= 0),
    write_schema_version TEXT NOT NULL,
    read_schema_version TEXT NOT NULL,
    write_healthy BOOLEAN NOT NULL,
    read_healthy BOOLEAN NOT NULL,
    storage_healthy BOOLEAN NOT NULL,
    directory_ready BOOLEAN NOT NULL,
    projections_healthy BOOLEAN NOT NULL,
    backup_verified BOOLEAN NOT NULL,
    failure_category TEXT,
    correlation_id UUID NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (tenant_id) REFERENCES erpx_control.customer_tenant_bindings(tenant_id)
        ON DELETE RESTRICT,
    CHECK (length(btrim(write_schema_version)) BETWEEN 1 AND 128),
    CHECK (length(btrim(read_schema_version)) BETWEEN 1 AND 128),
    CHECK (failure_category IS NULL OR length(btrim(failure_category)) BETWEEN 1 AND 255),
    CHECK (failure_category IS NULL OR lower(failure_category) !~ '(password|secret|token|credential)')
);

CREATE TABLE IF NOT EXISTS erpx_control.tenant_root_access_requests (
    request_id UUID PRIMARY KEY,
    requester_ref TEXT NOT NULL,
    device_ref TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    root_holding_id UUID NOT NULL,
    agent_id UUID NOT NULL,
    tenant_scope JSONB NOT NULL CHECK (jsonb_typeof(tenant_scope) = 'object'),
    mode TEXT NOT NULL CHECK (mode IN (
        'health_diagnostic', 'infrastructure_maintenance', 'business_diagnostic',
        'business_mutation', 'emergency_recovery'
    )),
    allowed_actions TEXT[] NOT NULL CHECK (cardinality(allowed_actions) BETWEEN 1 AND 64),
    business_units TEXT[] NOT NULL DEFAULT '{}',
    reason TEXT NOT NULL,
    ticket_reference TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'awaiting_customer_approval'
        CHECK (state IN ('awaiting_customer_approval', 'approved', 'rejected', 'active', 'revoked', 'expired')),
    requested_at TIMESTAMPTZ NOT NULL,
    requested_until TIMESTAMPTZ NOT NULL,
    customer_approver_ref TEXT,
    customer_approved_at TIMESTAMPTZ,
    customer_mfa_at TIMESTAMPTZ,
    rejected_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    revocation_reason TEXT,
    correlation_id UUID NOT NULL,
    FOREIGN KEY (tenant_id, root_holding_id)
        REFERENCES erpx_control.customer_tenant_bindings (tenant_id, root_holding_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (agent_id) REFERENCES erpx_control.data_plane_agents(agent_id)
        ON DELETE RESTRICT,
    CHECK (requested_until > requested_at),
    CHECK (length(btrim(requester_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(device_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(reason)) BETWEEN 1 AND 512),
    CHECK (length(btrim(ticket_reference)) BETWEEN 1 AND 255),
    CHECK (customer_approver_ref IS NULL OR customer_approver_ref <> requester_ref),
    CHECK ((customer_approver_ref IS NULL) = (customer_approved_at IS NULL)),
    CHECK (customer_approved_at IS NULL OR customer_mfa_at IS NOT NULL),
    CHECK (NOT (customer_approved_at IS NOT NULL AND rejected_at IS NOT NULL)),
    CHECK ((revoked_at IS NULL) = (revocation_reason IS NULL))
);

CREATE INDEX IF NOT EXISTS tenant_root_access_approval_queue_idx
    ON erpx_control.tenant_root_access_requests
        (tenant_id, state, requested_until, requested_at);

CREATE TABLE IF NOT EXISTS erpx_control.tenant_root_access_capabilities (
    capability_id UUID PRIMARY KEY,
    request_id UUID NOT NULL UNIQUE
        REFERENCES erpx_control.tenant_root_access_requests(request_id) ON DELETE RESTRICT,
    token_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    root_principal_ref TEXT NOT NULL,
    device_ref TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    agent_id UUID NOT NULL,
    tenant_scope JSONB NOT NULL CHECK (jsonb_typeof(tenant_scope) = 'object'),
    allowed_actions TEXT[] NOT NULL CHECK (cardinality(allowed_actions) BETWEEN 1 AND 64),
    business_units TEXT[] NOT NULL DEFAULT '{}',
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    CHECK (expires_at > issued_at),
    CHECK (length(btrim(root_principal_ref)) BETWEEN 1 AND 255),
    CHECK (length(btrim(device_ref)) BETWEEN 1 AND 255),
    FOREIGN KEY (tenant_id) REFERENCES erpx_control.customer_tenant_bindings(tenant_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (agent_id) REFERENCES erpx_control.data_plane_agents(agent_id)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS tenant_root_access_capability_active_idx
    ON erpx_control.tenant_root_access_capabilities
        (tenant_id, root_principal_ref, agent_id, expires_at)
    WHERE revoked_at IS NULL;

CREATE TABLE IF NOT EXISTS erpx_control.business_unit_configurations (
    tenant_id TEXT NOT NULL,
    root_holding_id UUID NOT NULL,
    business_unit TEXT NOT NULL CHECK (business_unit IN (
        'operations', 'finance', 'sales', 'procurement', 'inventory', 'assets',
        'maintenance', 'quality', 'marketing-crm', 'hr-payroll', 'point-of-sale', 'legal'
    )),
    configuration_version BIGINT NOT NULL CHECK (configuration_version > 0),
    active BOOLEAN NOT NULL,
    organization_scope JSONB NOT NULL CHECK (jsonb_typeof(organization_scope) = 'object'),
    directory_role_mappings JSONB NOT NULL DEFAULT '[]'::jsonb
        CHECK (jsonb_typeof(directory_role_mappings) = 'array'),
    integration_references TEXT[] NOT NULL DEFAULT '{}',
    retention_policy JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(retention_policy) = 'object'),
    projection_policy JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(projection_policy) = 'object'),
    workflow_policy JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(workflow_policy) = 'object'),
    updated_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, business_unit),
    FOREIGN KEY (tenant_id, root_holding_id)
        REFERENCES erpx_control.customer_tenant_bindings (tenant_id, root_holding_id)
        ON DELETE RESTRICT,
    CHECK (length(btrim(updated_by)) BETWEEN 1 AND 255)
);

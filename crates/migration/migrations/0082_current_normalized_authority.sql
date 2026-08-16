-- Typed relational authority for current control-plane aggregates. CQRS
-- receipts, audit and outbox rows remain durable facts, but aggregate JSON is
-- not a command rehydration source.

ALTER TABLE erpx_commercial_write.catalog_plan_guards
    ADD COLUMN display_name TEXT NOT NULL,
    ADD COLUMN amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    ADD COLUMN currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    ADD COLUMN created_at TIMESTAMPTZ NOT NULL,
    ADD CONSTRAINT commercial_catalog_plan_time CHECK (updated_at >= created_at);

ALTER TABLE erpx_platform_administration_write.tenant_guards
    ADD COLUMN created_at TIMESTAMPTZ NOT NULL,
    ADD CONSTRAINT platform_tenant_time CHECK (updated_at >= created_at);

ALTER TABLE erpx_platform_administration_write.connector_guards
    DROP COLUMN capabilities,
    ADD COLUMN scope_kind TEXT NOT NULL CHECK (scope_kind IN (
        'platform','holding','company','branch'
    )),
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID,
    ADD COLUMN connector_kind_type TEXT NOT NULL CHECK (connector_kind_type IN (
        'stripe','minio','n8n','ollama','other'
    )),
    ADD COLUMN connector_kind_other TEXT,
    ADD COLUMN readiness_evidence_id TEXT,
    ADD COLUMN readiness_configuration_digest BYTEA CHECK (
        readiness_configuration_digest IS NULL
        OR octet_length(readiness_configuration_digest) = 32
    ),
    ADD COLUMN readiness_ready BOOLEAN,
    ADD COLUMN readiness_failure_category TEXT,
    ADD COLUMN readiness_observed_at TIMESTAMPTZ,
    ADD COLUMN created_at TIMESTAMPTZ NOT NULL,
    ADD CONSTRAINT platform_connector_scope_shape CHECK (
        (scope_kind='platform' AND holding_id IS NULL AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_kind='holding' AND holding_id IS NOT NULL AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_kind='company' AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NULL)
        OR (scope_kind='branch' AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NOT NULL)
    ),
    ADD CONSTRAINT platform_connector_kind_shape CHECK (
        (connector_kind_type='other' AND connector_kind_other IS NOT NULL)
        OR (connector_kind_type<>'other' AND connector_kind_other IS NULL)
    ),
    ADD CONSTRAINT platform_connector_readiness_shape CHECK (
        (readiness_evidence_id IS NULL
            AND readiness_configuration_digest IS NULL
            AND readiness_ready IS NULL
            AND readiness_failure_category IS NULL
            AND readiness_observed_at IS NULL)
        OR (readiness_evidence_id IS NOT NULL
            AND readiness_configuration_digest IS NOT NULL
            AND readiness_ready IS NOT NULL
            AND readiness_observed_at IS NOT NULL
            AND (readiness_ready = (readiness_failure_category IS NULL)))
    ),
    ADD CONSTRAINT platform_connector_time CHECK (updated_at >= created_at);

CREATE TABLE erpx_platform_administration_write.connector_capabilities (
    connector_id UUID NOT NULL,
    capability TEXT NOT NULL,
    PRIMARY KEY (connector_id, capability),
    FOREIGN KEY (connector_id)
        REFERENCES erpx_platform_administration_write.connector_guards(connector_id)
        ON DELETE CASCADE
);

CREATE TABLE erpx_platform_administration_write.connector_readiness_capabilities (
    connector_id UUID NOT NULL,
    capability TEXT NOT NULL,
    PRIMARY KEY (connector_id, capability),
    FOREIGN KEY (connector_id)
        REFERENCES erpx_platform_administration_write.connector_guards(connector_id)
        ON DELETE CASCADE
);

ALTER TABLE erpx_platform_administration_write.admin_context_guards
    ADD COLUMN holding_id UUID NOT NULL;

ALTER TABLE erpx_customer_authority_write.account_guards
    ADD COLUMN trading_name TEXT NOT NULL,
    ADD COLUMN legal_name TEXT,
    ADD COLUMN registration_number TEXT,
    ADD COLUMN tax_identifier TEXT,
    ADD COLUMN country TEXT NOT NULL CHECK (country ~ '^[A-Z]{2}$'),
    ADD COLUMN created_at TIMESTAMPTZ NOT NULL,
    ADD CONSTRAINT customer_authority_account_time CHECK (updated_at >= created_at);

ALTER TABLE erpx_brand_write.profile_guards
    ADD COLUMN scope_kind TEXT NOT NULL CHECK (scope_kind IN (
        'platform','holding','business_unit'
    )),
    ADD COLUMN tenant_ref TEXT,
    ADD COLUMN holding_id UUID,
    ADD COLUMN business_unit TEXT CHECK (business_unit IN (
        'operations','finance','sales','procurement','inventory','assets',
        'maintenance','quality','marketing-crm','hr-payroll','point-of-sale','legal'
    )),
    ADD COLUMN created_at TIMESTAMPTZ NOT NULL,
    ADD CONSTRAINT brand_profile_scope_shape CHECK (
        (scope_kind='platform' AND tenant_ref IS NULL AND holding_id IS NULL AND business_unit IS NULL)
        OR (scope_kind='holding' AND tenant_ref IS NOT NULL AND holding_id IS NOT NULL AND business_unit IS NULL)
        OR (scope_kind='business_unit' AND tenant_ref IS NOT NULL AND holding_id IS NOT NULL AND business_unit IS NOT NULL)
    ),
    ADD CONSTRAINT brand_profile_time CHECK (updated_at >= created_at);

ALTER TABLE erpx_brand_write.profile_revisions
    DROP COLUMN definition,
    ADD COLUMN updated_at TIMESTAMPTZ NOT NULL,
    ADD CONSTRAINT brand_revision_time CHECK (updated_at >= created_at);

CREATE TABLE erpx_brand_write.profile_revision_tokens (
    profile_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    display_name TEXT NOT NULL,
    short_name TEXT NOT NULL,
    light_primary TEXT NOT NULL, light_on_primary TEXT NOT NULL,
    light_accent TEXT NOT NULL, light_on_accent TEXT NOT NULL,
    light_canvas TEXT NOT NULL, light_surface TEXT NOT NULL,
    light_surface_raised TEXT NOT NULL, light_border TEXT NOT NULL,
    light_text TEXT NOT NULL, light_text_muted TEXT NOT NULL,
    light_link TEXT NOT NULL, light_focus TEXT NOT NULL,
    dark_primary TEXT NOT NULL, dark_on_primary TEXT NOT NULL,
    dark_accent TEXT NOT NULL, dark_on_accent TEXT NOT NULL,
    dark_canvas TEXT NOT NULL, dark_surface TEXT NOT NULL,
    dark_surface_raised TEXT NOT NULL, dark_border TEXT NOT NULL,
    dark_text TEXT NOT NULL, dark_text_muted TEXT NOT NULL,
    dark_link TEXT NOT NULL, dark_focus TEXT NOT NULL,
    font_pack TEXT NOT NULL CHECK (font_pack IN (
        'erpx_sans','corporate_sans','thai_formal','system_ui'
    )),
    heading_scale_percent INTEGER NOT NULL CHECK (heading_scale_percent BETWEEN 90 AND 125),
    body_scale_percent INTEGER NOT NULL CHECK (body_scale_percent BETWEEN 90 AND 120),
    line_height_percent INTEGER NOT NULL CHECK (line_height_percent BETWEEN 120 AND 180),
    density TEXT NOT NULL CHECK (density IN ('compact','comfortable','spacious')),
    radius TEXT NOT NULL CHECK (radius IN ('square','soft','rounded')),
    elevation TEXT NOT NULL CHECK (elevation IN ('flat','subtle','elevated')),
    navigation TEXT NOT NULL CHECK (navigation IN ('solid','tinted')),
    navigation_expanded_by_default BOOLEAN NOT NULL,
    motion TEXT NOT NULL CHECK (motion IN ('none','subtle','standard')),
    PRIMARY KEY (profile_id, revision),
    FOREIGN KEY (profile_id, revision)
        REFERENCES erpx_brand_write.profile_revisions(profile_id, revision)
        ON DELETE CASCADE
);

CREATE TABLE erpx_brand_write.profile_revision_categories (
    profile_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    category TEXT NOT NULL CHECK (category IN (
        'identity','color','typography','shape','elevation','density','navigation','motion'
    )),
    category_role TEXT NOT NULL CHECK (category_role IN ('overridden','locked')),
    PRIMARY KEY (profile_id, revision, category, category_role),
    FOREIGN KEY (profile_id, revision)
        REFERENCES erpx_brand_write.profile_revisions(profile_id, revision)
        ON DELETE CASCADE
);

CREATE TABLE erpx_brand_write.profile_revision_assets (
    profile_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    asset_kind TEXT NOT NULL CHECK (asset_kind IN (
        'logo_mark','wordmark','favicon','login_artwork'
    )),
    object_key TEXT NOT NULL,
    sha256 TEXT NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    media_type TEXT NOT NULL,
    byte_length BIGINT NOT NULL CHECK (byte_length > 0),
    width BIGINT CHECK (width BETWEEN 1 AND 4096),
    height BIGINT CHECK (height BETWEEN 1 AND 4096),
    PRIMARY KEY (profile_id, revision, asset_kind),
    FOREIGN KEY (profile_id, revision)
        REFERENCES erpx_brand_write.profile_revisions(profile_id, revision)
        ON DELETE CASCADE
);

-- PLATFORM_COMPLETION_TYPED_AUTHORITY_BEGIN
-- Active platform-completion commands use the existing tenant registry,
-- backup-job and projection-switch rows as their typed lifecycle authority.
-- Reconciliation and connector roots receive dedicated scalar/child tables.
-- No serialized aggregate document is retained or consulted.
ALTER TABLE erpx_control.tenant_registry
    ADD COLUMN lifecycle_aggregate_version BIGINT
        CHECK (lifecycle_aggregate_version IS NULL OR lifecycle_aggregate_version > 0);

ALTER TABLE erpx_control.tenant_backup_jobs
    ADD COLUMN aggregate_version BIGINT NOT NULL DEFAULT 1
        CHECK (aggregate_version > 0),
    ADD CONSTRAINT tenant_backup_jobs_tenant_holding_fk
        FOREIGN KEY (tenant_id, holding_ref)
        REFERENCES erpx_control.tenant_registry(tenant_id, holding_ref)
        ON DELETE RESTRICT;

ALTER TABLE erpx_control.tenant_projection_switches
    ADD COLUMN aggregate_version BIGINT NOT NULL DEFAULT 1
        CHECK (aggregate_version > 0);

CREATE SCHEMA erpx_platform_completion_write;

CREATE TABLE erpx_platform_completion_write.reconciliation_authority (
    aggregate_id UUID PRIMARY KEY,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    report_id UUID NOT NULL UNIQUE,
    tenant_id TEXT NOT NULL,
    holding_ref TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    reconciliation_state TEXT NOT NULL CHECK (
        reconciliation_state IN ('healthy','drifted','unavailable')
    ),
    registry_state TEXT NOT NULL CHECK (
        registry_state IN ('provisioning','active','suspended','recovery')
    ),
    provisioning_state TEXT NOT NULL CHECK (
        provisioning_state IN ('missing','provisioning','active','suspended','recovery','failed')
    ),
    write_healthy BOOLEAN NOT NULL,
    read_healthy BOOLEAN NOT NULL,
    storage_healthy BOOLEAN NOT NULL,
    replacement_read_state TEXT CHECK (
        replacement_read_state IS NULL OR replacement_read_state IN (
            'rebuilding','ready','failed','activated'
        )
    ),
    latest_backup_verified BOOLEAN NOT NULL,
    connector_count INTEGER NOT NULL CHECK (connector_count >= 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (tenant_id),
    FOREIGN KEY (tenant_id, holding_ref)
        REFERENCES erpx_control.tenant_registry(tenant_id, holding_ref)
        ON DELETE RESTRICT,
    CHECK (updated_at >= observed_at)
);

CREATE TABLE erpx_platform_completion_write.reconciliation_drift_codes (
    aggregate_id UUID NOT NULL,
    drift_code TEXT NOT NULL CHECK (octet_length(drift_code) BETWEEN 1 AND 120),
    PRIMARY KEY (aggregate_id, drift_code),
    FOREIGN KEY (aggregate_id)
        REFERENCES erpx_platform_completion_write.reconciliation_authority(aggregate_id)
        ON DELETE CASCADE
);

CREATE TABLE erpx_platform_completion_write.connector_authority (
    connector_id UUID PRIMARY KEY,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    connector_version BIGINT NOT NULL CHECK (connector_version > 0),
    connector_kind TEXT NOT NULL CHECK (connector_kind IN (
        'stripe','marketing_provider','finance_bank','generic_ldap',
        'active_directory','univention_corporate_server'
    )),
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('platform','holding')),
    tenant_id TEXT,
    holding_ref TEXT,
    provider_key TEXT NOT NULL CHECK (octet_length(provider_key) BETWEEN 1 AND 120),
    credential_ref TEXT NOT NULL CHECK (
        octet_length(credential_ref) BETWEEN 1 AND 200
        AND position('://' IN credential_ref)=0
        AND position('=' IN credential_ref)=0
    ),
    configuration_digest TEXT NOT NULL CHECK (configuration_digest ~ '^[0-9a-f]{64}$'),
    connector_state TEXT NOT NULL CHECK (
        connector_state IN ('pending','active','suspended','revoked')
    ),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (connector_id, connector_version),
    FOREIGN KEY (tenant_id, holding_ref)
        REFERENCES erpx_control.tenant_registry(tenant_id, holding_ref)
        ON DELETE RESTRICT,
    CHECK (
        (scope_kind='platform' AND tenant_id IS NULL AND holding_ref IS NULL)
        OR (scope_kind='holding' AND tenant_id IS NOT NULL AND holding_ref IS NOT NULL)
    ),
    CHECK (connector_version=aggregate_version)
);

CREATE INDEX platform_completion_connector_scope
    ON erpx_platform_completion_write.connector_authority
        (scope_kind, tenant_id, holding_ref, connector_state, connector_id);

CREATE TABLE erpx_platform_completion_write.connector_capabilities (
    connector_id UUID NOT NULL,
    capability TEXT NOT NULL CHECK (octet_length(capability) BETWEEN 1 AND 120),
    PRIMARY KEY (connector_id, capability),
    FOREIGN KEY (connector_id)
        REFERENCES erpx_platform_completion_write.connector_authority(connector_id)
        ON DELETE CASCADE
);
-- PLATFORM_COMPLETION_TYPED_AUTHORITY_END

-- TENANT_BACKUP_JOB_TYPED_VERSION_AUTHORITY
-- The typed tenant_backup_jobs row owns lifecycle state and version. Outbox
-- payloads are evidence only; no generic control aggregate snapshot remains.
ALTER TABLE erpx_control.tenant_backup_jobs
    ADD COLUMN IF NOT EXISTS aggregate_version BIGINT NOT NULL DEFAULT 0
        CHECK (aggregate_version >= 0);

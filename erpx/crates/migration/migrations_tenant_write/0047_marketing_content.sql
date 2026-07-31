-- Authoritative Marketing content and digital-experience write state.
-- Metadata and opaque object references live here; MinIO bytes never do.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_assets (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    asset_id UUID NOT NULL,
    scope JSONB NOT NULL,
    name TEXT NOT NULL CHECK (btrim(name) <> '' AND octet_length(name) <= 255),
    latest_version BIGINT NOT NULL CHECK (latest_version > 0),
    state JSONB NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    last_command_id UUID NOT NULL,
    last_idempotency_key TEXT NOT NULL CHECK (btrim(last_idempotency_key) <> '' AND octet_length(last_idempotency_key) <= 255),
    actor JSONB NOT NULL,
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_ref, asset_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_asset_versions (
    holding_ref TEXT NOT NULL,
    asset_id UUID NOT NULL,
    asset_version_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    scope JSONB NOT NULL,
    object_ref JSONB NOT NULL,
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^[0-9A-Fa-f]{64}$'),
    byte_length BIGINT NOT NULL CHECK (byte_length > 0 AND byte_length <= 536870912),
    media_type TEXT NOT NULL CHECK (btrim(media_type) <> '' AND octet_length(media_type) <= 128),
    created_by TEXT NOT NULL CHECK (btrim(created_by) <> '' AND octet_length(created_by) <= 255),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_ref, asset_id, version),
    UNIQUE (holding_ref, asset_version_id),
    FOREIGN KEY (holding_ref, asset_id)
        REFERENCES erpx_tenant.marketing_content_assets (holding_ref, asset_id)
        ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_templates (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    template_id UUID NOT NULL,
    scope JSONB NOT NULL,
    slug TEXT NOT NULL CHECK (btrim(slug) <> '' AND octet_length(slug) <= 128),
    status TEXT NOT NULL CHECK (status IN ('draft', 'in_review', 'approved', 'scheduled', 'published', 'rolled_back', 'archived')),
    latest_version BIGINT NOT NULL CHECK (latest_version > 0),
    approved_version BIGINT CHECK (approved_version IS NULL OR approved_version > 0),
    published_version BIGINT CHECK (published_version IS NULL OR published_version > 0),
    rolled_back_from BIGINT CHECK (rolled_back_from IS NULL OR rolled_back_from > 0),
    state JSONB NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    last_command_id UUID NOT NULL,
    last_idempotency_key TEXT NOT NULL CHECK (btrim(last_idempotency_key) <> '' AND octet_length(last_idempotency_key) <= 255),
    actor JSONB NOT NULL,
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_ref, template_id),
    UNIQUE (holding_ref, slug)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_template_versions (
    holding_ref TEXT NOT NULL,
    template_id UUID NOT NULL,
    template_version_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    scope JSONB NOT NULL,
    state JSONB NOT NULL,
    created_by TEXT NOT NULL CHECK (btrim(created_by) <> '' AND octet_length(created_by) <= 255),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_ref, template_id, version),
    UNIQUE (holding_ref, template_version_id),
    FOREIGN KEY (holding_ref, template_id)
        REFERENCES erpx_tenant.marketing_content_templates (holding_ref, template_id)
        ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_previews (
    holding_ref TEXT NOT NULL,
    preview_id UUID NOT NULL,
    template_id UUID NOT NULL,
    template_version BIGINT NOT NULL CHECK (template_version > 0),
    scope JSONB NOT NULL,
    token_hash TEXT NOT NULL CHECK (token_hash ~ '^[0-9A-Fa-f]{64}$'),
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    PRIMARY KEY (holding_ref, preview_id),
    FOREIGN KEY (holding_ref, template_id, template_version)
        REFERENCES erpx_tenant.marketing_content_template_versions (holding_ref, template_id, version)
        ON DELETE RESTRICT,
    CHECK (expires_at > issued_at),
    CHECK (revoked_at IS NULL OR revoked_at >= issued_at)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_forms (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    form_id UUID NOT NULL,
    scope JSONB NOT NULL,
    slug TEXT NOT NULL CHECK (btrim(slug) <> '' AND octet_length(slug) <= 128),
    status TEXT NOT NULL CHECK (status IN ('draft', 'in_review', 'published', 'archived')),
    state JSONB NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    last_command_id UUID NOT NULL,
    last_idempotency_key TEXT NOT NULL CHECK (btrim(last_idempotency_key) <> '' AND octet_length(last_idempotency_key) <= 255),
    actor JSONB NOT NULL,
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_ref, form_id),
    UNIQUE (holding_ref, slug)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_form_submissions (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    submission_id UUID NOT NULL,
    form_id UUID NOT NULL,
    scope JSONB NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 255),
    values_json JSONB NOT NULL,
    received_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_ref, submission_id),
    UNIQUE (holding_ref, form_id, idempotency_key),
    FOREIGN KEY (holding_ref, form_id)
        REFERENCES erpx_tenant.marketing_content_forms (holding_ref, form_id)
        ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_domains (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    domain_id UUID NOT NULL,
    scope JSONB NOT NULL,
    hostname TEXT NOT NULL CHECK (btrim(hostname) <> '' AND octet_length(hostname) <= 253),
    status TEXT NOT NULL CHECK (status IN ('pending_verification', 'active', 'suspended')),
    verification_hash TEXT CHECK (verification_hash IS NULL OR verification_hash ~ '^[0-9A-Fa-f]{64}$'),
    state JSONB NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    last_command_id UUID NOT NULL,
    last_idempotency_key TEXT NOT NULL CHECK (btrim(last_idempotency_key) <> '' AND octet_length(last_idempotency_key) <= 255),
    actor JSONB NOT NULL,
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_ref, domain_id),
    UNIQUE (holding_ref, hostname)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_domain_bindings (
    holding_ref TEXT NOT NULL,
    domain_id UUID NOT NULL,
    template_id UUID NOT NULL,
    template_version BIGINT NOT NULL CHECK (template_version > 0),
    scope JSONB NOT NULL,
    bound_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_ref, domain_id),
    FOREIGN KEY (holding_ref, domain_id)
        REFERENCES erpx_tenant.marketing_content_domains (holding_ref, domain_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (holding_ref, template_id, template_version)
        REFERENCES erpx_tenant.marketing_content_template_versions (holding_ref, template_id, version)
        ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_approvals (
    holding_ref TEXT NOT NULL,
    approval_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('template', 'form', 'domain')),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    approver TEXT NOT NULL CHECK (btrim(approver) <> '' AND octet_length(approver) <= 255),
    approved_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_ref, approval_id),
    UNIQUE (holding_ref, aggregate_type, aggregate_id, aggregate_version)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_command_idempotency (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 255),
    operation TEXT NOT NULL CHECK (btrim(operation) <> '' AND octet_length(operation) <= 64),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> '' AND octet_length(aggregate_type) <= 64),
    aggregate_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    event_id UUID NOT NULL,
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, idempotency_key)
);

CREATE INDEX IF NOT EXISTS marketing_content_assets_scope_idx
    ON erpx_tenant.marketing_content_assets (holding_ref, updated_at DESC, asset_id);
CREATE INDEX IF NOT EXISTS marketing_content_templates_status_idx
    ON erpx_tenant.marketing_content_templates (holding_ref, status, updated_at DESC, template_id);
CREATE INDEX IF NOT EXISTS marketing_content_forms_status_idx
    ON erpx_tenant.marketing_content_forms (holding_ref, status, updated_at DESC, form_id);
CREATE INDEX IF NOT EXISTS marketing_content_domains_status_idx
    ON erpx_tenant.marketing_content_domains (holding_ref, status, hostname);
CREATE INDEX IF NOT EXISTS marketing_content_submissions_form_idx
    ON erpx_tenant.marketing_content_form_submissions (holding_ref, form_id, received_at DESC);

ALTER TABLE erpx_tenant.marketing_content_assets ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_assets FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_asset_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_asset_versions FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_templates ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_templates FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_template_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_template_versions FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_previews ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_previews FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_forms ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_forms FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_form_submissions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_form_submissions FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_domains ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_domains FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_domain_bindings ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_domain_bindings FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_approvals ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_approvals FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_command_idempotency FORCE ROW LEVEL SECURITY;

DO $$
DECLARE
    table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'marketing_content_assets', 'marketing_content_asset_versions',
        'marketing_content_templates', 'marketing_content_template_versions',
        'marketing_content_previews', 'marketing_content_forms',
        'marketing_content_form_submissions', 'marketing_content_domains',
        'marketing_content_domain_bindings', 'marketing_content_approvals',
        'marketing_content_command_idempotency'
    ] LOOP
        EXECUTE format('DROP POLICY IF EXISTS tenant_scope_%s ON erpx_tenant.%I', table_name, table_name);
        EXECUTE format(
            'CREATE POLICY tenant_scope_%s ON erpx_tenant.%I USING (holding_ref = current_setting(''erpx.holding_ref'', true)) WITH CHECK (holding_ref = current_setting(''erpx.holding_ref'', true))',
            table_name, table_name
        );
    END LOOP;
END $$;

-- Version records, approvals, and source events are immutable audit facts.
CREATE OR REPLACE FUNCTION erpx_tenant.reject_marketing_content_fact_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Marketing content version or approval facts are append-only';
END;
$$;

DROP TRIGGER IF EXISTS marketing_content_asset_version_append_only ON erpx_tenant.marketing_content_asset_versions;
CREATE TRIGGER marketing_content_asset_version_append_only
    BEFORE UPDATE OR DELETE ON erpx_tenant.marketing_content_asset_versions
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_marketing_content_fact_mutation();
DROP TRIGGER IF EXISTS marketing_content_template_version_append_only ON erpx_tenant.marketing_content_template_versions;
CREATE TRIGGER marketing_content_template_version_append_only
    BEFORE UPDATE OR DELETE ON erpx_tenant.marketing_content_template_versions
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_marketing_content_fact_mutation();
DROP TRIGGER IF EXISTS marketing_content_approval_append_only ON erpx_tenant.marketing_content_approvals;
CREATE TRIGGER marketing_content_approval_append_only
    BEFORE UPDATE OR DELETE ON erpx_tenant.marketing_content_approvals
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_marketing_content_fact_mutation();

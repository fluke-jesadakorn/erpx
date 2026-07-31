-- Rebuildable Marketing workflow read models. Raw preview token and domain
-- verification hashes are deliberately excluded; form values remain tenant
-- scoped and bounded for authorized CRM/support reads.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_preview_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    preview_id UUID NOT NULL,
    template_id UUID NOT NULL,
    template_version BIGINT NOT NULL CHECK (template_version > 0),
    scope JSONB NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_actor JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, preview_id),
    CHECK (expires_at > issued_at)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_submission_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    submission_id UUID NOT NULL,
    form_id UUID NOT NULL,
    scope JSONB NOT NULL,
    values_json JSONB NOT NULL CHECK (octet_length(values_json::text) <= 2097152),
    received_at TIMESTAMPTZ NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_actor JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, submission_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_domain_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> '' AND octet_length(holding_ref) <= 255),
    domain_id UUID NOT NULL,
    scope JSONB NOT NULL,
    hostname TEXT NOT NULL CHECK (btrim(hostname) <> '' AND octet_length(hostname) <= 253),
    status TEXT NOT NULL CHECK (status IN ('pending_verification', 'active', 'suspended')),
    binding_template_id UUID,
    binding_template_version BIGINT CHECK (binding_template_version IS NULL OR binding_template_version > 0),
    bound_at TIMESTAMPTZ,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_actor JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, domain_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_workflow_projection_checkpoints (
    holding_ref TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'marketing_content_workflow'),
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('preview', 'form_submission', 'domain')),
    aggregate_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    last_event_id UUID NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, projection_name, aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.marketing_content_workflow_projection_event_receipts (
    holding_ref TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'marketing_content_workflow'),
    event_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('preview', 'form_submission', 'domain')),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, projection_name, event_id)
);

CREATE INDEX IF NOT EXISTS marketing_content_preview_read_expiry_idx
    ON erpx_tenant.marketing_content_preview_read_rows (holding_ref, expires_at, revoked_at);
CREATE INDEX IF NOT EXISTS marketing_content_submission_read_form_idx
    ON erpx_tenant.marketing_content_submission_read_rows (holding_ref, form_id, received_at DESC);
CREATE INDEX IF NOT EXISTS marketing_content_domain_read_status_idx
    ON erpx_tenant.marketing_content_domain_read_rows (holding_ref, status, hostname);

ALTER TABLE erpx_tenant.marketing_content_preview_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_preview_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_submission_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_submission_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_domain_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_domain_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_workflow_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_workflow_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_workflow_projection_event_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.marketing_content_workflow_projection_event_receipts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_marketing_content_preview_read_rows ON erpx_tenant.marketing_content_preview_read_rows;
CREATE POLICY tenant_scope_marketing_content_preview_read_rows
    ON erpx_tenant.marketing_content_preview_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_content_submission_read_rows ON erpx_tenant.marketing_content_submission_read_rows;
CREATE POLICY tenant_scope_marketing_content_submission_read_rows
    ON erpx_tenant.marketing_content_submission_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_content_domain_read_rows ON erpx_tenant.marketing_content_domain_read_rows;
CREATE POLICY tenant_scope_marketing_content_domain_read_rows
    ON erpx_tenant.marketing_content_domain_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_content_workflow_projection_checkpoints ON erpx_tenant.marketing_content_workflow_projection_checkpoints;
CREATE POLICY tenant_scope_marketing_content_workflow_projection_checkpoints
    ON erpx_tenant.marketing_content_workflow_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_marketing_content_workflow_projection_event_receipts ON erpx_tenant.marketing_content_workflow_projection_event_receipts;
CREATE POLICY tenant_scope_marketing_content_workflow_projection_event_receipts
    ON erpx_tenant.marketing_content_workflow_projection_event_receipts
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

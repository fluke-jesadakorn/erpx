-- Rebuildable CRM read projection. CRM write rows in tenant_write/0036 remain
-- authoritative; this schema is disposable and rebuilt from CRM outbox rows.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.crm_accepted_lead_handoff_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    handoff_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    scope JSONB NOT NULL,
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    account_id TEXT NOT NULL CHECK (btrim(account_id) <> ''),
    contact_id TEXT NOT NULL CHECK (btrim(contact_id) <> ''),
    accepted_at TIMESTAMPTZ NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, handoff_id),
    UNIQUE (holding_ref, reference)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.crm_opportunity_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    opportunity_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    scope JSONB NOT NULL,
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    accepted_lead_reference TEXT NOT NULL CHECK (btrim(accepted_lead_reference) <> ''),
    account_id TEXT NOT NULL CHECK (btrim(account_id) <> ''),
    contact_id TEXT NOT NULL CHECK (btrim(contact_id) <> ''),
    name TEXT NOT NULL CHECK (btrim(name) <> ''),
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    expected_revenue_minor BIGINT NOT NULL CHECK (expected_revenue_minor >= 0),
    status TEXT NOT NULL CHECK (status IN ('draft', 'qualified', 'won', 'lost')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, opportunity_id),
    UNIQUE (holding_ref, reference)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.crm_attribution_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    record_id UUID NOT NULL,
    opportunity_id UUID NOT NULL,
    model TEXT NOT NULL CHECK (model IN ('first_touch', 'last_touch', 'linear', 'u_shaped', 'weighted')),
    source_opportunity_version BIGINT NOT NULL CHECK (source_opportunity_version > 0),
    total_revenue_minor BIGINT NOT NULL CHECK (total_revenue_minor >= 0),
    total_cost_minor BIGINT NOT NULL CHECK (total_cost_minor >= 0),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, record_id),
    UNIQUE (holding_ref, opportunity_id, model)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.crm_projection_event_receipts (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    event_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN (
        'marketing_crm_accepted_lead_handoff', 'marketing_crm_opportunity',
        'marketing_crm_attribution'
    )),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_type TEXT NOT NULL CHECK (btrim(event_type) <> ''),
    payload_hash TEXT NOT NULL CHECK (length(payload_hash) = 64),
    occurred_at TIMESTAMPTZ NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, event_id),
    UNIQUE (holding_ref, aggregate_type, aggregate_id, aggregate_version)
);

CREATE INDEX IF NOT EXISTS crm_handoff_read_scope_idx
    ON erpx_tenant.crm_accepted_lead_handoff_read_rows
       (holding_ref, holding_id, company_id, branch_id, accepted_at DESC);
CREATE INDEX IF NOT EXISTS crm_opportunity_read_scope_idx
    ON erpx_tenant.crm_opportunity_read_rows
       (holding_ref, holding_id, company_id, branch_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS crm_attribution_read_opportunity_idx
    ON erpx_tenant.crm_attribution_read_rows
       (holding_ref, opportunity_id, projected_at DESC);
CREATE INDEX IF NOT EXISTS crm_projection_receipts_aggregate_idx
    ON erpx_tenant.crm_projection_event_receipts
       (holding_ref, aggregate_type, aggregate_id, aggregate_version);

ALTER TABLE erpx_tenant.crm_accepted_lead_handoff_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_accepted_lead_handoff_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_opportunity_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_opportunity_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_attribution_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_attribution_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_projection_event_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_projection_event_receipts FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_scope_crm_handoff_read_rows
    ON erpx_tenant.crm_accepted_lead_handoff_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
CREATE POLICY tenant_scope_crm_opportunity_read_rows
    ON erpx_tenant.crm_opportunity_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
CREATE POLICY tenant_scope_crm_attribution_read_rows
    ON erpx_tenant.crm_attribution_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
CREATE POLICY tenant_scope_crm_projection_event_receipts
    ON erpx_tenant.crm_projection_event_receipts
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

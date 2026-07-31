-- Marketing-to-CRM persistence boundary.
--
-- Marketing owns acquisition and qualification.  These tables only accept
-- the immutable handoff references and the CRM-owned opportunity lifecycle.
-- The tenant write route plus the transaction-local erpx.holding_ref setting
-- are the isolation boundary; the same rows are never used as a read model.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.crm_idempotency_bindings (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> ''),
    aggregate_id UUID NOT NULL,
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, idempotency_key),
    UNIQUE (holding_ref, aggregate_type, aggregate_id, idempotency_key),
    UNIQUE (holding_ref, aggregate_type, reference, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.crm_accepted_lead_handoffs (
    handoff_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    scope JSONB NOT NULL,
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    command_id UUID NOT NULL,
    source JSONB NOT NULL,
    account_id TEXT NOT NULL CHECK (btrim(account_id) <> ''),
    account_scope JSONB NOT NULL,
    contact_id TEXT NOT NULL CHECK (btrim(contact_id) <> ''),
    contact_scope JSONB NOT NULL,
    accepted_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL CHECK (version = 1),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, handoff_id),
    UNIQUE (holding_ref, reference),
    UNIQUE (holding_ref, handoff_id, reference),
    UNIQUE (holding_ref, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.crm_opportunities (
    opportunity_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    scope JSONB NOT NULL,
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    command_id UUID NOT NULL,
    accepted_lead_handoff_id UUID NOT NULL,
    accepted_lead_reference TEXT NOT NULL CHECK (btrim(accepted_lead_reference) <> ''),
    account_id TEXT NOT NULL CHECK (btrim(account_id) <> ''),
    account_scope JSONB NOT NULL,
    contact_id TEXT NOT NULL CHECK (btrim(contact_id) <> ''),
    contact_scope JSONB NOT NULL,
    name TEXT NOT NULL CHECK (btrim(name) <> ''),
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    expected_revenue_minor BIGINT NOT NULL CHECK (expected_revenue_minor >= 0),
    status TEXT NOT NULL CHECK (status IN ('draft', 'qualified', 'won', 'lost')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    attribution_hash TEXT NOT NULL CHECK (length(attribution_hash) = 64),
    payload JSONB NOT NULL,
    PRIMARY KEY (holding_ref, opportunity_id),
    UNIQUE (holding_ref, opportunity_id, reference),
    UNIQUE (holding_ref, reference),
    UNIQUE (holding_ref, idempotency_key),
    FOREIGN KEY (holding_ref, accepted_lead_handoff_id, accepted_lead_reference)
        REFERENCES erpx_tenant.crm_accepted_lead_handoffs
            (holding_ref, handoff_id, reference)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.crm_opportunity_attribution_inputs (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    opportunity_id UUID NOT NULL,
    touch_id TEXT NOT NULL CHECK (btrim(touch_id) <> ''),
    source_id TEXT NOT NULL CHECK (btrim(source_id) <> ''),
    campaign_id TEXT,
    scope JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    cost_minor BIGINT NOT NULL CHECK (cost_minor >= 0),
    weight_bps INTEGER CHECK (weight_bps BETWEEN 0 AND 10000),
    PRIMARY KEY (holding_ref, opportunity_id, touch_id),
    FOREIGN KEY (holding_ref, opportunity_id)
        REFERENCES erpx_tenant.crm_opportunities (holding_ref, opportunity_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.crm_attribution_records (
    record_id UUID NOT NULL,
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    opportunity_id UUID NOT NULL,
    model TEXT NOT NULL CHECK (model IN (
        'first_touch', 'last_touch', 'linear', 'u_shaped', 'weighted'
    )),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    source_opportunity_version BIGINT NOT NULL CHECK (source_opportunity_version > 0),
    version BIGINT NOT NULL CHECK (version = 1),
    total_revenue_minor BIGINT NOT NULL CHECK (total_revenue_minor >= 0),
    total_cost_minor BIGINT NOT NULL CHECK (total_cost_minor >= 0),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, record_id),
    UNIQUE (holding_ref, opportunity_id, model),
    UNIQUE (holding_ref, idempotency_key),
    FOREIGN KEY (holding_ref, opportunity_id)
        REFERENCES erpx_tenant.crm_opportunities (holding_ref, opportunity_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.crm_attribution_entries (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    record_id UUID NOT NULL,
    touch_id TEXT NOT NULL CHECK (btrim(touch_id) <> ''),
    source_id TEXT NOT NULL CHECK (btrim(source_id) <> ''),
    campaign_id TEXT,
    revenue_minor BIGINT NOT NULL CHECK (revenue_minor >= 0),
    cost_minor BIGINT NOT NULL CHECK (cost_minor >= 0),
    PRIMARY KEY (holding_ref, record_id, touch_id),
    FOREIGN KEY (holding_ref, record_id)
        REFERENCES erpx_tenant.crm_attribution_records (holding_ref, record_id)
);

CREATE INDEX IF NOT EXISTS crm_handoffs_scope_idx
    ON erpx_tenant.crm_accepted_lead_handoffs
        (holding_ref, holding_id, company_id, branch_id, accepted_at DESC);
CREATE INDEX IF NOT EXISTS crm_opportunities_scope_idx
    ON erpx_tenant.crm_opportunities
        (holding_ref, holding_id, company_id, branch_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS crm_attribution_opportunity_idx
    ON erpx_tenant.crm_attribution_records
        (holding_ref, opportunity_id, created_at DESC);

ALTER TABLE erpx_tenant.crm_idempotency_bindings ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_idempotency_bindings FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_accepted_lead_handoffs ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_accepted_lead_handoffs FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_opportunities ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_opportunities FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_opportunity_attribution_inputs ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_opportunity_attribution_inputs FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_attribution_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_attribution_records FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_attribution_entries ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.crm_attribution_entries FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_crm_idempotency_bindings
    ON erpx_tenant.crm_idempotency_bindings;
CREATE POLICY tenant_scope_crm_idempotency_bindings
    ON erpx_tenant.crm_idempotency_bindings
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_crm_accepted_lead_handoffs
    ON erpx_tenant.crm_accepted_lead_handoffs;
CREATE POLICY tenant_scope_crm_accepted_lead_handoffs
    ON erpx_tenant.crm_accepted_lead_handoffs
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_crm_opportunities ON erpx_tenant.crm_opportunities;
CREATE POLICY tenant_scope_crm_opportunities
    ON erpx_tenant.crm_opportunities
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_crm_opportunity_attribution_inputs
    ON erpx_tenant.crm_opportunity_attribution_inputs;
CREATE POLICY tenant_scope_crm_opportunity_attribution_inputs
    ON erpx_tenant.crm_opportunity_attribution_inputs
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_crm_attribution_records
    ON erpx_tenant.crm_attribution_records;
CREATE POLICY tenant_scope_crm_attribution_records
    ON erpx_tenant.crm_attribution_records
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_crm_attribution_entries
    ON erpx_tenant.crm_attribution_entries;
CREATE POLICY tenant_scope_crm_attribution_entries
    ON erpx_tenant.crm_attribution_entries
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

CREATE OR REPLACE FUNCTION erpx_tenant.reject_crm_immutable_row_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION '% rows are immutable', TG_TABLE_NAME;
END;
$$;

DROP TRIGGER IF EXISTS crm_handoffs_immutable
    ON erpx_tenant.crm_accepted_lead_handoffs;
CREATE TRIGGER crm_handoffs_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.crm_accepted_lead_handoffs
    FOR EACH ROW
    EXECUTE FUNCTION erpx_tenant.reject_crm_immutable_row_mutation();

DROP TRIGGER IF EXISTS crm_attribution_inputs_immutable
    ON erpx_tenant.crm_opportunity_attribution_inputs;
CREATE TRIGGER crm_attribution_inputs_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.crm_opportunity_attribution_inputs
    FOR EACH ROW
    EXECUTE FUNCTION erpx_tenant.reject_crm_immutable_row_mutation();

DROP TRIGGER IF EXISTS crm_attribution_records_immutable
    ON erpx_tenant.crm_attribution_records;
CREATE TRIGGER crm_attribution_records_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.crm_attribution_records
    FOR EACH ROW
    EXECUTE FUNCTION erpx_tenant.reject_crm_immutable_row_mutation();

DROP TRIGGER IF EXISTS crm_attribution_entries_immutable
    ON erpx_tenant.crm_attribution_entries;
CREATE TRIGGER crm_attribution_entries_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.crm_attribution_entries
    FOR EACH ROW
    EXECUTE FUNCTION erpx_tenant.reject_crm_immutable_row_mutation();

DROP TRIGGER IF EXISTS crm_idempotency_bindings_immutable
    ON erpx_tenant.crm_idempotency_bindings;
CREATE TRIGGER crm_idempotency_bindings_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.crm_idempotency_bindings
    FOR EACH ROW
    EXECUTE FUNCTION erpx_tenant.reject_crm_immutable_row_mutation();

CREATE OR REPLACE FUNCTION erpx_tenant.reject_crm_opportunity_binding_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.holding_ref IS DISTINCT FROM NEW.holding_ref
       OR OLD.holding_id IS DISTINCT FROM NEW.holding_id
       OR OLD.company_id IS DISTINCT FROM NEW.company_id
       OR OLD.branch_id IS DISTINCT FROM NEW.branch_id
       OR OLD.scope IS DISTINCT FROM NEW.scope
       OR OLD.reference IS DISTINCT FROM NEW.reference
       OR OLD.accepted_lead_handoff_id IS DISTINCT FROM NEW.accepted_lead_handoff_id
       OR OLD.accepted_lead_reference IS DISTINCT FROM NEW.accepted_lead_reference
       OR OLD.account_id IS DISTINCT FROM NEW.account_id
       OR OLD.account_scope IS DISTINCT FROM NEW.account_scope
       OR OLD.contact_id IS DISTINCT FROM NEW.contact_id
       OR OLD.contact_scope IS DISTINCT FROM NEW.contact_scope
       OR OLD.name IS DISTINCT FROM NEW.name
       OR OLD.currency IS DISTINCT FROM NEW.currency
       OR OLD.expected_revenue_minor IS DISTINCT FROM NEW.expected_revenue_minor
       OR OLD.created_at IS DISTINCT FROM NEW.created_at
       OR OLD.attribution_hash IS DISTINCT FROM NEW.attribution_hash
    THEN
        RAISE EXCEPTION 'CRM opportunity source and commercial bindings are immutable';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS crm_opportunities_binding_immutable
    ON erpx_tenant.crm_opportunities;
CREATE TRIGGER crm_opportunities_binding_immutable
    BEFORE UPDATE ON erpx_tenant.crm_opportunities
    FOR EACH ROW
    EXECUTE FUNCTION erpx_tenant.reject_crm_opportunity_binding_mutation();

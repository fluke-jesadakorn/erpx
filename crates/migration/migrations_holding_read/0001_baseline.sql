
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.projection_checkpoints (
    projection_name TEXT PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    last_event_id UUID,
    source_version BIGINT NOT NULL DEFAULT 0 CHECK (source_version >= 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_holding.customer_summary (
    customer_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE erpx_holding.projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.customer_summary ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.customer_summary FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_projection_checkpoints ON erpx_holding.projection_checkpoints;
CREATE POLICY holding_scope_projection_checkpoints ON erpx_holding.projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_customer_summary ON erpx_holding.customer_summary;
CREATE POLICY holding_scope_customer_summary ON erpx_holding.customer_summary
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Operations read models are rebuildable and live only in the holding read
-- database. They do not contain credentials or control-plane metadata.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.operations_projection_checkpoints (
    projection_name TEXT NOT NULL CHECK (btrim(projection_name) <> ''),
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> ''),
    aggregate_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    last_event_id UUID,
    last_aggregate_version BIGINT NOT NULL CHECK (last_aggregate_version >= 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.operations_read_rows (
    projection_name TEXT NOT NULL CHECK (btrim(projection_name) <> ''),
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> ''),
    aggregate_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    lifecycle_status TEXT NOT NULL CHECK (btrim(lifecycle_status) <> ''),
    state JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (projection_name, aggregate_type, aggregate_id)
);

CREATE INDEX IF NOT EXISTS operations_projection_checkpoints_holding_idx
    ON erpx_holding.operations_projection_checkpoints
    (data_profile_key, projection_name, projected_at);
CREATE INDEX IF NOT EXISTS operations_read_rows_holding_idx
    ON erpx_holding.operations_read_rows
    (data_profile_key, projection_name, updated_at);

ALTER TABLE erpx_holding.operations_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.operations_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.operations_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.operations_read_rows FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_operations_projection_checkpoints
    ON erpx_holding.operations_projection_checkpoints;
CREATE POLICY holding_scope_operations_projection_checkpoints
    ON erpx_holding.operations_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_operations_read_rows ON erpx_holding.operations_read_rows;
CREATE POLICY holding_scope_operations_read_rows
    ON erpx_holding.operations_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Finance read state is a disposable projection of immutable posted journals.
-- It is intentionally separate from the holding write tables and is rebuilt by
-- the worker from the write-side journal/outbox history.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.finance_posted_journals (
    journal_id UUID PRIMARY KEY,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    period_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    posted_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS finance_read_journals_period_idx
    ON erpx_holding.finance_posted_journals
       (data_profile_key, company_id, period_id, posted_at, journal_id);

ALTER TABLE erpx_holding.finance_posted_journals ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_posted_journals FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_read_journals
    ON erpx_holding.finance_posted_journals;
CREATE POLICY holding_scope_finance_read_journals
    ON erpx_holding.finance_posted_journals
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Marketing read-side projection. It is disposable and must be rebuilt from
-- holding write state; no command may use these rows as its source of truth.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_profiles (
    profile_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    scope JSONB NOT NULL,
    external_ref TEXT NOT NULL,
    dedupe_key TEXT NOT NULL,
    email TEXT,
    phone TEXT,
    locale TEXT NOT NULL,
    attributes JSONB NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_consents (
    consent_id UUID PRIMARY KEY,
    profile_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    scope JSONB NOT NULL,
    purpose TEXT NOT NULL,
    channel TEXT NOT NULL,
    state TEXT NOT NULL,
    source TEXT NOT NULL,
    jurisdiction TEXT NOT NULL,
    policy_version TEXT NOT NULL,
    proof_ref TEXT,
    recorded_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_suppressions (
    suppression_id UUID PRIMARY KEY,
    profile_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    scope JSONB NOT NULL,
    reason TEXT NOT NULL,
    source TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_audiences (
    audience_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    scope JSONB NOT NULL,
    materialized_at TIMESTAMPTZ NOT NULL,
    profile_count BIGINT NOT NULL CHECK (profile_count >= 0),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_audience_memberships (
    audience_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    materialized_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (audience_id, profile_id)
);

CREATE INDEX IF NOT EXISTS marketing_read_profiles_external_ref_idx
    ON erpx_holding.marketing_profiles (data_profile_key, external_ref);
CREATE INDEX IF NOT EXISTS marketing_read_consents_profile_idx
    ON erpx_holding.marketing_consents (data_profile_key, profile_id, recorded_at DESC);
CREATE INDEX IF NOT EXISTS marketing_read_suppressions_profile_idx
    ON erpx_holding.marketing_suppressions (data_profile_key, profile_id, created_at DESC);
CREATE INDEX IF NOT EXISTS marketing_read_memberships_page_idx
    ON erpx_holding.marketing_audience_memberships
       (data_profile_key, audience_id, profile_id);

ALTER TABLE erpx_holding.marketing_profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_profiles FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_consents ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_consents FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_suppressions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_suppressions FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_audiences ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_audiences FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_audience_memberships ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_audience_memberships FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_read_profiles ON erpx_holding.marketing_profiles;
CREATE POLICY holding_scope_marketing_read_profiles ON erpx_holding.marketing_profiles
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_read_consents ON erpx_holding.marketing_consents;
CREATE POLICY holding_scope_marketing_read_consents ON erpx_holding.marketing_consents
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_read_suppressions ON erpx_holding.marketing_suppressions;
CREATE POLICY holding_scope_marketing_read_suppressions ON erpx_holding.marketing_suppressions
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_read_audiences ON erpx_holding.marketing_audiences;
CREATE POLICY holding_scope_marketing_read_audiences ON erpx_holding.marketing_audiences
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_read_memberships ON erpx_holding.marketing_audience_memberships;
CREATE POLICY holding_scope_marketing_read_memberships ON erpx_holding.marketing_audience_memberships
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Sales read state is disposable and rebuilt from the holding write aggregates
-- and outbox. It is never a command authority.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.sales_aggregates (
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.sales_projection_checkpoints (
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE INDEX IF NOT EXISTS sales_read_aggregates_scope_idx
    ON erpx_holding.sales_aggregates (data_profile_key, company_id, branch_id, aggregate_type, reference);

ALTER TABLE erpx_holding.sales_aggregates ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.sales_aggregates FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.sales_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.sales_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_sales_read_aggregates ON erpx_holding.sales_aggregates;
CREATE POLICY holding_scope_sales_read_aggregates ON erpx_holding.sales_aggregates
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_sales_projection_checkpoints
    ON erpx_holding.sales_projection_checkpoints;
CREATE POLICY holding_scope_sales_projection_checkpoints
    ON erpx_holding.sales_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- SCM read state is disposable and is rebuilt from the source outbox.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.scm_aggregates (
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN (
        'product', 'warehouse', 'purchase_request', 'purchase_order',
        'goods_receipt', 'stock_movement', 'stock_transfer', 'stock_reservation'
    )),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.scm_projection_checkpoints (
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE INDEX IF NOT EXISTS scm_read_aggregates_scope_idx
    ON erpx_holding.scm_aggregates
       (data_profile_key, company_id, branch_id, aggregate_type, reference);

ALTER TABLE erpx_holding.scm_aggregates ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.scm_aggregates FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.scm_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.scm_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_scm_read_aggregates ON erpx_holding.scm_aggregates;
CREATE POLICY holding_scope_scm_read_aggregates ON erpx_holding.scm_aggregates
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_scm_projection_checkpoints
    ON erpx_holding.scm_projection_checkpoints;
CREATE POLICY holding_scope_scm_projection_checkpoints
    ON erpx_holding.scm_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Disposable Asset and Maintenance projections for one routed holding.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.asset_maintenance_read_rows (
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('asset', 'maintenance_work_order')),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.asset_maintenance_projection_checkpoints (
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.asset_custody_transfer_read_rows (
    asset_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    transfer JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (asset_id, aggregate_version)
);

CREATE TABLE IF NOT EXISTS erpx_holding.asset_capacity_handoff_read_rows (
    source_event_id UUID PRIMARY KEY,
    source_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS asset_maintenance_read_scope_idx
    ON erpx_holding.asset_maintenance_read_rows
        (data_profile_key, company_id, branch_id, aggregate_type, reference);
CREATE INDEX IF NOT EXISTS asset_custody_read_scope_idx
    ON erpx_holding.asset_custody_transfer_read_rows (data_profile_key, asset_id, aggregate_version);
CREATE INDEX IF NOT EXISTS asset_capacity_handoff_read_scope_idx
    ON erpx_holding.asset_capacity_handoff_read_rows
        (data_profile_key, company_id, branch_id, projected_at DESC);

ALTER TABLE erpx_holding.asset_maintenance_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.asset_maintenance_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.asset_maintenance_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.asset_maintenance_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.asset_custody_transfer_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.asset_custody_transfer_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.asset_capacity_handoff_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.asset_capacity_handoff_read_rows FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_asset_maintenance_read_rows
    ON erpx_holding.asset_maintenance_read_rows;
CREATE POLICY holding_scope_asset_maintenance_read_rows
    ON erpx_holding.asset_maintenance_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_asset_maintenance_checkpoints
    ON erpx_holding.asset_maintenance_projection_checkpoints;
CREATE POLICY holding_scope_asset_maintenance_checkpoints
    ON erpx_holding.asset_maintenance_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_asset_custody_read_rows
    ON erpx_holding.asset_custody_transfer_read_rows;
CREATE POLICY holding_scope_asset_custody_read_rows
    ON erpx_holding.asset_custody_transfer_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_asset_capacity_handoff_read_rows
    ON erpx_holding.asset_capacity_handoff_read_rows;
CREATE POLICY holding_scope_asset_capacity_handoff_read_rows
    ON erpx_holding.asset_capacity_handoff_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Durable checkpoints for rebuilding the disposable Finance journal read
-- model from the holding write outbox.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.finance_projection_checkpoints (
    journal_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS finance_projection_checkpoints_scope_idx
    ON erpx_holding.finance_projection_checkpoints
        (data_profile_key, source_version, journal_id);

ALTER TABLE erpx_holding.finance_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_projection_checkpoints
    ON erpx_holding.finance_projection_checkpoints;
CREATE POLICY holding_scope_finance_projection_checkpoints
    ON erpx_holding.finance_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Disposable Finance AP/AR and cash-control projection for one routed holding.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.finance_subledger_read_rows (
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN (
        'payable_invoice', 'receivable_invoice', 'vendor_payment',
        'customer_payment', 'allocation', 'cash_receipt',
        'cash_disbursement', 'cash_reconciliation',
        'vendor_subledger_entry', 'customer_subledger_entry'
    )),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    period_id UUID NOT NULL,
    accounting_date DATE NOT NULL,
    reference TEXT NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_subledger_projection_checkpoints (
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE INDEX IF NOT EXISTS finance_subledger_read_scope_idx
    ON erpx_holding.finance_subledger_read_rows
        (data_profile_key, company_id, branch_id, aggregate_type, accounting_date DESC);

ALTER TABLE erpx_holding.finance_subledger_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_subledger_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_subledger_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_subledger_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_subledger_read_rows
    ON erpx_holding.finance_subledger_read_rows;
CREATE POLICY holding_scope_finance_subledger_read_rows
    ON erpx_holding.finance_subledger_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_finance_subledger_checkpoints
    ON erpx_holding.finance_subledger_projection_checkpoints;
CREATE POLICY holding_scope_finance_subledger_checkpoints
    ON erpx_holding.finance_subledger_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Durable checkpoints for rebuilding the disposable Marketing read model
-- from the holding write outbox.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_projection_checkpoints (
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN (
        'marketing_profile', 'marketing_consent',
        'marketing_suppression', 'marketing_audience'
    )),
    aggregate_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE INDEX IF NOT EXISTS marketing_projection_checkpoints_scope_idx
    ON erpx_holding.marketing_projection_checkpoints
       (data_profile_key, aggregate_type, source_version, aggregate_id);

ALTER TABLE erpx_holding.marketing_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_projection_checkpoints
    ON erpx_holding.marketing_projection_checkpoints;
CREATE POLICY holding_scope_marketing_projection_checkpoints
    ON erpx_holding.marketing_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Disposable Marketing delivery read model. The write-side dispatch table
-- remains authoritative; workers rebuild these rows from delivery events.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_delivery_read_rows (
    delivery_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    profile_id UUID NOT NULL,
    campaign_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    state TEXT NOT NULL CHECK (state IN (
        'queued', 'sending', 'retry_scheduled', 'provider_accepted',
        'delivered', 'suppressed', 'failed_terminal'
    )),
    attempt_count INTEGER NOT NULL CHECK (attempt_count >= 0),
    next_attempt_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS marketing_delivery_read_due_idx
    ON erpx_holding.marketing_delivery_read_rows
       (data_profile_key, state, next_attempt_at, delivery_id);
CREATE INDEX IF NOT EXISTS marketing_delivery_read_profile_idx
    ON erpx_holding.marketing_delivery_read_rows
       (data_profile_key, profile_id, projected_at DESC, delivery_id);

ALTER TABLE erpx_holding.marketing_delivery_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_delivery_read_rows FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_delivery_read_rows
    ON erpx_holding.marketing_delivery_read_rows;
CREATE POLICY holding_scope_marketing_delivery_read_rows
    ON erpx_holding.marketing_delivery_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Disposable Finance banking read model for one routed holding.
-- Source rows remain authoritative in holding_write/0028_finance_banking.sql.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.finance_banking_read_rows (
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN (
        'finance_bank_account',
        'finance_bank_statement_import',
        'finance_bank_reconciliation'
    )),
    aggregate_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_banking_projection_checkpoints (
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN (
        'finance_bank_account',
        'finance_bank_statement_import',
        'finance_bank_reconciliation'
    )),
    aggregate_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE INDEX IF NOT EXISTS finance_banking_read_rows_scope_idx
    ON erpx_holding.finance_banking_read_rows
       (data_profile_key, company_id, aggregate_type, source_version, aggregate_id);

CREATE INDEX IF NOT EXISTS finance_banking_projection_checkpoints_scope_idx
    ON erpx_holding.finance_banking_projection_checkpoints
       (data_profile_key, aggregate_type, source_version, aggregate_id);

ALTER TABLE erpx_holding.finance_banking_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_banking_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_banking_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_banking_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_banking_read_rows
    ON erpx_holding.finance_banking_read_rows;
CREATE POLICY holding_scope_finance_banking_read_rows
    ON erpx_holding.finance_banking_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_finance_banking_projection_checkpoints
    ON erpx_holding.finance_banking_projection_checkpoints;
CREATE POLICY holding_scope_finance_banking_projection_checkpoints
    ON erpx_holding.finance_banking_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Rebuildable CRM read projection. CRM write rows in holding_write/0036 remain
-- authoritative; this schema is disposable and rebuilt from CRM outbox rows.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.crm_accepted_lead_handoff_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
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
    PRIMARY KEY (data_profile_key, handoff_id),
    UNIQUE (data_profile_key, reference)
);

CREATE TABLE IF NOT EXISTS erpx_holding.crm_opportunity_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
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
    PRIMARY KEY (data_profile_key, opportunity_id),
    UNIQUE (data_profile_key, reference)
);

CREATE TABLE IF NOT EXISTS erpx_holding.crm_attribution_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
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
    PRIMARY KEY (data_profile_key, record_id),
    UNIQUE (data_profile_key, opportunity_id, model)
);

CREATE TABLE IF NOT EXISTS erpx_holding.crm_projection_event_receipts (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
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
    PRIMARY KEY (data_profile_key, event_id),
    UNIQUE (data_profile_key, aggregate_type, aggregate_id, aggregate_version)
);

CREATE INDEX IF NOT EXISTS crm_handoff_read_scope_idx
    ON erpx_holding.crm_accepted_lead_handoff_read_rows
       (data_profile_key, holding_id, company_id, branch_id, accepted_at DESC);
CREATE INDEX IF NOT EXISTS crm_opportunity_read_scope_idx
    ON erpx_holding.crm_opportunity_read_rows
       (data_profile_key, holding_id, company_id, branch_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS crm_attribution_read_opportunity_idx
    ON erpx_holding.crm_attribution_read_rows
       (data_profile_key, opportunity_id, projected_at DESC);
CREATE INDEX IF NOT EXISTS crm_projection_receipts_aggregate_idx
    ON erpx_holding.crm_projection_event_receipts
       (data_profile_key, aggregate_type, aggregate_id, aggregate_version);

ALTER TABLE erpx_holding.crm_accepted_lead_handoff_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_accepted_lead_handoff_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_opportunity_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_opportunity_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_attribution_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_attribution_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_projection_event_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_projection_event_receipts FORCE ROW LEVEL SECURITY;

CREATE POLICY holding_scope_crm_handoff_read_rows
    ON erpx_holding.crm_accepted_lead_handoff_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
CREATE POLICY holding_scope_crm_opportunity_read_rows
    ON erpx_holding.crm_opportunity_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
CREATE POLICY holding_scope_crm_attribution_read_rows
    ON erpx_holding.crm_attribution_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
CREATE POLICY holding_scope_crm_projection_event_receipts
    ON erpx_holding.crm_projection_event_receipts
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Disposable Finance close-review read model for one routed holding.
--
-- The close-review aggregate and control evidence remain authoritative on the
-- holding write database. These tables contain only bounded, typed projection
-- values so the holding read database can be rebuilt without exposing the
-- aggregate payload or command context.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.finance_close_review_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    period_id UUID NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('open', 'review', 'closed')),
    idempotency_key TEXT NOT NULL CHECK (
        btrim(idempotency_key) <> '' AND char_length(idempotency_key) <= 255
    ),
    opened_by TEXT NOT NULL CHECK (
        btrim(opened_by) <> '' AND char_length(opened_by) <= 255
    ),
    reviewed_by TEXT CHECK (
        reviewed_by IS NULL
        OR (btrim(reviewed_by) <> '' AND char_length(reviewed_by) <= 255)
    ),
    closed_by TEXT CHECK (
        closed_by IS NULL
        OR (btrim(closed_by) <> '' AND char_length(closed_by) <= 255)
    ),
    statement_present BOOLEAN NOT NULL DEFAULT FALSE,
    statement_debit_minor BIGINT,
    statement_credit_minor BIGINT,
    statement_net_minor BIGINT,
    statement_balanced BOOLEAN,
    control_total_count INTEGER NOT NULL DEFAULT 0 CHECK (
        control_total_count BETWEEN 0 AND 4096
    ),
    control_resolved_count INTEGER NOT NULL DEFAULT 0 CHECK (
        control_resolved_count BETWEEN 0 AND 4096
    ),
    control_unresolved_count INTEGER NOT NULL DEFAULT 0 CHECK (
        control_unresolved_count BETWEEN 0 AND 4096
    ),
    expected_fact_count INTEGER NOT NULL DEFAULT 0 CHECK (
        expected_fact_count BETWEEN 0 AND 4096
    ),
    observed_fact_count INTEGER NOT NULL DEFAULT 0 CHECK (
        observed_fact_count BETWEEN 0 AND 4096
    ),
    missing_fact_count INTEGER NOT NULL DEFAULT 0 CHECK (
        missing_fact_count BETWEEN 0 AND 4096
    ),
    account_summary_count INTEGER NOT NULL DEFAULT 0 CHECK (
        account_summary_count BETWEEN 0 AND 8192
    ),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    review_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, period_id),
    UNIQUE (data_profile_key, idempotency_key),
    CHECK (
        (statement_present
            AND statement_debit_minor IS NOT NULL
            AND statement_credit_minor IS NOT NULL
            AND statement_net_minor IS NOT NULL
            AND statement_balanced IS NOT NULL
            AND statement_debit_minor >= 0
            AND statement_credit_minor >= 0
            AND statement_net_minor = statement_debit_minor - statement_credit_minor)
        OR
        (NOT statement_present
            AND statement_debit_minor IS NULL
            AND statement_credit_minor IS NULL
            AND statement_net_minor IS NULL
            AND statement_balanced IS NULL)
    ),
    CHECK (control_total_count = control_resolved_count + control_unresolved_count),
    CHECK (expected_fact_count = observed_fact_count + missing_fact_count)
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_close_account_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    period_id UUID NOT NULL,
    account_id UUID NOT NULL,
    debit_minor BIGINT NOT NULL CHECK (debit_minor >= 0),
    credit_minor BIGINT NOT NULL CHECK (credit_minor >= 0),
    net_minor BIGINT NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, period_id, account_id),
    CHECK (net_minor = debit_minor - credit_minor)
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_close_projection_checkpoints (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    period_id UUID NOT NULL,
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, period_id)
);

CREATE INDEX IF NOT EXISTS finance_close_review_read_scope_idx
    ON erpx_holding.finance_close_review_read_rows
       (data_profile_key, holding_id, company_id, status, projected_at DESC, period_id);

CREATE INDEX IF NOT EXISTS finance_close_account_read_scope_idx
    ON erpx_holding.finance_close_account_read_rows
       (data_profile_key, holding_id, company_id, period_id, account_id);

CREATE INDEX IF NOT EXISTS finance_close_projection_checkpoints_scope_idx
    ON erpx_holding.finance_close_projection_checkpoints
       (data_profile_key, holding_id, company_id, source_version, period_id);

ALTER TABLE erpx_holding.finance_close_review_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_close_review_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_close_account_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_close_account_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_close_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_close_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_close_review_read_rows
    ON erpx_holding.finance_close_review_read_rows;
CREATE POLICY holding_scope_finance_close_review_read_rows
    ON erpx_holding.finance_close_review_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_finance_close_account_read_rows
    ON erpx_holding.finance_close_account_read_rows;
CREATE POLICY holding_scope_finance_close_account_read_rows
    ON erpx_holding.finance_close_account_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_finance_close_projection_checkpoints
    ON erpx_holding.finance_close_projection_checkpoints;
CREATE POLICY holding_scope_finance_close_projection_checkpoints
    ON erpx_holding.finance_close_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Rebuildable HR and payroll read models for one routed holding.
--
-- The holding-write HR aggregates remain authoritative. These tables contain
-- bounded typed values and source event metadata only; workers may truncate and
-- rebuild them from the holding-write outbox without copying aggregate JSON.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.hr_employee_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    employee_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    employee_ref TEXT NOT NULL CHECK (
        btrim(employee_ref) <> '' AND char_length(employee_ref) <= 128
    ),
    display_name TEXT NOT NULL CHECK (
        btrim(display_name) <> '' AND char_length(display_name) <= 255
    ),
    status TEXT NOT NULL CHECK (status IN (
        'draft', 'active', 'on_leave', 'suspended', 'terminated'
    )),
    hired_on DATE NOT NULL,
    terminated_on DATE,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, employee_id),
    UNIQUE (data_profile_key, employee_ref),
    CHECK (status <> 'terminated' OR terminated_on IS NOT NULL),
    CHECK (status = 'terminated' OR terminated_on IS NULL)
);

CREATE TABLE IF NOT EXISTS erpx_holding.hr_leave_request_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    leave_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    employee_id UUID NOT NULL,
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    reason TEXT NOT NULL CHECK (
        btrim(reason) <> '' AND char_length(reason) <= 1024
    ),
    status TEXT NOT NULL CHECK (status IN (
        'draft', 'submitted', 'approved', 'rejected', 'cancelled', 'taken'
    )),
    requested_by TEXT NOT NULL CHECK (
        btrim(requested_by) <> '' AND char_length(requested_by) <= 255
    ),
    approved_by TEXT CHECK (
        approved_by IS NULL
        OR (btrim(approved_by) <> '' AND char_length(approved_by) <= 255)
    ),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, leave_id),
    CHECK (ends_on >= starts_on),
    CHECK (status <> 'approved' OR approved_by IS NOT NULL),
    CHECK (status NOT IN ('draft', 'submitted') OR approved_by IS NULL)
);

CREATE TABLE IF NOT EXISTS erpx_holding.hr_expense_claim_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    expense_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    employee_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (
        btrim(currency) <> ''
        AND char_length(currency) = 3
        AND currency = upper(currency)
    ),
    amount_minor BIGINT NOT NULL CHECK (amount_minor >= 0),
    status TEXT NOT NULL CHECK (status IN (
        'draft', 'submitted', 'approved', 'rejected', 'reimbursed'
    )),
    submitted_by TEXT NOT NULL CHECK (
        btrim(submitted_by) <> '' AND char_length(submitted_by) <= 255
    ),
    approved_by TEXT CHECK (
        approved_by IS NULL
        OR (btrim(approved_by) <> '' AND char_length(approved_by) <= 255)
    ),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, expense_id),
    CHECK (status NOT IN ('approved', 'reimbursed') OR approved_by IS NOT NULL),
    CHECK (status IN ('draft', 'submitted', 'rejected') OR amount_minor >= 0)
);

CREATE TABLE IF NOT EXISTS erpx_holding.hr_payroll_run_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    payroll_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    status TEXT NOT NULL CHECK (status IN (
        'draft', 'review', 'approved', 'posted', 'cancelled'
    )),
    prepared_by TEXT NOT NULL CHECK (
        btrim(prepared_by) <> '' AND char_length(prepared_by) <= 255
    ),
    reviewed_by TEXT CHECK (
        reviewed_by IS NULL
        OR (btrim(reviewed_by) <> '' AND char_length(reviewed_by) <= 255)
    ),
    approved_by TEXT CHECK (
        approved_by IS NULL
        OR (btrim(approved_by) <> '' AND char_length(approved_by) <= 255)
    ),
    total_gross_minor BIGINT NOT NULL CHECK (total_gross_minor >= 0),
    total_deductions_minor BIGINT NOT NULL CHECK (total_deductions_minor >= 0),
    total_net_minor BIGINT NOT NULL CHECK (total_net_minor >= 0),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, payroll_id),
    CHECK (ends_on >= starts_on),
    CHECK (total_gross_minor - total_deductions_minor = total_net_minor),
    CHECK (status <> 'posted' OR approved_by IS NOT NULL),
    CHECK (status NOT IN ('review', 'approved', 'posted') OR reviewed_by IS NOT NULL)
);

CREATE TABLE IF NOT EXISTS erpx_holding.hr_payroll_line_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    payroll_id UUID NOT NULL,
    line_no INTEGER NOT NULL CHECK (line_no BETWEEN 0 AND 100000),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    employee_id UUID NOT NULL,
    gross_minor BIGINT NOT NULL CHECK (gross_minor >= 0),
    deductions_minor BIGINT NOT NULL CHECK (
        deductions_minor >= 0 AND deductions_minor <= gross_minor
    ),
    net_minor BIGINT NOT NULL CHECK (net_minor = gross_minor - deductions_minor),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, payroll_id, line_no)
);

CREATE TABLE IF NOT EXISTS erpx_holding.hr_expired_role_reassignment_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    workflow_ref TEXT NOT NULL CHECK (
        btrim(workflow_ref) <> '' AND char_length(workflow_ref) <= 128
    ),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    expired_principal_ref TEXT NOT NULL CHECK (
        btrim(expired_principal_ref) <> '' AND char_length(expired_principal_ref) <= 128
    ),
    replacement_principal_ref TEXT NOT NULL CHECK (
        btrim(replacement_principal_ref) <> '' AND char_length(replacement_principal_ref) <= 128
    ),
    reason TEXT NOT NULL CHECK (
        btrim(reason) <> '' AND char_length(reason) <= 1024
    ),
    reassigned_at TIMESTAMPTZ NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, workflow_ref),
    CHECK (expired_principal_ref <> replacement_principal_ref)
);

CREATE TABLE IF NOT EXISTS erpx_holding.hr_projection_checkpoints (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    projection_name TEXT NOT NULL CHECK (
        btrim(projection_name) <> '' AND char_length(projection_name) <= 120
    ),
    aggregate_type TEXT NOT NULL CHECK (
        btrim(aggregate_type) <> '' AND char_length(aggregate_type) <= 120
    ),
    aggregate_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, projection_name, aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.hr_projection_event_receipts (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    projection_name TEXT NOT NULL CHECK (
        btrim(projection_name) <> '' AND char_length(projection_name) <= 120
    ),
    event_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (
        btrim(aggregate_type) <> '' AND char_length(aggregate_type) <= 120
    ),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, projection_name, event_id)
);

CREATE INDEX IF NOT EXISTS hr_employee_read_scope_idx
    ON erpx_holding.hr_employee_read_rows
       (data_profile_key, holding_id, company_id, branch_id, status, projected_at DESC);
CREATE INDEX IF NOT EXISTS hr_leave_request_read_scope_idx
    ON erpx_holding.hr_leave_request_read_rows
       (data_profile_key, holding_id, company_id, branch_id, status, starts_on);
CREATE INDEX IF NOT EXISTS hr_expense_claim_read_scope_idx
    ON erpx_holding.hr_expense_claim_read_rows
       (data_profile_key, holding_id, company_id, branch_id, status, projected_at DESC);
CREATE INDEX IF NOT EXISTS hr_payroll_run_read_scope_idx
    ON erpx_holding.hr_payroll_run_read_rows
       (data_profile_key, holding_id, company_id, branch_id, status, starts_on);
CREATE INDEX IF NOT EXISTS hr_payroll_line_read_employee_idx
    ON erpx_holding.hr_payroll_line_read_rows
       (data_profile_key, holding_id, company_id, branch_id, employee_id, payroll_id, line_no);
CREATE INDEX IF NOT EXISTS hr_expired_role_reassignment_read_scope_idx
    ON erpx_holding.hr_expired_role_reassignment_read_rows
       (data_profile_key, holding_id, company_id, branch_id, reassigned_at DESC);
CREATE INDEX IF NOT EXISTS hr_projection_checkpoints_scope_idx
    ON erpx_holding.hr_projection_checkpoints
       (data_profile_key, projection_name, aggregate_type, source_version, projected_at DESC);
CREATE INDEX IF NOT EXISTS hr_projection_event_receipts_aggregate_idx
    ON erpx_holding.hr_projection_event_receipts
       (data_profile_key, projection_name, aggregate_type, aggregate_id, aggregate_version);

ALTER TABLE erpx_holding.hr_employee_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_employee_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_leave_request_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_leave_request_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_expense_claim_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_expense_claim_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_payroll_run_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_payroll_run_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_payroll_line_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_payroll_line_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_expired_role_reassignment_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_expired_role_reassignment_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_projection_event_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_projection_event_receipts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_hr_employee_read_rows
    ON erpx_holding.hr_employee_read_rows;
CREATE POLICY holding_scope_hr_employee_read_rows
    ON erpx_holding.hr_employee_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_hr_leave_request_read_rows
    ON erpx_holding.hr_leave_request_read_rows;
CREATE POLICY holding_scope_hr_leave_request_read_rows
    ON erpx_holding.hr_leave_request_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_hr_expense_claim_read_rows
    ON erpx_holding.hr_expense_claim_read_rows;
CREATE POLICY holding_scope_hr_expense_claim_read_rows
    ON erpx_holding.hr_expense_claim_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_hr_payroll_run_read_rows
    ON erpx_holding.hr_payroll_run_read_rows;
CREATE POLICY holding_scope_hr_payroll_run_read_rows
    ON erpx_holding.hr_payroll_run_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_hr_payroll_line_read_rows
    ON erpx_holding.hr_payroll_line_read_rows;
CREATE POLICY holding_scope_hr_payroll_line_read_rows
    ON erpx_holding.hr_payroll_line_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_hr_expired_role_reassignment_read_rows
    ON erpx_holding.hr_expired_role_reassignment_read_rows;
CREATE POLICY holding_scope_hr_expired_role_reassignment_read_rows
    ON erpx_holding.hr_expired_role_reassignment_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_hr_projection_checkpoints
    ON erpx_holding.hr_projection_checkpoints;
CREATE POLICY holding_scope_hr_projection_checkpoints
    ON erpx_holding.hr_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_hr_projection_event_receipts
    ON erpx_holding.hr_projection_event_receipts;
CREATE POLICY holding_scope_hr_projection_event_receipts
    ON erpx_holding.hr_projection_event_receipts
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Rebuildable typed POS read model for one physically isolated holding.
--
-- The holding-write POS tables and outbox remain authoritative.  This read
-- model contains only bounded, normalized facts and source-event metadata;
-- aggregate JSON is intentionally not persisted here.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.pos_sale_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    sale_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL CHECK (
        btrim(reference) <> '' AND char_length(reference) <= 255
    ),
    customer_id UUID,
    customer_scope_kind TEXT,
    customer_scope_holding_id UUID,
    customer_scope_company_id UUID,
    customer_scope_branch_id UUID,
    register_id UUID,
    register_scope_kind TEXT,
    register_scope_holding_id UUID,
    register_scope_company_id UUID,
    register_scope_branch_id UUID,
    currency TEXT NOT NULL CHECK (
        char_length(currency) = 3 AND currency = upper(currency)
    ),
    subtotal_minor BIGINT NOT NULL CHECK (subtotal_minor >= 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    total_minor BIGINT NOT NULL CHECK (total_minor = subtotal_minor + tax_minor),
    paid_minor BIGINT NOT NULL CHECK (paid_minor >= 0 AND paid_minor <= total_minor),
    status TEXT NOT NULL CHECK (status IN (
        'draft', 'open', 'held', 'completed', 'voided', 'refunded'
    )),
    command_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (
        btrim(idempotency_key) <> '' AND char_length(idempotency_key) <= 512
    ),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, sale_id),
    CHECK (updated_at >= created_at),
    CHECK (customer_scope_kind IS NULL OR customer_scope_kind IN ('holding', 'company', 'branch')),
    CHECK (register_scope_kind IS NULL OR register_scope_kind IN ('holding', 'company', 'branch')),
    CHECK ((customer_id IS NULL AND customer_scope_kind IS NULL
            AND customer_scope_holding_id IS NULL AND customer_scope_company_id IS NULL
            AND customer_scope_branch_id IS NULL)
        OR (customer_id IS NOT NULL AND customer_scope_kind IS NOT NULL
            AND customer_scope_holding_id IS NOT NULL)),
    CHECK ((register_id IS NULL AND register_scope_kind IS NULL
            AND register_scope_holding_id IS NULL AND register_scope_company_id IS NULL
            AND register_scope_branch_id IS NULL)
        OR (register_id IS NOT NULL AND register_scope_kind IS NOT NULL
            AND register_scope_holding_id IS NOT NULL))
);

CREATE TABLE IF NOT EXISTS erpx_holding.pos_sale_line_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    sale_id UUID NOT NULL,
    line_id UUID NOT NULL,
    line_no INTEGER NOT NULL CHECK (line_no BETWEEN 0 AND 100000),
    item_reference TEXT NOT NULL CHECK (
        btrim(item_reference) <> '' AND char_length(item_reference) <= 128
    ),
    description TEXT NOT NULL CHECK (
        btrim(description) <> '' AND char_length(description) <= 255
    ),
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    unit_price_minor BIGINT NOT NULL CHECK (unit_price_minor >= 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    subtotal_minor BIGINT NOT NULL CHECK (subtotal_minor = quantity * unit_price_minor),
    total_minor BIGINT NOT NULL CHECK (total_minor = subtotal_minor + tax_minor),
    product_id UUID,
    product_scope_kind TEXT,
    product_scope_holding_id UUID,
    product_scope_company_id UUID,
    product_scope_branch_id UUID,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, sale_id, line_id),
    UNIQUE (data_profile_key, sale_id, line_no),
    CHECK (product_scope_kind IS NULL OR product_scope_kind IN ('holding', 'company', 'branch')),
    CHECK ((product_id IS NULL AND product_scope_kind IS NULL
            AND product_scope_holding_id IS NULL AND product_scope_company_id IS NULL
            AND product_scope_branch_id IS NULL)
        OR (product_id IS NOT NULL AND product_scope_kind IS NOT NULL
            AND product_scope_holding_id IS NOT NULL))
);

CREATE TABLE IF NOT EXISTS erpx_holding.pos_sale_payment_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    sale_id UUID NOT NULL,
    payment_id UUID NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    method TEXT NOT NULL CHECK (method IN (
        'cash', 'card', 'bank_transfer', 'mobile_wallet', 'voucher', 'other'
    )),
    received_at TIMESTAMPTZ NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, sale_id, payment_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.pos_projection_checkpoints (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    projection_name TEXT NOT NULL CHECK (
        projection_name = 'pos'
    ),
    aggregate_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, projection_name, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.pos_projection_event_receipts (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    projection_name TEXT NOT NULL CHECK (
        projection_name = 'pos'
    ),
    event_id UUID NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_type TEXT NOT NULL CHECK (
        event_type IN (
            'pos.sale_created.v1', 'pos.sale_opened.v1', 'pos.sale_held.v1',
            'pos.sale_resumed.v1', 'pos.payment_recorded.v1',
            'pos.sale_completed.v1', 'pos.sale_voided.v1', 'pos.sale_refunded.v1'
        )
    ),
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, projection_name, event_id)
);

CREATE INDEX IF NOT EXISTS pos_sale_read_scope_idx
    ON erpx_holding.pos_sale_read_rows
        (data_profile_key, holding_id, company_id, branch_id, updated_at DESC, sale_id DESC);
CREATE INDEX IF NOT EXISTS pos_sale_line_read_sale_idx
    ON erpx_holding.pos_sale_line_read_rows (data_profile_key, sale_id, line_no);
CREATE INDEX IF NOT EXISTS pos_sale_payment_read_sale_idx
    ON erpx_holding.pos_sale_payment_read_rows
        (data_profile_key, sale_id, received_at, payment_id);
CREATE INDEX IF NOT EXISTS pos_projection_checkpoint_source_idx
    ON erpx_holding.pos_projection_checkpoints
        (data_profile_key, aggregate_id, source_version);
CREATE INDEX IF NOT EXISTS pos_projection_receipt_source_idx
    ON erpx_holding.pos_projection_event_receipts
        (data_profile_key, aggregate_id, aggregate_version);

ALTER TABLE erpx_holding.pos_sale_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_sale_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_sale_line_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_sale_line_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_sale_payment_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_sale_payment_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_projection_event_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_projection_event_receipts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_pos_sale_read_rows ON erpx_holding.pos_sale_read_rows;
CREATE POLICY holding_scope_pos_sale_read_rows ON erpx_holding.pos_sale_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_pos_sale_line_read_rows ON erpx_holding.pos_sale_line_read_rows;
CREATE POLICY holding_scope_pos_sale_line_read_rows ON erpx_holding.pos_sale_line_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_pos_sale_payment_read_rows ON erpx_holding.pos_sale_payment_read_rows;
CREATE POLICY holding_scope_pos_sale_payment_read_rows ON erpx_holding.pos_sale_payment_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_pos_projection_checkpoints ON erpx_holding.pos_projection_checkpoints;
CREATE POLICY holding_scope_pos_projection_checkpoints ON erpx_holding.pos_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_pos_projection_event_receipts ON erpx_holding.pos_projection_event_receipts;
CREATE POLICY holding_scope_pos_projection_event_receipts ON erpx_holding.pos_projection_event_receipts
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Rebuildable, typed Legal/document read projection for one physically
-- isolated holding.  The holding-write Legal tables and outbox remain
-- authoritative.  No raw document bytes, JSON payloads, credentials, URLs,
-- tokens, or provider state are represented here.

CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.legal_contract_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL CHECK (btrim(reference) <> '' AND char_length(reference) <= 255),
    title TEXT NOT NULL CHECK (btrim(title) <> '' AND char_length(title) <= 255),
    object_ref TEXT NOT NULL CHECK (btrim(object_ref) <> '' AND char_length(object_ref) <= 255),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    retention_until TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('draft', 'review', 'approved', 'active', 'expired', 'terminated', 'archived')),
    created_by TEXT NOT NULL CHECK (btrim(created_by) <> '' AND char_length(created_by) <= 255),
    approved_by TEXT CHECK (approved_by IS NULL OR (btrim(approved_by) <> '' AND char_length(approved_by) <= 255)),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'holding_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, contract_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.legal_contract_party_read_rows (
    data_profile_key TEXT NOT NULL,
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    party_id UUID NOT NULL,
    party_role TEXT NOT NULL CHECK (btrim(party_role) <> '' AND char_length(party_role) <= 255),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'holding_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, contract_id, party_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.legal_contract_approval_read_rows (
    data_profile_key TEXT NOT NULL,
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    approved_by TEXT NOT NULL CHECK (btrim(approved_by) <> '' AND char_length(approved_by) <= 255),
    approved_at TIMESTAMPTZ NOT NULL,
    contract_version BIGINT NOT NULL CHECK (contract_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'holding_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, contract_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.legal_contract_retention_read_rows (
    data_profile_key TEXT NOT NULL,
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    retention_until TIMESTAMPTZ NOT NULL,
    retention_state TEXT NOT NULL CHECK (retention_state IN ('active', 'ended')),
    contract_version BIGINT NOT NULL CHECK (contract_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'holding_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, contract_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.legal_document_read_rows (
    data_profile_key TEXT NOT NULL,
    document_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    title TEXT NOT NULL CHECK (btrim(title) <> '' AND char_length(title) <= 255),
    current_version BIGINT NOT NULL CHECK (current_version >= 0),
    archived BOOLEAN NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'holding_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, document_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.legal_document_version_read_rows (
    data_profile_key TEXT NOT NULL,
    document_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    object_ref TEXT NOT NULL CHECK (btrim(object_ref) <> '' AND char_length(object_ref) <= 255),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    created_at TIMESTAMPTZ NOT NULL,
    created_by TEXT NOT NULL CHECK (btrim(created_by) <> '' AND char_length(created_by) <= 255),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'holding_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, document_id, version)
);

CREATE TABLE IF NOT EXISTS erpx_holding.legal_access_history_read_rows (
    data_profile_key TEXT NOT NULL,
    entry_id UUID NOT NULL,
    document_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    document_version BIGINT NOT NULL CHECK (document_version > 0),
    actor_ref TEXT NOT NULL CHECK (btrim(actor_ref) <> '' AND char_length(actor_ref) <= 255),
    action TEXT NOT NULL CHECK (btrim(action) <> '' AND char_length(action) <= 255),
    purpose TEXT NOT NULL CHECK (btrim(purpose) <> '' AND char_length(purpose) <= 512),
    observed_at TIMESTAMPTZ NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'holding_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, entry_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.legal_projection_checkpoints (
    data_profile_key TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'legal_documents'),
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('legal_contract', 'legal_document', 'legal_access_history')),
    aggregate_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, projection_name, aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.legal_projection_event_receipts (
    data_profile_key TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'legal_documents'),
    event_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('legal_contract', 'legal_document', 'legal_access_history')),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_type TEXT NOT NULL CHECK (event_type IN ('legal.contract_saved.v1', 'legal.document_saved.v1', 'legal.access_recorded.v1')),
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'holding_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, projection_name, event_id)
);

CREATE INDEX IF NOT EXISTS legal_contract_read_scope_idx
    ON erpx_holding.legal_contract_read_rows
       (data_profile_key, holding_id, company_id, branch_id, status, projected_at DESC);
CREATE INDEX IF NOT EXISTS legal_contract_party_read_scope_idx
    ON erpx_holding.legal_contract_party_read_rows
       (data_profile_key, holding_id, contract_id, party_id);
CREATE INDEX IF NOT EXISTS legal_contract_approval_read_scope_idx
    ON erpx_holding.legal_contract_approval_read_rows
       (data_profile_key, holding_id, contract_id, approved_at DESC);
CREATE INDEX IF NOT EXISTS legal_contract_retention_read_scope_idx
    ON erpx_holding.legal_contract_retention_read_rows
       (data_profile_key, holding_id, retention_until);
CREATE INDEX IF NOT EXISTS legal_document_read_scope_idx
    ON erpx_holding.legal_document_read_rows
       (data_profile_key, holding_id, company_id, branch_id, archived, projected_at DESC);
CREATE INDEX IF NOT EXISTS legal_document_version_read_scope_idx
    ON erpx_holding.legal_document_version_read_rows
       (data_profile_key, holding_id, document_id, version);
CREATE INDEX IF NOT EXISTS legal_access_history_read_scope_idx
    ON erpx_holding.legal_access_history_read_rows
       (data_profile_key, holding_id, company_id, branch_id, observed_at DESC, entry_id);
CREATE INDEX IF NOT EXISTS legal_projection_checkpoint_source_idx
    ON erpx_holding.legal_projection_checkpoints
       (data_profile_key, projection_name, aggregate_type, source_version, projected_at DESC);
CREATE INDEX IF NOT EXISTS legal_projection_receipt_source_idx
    ON erpx_holding.legal_projection_event_receipts
       (data_profile_key, projection_name, aggregate_type, aggregate_id, aggregate_version);

ALTER TABLE erpx_holding.legal_contract_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_contract_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_contract_party_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_contract_party_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_contract_approval_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_contract_approval_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_contract_retention_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_contract_retention_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_document_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_document_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_document_version_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_document_version_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_access_history_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_access_history_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_projection_event_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_projection_event_receipts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_legal_contract_read_rows ON erpx_holding.legal_contract_read_rows;
CREATE POLICY holding_scope_legal_contract_read_rows ON erpx_holding.legal_contract_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_legal_contract_party_read_rows ON erpx_holding.legal_contract_party_read_rows;
CREATE POLICY holding_scope_legal_contract_party_read_rows ON erpx_holding.legal_contract_party_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_legal_contract_approval_read_rows ON erpx_holding.legal_contract_approval_read_rows;
CREATE POLICY holding_scope_legal_contract_approval_read_rows ON erpx_holding.legal_contract_approval_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_legal_contract_retention_read_rows ON erpx_holding.legal_contract_retention_read_rows;
CREATE POLICY holding_scope_legal_contract_retention_read_rows ON erpx_holding.legal_contract_retention_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_legal_document_read_rows ON erpx_holding.legal_document_read_rows;
CREATE POLICY holding_scope_legal_document_read_rows ON erpx_holding.legal_document_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_legal_document_version_read_rows ON erpx_holding.legal_document_version_read_rows;
CREATE POLICY holding_scope_legal_document_version_read_rows ON erpx_holding.legal_document_version_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_legal_access_history_read_rows ON erpx_holding.legal_access_history_read_rows;
CREATE POLICY holding_scope_legal_access_history_read_rows ON erpx_holding.legal_access_history_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_legal_projection_checkpoints ON erpx_holding.legal_projection_checkpoints;
CREATE POLICY holding_scope_legal_projection_checkpoints ON erpx_holding.legal_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_legal_projection_event_receipts ON erpx_holding.legal_projection_event_receipts;
CREATE POLICY holding_scope_legal_projection_event_receipts ON erpx_holding.legal_projection_event_receipts
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Rebuildable, normalized holding-read Chart of Accounts projection.
-- The holding-write finance_chart_* tables and outbox are authoritative.

CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.finance_chart_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    chart_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    chart_name TEXT NOT NULL CHECK (btrim(chart_name) <> '' AND char_length(chart_name) <= 255),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'holding_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, chart_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_chart_account_read_rows (
    data_profile_key TEXT NOT NULL,
    chart_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    account_id UUID NOT NULL,
    code TEXT NOT NULL CHECK (btrim(code) <> '' AND char_length(code) <= 64),
    account_name TEXT NOT NULL CHECK (btrim(account_name) <> '' AND char_length(account_name) <= 255),
    category TEXT NOT NULL CHECK (category IN ('asset', 'liability', 'equity', 'revenue', 'expense')),
    normal_balance TEXT NOT NULL CHECK (normal_balance IN ('debit', 'credit')),
    parent_account_id UUID,
    status TEXT NOT NULL CHECK (status IN ('active', 'inactive')),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'holding_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, chart_id, account_id),
    UNIQUE (data_profile_key, chart_id, code),
    FOREIGN KEY (data_profile_key, chart_id)
        REFERENCES erpx_holding.finance_chart_read_rows (data_profile_key, chart_id)
        ON DELETE CASCADE,
    CHECK (parent_account_id IS NULL OR parent_account_id <> account_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_chart_projection_checkpoints (
    data_profile_key TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'finance_chart'),
    aggregate_type TEXT NOT NULL CHECK (aggregate_type = 'finance_chart_of_accounts'),
    aggregate_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, projection_name, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_chart_projection_event_receipts (
    data_profile_key TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'finance_chart'),
    event_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type = 'finance_chart_of_accounts'),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_type TEXT NOT NULL CHECK (event_type = 'finance.chart_of_accounts_saved.v1'),
    source_contract_version INTEGER NOT NULL CHECK (source_contract_version > 0),
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_actor_ref TEXT NOT NULL CHECK (btrim(source_actor_ref) <> '' AND char_length(source_actor_ref) <= 255),
    source_actor_kind TEXT NOT NULL CHECK (source_actor_kind IN ('platform_user', 'holding_user', 'service', 'system_worker')),
    source_correlation_id UUID NOT NULL,
    source_causation_id UUID,
    source_idempotency_key TEXT NOT NULL CHECK (btrim(source_idempotency_key) <> '' AND char_length(source_idempotency_key) <= 512),
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, projection_name, event_id)
);

CREATE INDEX IF NOT EXISTS finance_chart_read_scope_idx
    ON erpx_holding.finance_chart_read_rows
       (data_profile_key, holding_id, company_id, projected_at DESC, chart_id);
CREATE INDEX IF NOT EXISTS finance_chart_account_read_scope_idx
    ON erpx_holding.finance_chart_account_read_rows
       (data_profile_key, holding_id, company_id, chart_id, code);
CREATE INDEX IF NOT EXISTS finance_chart_projection_checkpoint_source_idx
    ON erpx_holding.finance_chart_projection_checkpoints
       (data_profile_key, aggregate_id, source_version);
CREATE INDEX IF NOT EXISTS finance_chart_projection_receipt_source_idx
    ON erpx_holding.finance_chart_projection_event_receipts
       (data_profile_key, aggregate_id, aggregate_version);

ALTER TABLE erpx_holding.finance_chart_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_chart_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_chart_account_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_chart_account_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_chart_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_chart_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_chart_projection_event_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_chart_projection_event_receipts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_chart_read_rows ON erpx_holding.finance_chart_read_rows;
CREATE POLICY holding_scope_finance_chart_read_rows ON erpx_holding.finance_chart_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_finance_chart_account_read_rows ON erpx_holding.finance_chart_account_read_rows;
CREATE POLICY holding_scope_finance_chart_account_read_rows ON erpx_holding.finance_chart_account_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_finance_chart_projection_checkpoints ON erpx_holding.finance_chart_projection_checkpoints;
CREATE POLICY holding_scope_finance_chart_projection_checkpoints ON erpx_holding.finance_chart_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_finance_chart_projection_event_receipts ON erpx_holding.finance_chart_projection_event_receipts;
CREATE POLICY holding_scope_finance_chart_projection_event_receipts ON erpx_holding.finance_chart_projection_event_receipts
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Rebuildable Marketing content projection. It stores bounded metadata and
-- opaque references only; authoritative content remains in holding-write.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('asset', 'template', 'form', 'domain')),
    aggregate_id UUID NOT NULL,
    scope JSONB NOT NULL,
    slug TEXT,
    status TEXT NOT NULL CHECK (btrim(status) <> '' AND octet_length(status) <= 64),
    state JSONB NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_actor JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_projection_checkpoints (
    data_profile_key TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'marketing_content'),
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('asset', 'template', 'form', 'domain')),
    aggregate_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    last_event_id UUID NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, projection_name, aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_projection_event_receipts (
    data_profile_key TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'marketing_content'),
    event_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('asset', 'template', 'form', 'domain')),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, projection_name, event_id)
);

CREATE INDEX IF NOT EXISTS marketing_content_read_scope_idx
    ON erpx_holding.marketing_content_read_rows (data_profile_key, aggregate_type, status, projected_at DESC);
CREATE INDEX IF NOT EXISTS marketing_content_read_source_idx
    ON erpx_holding.marketing_content_read_rows (data_profile_key, aggregate_type, source_version, projected_at DESC);

ALTER TABLE erpx_holding.marketing_content_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_projection_event_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_projection_event_receipts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_content_read_rows ON erpx_holding.marketing_content_read_rows;
CREATE POLICY holding_scope_marketing_content_read_rows
    ON erpx_holding.marketing_content_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_content_projection_checkpoints ON erpx_holding.marketing_content_projection_checkpoints;
CREATE POLICY holding_scope_marketing_content_projection_checkpoints
    ON erpx_holding.marketing_content_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_content_projection_event_receipts ON erpx_holding.marketing_content_projection_event_receipts;
CREATE POLICY holding_scope_marketing_content_projection_event_receipts
    ON erpx_holding.marketing_content_projection_event_receipts
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Rebuildable Marketing workflow read models. Raw preview token and domain
-- verification hashes are deliberately excluded; form values remain holding
-- scoped and bounded for authorized CRM/support reads.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_preview_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
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
    PRIMARY KEY (data_profile_key, preview_id),
    CHECK (expires_at > issued_at)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_submission_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    submission_id UUID NOT NULL,
    form_id UUID NOT NULL,
    scope JSONB NOT NULL,
    values_json JSONB NOT NULL CHECK (octet_length(values_json::text) <= 2097152),
    received_at TIMESTAMPTZ NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    source_actor JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, submission_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_domain_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
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
    PRIMARY KEY (data_profile_key, domain_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_workflow_projection_checkpoints (
    data_profile_key TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'marketing_content_workflow'),
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('preview', 'form_submission', 'domain')),
    aggregate_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    last_event_id UUID NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, projection_name, aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_workflow_projection_event_receipts (
    data_profile_key TEXT NOT NULL,
    projection_name TEXT NOT NULL CHECK (projection_name = 'marketing_content_workflow'),
    event_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('preview', 'form_submission', 'domain')),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, projection_name, event_id)
);

CREATE INDEX IF NOT EXISTS marketing_content_preview_read_expiry_idx
    ON erpx_holding.marketing_content_preview_read_rows (data_profile_key, expires_at, revoked_at);
CREATE INDEX IF NOT EXISTS marketing_content_submission_read_form_idx
    ON erpx_holding.marketing_content_submission_read_rows (data_profile_key, form_id, received_at DESC);
CREATE INDEX IF NOT EXISTS marketing_content_domain_read_status_idx
    ON erpx_holding.marketing_content_domain_read_rows (data_profile_key, status, hostname);

ALTER TABLE erpx_holding.marketing_content_preview_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_preview_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_submission_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_submission_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_domain_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_domain_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_workflow_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_workflow_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_workflow_projection_event_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_workflow_projection_event_receipts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_content_preview_read_rows ON erpx_holding.marketing_content_preview_read_rows;
CREATE POLICY holding_scope_marketing_content_preview_read_rows
    ON erpx_holding.marketing_content_preview_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_content_submission_read_rows ON erpx_holding.marketing_content_submission_read_rows;
CREATE POLICY holding_scope_marketing_content_submission_read_rows
    ON erpx_holding.marketing_content_submission_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_content_domain_read_rows ON erpx_holding.marketing_content_domain_read_rows;
CREATE POLICY holding_scope_marketing_content_domain_read_rows
    ON erpx_holding.marketing_content_domain_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_content_workflow_projection_checkpoints ON erpx_holding.marketing_content_workflow_projection_checkpoints;
CREATE POLICY holding_scope_marketing_content_workflow_projection_checkpoints
    ON erpx_holding.marketing_content_workflow_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_content_workflow_projection_event_receipts ON erpx_holding.marketing_content_workflow_projection_event_receipts;
CREATE POLICY holding_scope_marketing_content_workflow_projection_event_receipts
    ON erpx_holding.marketing_content_workflow_projection_event_receipts
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Read projection metadata follows source redaction without copying any
-- token, secret, or deleted submission value into a new durable fact.
ALTER TABLE erpx_holding.marketing_content_submission_read_rows
    ADD COLUMN IF NOT EXISTS retention_until TIMESTAMPTZ NOT NULL
        DEFAULT (now() + INTERVAL '365 days'),
    ADD COLUMN IF NOT EXISTS redacted_at TIMESTAMPTZ;

ALTER TABLE erpx_holding.marketing_content_submission_read_rows
    ALTER COLUMN retention_until DROP DEFAULT,
    ADD CONSTRAINT marketing_content_read_submission_retention_after_received
        CHECK (retention_until >= received_at),
    ADD CONSTRAINT marketing_content_read_submission_redacted_after_received
        CHECK (redacted_at IS NULL OR redacted_at >= received_at);


-- Disposable read profile rows carry only the redacted erasure marker; old
-- outbox profile snapshots must not be able to repopulate contact values.
ALTER TABLE erpx_holding.marketing_profiles
    ADD COLUMN IF NOT EXISTS erased_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS erasure_reason TEXT;

ALTER TABLE erpx_holding.marketing_profiles
    ADD CONSTRAINT marketing_read_profile_erasure_reason_bound
        CHECK (erasure_reason IS NULL OR (btrim(erasure_reason) <> '' AND octet_length(erasure_reason) <= 512)),
    ADD CONSTRAINT marketing_read_profile_erased_at_pair
        CHECK ((erased_at IS NULL AND erasure_reason IS NULL)
            OR (erased_at IS NOT NULL AND erasure_reason IS NOT NULL));

CREATE INDEX IF NOT EXISTS marketing_read_profiles_erasure_idx
    ON erpx_holding.marketing_profiles (data_profile_key, erased_at, profile_id);


-- Rebuildable, payload-free G13 target read model.
-- The target write ledger and its outbox remain authoritative. Source keys and
-- payloads are represented only by SHA-256 digests in the read database.

CREATE TABLE IF NOT EXISTS erpx_holding.g13_migration_import_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    run_id UUID NOT NULL,
    source_kind TEXT NOT NULL CHECK (btrim(source_kind) <> ''),
    source_key_hash TEXT NOT NULL CHECK (source_key_hash ~ '^[0-9a-fA-F]{64}$'),
    target_id UUID NOT NULL,
    source_contract_hash TEXT NOT NULL CHECK (source_contract_hash ~ '^[0-9a-fA-F]{64}$'),
    source_fence TEXT NOT NULL CHECK (btrim(source_fence) <> ''),
    payload_hash TEXT NOT NULL CHECK (payload_hash ~ '^[0-9a-fA-F]{64}$'),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, run_id, source_kind, source_key_hash)
);

CREATE INDEX IF NOT EXISTS g13_migration_import_read_run_idx
    ON erpx_holding.g13_migration_import_read_rows
        (data_profile_key, run_id, source_kind, projected_at DESC);

ALTER TABLE erpx_holding.g13_migration_import_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.g13_migration_import_read_rows FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS holding_scope_g13_migration_import_read_rows
    ON erpx_holding.g13_migration_import_read_rows;
CREATE POLICY holding_scope_g13_migration_import_read_rows
    ON erpx_holding.g13_migration_import_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Rebuildable Thai tax report projection. No journal payload or command
-- context is copied into the read database.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.finance_thai_tax_fact_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    fact_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    journal_id UUID NOT NULL,
    line_id UUID NOT NULL,
    tax_date DATE NOT NULL,
    tax_period CHAR(7) NOT NULL CHECK (tax_period ~ '^[0-9]{4}-[0-9]{2}$'),
    kind TEXT NOT NULL CHECK (kind IN ('output_vat', 'input_vat', 'withholding_payable')),
    currency CHAR(3) NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    base_minor BIGINT NOT NULL CHECK (base_minor > 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    tax_rate_basis_points INTEGER NOT NULL CHECK (tax_rate_basis_points BETWEEN 0 AND 10000),
    tax_identifier TEXT NOT NULL CHECK (btrim(tax_identifier) <> '' AND char_length(tax_identifier) <= 255),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, fact_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_thai_tax_projection_checkpoints (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    fact_id UUID NOT NULL,
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, fact_id)
);

CREATE INDEX IF NOT EXISTS finance_thai_tax_fact_read_report_idx
    ON erpx_holding.finance_thai_tax_fact_read_rows
       (data_profile_key, company_id, tax_period, tax_date, fact_id);

ALTER TABLE erpx_holding.finance_thai_tax_fact_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_thai_tax_fact_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_thai_tax_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_thai_tax_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_thai_tax_fact_read_rows
    ON erpx_holding.finance_thai_tax_fact_read_rows;
CREATE POLICY holding_scope_finance_thai_tax_fact_read_rows
    ON erpx_holding.finance_thai_tax_fact_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_finance_thai_tax_projection_checkpoints
    ON erpx_holding.finance_thai_tax_projection_checkpoints;
CREATE POLICY holding_scope_finance_thai_tax_projection_checkpoints
    ON erpx_holding.finance_thai_tax_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Durable cursor for the Marketing delivery projection. The write-side
-- outbox remains authoritative; replaying a committed source event is safe
-- because the read row is version fenced.
CREATE TABLE IF NOT EXISTS erpx_holding.marketing_delivery_projection_checkpoints (
    projection_name TEXT PRIMARY KEY CHECK (projection_name = 'marketing_delivery'),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    last_event_id UUID,
    last_occurred_at TIMESTAMPTZ,
    source_event_count BIGINT NOT NULL DEFAULT 0 CHECK (source_event_count >= 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE erpx_holding.marketing_delivery_projection_checkpoints
    ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_delivery_projection_checkpoints
    FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_delivery_projection_checkpoints
    ON erpx_holding.marketing_delivery_projection_checkpoints;
CREATE POLICY holding_scope_marketing_delivery_projection_checkpoints
    ON erpx_holding.marketing_delivery_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Disposable Quality read model and projection checkpoints.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.quality_inspection_read_rows (
    inspection_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_holding.quality_projection_checkpoints (
    inspection_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS quality_inspection_read_scope_idx
    ON erpx_holding.quality_inspection_read_rows
       (data_profile_key, company_id, branch_id, reference, inspection_id);

ALTER TABLE erpx_holding.quality_inspection_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.quality_inspection_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.quality_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.quality_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_quality_read_rows
    ON erpx_holding.quality_inspection_read_rows;
CREATE POLICY holding_scope_quality_read_rows
    ON erpx_holding.quality_inspection_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_quality_checkpoints
    ON erpx_holding.quality_projection_checkpoints;
CREATE POLICY holding_scope_quality_checkpoints
    ON erpx_holding.quality_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- A delivery projection cursor is holding/holding scoped. 0042 originally
-- keyed the table by projection name alone, which allowed one holding to
-- overwrite another holding's cursor and made a concurrent projector fail
-- closed as a scope conflict.  Preserve existing rows while widening the
-- durable key to the actual read-model scope.
ALTER TABLE erpx_holding.marketing_delivery_projection_checkpoints
    DROP CONSTRAINT IF EXISTS marketing_delivery_projection_checkpoints_pkey;

ALTER TABLE erpx_holding.marketing_delivery_projection_checkpoints
    ADD PRIMARY KEY (projection_name, data_profile_key);


-- Rebuildable Marketing journey conversion read model.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

ALTER TABLE IF EXISTS erpx_holding.marketing_projection_checkpoints
    DROP CONSTRAINT IF EXISTS marketing_projection_checkpoints_aggregate_type_check;
ALTER TABLE IF EXISTS erpx_holding.marketing_projection_checkpoints
    ADD CONSTRAINT marketing_projection_checkpoints_aggregate_type_check
    CHECK (aggregate_type IN (
        'marketing_profile', 'marketing_consent',
        'marketing_suppression', 'marketing_audience',
        'marketing_journey_conversion',
        'marketing_crm_accepted_lead_handoff', 'marketing_crm_opportunity',
        'marketing_crm_attribution'
    ));

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_journey_conversion_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    conversion_ref UUID NOT NULL,
    journey_id UUID NOT NULL,
    run_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    scope JSONB NOT NULL,
    revenue_minor BIGINT CHECK (revenue_minor >= 0),
    currency TEXT CHECK (currency IS NULL OR (length(currency) = 3 AND currency = upper(currency))),
    occurred_at TIMESTAMPTZ NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, conversion_ref)
);

CREATE INDEX IF NOT EXISTS marketing_journey_conversion_read_scope_idx
    ON erpx_holding.marketing_journey_conversion_read_rows
       (data_profile_key, occurred_at DESC, conversion_ref);

ALTER TABLE erpx_holding.marketing_journey_conversion_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_journey_conversion_read_rows FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_journey_conversion_read_rows
    ON erpx_holding.marketing_journey_conversion_read_rows;
CREATE POLICY holding_scope_marketing_journey_conversion_read_rows
    ON erpx_holding.marketing_journey_conversion_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Rebuildable internal owner-notification read model.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

ALTER TABLE IF EXISTS erpx_holding.marketing_projection_checkpoints
    DROP CONSTRAINT IF EXISTS marketing_projection_checkpoints_aggregate_type_check;
ALTER TABLE IF EXISTS erpx_holding.marketing_projection_checkpoints
    ADD CONSTRAINT marketing_projection_checkpoints_aggregate_type_check
    CHECK (aggregate_type IN (
        'marketing_profile', 'marketing_consent',
        'marketing_suppression', 'marketing_audience',
        'marketing_journey_conversion', 'marketing_journey_owner_notification',
        'marketing_crm_accepted_lead_handoff', 'marketing_crm_opportunity',
        'marketing_crm_attribution'
    ));

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_journey_owner_notification_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    notification_ref UUID NOT NULL,
    journey_id UUID NOT NULL,
    run_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    scope JSONB NOT NULL,
    owner_ref TEXT NOT NULL CHECK (btrim(owner_ref) <> ''),
    occurred_at TIMESTAMPTZ NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    payload JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, notification_ref)
);

CREATE INDEX IF NOT EXISTS marketing_journey_owner_notification_read_scope_idx
    ON erpx_holding.marketing_journey_owner_notification_read_rows
       (data_profile_key, occurred_at DESC, notification_ref);

ALTER TABLE erpx_holding.marketing_journey_owner_notification_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_journey_owner_notification_read_rows FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_journey_owner_notification_read_rows
    ON erpx_holding.marketing_journey_owner_notification_read_rows;
CREATE POLICY holding_scope_marketing_journey_owner_notification_read_rows
    ON erpx_holding.marketing_journey_owner_notification_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Rebuildable read model for profile-redacted Marketing journey integration
-- handoffs. The write-side invocation and outbox remain authoritative.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_journey_integration_read_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    invocation_id UUID NOT NULL,
    integration_ref TEXT NOT NULL CHECK (btrim(integration_ref) <> ''),
    journey_id UUID NOT NULL,
    run_id UUID NOT NULL,
    node_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    scope JSONB NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    state TEXT NOT NULL CHECK (state IN ('queued', 'sending', 'succeeded', 'retry_scheduled', 'failed')),
    attempt_count INTEGER NOT NULL CHECK (attempt_count >= 0),
    version BIGINT NOT NULL CHECK (version > 0),
    provider_reference TEXT,
    last_error TEXT,
    next_attempt_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    source_event_id UUID NOT NULL,
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_profile_key, invocation_id)
);

CREATE INDEX IF NOT EXISTS marketing_journey_integration_read_scope_idx
    ON erpx_holding.marketing_journey_integration_read_rows
       (data_profile_key, updated_at DESC, invocation_id);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_journey_integration_projection_checkpoints (
    data_profile_key TEXT PRIMARY KEY CHECK (btrim(data_profile_key) <> ''),
    last_occurred_at TIMESTAMPTZ NOT NULL,
    last_event_id UUID NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE erpx_holding.marketing_journey_integration_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_journey_integration_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_journey_integration_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_journey_integration_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_journey_integration_read_rows
    ON erpx_holding.marketing_journey_integration_read_rows;
CREATE POLICY holding_scope_marketing_journey_integration_read_rows
    ON erpx_holding.marketing_journey_integration_read_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_marketing_journey_integration_projection_checkpoints
    ON erpx_holding.marketing_journey_integration_projection_checkpoints;
CREATE POLICY holding_scope_marketing_journey_integration_projection_checkpoints
    ON erpx_holding.marketing_journey_integration_projection_checkpoints
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


CREATE SCHEMA IF NOT EXISTS erpx_cqrs_read;

CREATE OR REPLACE FUNCTION erpx_cqrs_read.current_holding_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.holding_id', true), '')::uuid
$$;

CREATE TABLE erpx_cqrs_read.projection_generations (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('building', 'active', 'retired')),
    source_sequence BIGINT NOT NULL DEFAULT 0 CHECK (source_sequence >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    activated_at TIMESTAMPTZ,
    retired_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, projection_name, generation)
);

CREATE UNIQUE INDEX cqrs_one_active_projection_generation
    ON erpx_cqrs_read.projection_generations (holding_id, projection_name)
    WHERE status = 'active';

CREATE TABLE erpx_cqrs_read.projection_checkpoints (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    event_id UUID NOT NULL,
    global_sequence BIGINT NOT NULL CHECK (global_sequence > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_cqrs_read.projection_applied_events (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    event_id UUID NOT NULL,
    global_sequence BIGINT NOT NULL CHECK (global_sequence > 0),
    event_type TEXT NOT NULL,
    event_version INTEGER NOT NULL CHECK (event_version > 0),
    applied_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, event_id),
    UNIQUE (holding_id, projection_name, generation, partition_key, global_sequence),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_cqrs_read.projection_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    event_id UUID NOT NULL,
    global_sequence BIGINT NOT NULL CHECK (global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, command_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_cqrs_read.projection_failures (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    event_id UUID NOT NULL,
    global_sequence BIGINT NOT NULL CHECK (global_sequence > 0),
    attempts INTEGER NOT NULL CHECK (attempts > 0),
    next_retry_at TIMESTAMPTZ,
    dead_lettered_at TIMESTAMPTZ,
    last_error TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, event_id),
    CHECK (dead_lettered_at IS NULL OR next_retry_at IS NULL),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX cqrs_projection_retry_queue
    ON erpx_cqrs_read.projection_failures
        (holding_id, projection_name, generation, next_retry_at, global_sequence)
    WHERE dead_lettered_at IS NULL;

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'projection_generations','projection_checkpoints','projection_applied_events',
        'projection_receipts','projection_failures'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_cqrs_read.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_cqrs_read.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY cqrs_read_holding_isolation ON erpx_cqrs_read.%I '
            'USING (holding_id = erpx_cqrs_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_read.current_holding_id())',
            table_name
        );
    END LOOP;
END;
$$;


CREATE SCHEMA IF NOT EXISTS erpx_quality_read;

CREATE TABLE erpx_quality_read.inspection_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'quality.inspections'
        CHECK (projection_name = 'quality.inspections'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    inspection_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    reference TEXT NOT NULL,
    holding_scope JSONB NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    inspection_kind TEXT NOT NULL
        CHECK (inspection_kind IN ('incoming', 'in_process', 'final')),
    product_id UUID NOT NULL,
    lot_reference TEXT,
    quantity NUMERIC(20, 0) NOT NULL CHECK (quantity > 0),
    passed_quantity NUMERIC(20, 0) NOT NULL CHECK (passed_quantity >= 0),
    failed_quantity NUMERIC(20, 0) NOT NULL CHECK (failed_quantity >= 0),
    inspection_status TEXT NOT NULL
        CHECK (inspection_status IN (
            'draft', 'in_progress', 'passed', 'failed', 'released', 'cancelled'
        )),
    disposition_note TEXT,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, inspection_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    CHECK (passed_quantity + failed_quantity <= quantity)
);

CREATE UNIQUE INDEX quality_projection_reference
    ON erpx_quality_read.inspection_projection
        (holding_id, generation, holding_id, company_id,
         COALESCE(branch_id, '00000000-0000-0000-0000-000000000000'::uuid), reference);

CREATE INDEX quality_projection_scope_updated
    ON erpx_quality_read.inspection_projection
        (holding_id, generation, holding_id, company_id, branch_id,
         aggregate_updated_at DESC, inspection_id DESC);

CREATE INDEX quality_projection_status_updated
    ON erpx_quality_read.inspection_projection
        (holding_id, generation, inspection_status, aggregate_updated_at DESC, inspection_id DESC);

CREATE INDEX quality_projection_product_updated
    ON erpx_quality_read.inspection_projection
        (holding_id, generation, product_id, aggregate_updated_at DESC, inspection_id DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_quality_read','inspection_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY holding_isolation ON %I.%I '
            'USING (holding_id = erpx_cqrs_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_read.current_holding_id())',
            schema_name, table_name
        );
    END LOOP;
END;
$$;


-- Rebuildable Wave 4 read models. Every row is generation-scoped so a shadow
-- rebuild can be activated without mutating the currently active projection.

CREATE SCHEMA IF NOT EXISTS erpx_identity_access_read;

CREATE TABLE erpx_identity_access_read.principal_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'identity_access.principals'
        CHECK (projection_name = 'identity_access.principals'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    principal_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    principal_status TEXT NOT NULL CHECK (principal_status IN (
        'invited', 'active', 'suspended', 'disabled'
    )),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, principal_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX identity_access_principal_scope_status
    ON erpx_identity_access_read.principal_projection
        (holding_id, generation, scope_key, principal_status, aggregate_updated_at DESC, principal_id DESC);

CREATE TABLE erpx_identity_access_read.role_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'identity_access.roles'
        CHECK (projection_name = 'identity_access.roles'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    role_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
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
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, role_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, role_code)
);

CREATE INDEX identity_access_role_scope_risk
    ON erpx_identity_access_read.role_projection
        (holding_id, generation, scope_key, risk_tier, aggregate_updated_at DESC, role_id DESC);

CREATE TABLE erpx_identity_access_read.assignment_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
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
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, assignment_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    CHECK (valid_until > valid_from)
);

CREATE INDEX identity_access_assignment_principal_validity
    ON erpx_identity_access_read.assignment_projection
        (holding_id, generation, principal_id, valid_from, valid_until)
    WHERE revoked_at IS NULL;

CREATE SCHEMA IF NOT EXISTS erpx_organization_read;

CREATE TABLE erpx_organization_read.node_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'organization.nodes'
        CHECK (projection_name = 'organization.nodes'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    node_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    node_kind TEXT NOT NULL CHECK (node_kind IN (
        'holding', 'company', 'headquarters', 'branch', 'department', 'team'
    )),
    node_status TEXT NOT NULL CHECK (node_status IN ('active', 'suspended', 'archived')),
    parent_node_id UUID,
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    placement JSONB NOT NULL,
    node_code TEXT NOT NULL,
    display_name TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, node_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX organization_node_scope_kind_status
    ON erpx_organization_read.node_projection
        (holding_id, generation, scope_key, node_kind, node_status, aggregate_updated_at DESC, node_id DESC);

CREATE INDEX organization_node_parent
    ON erpx_organization_read.node_projection
        (holding_id, generation, parent_node_id, aggregate_updated_at DESC, node_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_commercial_read;

CREATE TABLE erpx_commercial_read.catalog_plan_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'commercial.catalog_plans'
        CHECK (projection_name = 'commercial.catalog_plans'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    catalog_plan_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    plan_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    plan_status TEXT NOT NULL CHECK (plan_status IN ('draft', 'published')),
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, catalog_plan_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, plan_key)
);

CREATE INDEX commercial_catalog_status
    ON erpx_commercial_read.catalog_plan_projection
        (holding_id, generation, plan_status, aggregate_updated_at DESC, catalog_plan_id DESC);

CREATE TABLE erpx_commercial_read.subscription_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'commercial.subscriptions'
        CHECK (projection_name = 'commercial.subscriptions'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    subscription_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    catalog_plan_id UUID NOT NULL,
    subscription_status TEXT NOT NULL CHECK (subscription_status IN (
        'trial', 'active', 'past_due', 'suspended', 'ended'
    )),
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, subscription_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX commercial_subscription_customer_status
    ON erpx_commercial_read.subscription_projection
        (holding_id, generation, scope_key, customer_id, subscription_status,
         aggregate_updated_at DESC, subscription_id DESC);

CREATE TABLE erpx_commercial_read.entitlement_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'commercial.entitlements'
        CHECK (projection_name = 'commercial.entitlements'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    entitlement_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    subscription_id UUID NOT NULL,
    capability TEXT NOT NULL,
    enabled BOOLEAN NOT NULL,
    seat_capacity BIGINT NOT NULL CHECK (seat_capacity > 0),
    company_capacity BIGINT NOT NULL CHECK (company_capacity > 0),
    branch_capacity BIGINT NOT NULL CHECK (branch_capacity > 0),
    allocated_seats BIGINT NOT NULL CHECK (allocated_seats >= 0 AND allocated_seats <= seat_capacity),
    allocated_companies BIGINT NOT NULL CHECK (allocated_companies >= 0 AND allocated_companies <= company_capacity),
    allocated_branches BIGINT NOT NULL CHECK (allocated_branches >= 0 AND allocated_branches <= branch_capacity),
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, entitlement_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, subscription_id, capability)
);

CREATE INDEX commercial_entitlement_subscription
    ON erpx_commercial_read.entitlement_projection
        (holding_id, generation, scope_key, subscription_id, enabled,
         aggregate_updated_at DESC, entitlement_id DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_identity_access_read','principal_projection'),
        ('erpx_identity_access_read','role_projection'),
        ('erpx_identity_access_read','assignment_projection'),
        ('erpx_organization_read','node_projection'),
        ('erpx_commercial_read','catalog_plan_projection'),
        ('erpx_commercial_read','subscription_projection'),
        ('erpx_commercial_read','entitlement_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY holding_isolation ON %I.%I '
            'USING (holding_id = erpx_cqrs_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_read.current_holding_id())',
            schema_name, table_name
        );
    END LOOP;
END;
$$;


-- Catalog plans are projected once in control read, never once per holding.
DROP TABLE IF EXISTS erpx_commercial_read.catalog_plan_projection;



-- Disposable Wave 6 projections. Generation-scoped rows permit shadow
-- rebuilds and atomic activation while command-side writes continue.

CREATE SCHEMA IF NOT EXISTS erpx_marketing_read;
CREATE TABLE erpx_marketing_read.lead_candidate_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'marketing.lead_candidates'
        CHECK (projection_name = 'marketing.lead_candidates'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    lead_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    campaign_id UUID NOT NULL,
    audience_member_id UUID NOT NULL,
    source TEXT NOT NULL,
    existing_customer_id UUID,
    score_basis_points INTEGER CHECK (score_basis_points BETWEEN 0 AND 10000),
    lead_status TEXT NOT NULL CHECK (lead_status IN ('open', 'qualified', 'rejected')),
    engagement_count BIGINT NOT NULL CHECK (engagement_count >= 0),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, lead_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);
CREATE INDEX marketing_read_scope_status
    ON erpx_marketing_read.lead_candidate_projection
        (holding_id, generation, scope_key, lead_status, aggregate_updated_at DESC, lead_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_crm_read;
CREATE TABLE erpx_crm_read.lead_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'crm.leads'
        CHECK (projection_name = 'crm.leads'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    lead_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    marketing_candidate_id UUID NOT NULL,
    customer_id UUID,
    source TEXT NOT NULL,
    qualification_score_basis_points INTEGER NOT NULL
        CHECK (qualification_score_basis_points BETWEEN 0 AND 10000),
    accepted_at TIMESTAMPTZ NOT NULL,
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, lead_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, marketing_candidate_id)
);
CREATE TABLE erpx_crm_read.opportunity_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'crm.opportunities'
        CHECK (projection_name = 'crm.opportunities'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    opportunity_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    accepted_lead_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    name TEXT NOT NULL,
    expected_value_minor BIGINT NOT NULL CHECK (expected_value_minor >= 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    opportunity_stage TEXT NOT NULL CHECK (opportunity_stage IN (
        'qualified', 'discovery', 'proposal', 'negotiation', 'won', 'lost'
    )),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, opportunity_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);
CREATE INDEX crm_read_opportunity_scope_customer_stage
    ON erpx_crm_read.opportunity_projection
        (holding_id, generation, scope_key, customer_id, opportunity_stage,
         aggregate_updated_at DESC, opportunity_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_procurement_read;
CREATE TABLE erpx_procurement_read.purchase_order_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'procurement.purchase_orders'
        CHECK (projection_name = 'procurement.purchase_orders'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    purchase_order_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    order_code TEXT NOT NULL,
    supplier_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    order_status TEXT NOT NULL CHECK (order_status IN ('draft', 'released', 'cancelled')),
    lines JSONB NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, purchase_order_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, order_code)
);
CREATE INDEX procurement_read_scope_supplier_status
    ON erpx_procurement_read.purchase_order_projection
        (holding_id, generation, scope_key, supplier_id, order_status,
         aggregate_updated_at DESC, purchase_order_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_inventory_read;
CREATE TABLE erpx_inventory_read.stock_item_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'inventory.stock_items'
        CHECK (projection_name = 'inventory.stock_items'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    stock_item_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    warehouse_id UUID NOT NULL,
    item_code TEXT NOT NULL,
    on_hand BIGINT NOT NULL CHECK (on_hand >= 0),
    reserved BIGINT NOT NULL CHECK (reserved >= 0 AND reserved <= on_hand),
    available BIGINT NOT NULL CHECK (available = on_hand - reserved),
    reservations JSONB NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, stock_item_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, warehouse_id, item_code)
);
CREATE INDEX inventory_read_scope_warehouse_code
    ON erpx_inventory_read.stock_item_projection
        (holding_id, generation, scope_key, warehouse_id, item_code,
         aggregate_updated_at DESC, stock_item_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_assets_read;
CREATE TABLE erpx_assets_read.asset_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'assets.assets'
        CHECK (projection_name = 'assets.assets'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    asset_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    asset_code TEXT NOT NULL,
    asset_name TEXT NOT NULL,
    location_id UUID NOT NULL,
    asset_status TEXT NOT NULL CHECK (asset_status IN (
        'draft', 'in_service', 'under_maintenance', 'retired'
    )),
    asset_condition TEXT NOT NULL CHECK (asset_condition IN (
        'operational', 'degraded', 'out_of_service', 'retired'
    )),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, asset_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, asset_code)
);
CREATE INDEX assets_read_scope_location_status
    ON erpx_assets_read.asset_projection
        (holding_id, generation, scope_key, location_id, asset_status,
         aggregate_updated_at DESC, asset_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_maintenance_read;
CREATE TABLE erpx_maintenance_read.work_order_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'maintenance.work_orders'
        CHECK (projection_name = 'maintenance.work_orders'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    work_order_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    work_order_code TEXT NOT NULL,
    asset_id UUID NOT NULL,
    request_code TEXT NOT NULL,
    source_event_id UUID NOT NULL,
    source_asset_version BIGINT NOT NULL CHECK (source_asset_version > 0),
    title TEXT NOT NULL,
    work_order_status TEXT NOT NULL CHECK (work_order_status IN (
        'planned', 'scheduled', 'in_progress', 'blocked', 'completed', 'cancelled'
    )),
    scheduled_for TIMESTAMPTZ,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, work_order_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, source_event_id),
    UNIQUE (holding_id, generation, scope_key, work_order_code)
);
CREATE INDEX maintenance_read_scope_asset_status
    ON erpx_maintenance_read.work_order_projection
        (holding_id, generation, scope_key, asset_id, work_order_status,
         aggregate_updated_at DESC, work_order_id DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_marketing_read','lead_candidate_projection'),
        ('erpx_crm_read','lead_projection'),
        ('erpx_crm_read','opportunity_projection'),
        ('erpx_procurement_read','purchase_order_projection'),
        ('erpx_inventory_read','stock_item_projection'),
        ('erpx_assets_read','asset_projection'),
        ('erpx_maintenance_read','work_order_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY holding_isolation ON %I.%I '
            'USING (holding_id = erpx_cqrs_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_read.current_holding_id())',
            schema_name, table_name
        );
    END LOOP;
END;
$$;


-- Disposable Wave 7 transactional projections. Every row is fenced by a
-- projection generation so shadow rebuilds can be activated atomically while
-- command-side writes and outbox publication continue.

CREATE SCHEMA IF NOT EXISTS erpx_sales_read;
CREATE TABLE erpx_sales_read.quote_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'sales.quotes'
        CHECK (projection_name = 'sales.quotes'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    quote_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    opportunity_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    quote_code TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    total_minor BIGINT NOT NULL CHECK (total_minor >= 0),
    valid_until TIMESTAMPTZ NOT NULL,
    quote_status TEXT NOT NULL CHECK (quote_status IN (
        'draft', 'sent', 'accepted', 'rejected', 'expired'
    )),
    lines JSONB NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, quote_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, quote_code)
);
CREATE TABLE erpx_sales_read.sales_order_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'sales.orders'
        CHECK (projection_name = 'sales.orders'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    sales_order_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    quote_id UUID NOT NULL,
    opportunity_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    order_code TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    total_minor BIGINT NOT NULL CHECK (total_minor >= 0),
    order_status TEXT NOT NULL CHECK (order_status IN (
        'draft', 'confirmed', 'partially_fulfilled', 'fulfilled', 'cancelled'
    )),
    lines JSONB NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, sales_order_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, quote_id),
    UNIQUE (holding_id, generation, scope_key, order_code)
);
CREATE INDEX sales_read_quote_scope_customer_status
    ON erpx_sales_read.quote_projection
        (holding_id, generation, scope_key, customer_id, quote_status,
         aggregate_updated_at DESC, quote_id DESC);
CREATE INDEX sales_read_order_scope_customer_status
    ON erpx_sales_read.sales_order_projection
        (holding_id, generation, scope_key, customer_id, order_status,
         aggregate_updated_at DESC, sales_order_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_people_payroll_read;
CREATE TABLE erpx_people_payroll_read.employee_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'people_payroll.employees'
        CHECK (projection_name = 'people_payroll.employees'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    employee_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    employee_code TEXT NOT NULL,
    display_name TEXT NOT NULL,
    hired_on DATE NOT NULL,
    terminated_on DATE,
    employee_status TEXT NOT NULL CHECK (employee_status IN (
        'draft', 'active', 'on_leave', 'suspended', 'terminated'
    )),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, employee_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, employee_code)
);
CREATE TABLE erpx_people_payroll_read.leave_request_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'people_payroll.leave_requests'
        CHECK (projection_name = 'people_payroll.leave_requests'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    leave_request_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    employee_id UUID NOT NULL,
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    leave_status TEXT NOT NULL CHECK (leave_status IN (
        'draft', 'submitted', 'approved', 'rejected', 'cancelled', 'taken'
    )),
    requested_by TEXT NOT NULL,
    approved_by TEXT,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, leave_request_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);
CREATE TABLE erpx_people_payroll_read.expense_claim_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'people_payroll.expense_claims'
        CHECK (projection_name = 'people_payroll.expense_claims'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    expense_claim_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    employee_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    amount_minor BIGINT NOT NULL CHECK (amount_minor >= 0),
    expense_status TEXT NOT NULL CHECK (expense_status IN (
        'draft', 'submitted', 'approved', 'rejected', 'reimbursed'
    )),
    claimed_by TEXT NOT NULL,
    approved_by TEXT,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, expense_claim_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);
CREATE TABLE erpx_people_payroll_read.payroll_run_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'people_payroll.payroll_runs'
        CHECK (projection_name = 'people_payroll.payroll_runs'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    payroll_run_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    period_code TEXT NOT NULL,
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    payroll_status TEXT NOT NULL CHECK (payroll_status IN (
        'draft', 'review', 'approved', 'posted', 'cancelled'
    )),
    prepared_by TEXT NOT NULL,
    reviewed_by TEXT,
    approved_by TEXT,
    total_gross_minor BIGINT NOT NULL CHECK (total_gross_minor >= 0),
    total_deductions_minor BIGINT NOT NULL CHECK (total_deductions_minor >= 0),
    total_net_minor BIGINT NOT NULL CHECK (total_net_minor >= 0),
    lines JSONB NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, payroll_run_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, period_code)
);
CREATE INDEX people_payroll_read_employee_scope_status
    ON erpx_people_payroll_read.employee_projection
        (holding_id, generation, scope_key, employee_status,
         aggregate_updated_at DESC, employee_id DESC);
CREATE INDEX people_payroll_read_leave_scope_employee_status
    ON erpx_people_payroll_read.leave_request_projection
        (holding_id, generation, scope_key, employee_id, leave_status,
         aggregate_updated_at DESC, leave_request_id DESC);
CREATE INDEX people_payroll_read_expense_scope_employee_status
    ON erpx_people_payroll_read.expense_claim_projection
        (holding_id, generation, scope_key, employee_id, expense_status,
         aggregate_updated_at DESC, expense_claim_id DESC);
CREATE INDEX people_payroll_read_run_scope_status_period
    ON erpx_people_payroll_read.payroll_run_projection
        (holding_id, generation, scope_key, payroll_status, ends_on DESC, payroll_run_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_pos_read;
CREATE TABLE erpx_pos_read.sale_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'pos.sales'
        CHECK (projection_name = 'pos.sales'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    sale_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    receipt_code TEXT NOT NULL,
    register_id UUID NOT NULL,
    customer_id UUID,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    subtotal_minor BIGINT NOT NULL CHECK (subtotal_minor >= 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    total_minor BIGINT NOT NULL CHECK (total_minor >= 0),
    paid_minor BIGINT NOT NULL CHECK (paid_minor >= 0),
    sale_status TEXT NOT NULL CHECK (sale_status IN (
        'draft', 'open', 'held', 'completed', 'voided'
    )),
    lines JSONB NOT NULL,
    payments JSONB NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, sale_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, receipt_code)
);
CREATE TABLE erpx_pos_read.refund_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'pos.refunds'
        CHECK (projection_name = 'pos.refunds'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    refund_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    sale_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    total_amount_minor BIGINT NOT NULL CHECK (total_amount_minor > 0),
    refund_status TEXT NOT NULL CHECK (refund_status IN (
        'requested', 'approved', 'rejected', 'posted', 'cancelled'
    )),
    requester_principal_ref TEXT NOT NULL,
    reviewer_principal_ref TEXT,
    lines JSONB NOT NULL,
    state JSONB NOT NULL,
    aggregate_requested_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, refund_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);
CREATE INDEX pos_read_sale_scope_register_status
    ON erpx_pos_read.sale_projection
        (holding_id, generation, scope_key, register_id, sale_status,
         aggregate_updated_at DESC, sale_id DESC);
CREATE INDEX pos_read_refund_scope_sale_status
    ON erpx_pos_read.refund_projection
        (holding_id, generation, scope_key, sale_id, refund_status,
         aggregate_updated_at DESC, refund_id DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_sales_read','quote_projection'),
        ('erpx_sales_read','sales_order_projection'),
        ('erpx_people_payroll_read','employee_projection'),
        ('erpx_people_payroll_read','leave_request_projection'),
        ('erpx_people_payroll_read','expense_claim_projection'),
        ('erpx_people_payroll_read','payroll_run_projection'),
        ('erpx_pos_read','sale_projection'),
        ('erpx_pos_read','refund_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY holding_isolation ON %I.%I '
            'USING (holding_id = erpx_cqrs_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_read.current_holding_id())',
            schema_name, table_name
        );
    END LOOP;
END;
$$;


-- Disposable Wave 8 projections. Every table is fenced by a projection
-- generation so rebuilds can be populated and activated without reading or
-- blocking the authoritative write database.

CREATE SCHEMA IF NOT EXISTS erpx_finance_read;

CREATE TABLE erpx_finance_read.chart_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'finance.charts'
        CHECK (projection_name = 'finance.charts'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    chart_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    chart_code TEXT NOT NULL,
    chart_name TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    accounts JSONB NOT NULL CHECK (jsonb_typeof(accounts) = 'array'),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, chart_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, chart_code)
);

CREATE TABLE erpx_finance_read.period_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'finance.periods'
        CHECK (projection_name = 'finance.periods'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    period_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    period_code TEXT NOT NULL,
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    period_status TEXT NOT NULL CHECK (period_status IN ('open', 'closing', 'closed')),
    close_requested_by TEXT,
    closed_by TEXT,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, period_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, period_code)
);

CREATE TABLE erpx_finance_read.journal_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'finance.journals'
        CHECK (projection_name = 'finance.journals'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    journal_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    chart_id UUID NOT NULL,
    period_id UUID NOT NULL,
    journal_code TEXT NOT NULL,
    entry_date DATE NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    source_kind TEXT NOT NULL,
    source JSONB NOT NULL,
    description TEXT NOT NULL,
    lines JSONB NOT NULL CHECK (jsonb_typeof(lines) = 'array'),
    debit_total_minor BIGINT NOT NULL,
    credit_total_minor BIGINT NOT NULL,
    journal_status TEXT NOT NULL CHECK (journal_status IN ('draft', 'posted')),
    posted_at TIMESTAMPTZ,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, journal_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, journal_code),
    CHECK (debit_total_minor = credit_total_minor)
);

CREATE TABLE erpx_finance_read.subledger_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'finance.subledger'
        CHECK (projection_name = 'finance.subledger'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    subledger_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    subledger_kind TEXT NOT NULL CHECK (subledger_kind IN ('receivable', 'payable')),
    party_id UUID NOT NULL,
    document_code TEXT NOT NULL,
    source_reference TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    original_minor BIGINT NOT NULL,
    outstanding_minor BIGINT NOT NULL,
    subledger_status TEXT NOT NULL CHECK (
        subledger_status IN ('open', 'partially_settled', 'settled')
    ),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, subledger_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, subledger_kind, source_reference)
);

CREATE TABLE erpx_finance_read.trial_balance_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'finance.trial_balance'
        CHECK (projection_name = 'finance.trial_balance'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    chart_id UUID NOT NULL,
    account_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    account_code TEXT NOT NULL,
    account_name TEXT NOT NULL,
    debit_minor BIGINT NOT NULL,
    credit_minor BIGINT NOT NULL,
    net_minor BIGINT NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, chart_id, account_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_finance_read.bank_transaction_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'finance.bank_transactions'
        CHECK (projection_name = 'finance.bank_transactions'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    transaction_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    journal_id UUID NOT NULL,
    import_id UUID NOT NULL,
    reference TEXT NOT NULL,
    posted_on DATE NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor <> 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, transaction_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, import_id, transaction_id)
);

CREATE TABLE erpx_finance_read.statutory_tax_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'finance.statutory_tax'
        CHECK (projection_name = 'finance.statutory_tax'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    tax_fact_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    journal_id UUID NOT NULL,
    reference TEXT NOT NULL,
    tax_date DATE NOT NULL,
    tax_period TEXT NOT NULL,
    tax_kind TEXT NOT NULL CHECK (tax_kind IN ('output_vat', 'input_vat', 'withholding')),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    base_minor BIGINT NOT NULL,
    tax_minor BIGINT NOT NULL,
    tax_rate_basis_points INTEGER NOT NULL,
    tax_identifier TEXT NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, tax_fact_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX finance_read_chart_scope_updated
    ON erpx_finance_read.chart_projection
        (holding_id, generation, scope_key, aggregate_updated_at DESC, chart_id DESC);
CREATE INDEX finance_read_period_scope_status
    ON erpx_finance_read.period_projection
        (holding_id, generation, scope_key, period_status, ends_on DESC, period_id DESC);
CREATE INDEX finance_read_journal_scope_period_status
    ON erpx_finance_read.journal_projection
        (holding_id, generation, scope_key, period_id, journal_status, entry_date DESC, journal_id DESC);
CREATE INDEX finance_read_subledger_scope_status
    ON erpx_finance_read.subledger_projection
        (holding_id, generation, scope_key, subledger_kind, subledger_status,
         aggregate_updated_at DESC, subledger_id DESC);
CREATE INDEX finance_read_trial_balance_scope_chart
    ON erpx_finance_read.trial_balance_projection
        (holding_id, generation, scope_key, chart_id, account_code, account_id);
CREATE INDEX finance_read_bank_scope_date
    ON erpx_finance_read.bank_transaction_projection
        (holding_id, generation, scope_key, posted_on DESC, transaction_id DESC);
CREATE INDEX finance_read_tax_scope_period
    ON erpx_finance_read.statutory_tax_projection
        (holding_id, generation, scope_key, tax_period, currency, tax_date, tax_fact_id);

CREATE SCHEMA IF NOT EXISTS erpx_legal_read;

CREATE TABLE erpx_legal_read.contract_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'legal.contracts'
        CHECK (projection_name = 'legal.contracts'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    contract_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    reference TEXT NOT NULL,
    title TEXT NOT NULL,
    object_ref TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    retention_until TIMESTAMPTZ NOT NULL,
    contract_status TEXT NOT NULL CHECK (contract_status IN (
        'draft', 'review', 'approved', 'active', 'expired', 'terminated', 'archived'
    )),
    parties JSONB NOT NULL CHECK (jsonb_typeof(parties) = 'array'),
    created_by TEXT NOT NULL,
    approved_by TEXT,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, contract_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, reference)
);

CREATE TABLE erpx_legal_read.document_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'legal.documents'
        CHECK (projection_name = 'legal.documents'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    document_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    title TEXT NOT NULL,
    versions JSONB NOT NULL CHECK (jsonb_typeof(versions) = 'array'),
    current_document_version BIGINT NOT NULL CHECK (
        current_document_version >= 0 AND current_document_version <= 1024
    ),
    archived BOOLEAN NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, document_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_legal_read.access_history_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'legal.access_history'
        CHECK (projection_name = 'legal.access_history'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    access_entry_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version = 1),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    document_id UUID NOT NULL,
    document_version BIGINT NOT NULL CHECK (
        document_version > 0 AND document_version <= 1024
    ),
    actor_ref TEXT NOT NULL,
    action TEXT NOT NULL,
    purpose TEXT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, access_entry_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX legal_read_contract_scope_status
    ON erpx_legal_read.contract_projection
        (holding_id, generation, scope_key, contract_status, aggregate_updated_at DESC, contract_id DESC);
CREATE INDEX legal_read_document_scope_archived
    ON erpx_legal_read.document_projection
        (holding_id, generation, scope_key, archived, aggregate_updated_at DESC, document_id DESC);
CREATE INDEX legal_read_access_scope_document_time
    ON erpx_legal_read.access_history_projection
        (holding_id, generation, scope_key, document_id, observed_at DESC, access_entry_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_support_read;

CREATE TABLE erpx_support_read.ticket_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'support.tickets'
        CHECK (projection_name = 'support.tickets'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    ticket_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    case_code TEXT NOT NULL,
    category TEXT NOT NULL,
    severity TEXT NOT NULL,
    description TEXT NOT NULL,
    ticket_status TEXT NOT NULL CHECK (ticket_status IN (
        'new', 'triaged', 'waiting_for_customer', 'access_requested',
        'approved', 'in_progress', 'resolved', 'customer_confirmed', 'closed'
    )),
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, ticket_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, case_code)
);

CREATE TABLE erpx_support_read.ticket_customer_message_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'support.tickets'
        CHECK (projection_name = 'support.tickets'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    ticket_id UUID NOT NULL,
    message_id UUID NOT NULL,
    author_ref TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, ticket_id, message_id),
    FOREIGN KEY (holding_id, projection_name, generation, partition_key, ticket_id)
        REFERENCES erpx_support_read.ticket_projection
            (holding_id, projection_name, generation, partition_key, ticket_id)
        ON DELETE CASCADE
);

CREATE TABLE erpx_support_read.ticket_internal_message_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'support.tickets'
        CHECK (projection_name = 'support.tickets'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    ticket_id UUID NOT NULL,
    message_id UUID NOT NULL,
    author_ref TEXT NOT NULL,
    audience TEXT NOT NULL CHECK (audience IN ('customer_visible', 'internal')),
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, ticket_id, message_id),
    FOREIGN KEY (holding_id, projection_name, generation, partition_key, ticket_id)
        REFERENCES erpx_support_read.ticket_projection
            (holding_id, projection_name, generation, partition_key, ticket_id)
        ON DELETE CASCADE
);

CREATE TABLE erpx_support_read.incident_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'support.incidents'
        CHECK (projection_name = 'support.incidents'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    incident_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    incident_code TEXT NOT NULL,
    severity TEXT NOT NULL,
    commander_ref TEXT NOT NULL,
    affected_services JSONB NOT NULL,
    affected_customer_accounts JSONB NOT NULL,
    incident_status TEXT NOT NULL CHECK (incident_status IN (
        'investigating', 'identified', 'monitoring', 'resolved', 'postmortem_complete'
    )),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, incident_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, incident_code)
);

CREATE TABLE erpx_support_read.session_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'support.sessions'
        CHECK (projection_name = 'support.sessions'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    session_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    approved_scope JSONB NOT NULL CHECK (jsonb_typeof(approved_scope) = 'object'),
    scope_key TEXT NOT NULL,
    ticket_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    agent_principal_id UUID NOT NULL,
    represented_principal_id UUID,
    support_mode TEXT NOT NULL CHECK (support_mode IN (
        'diagnostic_read', 'holding_mutation', 'impersonation'
    )),
    permissions JSONB NOT NULL CHECK (
        jsonb_typeof(permissions) = 'array' AND jsonb_array_length(permissions) > 0
    ),
    module TEXT NOT NULL,
    reason TEXT NOT NULL,
    session_status TEXT NOT NULL CHECK (session_status IN ('requested', 'approved', 'revoked')),
    requested_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    approved_by TEXT,
    approved_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    revoked_reason TEXT,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, session_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    CHECK (expires_at > requested_at),
    CHECK ((support_mode = 'impersonation') = (represented_principal_id IS NOT NULL)),
    CHECK ((approved_by IS NULL) = (approved_at IS NULL)),
    CHECK ((revoked_at IS NULL) = (revoked_reason IS NULL)),
    CHECK (
        (session_status = 'requested' AND approved_at IS NULL AND revoked_at IS NULL)
        OR (session_status = 'approved' AND approved_at IS NOT NULL AND revoked_at IS NULL)
        OR (session_status = 'revoked' AND revoked_at IS NOT NULL)
    )
);

CREATE TABLE erpx_support_read.capability_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'support.capabilities'
        CHECK (projection_name = 'support.capabilities'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    capability_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    session_id UUID NOT NULL,
    ticket_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    agent_principal_id UUID NOT NULL,
    device_ref TEXT NOT NULL,
    support_mode TEXT NOT NULL CHECK (support_mode IN (
        'diagnostic_read', 'holding_mutation', 'impersonation'
    )),
    permissions JSONB NOT NULL CHECK (
        jsonb_typeof(permissions) = 'array' AND jsonb_array_length(permissions) > 0
    ),
    approved_scope JSONB NOT NULL CHECK (jsonb_typeof(approved_scope) = 'object'),
    scope_key TEXT NOT NULL,
    module TEXT NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revocation_reason TEXT CHECK (revocation_reason IN (
        'customer_requested', 'agent_disabled', 'device_revoked',
        'session_revoked', 'ticket_closed', 'administrative'
    )),
    revoked_at TIMESTAMPTZ,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, capability_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    CHECK (expires_at > issued_at),
    CHECK ((revocation_reason IS NULL) = (revoked_at IS NULL))
);

CREATE INDEX support_read_ticket_scope_customer_status
    ON erpx_support_read.ticket_projection
        (holding_id, generation, scope_key, customer_id, ticket_status,
         aggregate_updated_at DESC, ticket_id DESC);
CREATE INDEX support_read_incident_scope_status
    ON erpx_support_read.incident_projection
        (holding_id, generation, scope_key, incident_status,
         aggregate_updated_at DESC, incident_id DESC);
CREATE INDEX support_read_session_scope_ticket_status
    ON erpx_support_read.session_projection
        (holding_id, generation, scope_key, ticket_id, session_status, expires_at DESC, session_id DESC);
CREATE INDEX support_read_capability_scope_session_expiry
    ON erpx_support_read.capability_projection
        (holding_id, generation, scope_key, session_id, expires_at DESC, capability_id DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_finance_read','chart_projection'),
        ('erpx_finance_read','period_projection'),
        ('erpx_finance_read','journal_projection'),
        ('erpx_finance_read','subledger_projection'),
        ('erpx_finance_read','trial_balance_projection'),
        ('erpx_finance_read','bank_transaction_projection'),
        ('erpx_finance_read','statutory_tax_projection'),
        ('erpx_legal_read','contract_projection'),
        ('erpx_legal_read','document_projection'),
        ('erpx_legal_read','access_history_projection'),
        ('erpx_support_read','ticket_projection'),
        ('erpx_support_read','ticket_customer_message_projection'),
        ('erpx_support_read','ticket_internal_message_projection'),
        ('erpx_support_read','incident_projection'),
        ('erpx_support_read','session_projection'),
        ('erpx_support_read','capability_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY holding_isolation ON %I.%I '
            'USING (holding_id = erpx_cqrs_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_read.current_holding_id())',
            schema_name, table_name
        );
    END LOOP;
END;
$$;


-- Wave 9 disposable Operations projections and topology-neutral per-source
-- cursors. A source cursor is never compared with a cursor from another DB.

CREATE TABLE erpx_cqrs_read.projection_source_checkpoints (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL,
    generation UUID NOT NULL,
    source_key TEXT NOT NULL CHECK (octet_length(source_key) BETWEEN 1 AND 255),
    source_sequence BIGINT NOT NULL DEFAULT 0 CHECK (source_sequence >= 0),
    rebuild_target_sequence BIGINT CHECK (rebuild_target_sequence >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, source_key),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    CHECK (updated_at >= created_at)
);

CREATE INDEX cqrs_holding_projection_source_rebuild
    ON erpx_cqrs_read.projection_source_checkpoints
        (holding_id, projection_name, generation, rebuild_target_sequence, source_sequence);

CREATE SCHEMA IF NOT EXISTS erpx_operations_read;

CREATE FUNCTION erpx_operations_read.valid_source_context_array(values_to_check JSONB)
RETURNS BOOLEAN
LANGUAGE sql
IMMUTABLE
STRICT
AS $$
    SELECT CASE
        WHEN jsonb_typeof(values_to_check) <> 'array' THEN FALSE
        ELSE (
            SELECT count(*) BETWEEN 1 AND 16
               AND count(*) = count(DISTINCT value)
               AND bool_and(value #>> '{}' = ANY (ARRAY[
                   'commercial', 'customer', 'marketing', 'crm', 'procurement',
                   'inventory', 'assets', 'maintenance', 'quality', 'sales',
                   'people_payroll', 'pos', 'finance', 'support'
               ]::TEXT[]))
            FROM jsonb_array_elements(values_to_check) AS value
        )
    END
$$;

CREATE TABLE erpx_operations_read.source_signal_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'operations.source_signals'
        CHECK (projection_name = 'operations.source_signals'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    process_id UUID NOT NULL,
    source_context TEXT NOT NULL CHECK (source_context IN (
        'commercial', 'customer', 'marketing', 'crm', 'procurement', 'inventory',
        'assets', 'maintenance', 'quality', 'sales', 'people_payroll', 'pos',
        'finance', 'support'
    )),
    source_event_id UUID NOT NULL,
    source_event_type TEXT NOT NULL,
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_aggregate_type TEXT NOT NULL,
    source_aggregate_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    process_code TEXT NOT NULL,
    operational_state TEXT NOT NULL,
    source_fact JSONB NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, process_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, source_context, source_event_id)
);

CREATE INDEX operations_source_signal_scope_context
    ON erpx_operations_read.source_signal_projection
        (holding_id, generation, scope_key, source_context, observed_at DESC, process_id DESC);

CREATE TABLE erpx_operations_read.exception_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'operations.exceptions'
        CHECK (projection_name = 'operations.exceptions'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    exception_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    reference TEXT NOT NULL,
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    source_context TEXT NOT NULL CHECK (source_context IN (
        'commercial', 'customer', 'marketing', 'crm', 'procurement', 'inventory',
        'assets', 'maintenance', 'quality', 'sales', 'people_payroll', 'pos',
        'finance', 'support'
    )),
    source_event_id UUID NOT NULL,
    source_event_type TEXT NOT NULL,
    source_aggregate_type TEXT NOT NULL,
    source_aggregate_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    source_correlation_id UUID NOT NULL,
    policy_ref TEXT NOT NULL,
    category TEXT NOT NULL,
    severity TEXT NOT NULL,
    assignee_id TEXT NOT NULL,
    manager_id TEXT NOT NULL,
    due_at TIMESTAMPTZ NOT NULL,
    escalation_after_minutes INTEGER NOT NULL,
    exception_status TEXT NOT NULL CHECK (exception_status IN (
        'assigned', 'acknowledged', 'escalated', 'resolved', 'closed'
    )),
    state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, exception_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, source_context, source_event_id)
);

CREATE INDEX operations_exception_read_queue
    ON erpx_operations_read.exception_projection
        (holding_id, generation, scope_key, exception_status, updated_at DESC, exception_id DESC);
CREATE INDEX operations_exception_read_assignee
    ON erpx_operations_read.exception_projection
        (holding_id, generation, scope_key, assignee_id, updated_at DESC, exception_id DESC);
CREATE INDEX operations_exception_read_due
    ON erpx_operations_read.exception_projection
        (holding_id, generation, scope_key, due_at, exception_id);
CREATE INDEX operations_exception_read_scope_cursor
    ON erpx_operations_read.exception_projection
        (holding_id, generation, scope_key, exception_id);
CREATE INDEX operations_exception_read_platform_cursor
    ON erpx_operations_read.exception_projection
        (holding_id, generation, exception_id);

CREATE TABLE erpx_operations_read.work_queue_policy_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'operations.work_queue_policies'
        CHECK (projection_name = 'operations.work_queue_policies'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    policy_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    policy_name TEXT NOT NULL,
    allowed_source_contexts JSONB NOT NULL CHECK (
        jsonb_typeof(allowed_source_contexts) = 'array'
        AND jsonb_array_length(allowed_source_contexts) BETWEEN 1 AND 16
        AND erpx_operations_read.valid_source_context_array(allowed_source_contexts)
    ),
    escalation_after_minutes INTEGER NOT NULL,
    policy_status TEXT NOT NULL CHECK (policy_status IN ('active', 'paused', 'retired')),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, policy_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, policy_name)
);

CREATE INDEX operations_policy_read_scope_status
    ON erpx_operations_read.work_queue_policy_projection
        (holding_id, generation, scope_key, policy_status, updated_at DESC, policy_id DESC);
CREATE INDEX operations_policy_read_scope_cursor
    ON erpx_operations_read.work_queue_policy_projection
        (holding_id, generation, scope_key, policy_id);
CREATE INDEX operations_policy_read_platform_cursor
    ON erpx_operations_read.work_queue_policy_projection
        (holding_id, generation, policy_id);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_cqrs_read','projection_source_checkpoints'),
        ('erpx_operations_read','source_signal_projection'),
        ('erpx_operations_read','exception_projection'),
        ('erpx_operations_read','work_queue_policy_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY holding_isolation ON %I.%I '
            'USING (holding_id = erpx_cqrs_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_read.current_holding_id())',
            schema_name, table_name
        );
    END LOOP;
END;
$$;


-- Disposable Sales revenue-arrangement workbench projection. Source state is
-- rebuilt exclusively from Sales snapshot outbox events.

CREATE TABLE erpx_sales_read.revenue_arrangement_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'sales.revenue_arrangements'
        CHECK (projection_name = 'sales.revenue_arrangements'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    arrangement_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    sales_order_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    legal_contract_id UUID,
    order_code TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    arrangement_status TEXT NOT NULL CHECK (arrangement_status IN (
        'active', 'completed', 'cancelled'
    )),
    dimensions JSONB NOT NULL,
    obligations JSONB NOT NULL CHECK (jsonb_typeof(obligations) = 'array'),
    fulfillments JSONB NOT NULL CHECK (jsonb_typeof(fulfillments) = 'array'),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, arrangement_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, sales_order_id)
);

CREATE INDEX sales_read_revenue_arrangement_scope_customer_status
    ON erpx_sales_read.revenue_arrangement_projection
        (holding_id, generation, scope_key, customer_id, arrangement_status,
         aggregate_updated_at DESC, arrangement_id DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_sales_read','revenue_arrangement_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY holding_isolation ON %I.%I '
            'USING (holding_id = erpx_cqrs_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_read.current_holding_id())',
            schema_name, table_name
        );
    END LOOP;
END;
$$;


-- Disposable Finance customer-invoice workbench projection.

CREATE TABLE erpx_finance_read.customer_invoice_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'finance.customer_invoices'
        CHECK (projection_name = 'finance.customer_invoices'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    invoice_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    invoice_code TEXT NOT NULL,
    customer_id UUID NOT NULL,
    billing_event_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    receivable_minor BIGINT NOT NULL CHECK (receivable_minor > 0),
    due_on DATE NOT NULL,
    invoice_status TEXT NOT NULL CHECK (invoice_status IN ('draft', 'posted', 'cancelled')),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, invoice_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, billing_event_id),
    UNIQUE (holding_id, generation, scope_key, invoice_code)
);

CREATE INDEX finance_read_customer_invoice_scope_customer_status
    ON erpx_finance_read.customer_invoice_projection
        (holding_id, generation, scope_key, customer_id, invoice_status,
         aggregate_updated_at DESC, invoice_id DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_finance_read','customer_invoice_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY holding_isolation ON %I.%I '
            'USING (holding_id = erpx_cqrs_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_read.current_holding_id())',
            schema_name, table_name
        );
    END LOOP;
END;
$$;


-- Disposable billing-independent revenue-recognition workbench.

CREATE TABLE erpx_finance_read.revenue_schedule_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'finance.revenue_schedules'
        CHECK (projection_name = 'finance.revenue_schedules'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    schedule_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    arrangement_id UUID NOT NULL,
    sales_order_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    allocated_minor BIGINT NOT NULL CHECK (allocated_minor > 0),
    recognized_minor BIGINT NOT NULL CHECK (
        recognized_minor >= 0 AND recognized_minor <= allocated_minor
    ),
    schedule_status TEXT NOT NULL CHECK (schedule_status IN ('active', 'completed')),
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, schedule_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, arrangement_id)
);

CREATE INDEX finance_read_revenue_schedule_scope_customer_status
    ON erpx_finance_read.revenue_schedule_projection
        (holding_id, generation, scope_key, customer_id, schedule_status,
         aggregate_updated_at DESC, schedule_id DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_finance_read','revenue_schedule_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY holding_isolation ON %I.%I '
            'USING (holding_id = erpx_cqrs_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_read.current_holding_id())',
            schema_name, table_name
        );
    END LOOP;
END;
$$;


-- Forecast cockpit fields are disposable and rebuild from CRM snapshots.

ALTER TABLE erpx_crm_read.opportunity_projection
    ADD COLUMN account_id UUID,
    ADD COLUMN owner_ref TEXT,
    ADD COLUMN pipeline_category TEXT CHECK (
        pipeline_category IN ('pipeline', 'best_case', 'commit', 'closed')
    ),
    ADD COLUMN probability_basis_points INTEGER CHECK (
        probability_basis_points BETWEEN 0 AND 10000
    ),
    ADD COLUMN expected_close_on DATE,
    ADD COLUMN originating_campaign_id UUID,
    ADD CONSTRAINT crm_read_opportunity_forecast_complete CHECK (
        (account_id IS NULL AND owner_ref IS NULL AND pipeline_category IS NULL
         AND probability_basis_points IS NULL AND expected_close_on IS NULL)
        OR
        (account_id IS NOT NULL AND owner_ref IS NOT NULL AND pipeline_category IS NOT NULL
         AND probability_basis_points IS NOT NULL AND expected_close_on IS NOT NULL)
    );

CREATE INDEX crm_read_opportunity_forecast_cockpit
    ON erpx_crm_read.opportunity_projection
        (generation, scope_key, pipeline_category, expected_close_on, opportunity_id);


-- Disposable Legal relationship graph projection rebuilt from matter snapshots.

CREATE TABLE erpx_legal_read.matter_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'legal.matters'
        CHECK (projection_name = 'legal.matters'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    matter_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    reference TEXT NOT NULL,
    title TEXT NOT NULL,
    matter_status TEXT NOT NULL CHECK (matter_status IN ('open', 'closed')),
    links JSONB NOT NULL CHECK (jsonb_typeof(links) = 'array'),
    legal_hold JSONB NOT NULL,
    created_by TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, matter_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, reference)
);

CREATE INDEX legal_read_matter_scope_status
    ON erpx_legal_read.matter_projection
        (holding_id, generation, scope_key, matter_status, aggregate_updated_at DESC, matter_id DESC);
CREATE INDEX legal_read_matter_links_gin
    ON erpx_legal_read.matter_projection USING GIN (links jsonb_path_ops);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_legal_read','matter_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY holding_isolation ON %I.%I '
            'USING (holding_id = erpx_cqrs_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_read.current_holding_id())',
            schema_name, table_name
        );
    END LOOP;
END;
$$;


-- Disposable Marketing Campaign Budget projection. Financial actuals are
-- composed from Procurement and Finance facts and are not authoritative here.

CREATE SCHEMA IF NOT EXISTS erpx_marketing_read;

CREATE TABLE erpx_marketing_read.campaign_budget_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'marketing.campaign_budgets'
        CHECK (projection_name = 'marketing.campaign_budgets'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    budget_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    campaign_id UUID NOT NULL,
    budget_code TEXT NOT NULL,
    budget_name TEXT NOT NULL,
    period TEXT NOT NULL CHECK (period IN ('annual', 'quarterly', 'campaign')),
    budget_status TEXT NOT NULL CHECK (budget_status IN (
        'draft', 'review', 'approved', 'running', 'completed', 'cancelled'
    )),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    requested_minor BIGINT NOT NULL CHECK (requested_minor > 0),
    approved_minor BIGINT CHECK (approved_minor > 0),
    reserved_minor BIGINT NOT NULL CHECK (reserved_minor >= 0),
    available_minor BIGINT NOT NULL,
    dimensions JSONB NOT NULL,
    allocations JSONB NOT NULL,
    reservations JSONB NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, budget_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, budget_code),
    CHECK (available_minor = COALESCE(approved_minor, 0) - reserved_minor)
);

CREATE INDEX marketing_budget_read_scope_campaign_status
    ON erpx_marketing_read.campaign_budget_projection
        (holding_id, generation, scope_key, campaign_id, budget_status,
         aggregate_updated_at DESC, budget_id DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_marketing_read','campaign_budget_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY holding_isolation ON %I.%I '
            'USING (holding_id = erpx_cqrs_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_read.current_holding_id())',
            schema_name, table_name
        );
    END LOOP;
END;
$$;


-- Disposable holding-read database generation marker.
CREATE SCHEMA erpx_meta;

CREATE TABLE erpx_meta.schema_manifest (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    schema_revision SMALLINT NOT NULL CHECK (schema_revision = 2),
    manifest_version SMALLINT NOT NULL CHECK (manifest_version = 1),
    database_role TEXT NOT NULL CHECK (database_role = 'holding_read'),
    migration_set TEXT NOT NULL CHECK (migration_set = 'holding-read'),
    installed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO erpx_meta.schema_manifest
    (singleton, schema_revision, manifest_version, database_role, migration_set)
VALUES (TRUE, 2, 1, 'holding_read', 'holding-read');


-- Rebuildable Collaboration projections. Authoritative comment revisions and
-- moderation evidence remain in the anchor holding-write data plane.

CREATE SCHEMA erpx_collaboration_read;

CREATE FUNCTION erpx_collaboration_read.scope_kind(scope JSONB)
RETURNS TEXT
LANGUAGE sql
IMMUTABLE
STRICT
AS $$
    SELECT CASE
        WHEN jsonb_typeof(scope) = 'string' THEN trim(both '"' from scope::text)
        ELSE (SELECT key FROM jsonb_each(scope) LIMIT 1)
    END
$$;

CREATE FUNCTION erpx_collaboration_read.scope_holding(scope JSONB)
RETURNS TEXT
LANGUAGE sql
IMMUTABLE
STRICT
AS $$
    SELECT COALESCE(
        scope #>> '{Holding,holding_id}',
        scope #>> '{Company,holding_id}',
        scope #>> '{Branch,holding_id}',
        scope #>> '{Subtree,holding_id}'
    )
$$;

CREATE FUNCTION erpx_collaboration_read.scope_contains(authority JSONB, target JSONB)
RETURNS BOOLEAN
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    uuid_pattern CONSTANT TEXT :=
        '^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$';
    nil_uuid CONSTANT UUID := '00000000-0000-0000-0000-000000000000';
    authority_kind TEXT;
    authority_body JSONB;
    authority_holding_text TEXT;
    authority_company_text TEXT;
    authority_branch_text TEXT;
    authority_node_text TEXT;
    authority_holding UUID;
    authority_company UUID;
    authority_branch UUID;
    authority_node UUID;
    target_kind TEXT;
    target_body JSONB;
    target_holding_text TEXT;
    target_company_text TEXT;
    target_branch_text TEXT;
    target_node_text TEXT;
    target_holding UUID;
    target_company UUID;
    target_branch UUID;
    target_node UUID;
    target_node_kind TEXT;
    target_lineage UUID[];
BEGIN
    -- AuthorityScope is an externally tagged enum. Reject unknown, partial,
    -- over-posted, non-canonical, nil, and non-string UUID fields before any
    -- hierarchy lookup so malformed data can never broaden access.
    IF authority IS NULL OR target IS NULL THEN
        RETURN FALSE;
    END IF;
    IF jsonb_typeof(authority) = 'string' THEN
        IF authority <> to_jsonb('Platform'::TEXT) THEN
            RETURN FALSE;
        END IF;
        authority_kind := 'Platform';
    ELSIF jsonb_typeof(authority) = 'object' THEN
        IF (SELECT count(*) FROM jsonb_object_keys(authority)) <> 1 THEN
            RETURN FALSE;
        END IF;
        SELECT entry.key, entry.value
          INTO authority_kind, authority_body
          FROM jsonb_each(authority) AS entry(key,value);
        CASE authority_kind
            WHEN 'Holding' THEN
                authority_holding_text := authority_body ->> 'holding_id';
                IF authority <> jsonb_build_object(
                    'Holding', jsonb_build_object('holding_id', authority_holding_text)
                ) THEN
                    RETURN FALSE;
                END IF;
            WHEN 'Company' THEN
                authority_holding_text := authority_body ->> 'holding_id';
                authority_company_text := authority_body ->> 'company_id';
                IF authority <> jsonb_build_object(
                    'Company', jsonb_build_object(
                        'holding_id', authority_holding_text,
                        'company_id', authority_company_text
                    )
                ) THEN
                    RETURN FALSE;
                END IF;
            WHEN 'Branch' THEN
                authority_holding_text := authority_body ->> 'holding_id';
                authority_company_text := authority_body ->> 'company_id';
                authority_branch_text := authority_body ->> 'branch_id';
                IF authority <> jsonb_build_object(
                    'Branch', jsonb_build_object(
                        'holding_id', authority_holding_text,
                        'company_id', authority_company_text,
                        'branch_id', authority_branch_text
                    )
                ) THEN
                    RETURN FALSE;
                END IF;
            WHEN 'Subtree' THEN
                authority_holding_text := authority_body ->> 'holding_id';
                authority_node_text := authority_body ->> 'organization_node_id';
                IF authority <> jsonb_build_object(
                    'Subtree', jsonb_build_object(
                        'holding_id', authority_holding_text,
                        'organization_node_id', authority_node_text
                    )
                ) THEN
                    RETURN FALSE;
                END IF;
            ELSE
                RETURN FALSE;
        END CASE;
    ELSE
        RETURN FALSE;
    END IF;

    IF jsonb_typeof(target) = 'string' THEN
        IF target <> to_jsonb('Platform'::TEXT) THEN
            RETURN FALSE;
        END IF;
        target_kind := 'Platform';
    ELSIF jsonb_typeof(target) = 'object' THEN
        IF (SELECT count(*) FROM jsonb_object_keys(target)) <> 1 THEN
            RETURN FALSE;
        END IF;
        SELECT entry.key, entry.value
          INTO target_kind, target_body
          FROM jsonb_each(target) AS entry(key,value);
        CASE target_kind
            WHEN 'Holding' THEN
                target_holding_text := target_body ->> 'holding_id';
                IF target <> jsonb_build_object(
                    'Holding', jsonb_build_object('holding_id', target_holding_text)
                ) THEN
                    RETURN FALSE;
                END IF;
            WHEN 'Company' THEN
                target_holding_text := target_body ->> 'holding_id';
                target_company_text := target_body ->> 'company_id';
                IF target <> jsonb_build_object(
                    'Company', jsonb_build_object(
                        'holding_id', target_holding_text,
                        'company_id', target_company_text
                    )
                ) THEN
                    RETURN FALSE;
                END IF;
            WHEN 'Branch' THEN
                target_holding_text := target_body ->> 'holding_id';
                target_company_text := target_body ->> 'company_id';
                target_branch_text := target_body ->> 'branch_id';
                IF target <> jsonb_build_object(
                    'Branch', jsonb_build_object(
                        'holding_id', target_holding_text,
                        'company_id', target_company_text,
                        'branch_id', target_branch_text
                    )
                ) THEN
                    RETURN FALSE;
                END IF;
            WHEN 'Subtree' THEN
                target_holding_text := target_body ->> 'holding_id';
                target_node_text := target_body ->> 'organization_node_id';
                IF target <> jsonb_build_object(
                    'Subtree', jsonb_build_object(
                        'holding_id', target_holding_text,
                        'organization_node_id', target_node_text
                    )
                ) THEN
                    RETURN FALSE;
                END IF;
            ELSE
                RETURN FALSE;
        END CASE;
    ELSE
        RETURN FALSE;
    END IF;

    IF authority_kind <> 'Platform' THEN
        IF authority_holding_text IS NULL
           OR authority_holding_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        authority_holding := authority_holding_text::UUID;
        IF authority_holding = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;
    IF authority_kind IN ('Company','Branch') THEN
        IF authority_company_text IS NULL
           OR authority_company_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        authority_company := authority_company_text::UUID;
        IF authority_company = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;
    IF authority_kind = 'Branch' THEN
        IF authority_branch_text IS NULL
           OR authority_branch_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        authority_branch := authority_branch_text::UUID;
        IF authority_branch = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;
    IF authority_kind = 'Subtree' THEN
        IF authority_node_text IS NULL
           OR authority_node_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        authority_node := authority_node_text::UUID;
        IF authority_node = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;

    IF target_kind <> 'Platform' THEN
        IF target_holding_text IS NULL OR target_holding_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        target_holding := target_holding_text::UUID;
        IF target_holding = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;
    IF target_kind IN ('Company','Branch') THEN
        IF target_company_text IS NULL OR target_company_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        target_company := target_company_text::UUID;
        IF target_company = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;
    IF target_kind = 'Branch' THEN
        IF target_branch_text IS NULL OR target_branch_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        target_branch := target_branch_text::UUID;
        IF target_branch = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;
    IF target_kind = 'Subtree' THEN
        IF target_node_text IS NULL OR target_node_text !~* uuid_pattern THEN
            RETURN FALSE;
        END IF;
        target_node := target_node_text::UUID;
        IF target_node = nil_uuid THEN
            RETURN FALSE;
        END IF;
    END IF;

    IF authority_kind <> 'Platform'
       AND (target_kind = 'Platform' OR authority_holding <> target_holding) THEN
        RETURN FALSE;
    END IF;

    -- An exact Subtree is canonical only for Department or Team. Validate the
    -- entire active path in the Organization-authored cache, not just the
    -- target row. A missing, inactive, wrong-kind, or contradictory ancestor
    -- therefore denies even if a UUID happens to match a structured scope.
    IF target_kind = 'Subtree' THEN
        SELECT candidate.node_kind, candidate.lineage_node_ids
          INTO target_node_kind, target_lineage
          FROM erpx_authority_read.organization_nodes AS candidate
         WHERE candidate.holding_id = target_holding
           AND candidate.node_id = target_node
           AND candidate.node_status = 'active'
           AND candidate.node_kind IN ('department','team')
           AND candidate.lineage_node_ids[1] = target_holding
           AND cardinality(candidate.lineage_node_ids) = cardinality(
               ARRAY(
                   SELECT DISTINCT lineage_node_id
                     FROM unnest(candidate.lineage_node_ids) AS lineage(lineage_node_id)
               )
           )
           AND NOT (candidate.node_id = ANY(candidate.lineage_node_ids))
           AND candidate.parent_node_id IS NOT DISTINCT FROM
               candidate.lineage_node_ids[cardinality(candidate.lineage_node_ids)]
           AND (
               (candidate.node_kind = 'department'
                    AND cardinality(candidate.lineage_node_ids) IN (2,3))
               OR (candidate.node_kind = 'team'
                    AND cardinality(candidate.lineage_node_ids) IN (3,4))
           )
           AND NOT EXISTS (
               SELECT 1
                 FROM unnest(candidate.lineage_node_ids) WITH ORDINALITY
                      AS path(node_id,depth)
                 LEFT JOIN erpx_authority_read.organization_nodes AS ancestor
                   ON ancestor.holding_id = candidate.holding_id
                  AND ancestor.node_id = path.node_id
                WHERE ancestor.node_id IS NULL
                   OR ancestor.node_status <> 'active'
                   OR ancestor.node_kind IS DISTINCT FROM CASE
                       WHEN path.depth = 1 THEN 'holding'
                       WHEN path.depth = 2 THEN 'company'
                       WHEN candidate.node_kind = 'department'
                            AND cardinality(candidate.lineage_node_ids) = 3
                            AND path.depth = 3 THEN 'branch'
                       WHEN candidate.node_kind = 'team'
                            AND cardinality(candidate.lineage_node_ids) = 3
                            AND path.depth = 3 THEN 'department'
                       WHEN candidate.node_kind = 'team'
                            AND cardinality(candidate.lineage_node_ids) = 4
                            AND path.depth = 3 THEN 'branch'
                       WHEN candidate.node_kind = 'team'
                            AND cardinality(candidate.lineage_node_ids) = 4
                            AND path.depth = 4 THEN 'department'
                       ELSE NULL
                   END
                   OR ancestor.lineage_node_ids IS DISTINCT FROM CASE
                       WHEN path.depth = 1 THEN ARRAY[]::UUID[]
                       ELSE candidate.lineage_node_ids[1:(path.depth::INTEGER - 1)]
                   END
                   OR ancestor.parent_node_id IS DISTINCT FROM CASE
                       WHEN path.depth = 1 THEN NULL::UUID
                       ELSE candidate.lineage_node_ids[path.depth::INTEGER - 1]
                   END
           );
        IF NOT FOUND THEN
            RETURN FALSE;
        END IF;

        IF authority_kind = 'Platform' OR authority_kind = 'Holding' THEN
            RETURN TRUE;
        END IF;
        IF authority_kind = 'Company' THEN
            RETURN target_lineage[2] = authority_company;
        END IF;
        IF authority_kind = 'Branch' THEN
            RETURN (
                    (target_node_kind = 'department'
                        AND cardinality(target_lineage) = 3)
                    OR (target_node_kind = 'team'
                        AND cardinality(target_lineage) = 4)
                )
                AND target_lineage[2] = authority_company
                AND target_lineage[3] = authority_branch;
        END IF;
        IF authority_kind = 'Subtree' THEN
            IF authority_node = target_node THEN
                RETURN TRUE;
            END IF;
            -- The already validated target chain proves the prefix of this
            -- ancestor. Requiring a Department/Team row rejects Subtree scopes
            -- that merely encode a Company or Branch UUID.
            RETURN EXISTS (
                SELECT 1
                  FROM erpx_authority_read.organization_nodes AS ancestor
                 WHERE ancestor.holding_id = target_holding
                   AND ancestor.node_id = authority_node
                   AND ancestor.node_id = ANY(target_lineage)
                   AND ancestor.node_kind IN ('department','team')
                   AND ancestor.node_status = 'active'
            );
        END IF;
        RETURN FALSE;
    END IF;

    IF authority_kind = 'Platform' THEN
        RETURN TRUE;
    END IF;
    IF authority_kind = 'Holding' THEN
        RETURN target_kind IN ('Holding','Company','Branch');
    END IF;
    IF authority_kind = 'Company' THEN
        RETURN target_kind IN ('Company', 'Branch')
            AND authority_company = target_company;
    END IF;
    IF authority_kind = 'Branch' THEN
        RETURN target_kind = 'Branch'
            AND authority_company = target_company
            AND authority_branch = target_branch;
    END IF;
    -- Exact Department/Team authority never grants access to an ancestor.
    RETURN FALSE;
END;
$$;

CREATE FUNCTION erpx_collaboration_read.audience_allows(
    audience JSONB,
    principal_id UUID,
    authority_scope JSONB,
    actor_realm TEXT,
    actor_portal TEXT
)
RETURNS BOOLEAN
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    realm TEXT := audience ->> 'realm';
BEGIN
    IF realm = 'internal' THEN
        IF actor_realm <> 'internal' OR actor_portal IS NOT NULL THEN
            RETURN FALSE;
        END IF;
        RETURN erpx_collaboration_read.scope_contains(
            authority_scope,
            audience -> 'scope'
        ) AND (
            jsonb_array_length(audience -> 'principals') = 0
            OR EXISTS (
                SELECT 1
                FROM jsonb_array_elements(audience -> 'principals') AS member
                WHERE member ->> 'id' = principal_id::text
            )
        );
    END IF;
    IF realm = 'external_portal' THEN
        RETURN actor_realm = 'external_portal'
        AND actor_portal IS NOT NULL
        AND audience ->> 'portal' = actor_portal
        AND EXISTS (
            SELECT 1
            FROM jsonb_array_elements(audience -> 'participants') AS member
            WHERE member ->> 'id' = principal_id::text
        );
    END IF;
    RETURN FALSE;
END;
$$;

CREATE TABLE erpx_collaboration_read.thread_projection (
    holding_id UUID NOT NULL,
    projection_name TEXT NOT NULL DEFAULT 'collaboration.threads'
        CHECK (projection_name = 'collaboration.threads'),
    generation UUID NOT NULL,
    thread_id UUID NOT NULL,
    anchor_record_id UUID NOT NULL,
    visibility_scope JSONB NOT NULL,
    audience JSONB NOT NULL,
    projection JSONB NOT NULL,
    search_text TEXT NOT NULL,
    search_vector TSVECTOR GENERATED ALWAYS AS (
        to_tsvector('simple', search_text)
    ) STORED,
    source_event_cursor BIGINT NOT NULL CHECK (source_event_cursor > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, thread_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX collaboration_thread_anchor_projection
    ON erpx_collaboration_read.thread_projection
       (holding_id, generation, anchor_record_id, updated_at DESC, thread_id);
CREATE INDEX collaboration_thread_search
    ON erpx_collaboration_read.thread_projection USING GIN (search_vector);

CREATE TABLE erpx_collaboration_read.stream_projection (
    holding_id UUID NOT NULL,
    projection_name TEXT NOT NULL DEFAULT 'collaboration.stream'
        CHECK (projection_name = 'collaboration.stream'),
    generation UUID NOT NULL,
    thread_id UUID NOT NULL,
    event_cursor BIGINT NOT NULL CHECK (event_cursor > 0),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_kind JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, event_cursor),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX collaboration_thread_stream_projection
    ON erpx_collaboration_read.stream_projection
       (holding_id, generation, thread_id, event_cursor);

CREATE TABLE erpx_collaboration_read.opaque_mention_inbox_projection (
    inbox_cursor BIGSERIAL NOT NULL,
    event_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    projection_name TEXT NOT NULL DEFAULT 'collaboration.inbox'
        CHECK (projection_name = 'collaboration.inbox'),
    generation UUID NOT NULL,
    recipient_principal_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    mention_event_id UUID NOT NULL,
    anchor_record_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, inbox_cursor),
    UNIQUE (holding_id, projection_name, generation, event_id),
    UNIQUE (
        holding_id, projection_name, generation,
        recipient_principal_id, mention_event_id
    ),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX collaboration_inbox_projection_recipient
    ON erpx_collaboration_read.opaque_mention_inbox_projection
       (holding_id, generation, recipient_principal_id, inbox_cursor);

-- The Item Directory is a disposable, holding-local projection. Record owners
-- publish descriptors and explicit effective-principal grants; this schema is
-- never a business write authority and cannot infer access from descriptor
-- contents. Authorization rows are deliberately opaque and must be joined
-- before descriptor, relation, full-text match, or count data is selected.
CREATE TABLE erpx_collaboration_read.item_directory_projection_state (
    holding_id UUID PRIMARY KEY,
    rebuild_generation BIGINT NOT NULL CHECK (rebuild_generation > 0),
    source_high_watermark BIGINT NOT NULL CHECK (source_high_watermark >= 0),
    applied_position BIGINT NOT NULL CHECK (applied_position >= 0),
    state TEXT NOT NULL CHECK (state IN ('ready', 'rebuilding', 'unavailable')),
    last_success_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (applied_position <= source_high_watermark),
    CHECK (state <> 'ready' OR applied_position = source_high_watermark)
);

CREATE TABLE erpx_collaboration_read.item_directory_descriptors (
    holding_id UUID NOT NULL,
    record_id UUID NOT NULL,
    owner_context TEXT NOT NULL CHECK (octet_length(owner_context) BETWEEN 1 AND 64),
    record_type TEXT NOT NULL CHECK (octet_length(record_type) BETWEEN 1 AND 128),
    record_scope JSONB NOT NULL,
    authorization_policy_id UUID NOT NULL,
    authorization_policy_version BIGINT NOT NULL CHECK (authorization_policy_version > 0),
    descriptor JSONB,
    canonical_route TEXT,
    authorized_search_text TEXT NOT NULL DEFAULT '',
    search_vector TSVECTOR GENERATED ALWAYS AS (
        to_tsvector('simple', authorized_search_text)
    ) STORED,
    descriptor_version BIGINT NOT NULL CHECK (descriptor_version > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    tombstoned_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, record_id),
    CHECK (
        (tombstoned_at IS NULL AND descriptor IS NOT NULL AND canonical_route IS NOT NULL)
        OR (tombstoned_at IS NOT NULL AND descriptor IS NULL
            AND canonical_route IS NULL AND authorized_search_text = '')
    )
);

CREATE INDEX item_directory_authorized_search
    ON erpx_collaboration_read.item_directory_descriptors USING GIN (search_vector)
    WHERE tombstoned_at IS NULL;
CREATE INDEX item_directory_owner_position
    ON erpx_collaboration_read.item_directory_descriptors
       (holding_id, owner_context, source_event_position, record_id);

CREATE TABLE erpx_collaboration_read.item_directory_effective_grants (
    holding_id UUID NOT NULL,
    record_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    authorization_policy_id UUID NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    can_view BOOLEAN NOT NULL,
    effective_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, record_id, principal_id),
    CHECK (expires_at > effective_at),
    FOREIGN KEY (holding_id, record_id)
        REFERENCES erpx_collaboration_read.item_directory_descriptors
            (holding_id, record_id)
        ON DELETE CASCADE
);

CREATE INDEX item_directory_principal_grants
    ON erpx_collaboration_read.item_directory_effective_grants
       (holding_id, principal_id, expires_at, record_id)
    WHERE can_view;

CREATE TABLE erpx_collaboration_read.item_directory_relations (
    holding_id UUID NOT NULL,
    relation_id UUID NOT NULL,
    source_record_id UUID NOT NULL,
    target_record_id UUID NOT NULL,
    relation_type TEXT NOT NULL CHECK (
        relation_type IN (
            'relates-to', 'blocks', 'depends-on', 'implements',
            'supersedes', 'derived-from', 'evidence-for'
        )
    ),
    relation_version BIGINT NOT NULL CHECK (relation_version > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    tombstoned_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, relation_id),
    CHECK (source_record_id <> target_record_id),
    FOREIGN KEY (holding_id, source_record_id)
        REFERENCES erpx_collaboration_read.item_directory_descriptors
            (holding_id, record_id)
        ON DELETE CASCADE,
    FOREIGN KEY (holding_id, target_record_id)
        REFERENCES erpx_collaboration_read.item_directory_descriptors
            (holding_id, record_id)
        ON DELETE CASCADE
);

-- Opaque high-water marks prevent delayed descriptor/grant/relation facts
-- from resurrecting revoked authority after purge. They contain no owner,
-- record type, title, route, lifecycle, or search payload.
CREATE TABLE erpx_collaboration_read.item_directory_tombstones (
    holding_id UUID NOT NULL,
    record_id UUID NOT NULL,
    descriptor_version BIGINT NOT NULL CHECK (descriptor_version > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, record_id)
);

CREATE TABLE erpx_collaboration_read.item_directory_grant_tombstones (
    holding_id UUID NOT NULL,
    record_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    authorization_policy_id UUID NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, record_id, principal_id)
);

CREATE TABLE erpx_collaboration_read.item_directory_relation_tombstones (
    holding_id UUID NOT NULL,
    relation_id UUID NOT NULL,
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, relation_id)
);

CREATE INDEX item_directory_relation_source
    ON erpx_collaboration_read.item_directory_relations
       (holding_id, source_record_id, relation_id)
    WHERE tombstoned_at IS NULL;
CREATE INDEX item_directory_relation_target
    ON erpx_collaboration_read.item_directory_relations
       (holding_id, target_record_id, relation_id)
    WHERE tombstoned_at IS NULL;

DO $$
DECLARE
    table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'thread_projection', 'stream_projection',
        'opaque_mention_inbox_projection',
        'item_directory_projection_state', 'item_directory_descriptors',
        'item_directory_effective_grants', 'item_directory_relations',
        'item_directory_tombstones', 'item_directory_grant_tombstones',
        'item_directory_relation_tombstones'
    ]
    LOOP
        EXECUTE format(
            'ALTER TABLE erpx_collaboration_read.%I ENABLE ROW LEVEL SECURITY',
            table_name
        );
        EXECUTE format(
            'ALTER TABLE erpx_collaboration_read.%I FORCE ROW LEVEL SECURITY',
            table_name
        );
        EXECUTE format(
            'CREATE POLICY collaboration_read_holding_isolation '
            'ON erpx_collaboration_read.%I '
            'USING (holding_id::text = current_setting(''app.holding_id'', true)) '
            'WITH CHECK (holding_id::text = current_setting(''app.holding_id'', true))',
            table_name
        );
    END LOOP;
END;
$$;


-- Disposable authorized Agent Orchestration read projections. Prompt and
-- output material remain encrypted references in the write plane and never
-- enter these cross-workspace listings.

CREATE SCHEMA IF NOT EXISTS erpx_agent_read;

CREATE OR REPLACE FUNCTION erpx_agent_read.current_holding_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.holding_id', true), '')::uuid
$$;

CREATE TABLE erpx_agent_read.agent_directory_projection (
    holding_id UUID NOT NULL DEFAULT erpx_agent_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.agent_directory'
        CHECK (projection_name = 'agent_orchestration.agent_directory'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    agent_definition_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    holding_scope JSONB NOT NULL,
    agent_principal_ref TEXT NOT NULL,
    agent_name TEXT NOT NULL,
    definition_status TEXT NOT NULL CHECK (definition_status IN (
        'draft', 'approved', 'suspended', 'retired'
    )),
    approved_version_ids UUID[] NOT NULL,
    capabilities TEXT[] NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, agent_definition_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX agent_directory_scope_status
    ON erpx_agent_read.agent_directory_projection
        (holding_id, generation, scope_key, definition_status, agent_name, agent_definition_id);

CREATE TABLE erpx_agent_read.assignment_projection (
    holding_id UUID NOT NULL DEFAULT erpx_agent_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.assignments'
        CHECK (projection_name = 'agent_orchestration.assignments'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    assignment_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    holding_scope JSONB NOT NULL,
    root_assignment_id UUID NOT NULL,
    parent_assignment_id UUID,
    initiating_principal_ref TEXT NOT NULL,
    agent_principal_ref TEXT NOT NULL,
    agent_definition_id UUID NOT NULL,
    agent_version_id UUID NOT NULL,
    provider_ref TEXT NOT NULL,
    model_ref TEXT NOT NULL,
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    assignment_status TEXT NOT NULL CHECK (assignment_status IN (
        'queued', 'running', 'waiting', 'completed', 'failed', 'cancelled'
    )),
    attempt INTEGER NOT NULL CHECK (attempt >= 0),
    depth INTEGER NOT NULL CHECK (depth >= 0),
    queue_priority INTEGER NOT NULL CHECK (queue_priority BETWEEN 0 AND 65535),
    estimated_units BIGINT NOT NULL CHECK (estimated_units > 0),
    estimated_cost_micros BIGINT NOT NULL CHECK (estimated_cost_micros >= 0),
    actual_units BIGINT CHECK (actual_units >= 0),
    actual_cost_micros BIGINT CHECK (actual_cost_micros >= 0),
    failure_code TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, assignment_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    CHECK (updated_at >= created_at)
);

CREATE INDEX agent_assignment_inbox
    ON erpx_agent_read.assignment_projection
        (holding_id, generation, scope_key, initiating_principal_ref, assignment_status,
         updated_at DESC, assignment_id DESC);

CREATE TABLE erpx_agent_read.private_session_projection (
    holding_id UUID NOT NULL DEFAULT erpx_agent_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.private_sessions'
        CHECK (projection_name = 'agent_orchestration.private_sessions'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    agent_session_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    owner_human_ref TEXT NOT NULL,
    record_id UUID,
    expires_at TIMESTAMPTZ NOT NULL,
    legal_hold BOOLEAN NOT NULL,
    expired_at TIMESTAMPTZ,
    message_count INTEGER NOT NULL CHECK (message_count >= 0),
    published_message_count INTEGER NOT NULL CHECK (
        published_message_count BETWEEN 0 AND message_count
    ),
    updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, agent_session_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX agent_private_sessions_owner
    ON erpx_agent_read.private_session_projection
        (holding_id, generation, scope_key, owner_human_ref, updated_at DESC, agent_session_id DESC);

CREATE TABLE erpx_agent_read.pool_directory_projection (
    holding_id UUID NOT NULL DEFAULT erpx_agent_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.pools'
        CHECK (projection_name = 'agent_orchestration.pools'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    agent_pool_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    pool_name TEXT NOT NULL,
    member_version_ids UUID[] NOT NULL,
    enabled BOOLEAN NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, agent_pool_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE,
    CHECK (cardinality(member_version_ids) > 0)
);

CREATE INDEX agent_pool_directory_scope
    ON erpx_agent_read.pool_directory_projection
        (holding_id, generation, scope_key, enabled, pool_name, agent_pool_id);

CREATE TABLE erpx_agent_read.pool_routing_projection (
    holding_id UUID NOT NULL DEFAULT erpx_agent_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.pool_routing'
        CHECK (projection_name = 'agent_orchestration.pool_routing'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    routing_decision_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    requested_scope JSONB NOT NULL,
    agent_pool_id UUID NOT NULL,
    root_action_ref TEXT NOT NULL,
    capability TEXT NOT NULL,
    selected_agent_version_ids UUID[] NOT NULL,
    compare_requested BOOLEAN NOT NULL,
    decision_digest TEXT NOT NULL CHECK (decision_digest ~ '^[0-9A-Fa-f]{64}$'),
    decided_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, routing_decision_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_agent_read.provider_invocation_projection (
    holding_id UUID NOT NULL DEFAULT erpx_agent_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.provider_invocations'
        CHECK (projection_name = 'agent_orchestration.provider_invocations'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    invocation_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    holding_scope JSONB NOT NULL,
    assignment_id UUID NOT NULL,
    attempt INTEGER NOT NULL CHECK (attempt > 0),
    provider_ref TEXT NOT NULL,
    model_ref TEXT NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    payload_digest TEXT NOT NULL CHECK (payload_digest ~ '^[0-9A-Fa-f]{64}$'),
    output_digest TEXT CHECK (output_digest ~ '^[0-9A-Fa-f]{64}$'),
    provider_units BIGINT CHECK (provider_units > 0),
    provider_cost_micros BIGINT CHECK (provider_cost_micros >= 0),
    outcome TEXT CHECK (outcome IN ('completed', 'failed', 'cancelled')),
    invoked_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, invocation_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_agent_read.action_card_projection (
    holding_id UUID NOT NULL DEFAULT erpx_agent_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.action_cards'
        CHECK (projection_name = 'agent_orchestration.action_cards'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    action_card_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    assignment_id UUID NOT NULL,
    agent_principal_ref TEXT NOT NULL,
    owner_context TEXT NOT NULL,
    command_type TEXT NOT NULL,
    action_status TEXT NOT NULL CHECK (action_status IN (
        'proposed', 'rejected', 'confirmed_for_handoff'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    resolved_at TIMESTAMPTZ,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, action_card_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE TABLE erpx_agent_read.governance_projection (
    holding_id UUID NOT NULL DEFAULT erpx_agent_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'agent_orchestration.governance'
        CHECK (projection_name = 'agent_orchestration.governance'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    scope_key TEXT NOT NULL,
    holding_scope JSONB NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('delegation_tree','budget_decision')),
    subject_record_id UUID NOT NULL,
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    lifecycle_state TEXT NOT NULL CHECK (lifecycle_state IN (
        'proposed','approved','active','completed','denied','expired','cancelled'
    )),
    updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, record_id),
    UNIQUE (holding_id, projection_name, generation, record_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (holding_id, projection_name, generation)
        ON DELETE CASCADE
);

CREATE INDEX agent_governance_scope_state
    ON erpx_agent_read.governance_projection
        (holding_id, generation, scope_key, record_kind, lifecycle_state, updated_at DESC, record_id);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'agent_directory_projection','assignment_projection',
        'private_session_projection','pool_directory_projection',
        'pool_routing_projection','provider_invocation_projection',
        'action_card_projection','governance_projection'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_agent_read.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_agent_read.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY agent_read_holding_isolation ON erpx_agent_read.%I '
            'USING (holding_id = erpx_agent_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_agent_read.current_holding_id())',
            table_name
        );
    END LOOP;
END;
$$;


-- Disposable Projects projections rebuilt from typed Projects projection facts.
CREATE SCHEMA IF NOT EXISTS erpx_projects_read;

CREATE OR REPLACE FUNCTION erpx_projects_read.current_holding_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.holding_id', true), '')::uuid
$$;

CREATE TABLE erpx_projects_read.project_projection (
    holding_id UUID NOT NULL DEFAULT erpx_projects_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'projects.projects' CHECK (projection_name = 'projects.projects'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    project_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    project_code TEXT NOT NULL,
    project_class TEXT NOT NULL,
    project_status TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_created_at TIMESTAMPTZ NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, project_id),
    FOREIGN KEY (holding_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation) ON DELETE CASCADE,
    UNIQUE (holding_id, generation, scope_key, project_code)
);

CREATE TABLE erpx_projects_read.hierarchy_closure_projection (
    holding_id UUID NOT NULL DEFAULT erpx_projects_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'projects.hierarchy' CHECK (projection_name = 'projects.hierarchy'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    project_id UUID NOT NULL,
    ancestor_node_id UUID NOT NULL,
    descendant_node_id UUID NOT NULL,
    depth SMALLINT NOT NULL CHECK (depth BETWEEN 0 AND 2),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, project_id, ancestor_node_id, descendant_node_id),
    FOREIGN KEY (holding_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation) ON DELETE CASCADE
);

CREATE TABLE erpx_projects_read.dependency_projection (
    holding_id UUID NOT NULL DEFAULT erpx_projects_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'projects.dependencies' CHECK (projection_name = 'projects.dependencies'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    dependency_id UUID NOT NULL,
    source_project_id UUID NOT NULL,
    target_project_id UUID NOT NULL,
    dependency_status TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, dependency_id),
    FOREIGN KEY (holding_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation) ON DELETE CASCADE
);

CREATE TABLE erpx_projects_read.handoff_projection (
    holding_id UUID NOT NULL DEFAULT erpx_projects_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'projects.handoffs' CHECK (projection_name = 'projects.handoffs'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    handoff_id UUID NOT NULL,
    project_id UUID NOT NULL,
    handoff_status TEXT NOT NULL,
    owner_context TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, handoff_id),
    FOREIGN KEY (holding_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation) ON DELETE CASCADE
);

CREATE INDEX projects_read_scope_status ON erpx_projects_read.project_projection(holding_id, generation, scope_key, project_status, aggregate_updated_at DESC, project_id DESC);
CREATE INDEX projects_read_closure_descendant ON erpx_projects_read.hierarchy_closure_projection(holding_id, generation, project_id, descendant_node_id, depth);
CREATE INDEX projects_read_dependency_active ON erpx_projects_read.dependency_projection(holding_id, generation, source_project_id, target_project_id) WHERE dependency_status = 'accepted';
CREATE INDEX projects_read_handoff_pending ON erpx_projects_read.handoff_projection(holding_id, generation, project_id, handoff_status) WHERE handoff_status IN ('ready','uncertain');

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'project_projection','hierarchy_closure_projection',
        'dependency_projection','handoff_projection'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_projects_read.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_projects_read.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY projects_read_holding_isolation ON erpx_projects_read.%I '
            'USING (holding_id = erpx_projects_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_projects_read.current_holding_id())',
            table_name
        );
    END LOOP;
END;
$$;


-- Disposable, active-generation Manufacturing and Planning projections.
CREATE SCHEMA IF NOT EXISTS erpx_manufacturing_read;
CREATE SCHEMA IF NOT EXISTS erpx_planning_read;

CREATE OR REPLACE FUNCTION erpx_manufacturing_read.current_holding_id()
RETURNS uuid LANGUAGE SQL STABLE AS $$ SELECT nullif(current_setting('app.holding_id', true), '')::uuid $$;
CREATE OR REPLACE FUNCTION erpx_planning_read.current_holding_id()
RETURNS uuid LANGUAGE SQL STABLE AS $$ SELECT nullif(current_setting('app.holding_id', true), '')::uuid $$;

CREATE TABLE erpx_manufacturing_read.order_projection (
    holding_id uuid NOT NULL DEFAULT erpx_manufacturing_read.current_holding_id(),
    projection_name text NOT NULL DEFAULT 'manufacturing.orders' CHECK (projection_name = 'manufacturing.orders'),
    generation uuid NOT NULL, partition_key text NOT NULL,
    order_id uuid NOT NULL, authority_scope jsonb NOT NULL, lifecycle_state text NOT NULL,
    projection jsonb NOT NULL, source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, order_id),
    UNIQUE (holding_id, projection_name, generation, order_id),
    FOREIGN KEY (holding_id, projection_name, generation)
      REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation) ON DELETE CASCADE
);
CREATE TABLE erpx_manufacturing_read.genealogy_projection (
    holding_id uuid NOT NULL DEFAULT erpx_manufacturing_read.current_holding_id(),
    projection_name text NOT NULL DEFAULT 'manufacturing.orders' CHECK (projection_name = 'manufacturing.orders'),
    generation uuid NOT NULL, partition_key text NOT NULL,
    output_lot_or_serial text NOT NULL, source_lot_or_serial text NOT NULL, order_id uuid NOT NULL,
    result_fact_id uuid NOT NULL, source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0),
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, output_lot_or_serial, source_lot_or_serial, result_fact_id),
    FOREIGN KEY (holding_id, projection_name, generation, order_id)
      REFERENCES erpx_manufacturing_read.order_projection(holding_id, projection_name, generation, order_id) ON DELETE CASCADE
);
CREATE TABLE erpx_manufacturing_read.wip_cost_projection (
    holding_id uuid NOT NULL DEFAULT erpx_manufacturing_read.current_holding_id(),
    projection_name text NOT NULL DEFAULT 'manufacturing.orders' CHECK (projection_name = 'manufacturing.orders'),
    generation uuid NOT NULL, partition_key text NOT NULL, order_id uuid NOT NULL,
    wip_minor_units bigint NOT NULL, currency char(3) NOT NULL, reconciled boolean NOT NULL,
    source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0),
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, order_id),
    FOREIGN KEY (holding_id, projection_name, generation, order_id)
      REFERENCES erpx_manufacturing_read.order_projection(holding_id, projection_name, generation, order_id) ON DELETE CASCADE
);
CREATE TABLE erpx_manufacturing_read.governed_record_projection (
    holding_id uuid NOT NULL DEFAULT erpx_manufacturing_read.current_holding_id(), projection_name text NOT NULL DEFAULT 'manufacturing.orders' CHECK (projection_name='manufacturing.orders'),
    generation uuid NOT NULL,partition_key text NOT NULL,record_id uuid NOT NULL,record_kind text NOT NULL CHECK(record_kind IN('routing','batch','result')),
    authority_scope jsonb NOT NULL,lifecycle_state text NOT NULL,projection jsonb NOT NULL,
    source_aggregate_version bigint NOT NULL CHECK(source_aggregate_version>0),source_position bigint NOT NULL CHECK(source_position>0),projected_at timestamptz NOT NULL,
    PRIMARY KEY(holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY(holding_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation) ON DELETE CASCADE
);

CREATE TABLE erpx_planning_read.run_projection (
    holding_id uuid NOT NULL DEFAULT erpx_planning_read.current_holding_id(),
    projection_name text NOT NULL DEFAULT 'planning.runs' CHECK (projection_name = 'planning.runs'),
    generation uuid NOT NULL, partition_key text NOT NULL, run_id uuid NOT NULL, scenario_id uuid NOT NULL,
    authority_scope jsonb NOT NULL, input_digest text NOT NULL CHECK (input_digest ~ '^[0-9a-f]{64}$'),
    projection jsonb NOT NULL, source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, run_id),
    UNIQUE (holding_id, projection_name, generation, run_id),
    FOREIGN KEY (holding_id, projection_name, generation)
      REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation) ON DELETE CASCADE
);
CREATE TABLE erpx_planning_read.pegging_projection (
    holding_id uuid NOT NULL DEFAULT erpx_planning_read.current_holding_id(),
    projection_name text NOT NULL DEFAULT 'planning.runs' CHECK (projection_name = 'planning.runs'),
    generation uuid NOT NULL, partition_key text NOT NULL, run_id uuid NOT NULL,
    demand_id uuid NOT NULL, supply_id uuid NOT NULL, quantity_units bigint NOT NULL CHECK (quantity_units > 0),
    quantity_scale smallint NOT NULL CHECK (quantity_scale BETWEEN 0 AND 9),
    source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0),
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, run_id, demand_id, supply_id),
    FOREIGN KEY (holding_id, projection_name, generation, run_id)
      REFERENCES erpx_planning_read.run_projection(holding_id, projection_name, generation, run_id) ON DELETE CASCADE
);
CREATE TABLE erpx_planning_read.exception_projection (
    holding_id uuid NOT NULL DEFAULT erpx_planning_read.current_holding_id(),
    projection_name text NOT NULL DEFAULT 'planning.runs' CHECK (projection_name = 'planning.runs'),
    generation uuid NOT NULL, partition_key text NOT NULL, run_id uuid NOT NULL,
    exception_key text NOT NULL, exception_kind text NOT NULL CHECK (exception_kind IN ('shortage', 'capacity_overload')),
    payload jsonb NOT NULL, source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0),
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, run_id, exception_key),
    FOREIGN KEY (holding_id, projection_name, generation, run_id)
      REFERENCES erpx_planning_read.run_projection(holding_id, projection_name, generation, run_id) ON DELETE CASCADE
);
CREATE TABLE erpx_planning_read.plan_projection (
    holding_id uuid NOT NULL DEFAULT erpx_planning_read.current_holding_id(),projection_name text NOT NULL DEFAULT 'planning.runs' CHECK(projection_name='planning.runs'),generation uuid NOT NULL,partition_key text NOT NULL,
    plan_id uuid NOT NULL,plan_kind text NOT NULL CHECK(plan_kind IN('demand_plan','supply_plan','capacity_plan')),authority_scope jsonb NOT NULL,lifecycle_state text NOT NULL,projection jsonb NOT NULL,
    source_aggregate_version bigint NOT NULL CHECK(source_aggregate_version>0),source_position bigint NOT NULL CHECK(source_position>0),projected_at timestamptz NOT NULL,
    PRIMARY KEY(holding_id,projection_name,generation,partition_key,plan_id),FOREIGN KEY(holding_id,projection_name,generation)REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation)ON DELETE CASCADE
);

DO $$
DECLARE relation_name text;
BEGIN
  FOREACH relation_name IN ARRAY ARRAY[
    'erpx_manufacturing_read.order_projection', 'erpx_manufacturing_read.genealogy_projection',
    'erpx_manufacturing_read.wip_cost_projection','erpx_manufacturing_read.governed_record_projection', 'erpx_planning_read.run_projection',
    'erpx_planning_read.pegging_projection', 'erpx_planning_read.exception_projection','erpx_planning_read.plan_projection'
  ] LOOP
    EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', relation_name);
    EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', relation_name);
    EXECUTE format('CREATE POLICY holding_isolation ON %s USING (holding_id = nullif(current_setting(''app.holding_id'', true), '''')::uuid) WITH CHECK (holding_id = nullif(current_setting(''app.holding_id'', true), '''')::uuid)', relation_name);
  END LOOP;
END $$;


-- Disposable owner-scoped E-commerce and Talent projections.
CREATE SCHEMA IF NOT EXISTS erpx_ecommerce_read;
CREATE SCHEMA IF NOT EXISTS erpx_talent_read;

CREATE OR REPLACE FUNCTION erpx_ecommerce_read.current_holding_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$ SELECT nullif(current_setting('app.holding_id', true), '')::uuid $$;
CREATE OR REPLACE FUNCTION erpx_talent_read.current_holding_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$ SELECT nullif(current_setting('app.holding_id', true), '')::uuid $$;

CREATE TABLE erpx_ecommerce_read.listing_projection (
    holding_id UUID NOT NULL DEFAULT erpx_ecommerce_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'ecommerce.listings', generation UUID NOT NULL,
    partition_key TEXT NOT NULL, listing_id UUID NOT NULL, scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL,
    channel_id UUID NOT NULL, listing_status TEXT NOT NULL, price_minor BIGINT NOT NULL,
    currency TEXT NOT NULL, available BOOLEAN NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0), projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, listing_id),
    FOREIGN KEY (holding_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation) ON DELETE CASCADE
);
CREATE TABLE erpx_ecommerce_read.journey_projection (
    holding_id UUID NOT NULL DEFAULT erpx_ecommerce_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'ecommerce.journeys', generation UUID NOT NULL,
    partition_key TEXT NOT NULL, record_id UUID NOT NULL, record_type TEXT NOT NULL,
    scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL, owner_realm TEXT NOT NULL CHECK (owner_realm IN ('guest','customer')),
    owner_principal_id UUID NOT NULL, lifecycle_status TEXT NOT NULL, order_ref JSONB,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0), projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, record_id),
    FOREIGN KEY (holding_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation) ON DELETE CASCADE
);
CREATE TABLE erpx_talent_read.candidate_projection (
    holding_id UUID NOT NULL DEFAULT erpx_talent_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'talent.candidate', generation UUID NOT NULL,
    partition_key TEXT NOT NULL, record_id UUID NOT NULL, record_type TEXT NOT NULL,
    scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL, authority_realm TEXT NOT NULL CHECK (authority_realm = 'candidate'),
    candidate_principal_id UUID NOT NULL, lifecycle_status TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0), projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, record_id),
    FOREIGN KEY (holding_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation) ON DELETE CASCADE
);
CREATE TABLE erpx_talent_read.workforce_projection (
    holding_id UUID NOT NULL DEFAULT erpx_talent_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'talent.workforce', generation UUID NOT NULL,
    partition_key TEXT NOT NULL, record_id UUID NOT NULL, record_type TEXT NOT NULL,
    scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL, authority_realm TEXT NOT NULL CHECK (authority_realm = 'workforce'),
    lifecycle_status TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0), projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, record_id),
    FOREIGN KEY (holding_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation) ON DELETE CASCADE
);

CREATE INDEX ecommerce_listing_scope ON erpx_ecommerce_read.listing_projection(holding_id, generation, scope_key, listing_status, listing_id);
CREATE INDEX ecommerce_journey_owner ON erpx_ecommerce_read.journey_projection(holding_id, generation, owner_realm, owner_principal_id, projected_at DESC);
CREATE INDEX talent_candidate_owner ON erpx_talent_read.candidate_projection(holding_id, generation, candidate_principal_id, projected_at DESC);
CREATE INDEX talent_workforce_scope ON erpx_talent_read.workforce_projection(holding_id, generation, scope_key, record_type, projected_at DESC);

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_ecommerce_read','listing_projection'),('erpx_ecommerce_read','journey_projection'),
        ('erpx_talent_read','candidate_projection'),('erpx_talent_read','workforce_projection')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY holding_isolation ON %I.%I '
            'USING (holding_id = %I.current_holding_id()) '
            'WITH CHECK (holding_id = %I.current_holding_id())',
            schema_name, table_name, schema_name, schema_name
        );
    END LOOP;
END;
$$;


-- Disposable, active-generation Learning, Benefits and Field Service projections.
CREATE SCHEMA IF NOT EXISTS erpx_learning_read;
CREATE SCHEMA IF NOT EXISTS erpx_benefits_read;
CREATE SCHEMA IF NOT EXISTS erpx_field_service_read;

CREATE OR REPLACE FUNCTION erpx_learning_read.current_holding_id() RETURNS uuid LANGUAGE SQL STABLE AS $$ SELECT nullif(current_setting('app.holding_id', true), '')::uuid $$;
CREATE OR REPLACE FUNCTION erpx_benefits_read.current_holding_id() RETURNS uuid LANGUAGE SQL STABLE AS $$ SELECT nullif(current_setting('app.holding_id', true), '')::uuid $$;
CREATE OR REPLACE FUNCTION erpx_field_service_read.current_holding_id() RETURNS uuid LANGUAGE SQL STABLE AS $$ SELECT nullif(current_setting('app.holding_id', true), '')::uuid $$;

CREATE TABLE erpx_learning_read.enrollment_projection (
  holding_id uuid NOT NULL DEFAULT erpx_learning_read.current_holding_id(), projection_name text NOT NULL DEFAULT 'learning.enrollments' CHECK (projection_name = 'learning.enrollments'),
  generation uuid NOT NULL, partition_key text NOT NULL, enrollment_id uuid NOT NULL, authority_scope jsonb NOT NULL, lifecycle_state text NOT NULL, projection jsonb NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
  PRIMARY KEY (holding_id,projection_name,generation,partition_key,enrollment_id), UNIQUE (holding_id,projection_name,generation,enrollment_id),
  FOREIGN KEY (holding_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_learning_read.completion_projection (
  holding_id uuid NOT NULL DEFAULT erpx_learning_read.current_holding_id(), projection_name text NOT NULL DEFAULT 'learning.enrollments' CHECK (projection_name = 'learning.enrollments'),
  generation uuid NOT NULL, partition_key text NOT NULL, enrollment_id uuid NOT NULL, employee_record_id uuid NOT NULL, course_id uuid NOT NULL, course_version bigint NOT NULL CHECK (course_version > 0),
  completed_at timestamptz NOT NULL, source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0),
  PRIMARY KEY (holding_id,projection_name,generation,partition_key,enrollment_id),
  FOREIGN KEY (holding_id,projection_name,generation,enrollment_id) REFERENCES erpx_learning_read.enrollment_projection(holding_id,projection_name,generation,enrollment_id) ON DELETE CASCADE
);
CREATE TABLE erpx_learning_read.governed_record_projection (
  holding_id uuid NOT NULL DEFAULT erpx_learning_read.current_holding_id(), projection_name text NOT NULL DEFAULT 'learning.enrollments' CHECK (projection_name = 'learning.enrollments'),
  generation uuid NOT NULL, partition_key text NOT NULL, record_id uuid NOT NULL,
  record_kind text NOT NULL CHECK (record_kind IN ('assignment','completion')),
  authority_scope jsonb NOT NULL, lifecycle_state text NOT NULL, projection jsonb NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
  PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id), UNIQUE (holding_id,projection_name,generation,record_id),
  FOREIGN KEY (holding_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_benefits_read.enrollment_projection (
  holding_id uuid NOT NULL DEFAULT erpx_benefits_read.current_holding_id(), projection_name text NOT NULL DEFAULT 'benefits.enrollments' CHECK (projection_name = 'benefits.enrollments'),
  generation uuid NOT NULL, partition_key text NOT NULL, enrollment_id uuid NOT NULL, authority_scope jsonb NOT NULL, lifecycle_state text NOT NULL, projection jsonb NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
  PRIMARY KEY (holding_id,projection_name,generation,partition_key,enrollment_id), UNIQUE (holding_id,projection_name,generation,enrollment_id),
  FOREIGN KEY (holding_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_benefits_read.contribution_projection (
  holding_id uuid NOT NULL DEFAULT erpx_benefits_read.current_holding_id(), projection_name text NOT NULL DEFAULT 'benefits.enrollments' CHECK (projection_name = 'benefits.enrollments'),
  generation uuid NOT NULL, partition_key text NOT NULL, enrollment_id uuid NOT NULL, employee_minor bigint NOT NULL, employer_minor bigint NOT NULL, currency char(3) NOT NULL,
  provider_acknowledged boolean NOT NULL, payroll_handed_off boolean NOT NULL, source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0),
  PRIMARY KEY (holding_id,projection_name,generation,partition_key,enrollment_id),
  FOREIGN KEY (holding_id,projection_name,generation,enrollment_id) REFERENCES erpx_benefits_read.enrollment_projection(holding_id,projection_name,generation,enrollment_id) ON DELETE CASCADE
);
CREATE TABLE erpx_benefits_read.governed_record_projection (
  holding_id uuid NOT NULL DEFAULT erpx_benefits_read.current_holding_id(), projection_name text NOT NULL DEFAULT 'benefits.enrollments' CHECK (projection_name = 'benefits.enrollments'),
  generation uuid NOT NULL, partition_key text NOT NULL, record_id uuid NOT NULL,
  record_kind text NOT NULL CHECK (record_kind IN ('window','handoff')),
  authority_scope jsonb NOT NULL, lifecycle_state text NOT NULL, projection jsonb NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
  PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id), UNIQUE (holding_id,projection_name,generation,record_id),
  FOREIGN KEY (holding_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_field_service_read.order_projection (
  holding_id uuid NOT NULL DEFAULT erpx_field_service_read.current_holding_id(), projection_name text NOT NULL DEFAULT 'field_service.orders' CHECK (projection_name = 'field_service.orders'),
  generation uuid NOT NULL, partition_key text NOT NULL, service_order_id uuid NOT NULL, authority_scope jsonb NOT NULL, lifecycle_state text NOT NULL, projection jsonb NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
  PRIMARY KEY (holding_id,projection_name,generation,partition_key,service_order_id), UNIQUE (holding_id,projection_name,generation,service_order_id),
  FOREIGN KEY (holding_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_field_service_read.dispatch_projection (
  holding_id uuid NOT NULL DEFAULT erpx_field_service_read.current_holding_id(), projection_name text NOT NULL DEFAULT 'field_service.orders' CHECK (projection_name = 'field_service.orders'),
  generation uuid NOT NULL, partition_key text NOT NULL, service_order_id uuid NOT NULL, technician_record_id uuid, scheduled_on date,
  coverage_valid boolean NOT NULL, skills_valid boolean NOT NULL, inventory_valid boolean NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0),
  PRIMARY KEY (holding_id,projection_name,generation,partition_key,service_order_id),
  FOREIGN KEY (holding_id,projection_name,generation,service_order_id) REFERENCES erpx_field_service_read.order_projection(holding_id,projection_name,generation,service_order_id) ON DELETE CASCADE
);
CREATE TABLE erpx_field_service_read.exception_projection (
  holding_id uuid NOT NULL DEFAULT erpx_field_service_read.current_holding_id(), projection_name text NOT NULL DEFAULT 'field_service.orders' CHECK (projection_name = 'field_service.orders'),
  generation uuid NOT NULL, partition_key text NOT NULL, exception_id uuid NOT NULL, service_order_id uuid NOT NULL, exception_kind text NOT NULL, reason text NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0),
  PRIMARY KEY (holding_id,projection_name,generation,partition_key,exception_id),
  FOREIGN KEY (holding_id,projection_name,generation,service_order_id) REFERENCES erpx_field_service_read.order_projection(holding_id,projection_name,generation,service_order_id) ON DELETE CASCADE
);
CREATE TABLE erpx_field_service_read.governed_record_projection (
  holding_id uuid NOT NULL DEFAULT erpx_field_service_read.current_holding_id(), projection_name text NOT NULL DEFAULT 'field_service.orders' CHECK (projection_name = 'field_service.orders'),
  generation uuid NOT NULL, partition_key text NOT NULL, record_id uuid NOT NULL,
  record_kind text NOT NULL CHECK (record_kind IN ('request','dispatch','visit','handoff')),
  authority_scope jsonb NOT NULL, lifecycle_state text NOT NULL, projection jsonb NOT NULL,
  source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0), source_position bigint NOT NULL CHECK (source_position > 0), projected_at timestamptz NOT NULL,
  PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id), UNIQUE (holding_id,projection_name,generation,record_id),
  FOREIGN KEY (holding_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation) ON DELETE CASCADE
);

DO $$
DECLARE relation_name text;
BEGIN
  FOREACH relation_name IN ARRAY ARRAY[
    'erpx_learning_read.enrollment_projection','erpx_learning_read.completion_projection','erpx_learning_read.governed_record_projection',
    'erpx_benefits_read.enrollment_projection','erpx_benefits_read.contribution_projection','erpx_benefits_read.governed_record_projection',
    'erpx_field_service_read.order_projection','erpx_field_service_read.dispatch_projection','erpx_field_service_read.exception_projection','erpx_field_service_read.governed_record_projection'
  ] LOOP
    EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', relation_name);
    EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', relation_name);
    EXECUTE format('CREATE POLICY holding_isolation ON %s USING (holding_id = nullif(current_setting(''app.holding_id'', true), '''')::uuid) WITH CHECK (holding_id = nullif(current_setting(''app.holding_id'', true), '''')::uuid)', relation_name);
  END LOOP;
END $$;


-- Disposable shared-service projections. They are rebuilt only from typed
-- owner descriptors or shared-service coordination artifacts.

CREATE SCHEMA IF NOT EXISTS erpx_shared_read;

CREATE OR REPLACE FUNCTION erpx_shared_read.current_holding_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.holding_id', true), '')::uuid
$$;

CREATE TABLE erpx_shared_read.work_projection (
    holding_id UUID NOT NULL DEFAULT erpx_shared_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'shared.work' CHECK (projection_name = 'shared.work'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    source_id TEXT NOT NULL,
    work_id TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    source_record_id UUID NOT NULL,
    source_record_type TEXT NOT NULL,
    source_record_version BIGINT NOT NULL CHECK (source_record_version > 0),
    source_position BIGINT NOT NULL CHECK (source_position > 0),
    freshness TEXT NOT NULL CHECK (freshness IN ('fresh','stale','unknown')),
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, source_id, work_id),
    FOREIGN KEY (holding_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation) ON DELETE CASCADE
);

CREATE TABLE erpx_shared_read.notification_projection (
    holding_id UUID NOT NULL DEFAULT erpx_shared_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'shared.notifications' CHECK (projection_name = 'shared.notifications'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    notification_id UUID NOT NULL,
    recipient_ref TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    inbox_state TEXT NOT NULL CHECK (inbox_state IN ('unread','read','acknowledged','action_required','withdrawn')),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, notification_id),
    FOREIGN KEY (holding_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation) ON DELETE CASCADE
);

CREATE TABLE erpx_shared_read.reference_projection (
    holding_id UUID NOT NULL DEFAULT erpx_shared_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'shared.references' CHECK (projection_name = 'shared.references'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    owner_context TEXT NOT NULL,
    record_type TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    stable_code TEXT NOT NULL,
    classification TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    descriptor_version BIGINT NOT NULL CHECK (descriptor_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    descriptor JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, record_id),
    FOREIGN KEY (holding_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation) ON DELETE CASCADE
);

CREATE TABLE erpx_shared_read.search_projection (
    holding_id UUID NOT NULL DEFAULT erpx_shared_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'shared.search' CHECK (projection_name = 'shared.search'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    owner_context TEXT NOT NULL,
    record_type TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    classification TEXT NOT NULL,
    authorized_document TSVECTOR NOT NULL,
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    authorization_policy_version BIGINT NOT NULL CHECK (authorization_policy_version > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, record_id),
    FOREIGN KEY (holding_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation) ON DELETE CASCADE
);

CREATE TABLE erpx_shared_read.bulk_job_projection (
    holding_id UUID NOT NULL DEFAULT erpx_shared_read.current_holding_id(),
    projection_name TEXT NOT NULL CHECK (projection_name IN ('shared.reports','shared.imports')),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    job_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    job_status TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    state JSONB NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, job_id),
    FOREIGN KEY (holding_id, projection_name, generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation) ON DELETE CASCADE
);

CREATE INDEX shared_work_scope_due ON erpx_shared_read.work_projection(holding_id, generation, scope_key, (state->>'due_at'), source_id, work_id);
CREATE INDEX shared_notification_recipient ON erpx_shared_read.notification_projection(holding_id, generation, recipient_ref, inbox_state, projected_at DESC);
CREATE INDEX shared_reference_code ON erpx_shared_read.reference_projection(holding_id, generation, scope_key, owner_context, stable_code);
CREATE INDEX shared_search_holding_scope ON erpx_shared_read.search_projection(holding_id, generation, scope_key, owner_context, record_type);
CREATE INDEX shared_search_document ON erpx_shared_read.search_projection USING GIN(authorized_document);
CREATE INDEX shared_bulk_job_scope ON erpx_shared_read.bulk_job_projection(holding_id, generation, scope_key, job_status, projected_at DESC);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'work_projection','notification_projection','reference_projection',
        'search_projection','bulk_job_projection'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_shared_read.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_shared_read.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY shared_read_holding_isolation ON erpx_shared_read.%I '
            'USING (holding_id = erpx_shared_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_shared_read.current_holding_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- Final current role marker: erpx_meta.schema_manifest,
-- schema_revision=2, database_role=holding_read, migration_set=holding-read.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_revision = 2
  AND database_role = 'holding_read' AND migration_set = 'holding-read';


-- Disposable, typed authority evidence used only to calculate finite Item
-- Directory grants. Physical candidate databases provide the rebuild fence.

CREATE SCHEMA IF NOT EXISTS erpx_authority_read;

CREATE OR REPLACE FUNCTION erpx_authority_read.current_holding_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.holding_id', true), '')::uuid
$$;

CREATE TABLE erpx_authority_read.principals (
    holding_id UUID NOT NULL DEFAULT erpx_authority_read.current_holding_id(),
    principal_id UUID NOT NULL,
    principal_kind TEXT NOT NULL CHECK (
        principal_kind IN ('human','service','agent','human_group','agent_pool')
    ),
    realm TEXT NOT NULL CHECK (
        realm IN ('internal_workforce','platform_workforce','customer_portal')
    ),
    principal_scope JSONB NOT NULL,
    active BOOLEAN NOT NULL,
    principal_version BIGINT NOT NULL CHECK (principal_version > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    changed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, principal_id)
);

CREATE TABLE erpx_authority_read.roles (
    holding_id UUID NOT NULL DEFAULT erpx_authority_read.current_holding_id(),
    role_id UUID NOT NULL,
    role_scope JSONB NOT NULL,
    role_version BIGINT NOT NULL CHECK (role_version > 0),
    permissions TEXT[] NOT NULL CHECK (cardinality(permissions) BETWEEN 1 AND 256),
    classification_ceiling TEXT NOT NULL CHECK (
        classification_ceiling IN ('public','internal','confidential','restricted')
    ),
    active BOOLEAN NOT NULL,
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    changed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, role_id)
);

CREATE TABLE erpx_authority_read.assignments (
    holding_id UUID NOT NULL DEFAULT erpx_authority_read.current_holding_id(),
    assignment_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    role_id UUID NOT NULL,
    assignment_scope JSONB NOT NULL,
    assignment_version BIGINT NOT NULL CHECK (assignment_version > 0),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    changed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, assignment_id),
    FOREIGN KEY (holding_id, principal_id)
        REFERENCES erpx_authority_read.principals(holding_id, principal_id)
        ON DELETE CASCADE,
    FOREIGN KEY (holding_id, role_id)
        REFERENCES erpx_authority_read.roles(holding_id, role_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (valid_until > valid_from)
);

CREATE TABLE erpx_authority_read.group_memberships (
    holding_id UUID NOT NULL DEFAULT erpx_authority_read.current_holding_id(),
    group_id UUID NOT NULL,
    member_principal_id UUID NOT NULL,
    group_kind TEXT NOT NULL CHECK (group_kind IN ('human_group','agent_pool')),
    group_version BIGINT NOT NULL CHECK (group_version > 0),
    membership_version BIGINT NOT NULL CHECK (membership_version > 0),
    membership_scope JSONB NOT NULL,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    changed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, group_id, member_principal_id),
    FOREIGN KEY (holding_id, group_id)
        REFERENCES erpx_authority_read.principals(holding_id, principal_id)
        ON DELETE CASCADE,
    FOREIGN KEY (holding_id, member_principal_id)
        REFERENCES erpx_authority_read.principals(holding_id, principal_id)
        ON DELETE CASCADE,
    CHECK (valid_until > valid_from)
);

CREATE TABLE erpx_authority_read.organization_sharing_agreements (
    holding_id UUID NOT NULL DEFAULT erpx_authority_read.current_holding_id(),
    sharing_agreement_id UUID NOT NULL,
    agreement_version BIGINT NOT NULL CHECK (agreement_version > 0),
    source_scope JSONB NOT NULL,
    target_scope JSONB NOT NULL,
    direction TEXT NOT NULL CHECK (direction IN ('source_to_target','mutual')),
    permits_open_record BOOLEAN NOT NULL,
    source_approved BOOLEAN NOT NULL,
    target_approved BOOLEAN NOT NULL,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    sharing_status TEXT NOT NULL CHECK (
        sharing_status IN ('proposed','active','rejected','revoked','expired')
    ),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    changed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, sharing_agreement_id),
    CHECK (valid_until > valid_from)
);

CREATE TABLE erpx_authority_read.customer_portal_principal_bindings (
    holding_id UUID NOT NULL DEFAULT erpx_authority_read.current_holding_id(),
    binding_id UUID NOT NULL,
    binding_version BIGINT NOT NULL CHECK (binding_version > 0),
    customer_record_id UUID NOT NULL,
    customer_record_version BIGINT NOT NULL CHECK (customer_record_version > 0),
    portal_principal_id UUID NOT NULL,
    principal_kind TEXT NOT NULL CHECK (principal_kind IN ('human','service')),
    portal_realm TEXT NOT NULL CHECK (portal_realm = 'customer_portal'),
    binding_scope JSONB NOT NULL,
    verified_identity_digest TEXT NOT NULL CHECK (
        verified_identity_digest ~ '^[0-9A-Fa-f]{64}$'
    ),
    verified_identity_version BIGINT NOT NULL CHECK (verified_identity_version > 0),
    permitted_use_cases TEXT[] NOT NULL CHECK (
        cardinality(permitted_use_cases) = 1
        AND permitted_use_cases <@ ARRAY['field_service_record_access']::TEXT[]
    ),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    proposed_by UUID NOT NULL,
    approved_by UUID,
    ended_at TIMESTAMPTZ,
    binding_status TEXT NOT NULL CHECK (
        binding_status IN ('proposed','active','rejected','revoked','expired')
    ),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    changed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, binding_id),
    FOREIGN KEY (holding_id, portal_principal_id)
        REFERENCES erpx_authority_read.principals(holding_id, principal_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (valid_until > valid_from),
    CHECK (binding_status <> 'active' OR (
        approved_by IS NOT NULL AND approved_by <> proposed_by AND ended_at IS NULL
    ))
);

-- Rebuildable Organization-authored lineage witnesses used by the Item
-- Directory's bounded batch resolver. The ordered lineage excludes node_id;
-- no consumer may reinterpret a node UUID as a Company or Branch column.
CREATE TABLE erpx_authority_read.organization_nodes (
    holding_id UUID NOT NULL DEFAULT erpx_authority_read.current_holding_id(),
    node_id UUID NOT NULL,
    node_kind TEXT NOT NULL CHECK (node_kind IN (
        'holding','company','headquarters','branch','department','team'
    )),
    node_status TEXT NOT NULL CHECK (node_status IN ('active','suspended','archived')),
    parent_node_id UUID,
    lineage_node_ids UUID[] NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    source_event_position BIGINT NOT NULL CHECK (source_event_position > 0),
    changed_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,node_id),
    CHECK (cardinality(lineage_node_ids) BETWEEN 0 AND 4),
    CHECK (NOT node_id=ANY(lineage_node_ids)),
    CHECK (
        parent_node_id IS NOT DISTINCT FROM
            lineage_node_ids[cardinality(lineage_node_ids)]
    ),
    CHECK (
        (node_kind='holding' AND holding_id=node_id
            AND cardinality(lineage_node_ids)=0 AND parent_node_id IS NULL)
        OR (node_kind='company' AND cardinality(lineage_node_ids)=1
            AND lineage_node_ids[1]=holding_id)
        OR (node_kind IN ('headquarters','branch')
            AND cardinality(lineage_node_ids)=2
            AND lineage_node_ids[1]=holding_id)
        OR (node_kind='department' AND cardinality(lineage_node_ids) IN (2,3)
            AND lineage_node_ids[1]=holding_id)
        OR (node_kind='team' AND cardinality(lineage_node_ids) IN (3,4)
            AND lineage_node_ids[1]=holding_id)
    )
);

CREATE TABLE erpx_authority_read.policy_decision_checkpoint (
    holding_id UUID PRIMARY KEY DEFAULT erpx_authority_read.current_holding_id(),
    source_event_position BIGINT NOT NULL CHECK (source_event_position >= 0),
    decision_version BIGINT NOT NULL CHECK (decision_version > 0),
    last_success_at TIMESTAMPTZ,
    last_error_code TEXT,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX authority_read_assignments_effective
    ON erpx_authority_read.assignments
        (holding_id, principal_id, role_id, valid_from, valid_until)
    WHERE revoked_at IS NULL;
CREATE INDEX authority_read_memberships_effective
    ON erpx_authority_read.group_memberships
        (holding_id, group_id, member_principal_id, valid_from, valid_until)
    WHERE revoked_at IS NULL;
CREATE INDEX authority_read_sharing_effective
    ON erpx_authority_read.organization_sharing_agreements
        (holding_id, sharing_status, valid_from, valid_until)
    WHERE sharing_status = 'active';
CREATE INDEX authority_read_portal_binding_effective
    ON erpx_authority_read.customer_portal_principal_bindings
        (holding_id, portal_principal_id, customer_record_id, valid_from, valid_until)
    WHERE binding_status = 'active';
CREATE INDEX authority_read_organization_node_lineage
    ON erpx_authority_read.organization_nodes USING GIN (lineage_node_ids);
CREATE INDEX authority_read_organization_node_kind_status
    ON erpx_authority_read.organization_nodes
        (holding_id,node_kind,node_status,node_id);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'principals','roles','assignments','group_memberships',
        'organization_sharing_agreements','customer_portal_principal_bindings',
        'organization_nodes','policy_decision_checkpoint'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_authority_read.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_authority_read.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY authority_read_holding_isolation ON erpx_authority_read.%I '
            'USING (holding_id = erpx_authority_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_authority_read.current_holding_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- Final current role marker: erpx_meta.schema_manifest,
-- schema_revision=2, database_role=holding_read, migration_set=holding-read.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_revision = 2
  AND database_role = 'holding_read' AND migration_set = 'holding-read';


-- current current-context projection completion.

ALTER TABLE erpx_crm_read.lead_projection
    ADD COLUMN campaign_id UUID NOT NULL,
    ADD COLUMN audience_member_id UUID NOT NULL,
    ADD COLUMN marketing_source_version BIGINT NOT NULL
        CHECK (marketing_source_version > 0);

CREATE INDEX crm_lead_marketing_provenance
    ON erpx_crm_read.lead_projection
        (holding_id, generation, campaign_id, audience_member_id,
         marketing_source_version, lead_id);

-- Identity governance records are disposable projections sourced only from
-- committed governance snapshot facts. They never fall back to the write DB.
CREATE TABLE erpx_identity_access_read.governance_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'identity_access.governance'
        CHECK (projection_name = 'identity_access.governance'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (
        record_type IN ('identity.access-review','identity.break-glass-session')
    ),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation)
        ON DELETE CASCADE
);
CREATE INDEX identity_governance_projection_scope
    ON erpx_identity_access_read.governance_projection
        (holding_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_identity_access_read.governance_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_identity_access_read.governance_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY identity_governance_read_holding_isolation
    ON erpx_identity_access_read.governance_projection
    USING (holding_id=erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id());

-- D13 Batch B/F: active-generation employment and payslip projections.
CREATE TABLE erpx_people_payroll_read.employment_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'people_payroll.employees'
        CHECK (projection_name = 'people_payroll.employees'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    employment_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    employee_code TEXT NOT NULL,
    display_name TEXT NOT NULL,
    hired_on DATE NOT NULL,
    terminated_on DATE,
    employment_status TEXT NOT NULL CHECK (employment_status IN (
        'draft','active','on_leave','suspended','terminated'
    )),
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,employment_id),
    UNIQUE (holding_id,projection_name,generation,employee_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_people_payroll_read.payslip_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'people_payroll.payroll_runs'
        CHECK (projection_name = 'people_payroll.payroll_runs'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    payslip_id UUID NOT NULL,
    payroll_run_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    period_code TEXT NOT NULL,
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    gross_minor BIGINT NOT NULL CHECK (gross_minor >= 0),
    deductions_minor BIGINT NOT NULL CHECK (deductions_minor >= 0),
    net_minor BIGINT NOT NULL CHECK (net_minor >= 0),
    payslip_status TEXT NOT NULL CHECK (payslip_status IN (
        'draft','review','approved','posted','cancelled'
    )),
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,payslip_id),
    UNIQUE (holding_id,projection_name,generation,payroll_run_id,employee_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE,
    CHECK (gross_minor - deductions_minor = net_minor)
);
CREATE INDEX employment_projection_scope_status
    ON erpx_people_payroll_read.employment_projection
        (holding_id,generation,scope_key,employment_status,employment_id);
CREATE INDEX payslip_projection_scope_status
    ON erpx_people_payroll_read.payslip_projection
        (holding_id,generation,scope_key,payslip_status,payroll_run_id,employee_id);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY['employment_projection','payslip_projection'] LOOP
        EXECUTE format('ALTER TABLE erpx_people_payroll_read.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_people_payroll_read.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY people_completion_read_holding_isolation ON erpx_people_payroll_read.%I '
            'USING (holding_id = erpx_cqrs_read.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_read.current_holding_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch B/F: active-generation Legal hold projection.
CREATE TABLE erpx_legal_read.legal_hold_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'legal.matters'
        CHECK (projection_name = 'legal.matters'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    legal_hold_id UUID NOT NULL,
    matter_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    matter_reference TEXT NOT NULL,
    hold_active BOOLEAN NOT NULL,
    reason TEXT NOT NULL,
    placed_by TEXT NOT NULL,
    placed_at TIMESTAMPTZ NOT NULL,
    released_by TEXT,
    released_at TIMESTAMPTZ,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,legal_hold_id),
    UNIQUE (holding_id,projection_name,generation,matter_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE,
    CHECK (
        (hold_active AND released_by IS NULL AND released_at IS NULL)
        OR (NOT hold_active AND released_by IS NOT NULL
            AND released_at IS NOT NULL AND released_at >= placed_at)
    )
);
CREATE INDEX legal_hold_projection_scope_active
    ON erpx_legal_read.legal_hold_projection
        (holding_id,generation,scope_key,hold_active,legal_hold_id);
ALTER TABLE erpx_legal_read.legal_hold_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_legal_read.legal_hold_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY legal_hold_projection_holding_isolation
    ON erpx_legal_read.legal_hold_projection
    USING (holding_id = erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id = erpx_cqrs_read.current_holding_id());

-- D13 Batch G: rebuildable Marketing business-record projection.
CREATE TABLE erpx_marketing_read.business_record_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'marketing.business-records'
        CHECK (projection_name='marketing.business-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'marketing.campaign','marketing.content','marketing.audience-activation',
        'marketing.consent-purpose'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    holding_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX marketing_business_record_projection_scope
    ON erpx_marketing_read.business_record_projection
        (holding_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_marketing_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_marketing_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY marketing_business_record_read_holding_isolation
    ON erpx_marketing_read.business_record_projection
    USING (holding_id=erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id());

-- D13 Batch A: disposable active-generation Organization sharing projection.
CREATE TABLE erpx_organization_read.sharing_agreement_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'organization.sharing-agreements'
        CHECK (projection_name='organization.sharing-agreements'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    sharing_agreement_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    source_scope JSONB NOT NULL,
    target_scope JSONB NOT NULL,
    sharing_status TEXT NOT NULL CHECK (sharing_status IN (
        'proposed','active','rejected','revoked','expired'
    )),
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (
        holding_id,projection_name,generation,partition_key,sharing_agreement_id
    ),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX organization_sharing_projection_status
    ON erpx_organization_read.sharing_agreement_projection
        (holding_id,generation,sharing_status,sharing_agreement_id);
ALTER TABLE erpx_organization_read.sharing_agreement_projection
    ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_organization_read.sharing_agreement_projection
    FORCE ROW LEVEL SECURITY;
CREATE POLICY organization_sharing_projection_holding_isolation
    ON erpx_organization_read.sharing_agreement_projection
    USING (holding_id=erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id());

-- D13 Batch G: rebuildable CRM business-record projection.
CREATE TABLE erpx_crm_read.business_record_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'crm.business-records'
        CHECK (projection_name='crm.business-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'crm.activity','crm.forecast'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    holding_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX crm_business_record_projection_scope
    ON erpx_crm_read.business_record_projection
        (holding_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_crm_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_crm_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY crm_business_record_read_holding_isolation
    ON erpx_crm_read.business_record_projection
    USING (holding_id=erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id());

-- D13 Batch G: rebuildable Finance business-record projection.
CREATE TABLE erpx_finance_read.business_record_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'finance.business-records'
        CHECK (projection_name='finance.business-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'finance.credit-note','finance.debit-note','finance.payment','finance.reconciliation'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    holding_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX finance_business_record_projection_scope
    ON erpx_finance_read.business_record_projection
        (holding_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_finance_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_finance_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY finance_business_record_read_holding_isolation
    ON erpx_finance_read.business_record_projection
    USING (holding_id=erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id());



-- D13 Batch G: rebuildable Procurement business-record projection.
CREATE TABLE erpx_procurement_read.business_record_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'procurement.business-records'
        CHECK (projection_name='procurement.business-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'procurement.requisition','procurement.rfq','procurement.receipt','procurement.supplier-return'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    holding_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX procurement_business_record_projection_scope
    ON erpx_procurement_read.business_record_projection
        (holding_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_procurement_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_procurement_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY procurement_business_record_read_holding_isolation
    ON erpx_procurement_read.business_record_projection
    USING (holding_id=erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id());





-- D13 Batch B/F2: active-generation projections for the remaining current families.
CREATE SCHEMA IF NOT EXISTS erpx_customer_authority_read;
CREATE SCHEMA IF NOT EXISTS erpx_customer_360_read;

CREATE TABLE erpx_customer_authority_read.business_authority_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(), projection_name TEXT NOT NULL DEFAULT 'customer_authority.business-records',
    generation UUID NOT NULL, partition_key TEXT NOT NULL, record_id UUID NOT NULL, record_kind TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK(source_aggregate_version>0), source_global_sequence BIGINT NOT NULL CHECK(source_global_sequence>0),
    holding_scope JSONB NOT NULL, scope_key TEXT NOT NULL, customer_id UUID NOT NULL, record_status TEXT NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL, projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY(holding_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_customer_360_read.owned_record_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(), projection_name TEXT NOT NULL DEFAULT 'customer360.owned-records',
    generation UUID NOT NULL, partition_key TEXT NOT NULL, record_id UUID NOT NULL, record_kind TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK(source_aggregate_version>0), source_global_sequence BIGINT NOT NULL CHECK(source_global_sequence>0),
    holding_scope JSONB NOT NULL, scope_key TEXT NOT NULL, customer_account_id UUID NOT NULL, title TEXT NOT NULL, record_status TEXT NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL, PRIMARY KEY(holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY(holding_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_support_read.platform_case_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(), projection_name TEXT NOT NULL DEFAULT 'support.platform-cases',
    generation UUID NOT NULL, partition_key TEXT NOT NULL, case_id UUID NOT NULL, source_aggregate_version BIGINT NOT NULL CHECK(source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK(source_global_sequence>0), holding_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    case_code TEXT NOT NULL, title TEXT NOT NULL, case_status TEXT NOT NULL, projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(holding_id,projection_name,generation,partition_key,case_id), FOREIGN KEY(holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_people_payroll_read.time_record_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(), projection_name TEXT NOT NULL DEFAULT 'people_payroll.time-records',
    generation UUID NOT NULL, partition_key TEXT NOT NULL, time_record_id UUID NOT NULL, source_aggregate_version BIGINT NOT NULL CHECK(source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK(source_global_sequence>0), holding_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    employee_id UUID NOT NULL, work_date DATE NOT NULL, minutes INTEGER NOT NULL, cost_center TEXT NOT NULL, time_status TEXT NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL, PRIMARY KEY(holding_id,projection_name,generation,partition_key,time_record_id),
    FOREIGN KEY(holding_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_legal_read.legal_case_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(), projection_name TEXT NOT NULL DEFAULT 'legal.cases',
    generation UUID NOT NULL, partition_key TEXT NOT NULL, case_id UUID NOT NULL, source_aggregate_version BIGINT NOT NULL CHECK(source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK(source_global_sequence>0), holding_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    case_reference TEXT NOT NULL, title TEXT NOT NULL, case_status TEXT NOT NULL, retention_until TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL, PRIMARY KEY(holding_id,projection_name,generation,partition_key,case_id),
    FOREIGN KEY(holding_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE TABLE erpx_shared_read.signature_envelope_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(), projection_name TEXT NOT NULL DEFAULT 'shared.signatures',
    generation UUID NOT NULL, partition_key TEXT NOT NULL, envelope_id UUID NOT NULL, source_aggregate_version BIGINT NOT NULL CHECK(source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK(source_global_sequence>0), holding_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    document_id UUID NOT NULL, document_version BIGINT NOT NULL, envelope_status TEXT NOT NULL, retention_until TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL, PRIMARY KEY(holding_id,projection_name,generation,partition_key,envelope_id),
    FOREIGN KEY(holding_id,projection_name,generation) REFERENCES erpx_cqrs_read.projection_generations(holding_id,projection_name,generation) ON DELETE CASCADE
);

DO $$ DECLARE item RECORD; BEGIN
    FOR item IN SELECT * FROM (VALUES
        ('erpx_customer_authority_read','business_authority_projection','customer_authority_completion_read_isolation'),
        ('erpx_customer_360_read','owned_record_projection','customer_360_owned_read_isolation'),
        ('erpx_support_read','platform_case_projection','support_platform_case_read_isolation'),
        ('erpx_people_payroll_read','time_record_projection','people_time_record_read_isolation'),
        ('erpx_legal_read','legal_case_projection','legal_case_read_isolation'),
        ('erpx_shared_read','signature_envelope_projection','signature_envelope_read_isolation')) AS v(schema_name,table_name,policy_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY',item.schema_name,item.table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY',item.schema_name,item.table_name);
        EXECUTE format('CREATE POLICY %I ON %I.%I USING (holding_id=erpx_cqrs_read.current_holding_id()) WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id())',item.policy_name,item.schema_name,item.table_name);
    END LOOP;
END $$;

-- D13 Batch H: rebuildable Assets lifecycle-record projection.
CREATE TABLE erpx_assets_read.business_record_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'asset.lifecycle-records'
        CHECK (projection_name='asset.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'asset.transfer','asset.depreciation-run','asset.disposal'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    holding_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX assets_business_record_projection_scope
    ON erpx_assets_read.business_record_projection
        (holding_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_assets_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_assets_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY assets_business_record_read_holding_isolation
    ON erpx_assets_read.business_record_projection
    USING (holding_id=erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id());

-- D13 Batch I: rebuildable Maintenance lifecycle-record projection.
CREATE TABLE erpx_maintenance_read.business_record_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'maintenance.lifecycle-records'
        CHECK (projection_name='maintenance.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'maintenance.plan','maintenance.inspection','maintenance.request'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    holding_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX maintenance_business_record_projection_scope
    ON erpx_maintenance_read.business_record_projection
        (holding_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_maintenance_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_maintenance_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY maintenance_business_record_read_holding_isolation
    ON erpx_maintenance_read.business_record_projection
    USING (holding_id=erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id());

-- D13 Batch J: rebuildable Quality lifecycle-record projection.
CREATE TABLE erpx_quality_read.business_record_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'quality.lifecycle-records'
        CHECK (projection_name='quality.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'quality.plan','quality.nonconformance','quality.corrective-action'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    holding_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX quality_business_record_projection_scope
    ON erpx_quality_read.business_record_projection
        (holding_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_quality_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_quality_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY quality_business_record_read_holding_isolation
    ON erpx_quality_read.business_record_projection
    USING (holding_id=erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id());

-- D13 Batch K: rebuildable Operations lifecycle-record projection.
CREATE TABLE erpx_operations_read.business_record_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'operations.lifecycle-records'
        CHECK (projection_name='operations.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'operations.operation','operations.case','operations.approval-work'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    holding_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX operations_business_record_projection_scope
    ON erpx_operations_read.business_record_projection
        (holding_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_operations_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_operations_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY operations_business_record_read_holding_isolation
    ON erpx_operations_read.business_record_projection
    USING (holding_id=erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id());

-- D13 Batch L: rebuildable Commercial lifecycle-record projection.
CREATE TABLE erpx_commercial_read.business_record_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'commercial.lifecycle-records'
        CHECK (projection_name='commercial.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'commercial.account','commercial.adjustment'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    holding_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX commercial_business_record_projection_scope
    ON erpx_commercial_read.business_record_projection
        (holding_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_commercial_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_commercial_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY commercial_business_record_read_holding_isolation
    ON erpx_commercial_read.business_record_projection
    USING (holding_id=erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id());

-- D13 Batch M: rebuildable POS lifecycle-record projection.
CREATE TABLE erpx_pos_read.business_record_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'pos.lifecycle-records'
        CHECK (projection_name='pos.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'pos.register-session','pos.cash-movement'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    holding_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX pos_business_record_projection_scope
    ON erpx_pos_read.business_record_projection
        (holding_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_pos_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_pos_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY pos_business_record_read_holding_isolation
    ON erpx_pos_read.business_record_projection
    USING (holding_id=erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id());

-- D13 Batch N: rebuildable Sales lifecycle-record projection.
CREATE TABLE erpx_sales_read.business_record_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'sales.lifecycle-records'
        CHECK (projection_name='sales.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'sales.return','sales.fulfillment-handoff'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    holding_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX sales_business_record_projection_scope
    ON erpx_sales_read.business_record_projection
        (holding_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_sales_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_sales_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY sales_business_record_read_holding_isolation
    ON erpx_sales_read.business_record_projection
    USING (holding_id=erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id());

-- D13 Batch O: rebuildable Inventory lifecycle-record projection.
CREATE TABLE erpx_inventory_read.business_record_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'inventory.lifecycle-records'
        CHECK (projection_name='inventory.lifecycle-records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (record_type IN (
        'inventory.document','inventory.transfer','inventory.count','inventory.lot','inventory.serial'
    )),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version>0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence>0),
    holding_scope JSONB NOT NULL,
    record_title TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,record_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX inventory_business_record_projection_scope
    ON erpx_inventory_read.business_record_projection
        (holding_id,generation,record_type,lifecycle_state,record_id);
ALTER TABLE erpx_inventory_read.business_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_inventory_read.business_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY inventory_business_record_read_holding_isolation
    ON erpx_inventory_read.business_record_projection
    USING (holding_id=erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id());

-- Final current role marker: erpx_meta.schema_manifest,
-- D13 Batch O2: active-generation reservation projection derived from StockItem snapshots.
CREATE TABLE erpx_inventory_read.reservation_projection (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'inventory.stock_items'
        CHECK (projection_name='inventory.stock_items'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    reservation_id UUID NOT NULL,
    stock_item_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    holding_scope JSONB NOT NULL,
    warehouse_id UUID NOT NULL,
    item_code TEXT NOT NULL,
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    demand_reference TEXT NOT NULL,
    reservation_status TEXT NOT NULL CHECK (reservation_status IN ('active','released')),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,projection_name,generation,partition_key,reservation_id),
    FOREIGN KEY (holding_id,projection_name,generation)
        REFERENCES erpx_cqrs_read.projection_generations
            (holding_id,projection_name,generation) ON DELETE CASCADE
);
CREATE INDEX inventory_reservation_projection_scope
    ON erpx_inventory_read.reservation_projection
        (holding_id,generation,stock_item_id,reservation_status,reservation_id);
ALTER TABLE erpx_inventory_read.reservation_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_inventory_read.reservation_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY inventory_reservation_read_holding_isolation
    ON erpx_inventory_read.reservation_projection
    USING (holding_id=erpx_cqrs_read.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_read.current_holding_id());

-- Final current role marker: erpx_meta.schema_manifest,
-- schema_revision=2, database_role=holding_read, migration_set=holding-read.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_revision = 2
  AND database_role = 'holding_read' AND migration_set = 'holding-read';


-- Disposable generation-managed channel governance projection.
CREATE TABLE erpx_ecommerce_read.channel_projection (
    holding_id UUID NOT NULL DEFAULT erpx_ecommerce_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'ecommerce.channels',
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    channel_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    holding_scope JSONB NOT NULL,
    channel_code TEXT NOT NULL,
    channel_status TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, channel_id),
    FOREIGN KEY (holding_id, projection_name, generation)
      REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation)
      ON DELETE CASCADE
);

CREATE INDEX ecommerce_channel_scope
    ON erpx_ecommerce_read.channel_projection
       (holding_id, generation, scope_key, channel_status, channel_id);

ALTER TABLE erpx_ecommerce_read.channel_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_ecommerce_read.channel_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY holding_isolation ON erpx_ecommerce_read.channel_projection
USING (holding_id = erpx_ecommerce_read.current_holding_id())
WITH CHECK (holding_id = erpx_ecommerce_read.current_holding_id());


-- Final current role marker after the E-commerce channel projection.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_revision = 2
  AND database_role = 'holding_read' AND migration_set = 'holding-read';


-- Active-generation detail rows for workflow templates and typed handoffs.
CREATE TABLE erpx_projects_read.project_record_projection (
    holding_id UUID NOT NULL DEFAULT erpx_projects_read.current_holding_id(),
    projection_name TEXT NOT NULL DEFAULT 'projects.records' CHECK (projection_name = 'projects.records'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    record_id UUID NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('workflow_template', 'handoff')),
    project_id UUID,
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    lifecycle_status TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    state JSONB NOT NULL,
    aggregate_updated_at TIMESTAMPTZ NOT NULL,
    projected_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, record_id),
    FOREIGN KEY (holding_id, projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation)
        ON DELETE CASCADE,
    CHECK ((record_kind = 'workflow_template' AND project_id IS NULL)
        OR (record_kind = 'handoff' AND project_id IS NOT NULL))
);

CREATE INDEX projects_record_read_scope
    ON erpx_projects_read.project_record_projection
    (holding_id, generation, scope_key, record_kind, aggregate_updated_at DESC, record_id DESC);

ALTER TABLE erpx_projects_read.project_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_projects_read.project_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY projects_record_read_holding_isolation
    ON erpx_projects_read.project_record_projection
    USING (holding_id = erpx_projects_read.current_holding_id())
    WITH CHECK (holding_id = erpx_projects_read.current_holding_id());

UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_revision = 2
  AND database_role = 'holding_read' AND migration_set = 'holding-read';


-- Generation-managed detail projections for Manufacturing specifications and
-- governed Planning records. These tables are disposable read models only.
CREATE TABLE erpx_manufacturing_read.specification_projection (
    holding_id uuid NOT NULL DEFAULT erpx_manufacturing_read.current_holding_id(),
    projection_name text NOT NULL DEFAULT 'manufacturing.orders'
        CHECK (projection_name = 'manufacturing.orders'),
    generation uuid NOT NULL,
    partition_key text NOT NULL,
    specification_id uuid NOT NULL,
    authority_scope jsonb NOT NULL,
    lifecycle_state text NOT NULL,
    projection jsonb NOT NULL,
    source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0),
    projected_at timestamptz NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, specification_id),
    UNIQUE (holding_id, projection_name, generation, specification_id),
    FOREIGN KEY (holding_id, projection_name, generation)
      REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation)
      ON DELETE CASCADE
);

CREATE TABLE erpx_planning_read.governed_record_projection (
    holding_id uuid NOT NULL DEFAULT erpx_planning_read.current_holding_id(),
    projection_name text NOT NULL DEFAULT 'planning.runs'
        CHECK (projection_name = 'planning.runs'),
    generation uuid NOT NULL,
    partition_key text NOT NULL,
    record_id uuid NOT NULL,
    record_kind text NOT NULL CHECK (record_kind IN ('scenario', 'recommendation')),
    authority_scope jsonb NOT NULL,
    lifecycle_state text NOT NULL,
    projection jsonb NOT NULL,
    source_aggregate_version bigint NOT NULL CHECK (source_aggregate_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0),
    projected_at timestamptz NOT NULL,
    PRIMARY KEY (holding_id, projection_name, generation, partition_key, record_id),
    UNIQUE (holding_id, projection_name, generation, record_id),
    FOREIGN KEY (holding_id, projection_name, generation)
      REFERENCES erpx_cqrs_read.projection_generations(holding_id, projection_name, generation)
      ON DELETE CASCADE
);

ALTER TABLE erpx_manufacturing_read.specification_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_manufacturing_read.specification_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY manufacturing_specification_holding_isolation
    ON erpx_manufacturing_read.specification_projection
    USING (holding_id = erpx_manufacturing_read.current_holding_id())
    WITH CHECK (holding_id = erpx_manufacturing_read.current_holding_id());

ALTER TABLE erpx_planning_read.governed_record_projection ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_planning_read.governed_record_projection FORCE ROW LEVEL SECURITY;
CREATE POLICY planning_governed_record_holding_isolation
    ON erpx_planning_read.governed_record_projection
    USING (holding_id = erpx_planning_read.current_holding_id())
    WITH CHECK (holding_id = erpx_planning_read.current_holding_id());

UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_revision = 2
  AND database_role = 'holding_read' AND migration_set = 'holding-read';


-- OPERATIONS_LEGACY_PROJECTION_RETIREMENT
-- current Operations projections are generation-fenced under
-- erpx_operations_read and erpx_cqrs_read. Remove the old shared JSON read
-- rows/checkpoints so a route can never fall back to a generation-one model.
DROP TABLE IF EXISTS erpx_holding.operations_read_rows;
DROP TABLE IF EXISTS erpx_holding.operations_projection_checkpoints;

-- Reassert the final current manifest fence after the retirement.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_revision = 2
  AND database_role = 'holding_read' AND migration_set = 'holding-read';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='holding_read' AND migration_set='holding-read'
    ) THEN
        RAISE EXCEPTION 'holding-read current manifest is not current';
    END IF;
END;
$$;

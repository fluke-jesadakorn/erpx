
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.outbox_events (
    event_id UUID PRIMARY KEY,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS erpx_holding.command_idempotency (
    idempotency_key TEXT PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    request_hash TEXT NOT NULL,
    response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE erpx_holding.outbox_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.outbox_events FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_outbox_events ON erpx_holding.outbox_events;
CREATE POLICY holding_scope_outbox_events ON erpx_holding.outbox_events
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_command_idempotency ON erpx_holding.command_idempotency;
CREATE POLICY holding_scope_command_idempotency ON erpx_holding.command_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Operations is holding-local. The database route and the transaction-local
-- data_profile_key setting are the isolation boundary; no control-plane tables are
-- created by this migration.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.operations_aggregate_snapshots (
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> ''),
    aggregate_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    scope JSONB NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    lifecycle_status TEXT NOT NULL CHECK (btrim(lifecycle_status) <> ''),
    state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.operations_events (
    event_id UUID PRIMARY KEY,
    contract_version INTEGER NOT NULL CHECK (contract_version > 0),
    event_type TEXT NOT NULL CHECK (btrim(event_type) <> ''),
    event_version INTEGER NOT NULL CHECK (event_version > 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> ''),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    active_scope JSONB NOT NULL,
    actor JSONB NOT NULL,
    correlation_id UUID NOT NULL,
    causation_id UUID,
    idempotency_key TEXT,
    payload JSONB NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (aggregate_type, aggregate_id, aggregate_version),
    UNIQUE (idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.operations_source_references (
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> ''),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    source_domain TEXT NOT NULL CHECK (
        btrim(source_domain) <> '' AND lower(source_domain) <> 'operations'
    ),
    source_type TEXT NOT NULL CHECK (btrim(source_type) <> ''),
    source_id UUID NOT NULL,
    source_event_id UUID NOT NULL,
    operations_event_id UUID,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id, source_event_id),
    UNIQUE (source_domain, source_type, source_id, source_event_id),
    FOREIGN KEY (aggregate_type, aggregate_id)
        REFERENCES erpx_holding.operations_aggregate_snapshots (aggregate_type, aggregate_id),
    FOREIGN KEY (operations_event_id)
        REFERENCES erpx_holding.operations_events (event_id)
);

CREATE INDEX IF NOT EXISTS operations_aggregate_snapshots_holding_idx
    ON erpx_holding.operations_aggregate_snapshots (data_profile_key, aggregate_type, updated_at);
CREATE INDEX IF NOT EXISTS operations_events_holding_idx
    ON erpx_holding.operations_events (data_profile_key, occurred_at, event_id);
CREATE INDEX IF NOT EXISTS operations_source_references_holding_idx
    ON erpx_holding.operations_source_references (data_profile_key, source_domain, source_type);

ALTER TABLE erpx_holding.operations_aggregate_snapshots ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.operations_aggregate_snapshots FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.operations_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.operations_events FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.operations_source_references ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.operations_source_references FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_operations_aggregate_snapshots
    ON erpx_holding.operations_aggregate_snapshots;
CREATE POLICY holding_scope_operations_aggregate_snapshots
    ON erpx_holding.operations_aggregate_snapshots
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_operations_events ON erpx_holding.operations_events;
CREATE POLICY holding_scope_operations_events
    ON erpx_holding.operations_events
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_operations_source_references
    ON erpx_holding.operations_source_references;
CREATE POLICY holding_scope_operations_source_references
    ON erpx_holding.operations_source_references
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Finance write-side state is holding-local and is rebuildable only from the
-- authoritative posted journal rows. All tables are holding-bound and force
-- the transaction-local erpx.data_profile_key RLS setting.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.finance_charts (
    chart_id UUID PRIMARY KEY,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_periods (
    period_id UUID PRIMARY KEY,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    status TEXT NOT NULL CHECK (status IN ('open', 'locked', 'closed')),
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_posted_journals (
    journal_id UUID PRIMARY KEY,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    period_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    version BIGINT NOT NULL CHECK (version > 0),
    idempotency_key TEXT NOT NULL UNIQUE CHECK (btrim(idempotency_key) <> ''),
    posted_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_command_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    journal_id UUID NOT NULL,
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS finance_charts_company_idx
    ON erpx_holding.finance_charts (data_profile_key, company_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS finance_periods_company_idx
    ON erpx_holding.finance_periods (data_profile_key, company_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS finance_journals_period_idx
    ON erpx_holding.finance_posted_journals (data_profile_key, company_id, period_id, posted_at, journal_id);

ALTER TABLE erpx_holding.finance_charts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_charts FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_periods ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_periods FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_posted_journals ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_posted_journals FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_charts ON erpx_holding.finance_charts;
CREATE POLICY holding_scope_finance_charts ON erpx_holding.finance_charts
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_finance_periods ON erpx_holding.finance_periods;
CREATE POLICY holding_scope_finance_periods ON erpx_holding.finance_periods
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_finance_journals ON erpx_holding.finance_posted_journals;
CREATE POLICY holding_scope_finance_journals ON erpx_holding.finance_posted_journals
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_finance_idempotency ON erpx_holding.finance_command_idempotency;
CREATE POLICY holding_scope_finance_idempotency ON erpx_holding.finance_command_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Marketing write-side state. These rows are authoritative only in the
-- holding write database; read-side tables with the same logical names are
-- disposable projections rebuilt from this state and its outbox.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_profiles (
    profile_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    scope JSONB NOT NULL,
    external_ref TEXT NOT NULL CHECK (btrim(external_ref) <> ''),
    dedupe_key TEXT NOT NULL CHECK (length(dedupe_key) = 64),
    email TEXT,
    phone TEXT,
    locale TEXT NOT NULL CHECK (btrim(locale) <> ''),
    attributes JSONB NOT NULL,
    payload JSONB NOT NULL,
    payload_hash TEXT NOT NULL CHECK (length(payload_hash) = 64),
    version BIGINT NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (data_profile_key, dedupe_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_consents (
    consent_id UUID PRIMARY KEY,
    profile_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    scope JSONB NOT NULL,
    purpose TEXT NOT NULL CHECK (purpose IN ('newsletter', 'product_updates', 'promotional', 'transactional', 'lead_nurture')),
    channel TEXT NOT NULL CHECK (channel IN ('email', 'sms', 'social', 'push', 'webhook')),
    state TEXT NOT NULL CHECK (state IN ('pending_double_opt_in', 'granted', 'revoked')),
    source TEXT NOT NULL CHECK (btrim(source) <> ''),
    jurisdiction TEXT NOT NULL CHECK (btrim(jurisdiction) <> ''),
    policy_version TEXT NOT NULL CHECK (btrim(policy_version) <> ''),
    proof_ref TEXT,
    recorded_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (expires_at IS NULL OR expires_at > recorded_at)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_suppressions (
    suppression_id UUID PRIMARY KEY,
    profile_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    scope JSONB NOT NULL,
    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    source TEXT NOT NULL CHECK (btrim(source) <> ''),
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ,
    payload JSONB NOT NULL,
    payload_hash TEXT NOT NULL CHECK (length(payload_hash) = 64),
    version BIGINT NOT NULL CHECK (version > 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (expires_at IS NULL OR expires_at > created_at)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_audiences (
    audience_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    scope JSONB NOT NULL,
    materialized_at TIMESTAMPTZ NOT NULL,
    profile_count BIGINT NOT NULL CHECK (profile_count >= 0),
    membership_hash TEXT NOT NULL CHECK (length(membership_hash) = 64),
    version BIGINT NOT NULL CHECK (version > 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_audience_memberships (
    audience_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    membership_version BIGINT NOT NULL CHECK (membership_version > 0),
    materialized_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (audience_id, profile_id)
);

-- This ledger is reserved and completed in the same transaction as each
-- mutation. A request hash cannot be rebound to another operation or payload.
CREATE TABLE IF NOT EXISTS erpx_holding.marketing_command_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    operation TEXT NOT NULL CHECK (operation IN ('append_consent', 'revoke_consent', 'upsert_suppression', 'upsert_audience_membership')),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    resource_id UUID NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('reserved', 'completed')),
    response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    CHECK ((status = 'reserved' AND response IS NULL AND completed_at IS NULL)
        OR (status = 'completed' AND response IS NOT NULL AND completed_at IS NOT NULL))
);

CREATE OR REPLACE FUNCTION erpx_holding.reject_marketing_consent_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'marketing consent records are append-only';
END;
$$;

DROP TRIGGER IF EXISTS marketing_consent_append_only
    ON erpx_holding.marketing_consents;
CREATE TRIGGER marketing_consent_append_only
BEFORE UPDATE OR DELETE ON erpx_holding.marketing_consents
FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_marketing_consent_mutation();

CREATE INDEX IF NOT EXISTS marketing_profiles_external_ref_idx
    ON erpx_holding.marketing_profiles (data_profile_key, external_ref);
CREATE INDEX IF NOT EXISTS marketing_consents_profile_idx
    ON erpx_holding.marketing_consents (data_profile_key, profile_id, recorded_at DESC);
CREATE INDEX IF NOT EXISTS marketing_suppressions_profile_idx
    ON erpx_holding.marketing_suppressions (data_profile_key, profile_id, created_at DESC);
CREATE INDEX IF NOT EXISTS marketing_audiences_scope_idx
    ON erpx_holding.marketing_audiences (data_profile_key, updated_at DESC);
CREATE INDEX IF NOT EXISTS marketing_memberships_page_idx
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
ALTER TABLE erpx_holding.marketing_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_profiles ON erpx_holding.marketing_profiles;
CREATE POLICY holding_scope_marketing_profiles ON erpx_holding.marketing_profiles
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_consents ON erpx_holding.marketing_consents;
CREATE POLICY holding_scope_marketing_consents ON erpx_holding.marketing_consents
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_suppressions ON erpx_holding.marketing_suppressions;
CREATE POLICY holding_scope_marketing_suppressions ON erpx_holding.marketing_suppressions
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_audiences ON erpx_holding.marketing_audiences;
CREATE POLICY holding_scope_marketing_audiences ON erpx_holding.marketing_audiences
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_memberships ON erpx_holding.marketing_audience_memberships;
CREATE POLICY holding_scope_marketing_memberships ON erpx_holding.marketing_audience_memberships
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_idempotency ON erpx_holding.marketing_command_idempotency;
CREATE POLICY holding_scope_marketing_idempotency ON erpx_holding.marketing_command_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Sales source aggregates are authoritative in the holding write database.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.sales_aggregates (
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN (
        'lead', 'opportunity', 'quote', 'sales_order',
        'delivery_handoff', 'invoice_handoff', 'collection_state'
    )),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    version BIGINT NOT NULL CHECK (version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id),
    UNIQUE (data_profile_key, aggregate_type, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.sales_command_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS sales_aggregates_scope_idx
    ON erpx_holding.sales_aggregates (data_profile_key, company_id, branch_id, aggregate_type, updated_at DESC);

ALTER TABLE erpx_holding.sales_aggregates ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.sales_aggregates FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.sales_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.sales_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_sales_aggregates ON erpx_holding.sales_aggregates;
CREATE POLICY holding_scope_sales_aggregates ON erpx_holding.sales_aggregates
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_sales_idempotency ON erpx_holding.sales_command_idempotency;
CREATE POLICY holding_scope_sales_idempotency ON erpx_holding.sales_command_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Procurement and Inventory source aggregates are authoritative here.
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
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    version BIGINT NOT NULL CHECK (version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id),
    UNIQUE (data_profile_key, aggregate_type, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.scm_command_idempotency (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, idempotency_key)
);

CREATE INDEX IF NOT EXISTS scm_aggregates_scope_idx
    ON erpx_holding.scm_aggregates
       (data_profile_key, company_id, branch_id, aggregate_type, updated_at DESC);

ALTER TABLE erpx_holding.scm_aggregates ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.scm_aggregates FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.scm_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.scm_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_scm_aggregates ON erpx_holding.scm_aggregates;
CREATE POLICY holding_scope_scm_aggregates ON erpx_holding.scm_aggregates
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_scm_idempotency ON erpx_holding.scm_command_idempotency;
CREATE POLICY holding_scope_scm_idempotency ON erpx_holding.scm_command_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Authoritative Asset and Maintenance source state for one routed holding.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.asset_maintenance_aggregates (
    aggregate_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('asset', 'maintenance_work_order')),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    version BIGINT NOT NULL CHECK (version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id),
    UNIQUE (data_profile_key, aggregate_type, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.asset_maintenance_command_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    operation TEXT NOT NULL CHECK (btrim(operation) <> ''),
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Custody history is append-only and remains separately auditable even though
-- the aggregate payload also contains the complete history for rebuilds.
CREATE TABLE IF NOT EXISTS erpx_holding.asset_custody_transfers (
    asset_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    from_scope JSONB NOT NULL,
    to_scope JSONB NOT NULL,
    from_custodian_ref TEXT NOT NULL CHECK (btrim(from_custodian_ref) <> ''),
    to_custodian_ref TEXT NOT NULL CHECK (btrim(to_custodian_ref) <> ''),
    from_location_ref TEXT NOT NULL CHECK (btrim(from_location_ref) <> ''),
    to_location_ref TEXT NOT NULL CHECK (btrim(to_location_ref) <> ''),
    transferred_at TIMESTAMPTZ NOT NULL,
    actor_ref TEXT NOT NULL CHECK (btrim(actor_ref) <> ''),
    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    command_id UUID NOT NULL,
    transfer JSONB NOT NULL,
    PRIMARY KEY (asset_id, aggregate_version)
);

-- This is a source-owned handoff. Operations may project it, but cannot use
-- this table as an operational source transaction.
CREATE TABLE IF NOT EXISTS erpx_holding.asset_operations_capacity_handoffs (
    source_event_id UUID PRIMARY KEY,
    source_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    source_domain TEXT NOT NULL CHECK (source_domain = 'assets'),
    source_type TEXT NOT NULL CHECK (source_type = 'maintenance_work_order'),
    asset_id UUID NOT NULL,
    downtime_minutes BIGINT NOT NULL CHECK (downtime_minutes > 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (data_profile_key, idempotency_key)
);

CREATE INDEX IF NOT EXISTS asset_maintenance_aggregates_scope_idx
    ON erpx_holding.asset_maintenance_aggregates
        (data_profile_key, company_id, branch_id, aggregate_type, updated_at DESC);
CREATE INDEX IF NOT EXISTS asset_custody_transfers_scope_idx
    ON erpx_holding.asset_custody_transfers (data_profile_key, asset_id, aggregate_version);
CREATE INDEX IF NOT EXISTS asset_capacity_handoffs_scope_idx
    ON erpx_holding.asset_operations_capacity_handoffs
        (data_profile_key, company_id, branch_id, occurred_at DESC);

ALTER TABLE erpx_holding.asset_maintenance_aggregates ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.asset_maintenance_aggregates FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.asset_maintenance_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.asset_maintenance_command_idempotency FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.asset_custody_transfers ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.asset_custody_transfers FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.asset_operations_capacity_handoffs ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.asset_operations_capacity_handoffs FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_asset_maintenance_aggregates
    ON erpx_holding.asset_maintenance_aggregates;
CREATE POLICY holding_scope_asset_maintenance_aggregates
    ON erpx_holding.asset_maintenance_aggregates
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_asset_maintenance_idempotency
    ON erpx_holding.asset_maintenance_command_idempotency;
CREATE POLICY holding_scope_asset_maintenance_idempotency
    ON erpx_holding.asset_maintenance_command_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_asset_custody_transfers
    ON erpx_holding.asset_custody_transfers;
CREATE POLICY holding_scope_asset_custody_transfers
    ON erpx_holding.asset_custody_transfers
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_asset_capacity_handoffs
    ON erpx_holding.asset_operations_capacity_handoffs;
CREATE POLICY holding_scope_asset_capacity_handoffs
    ON erpx_holding.asset_operations_capacity_handoffs
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

CREATE OR REPLACE FUNCTION erpx_holding.reject_asset_custody_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'asset custody history is append-only';
END;
$$;

DROP TRIGGER IF EXISTS asset_custody_transfers_immutable
    ON erpx_holding.asset_custody_transfers;
CREATE TRIGGER asset_custody_transfers_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.asset_custody_transfers
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_asset_custody_mutation();


-- Authoritative Finance AP/AR and cash-control state for one routed holding.
-- The aggregate payload remains the domain-owned source of truth; the
-- specialized columns are routing and query guards only.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.finance_subledger_aggregates (
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
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    version BIGINT NOT NULL CHECK (version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (aggregate_type, aggregate_id),
    UNIQUE (data_profile_key, aggregate_type, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_subledger_command_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    operation TEXT NOT NULL CHECK (btrim(operation) <> ''),
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS finance_subledger_aggregates_scope_idx
    ON erpx_holding.finance_subledger_aggregates
        (data_profile_key, company_id, branch_id, aggregate_type, updated_at DESC);

ALTER TABLE erpx_holding.finance_subledger_aggregates ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_subledger_aggregates FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_subledger_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_subledger_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_subledger_aggregates
    ON erpx_holding.finance_subledger_aggregates;
CREATE POLICY holding_scope_finance_subledger_aggregates
    ON erpx_holding.finance_subledger_aggregates
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_finance_subledger_idempotency
    ON erpx_holding.finance_subledger_command_idempotency;
CREATE POLICY holding_scope_finance_subledger_idempotency
    ON erpx_holding.finance_subledger_command_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Authoritative Marketing delivery dispatches. Provider credentials and
-- provider payloads remain outside this table; the JSON stores only the
-- domain dispatch contract and safe provider references.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_deliveries (
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
    claim_id UUID,
    claimed_by UUID,
    claim_until TIMESTAMPTZ,
    last_failure TEXT,
    terminal_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (data_profile_key, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_delivery_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    delivery_id UUID NOT NULL,
    payload_hash TEXT NOT NULL CHECK (length(payload_hash) = 64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS marketing_deliveries_due_idx
    ON erpx_holding.marketing_deliveries
       (data_profile_key, state, next_attempt_at, expires_at);
CREATE INDEX IF NOT EXISTS marketing_deliveries_profile_idx
    ON erpx_holding.marketing_deliveries (data_profile_key, profile_id, created_at DESC);

ALTER TABLE erpx_holding.marketing_deliveries ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_deliveries FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_delivery_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_delivery_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_deliveries ON erpx_holding.marketing_deliveries;
CREATE POLICY holding_scope_marketing_deliveries ON erpx_holding.marketing_deliveries
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_marketing_delivery_idempotency
    ON erpx_holding.marketing_delivery_idempotency;
CREATE POLICY holding_scope_marketing_delivery_idempotency
    ON erpx_holding.marketing_delivery_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Authenticated provider callbacks are durable holding-write inbox records.
-- Raw signatures and provider payloads are never stored; only verified typed
-- metadata and the exact-body digest cross this boundary.
CREATE TABLE IF NOT EXISTS erpx_holding.marketing_provider_callback_inbox (
    inbox_key TEXT PRIMARY KEY CHECK (btrim(inbox_key) <> ''),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    provider_account_ref TEXT NOT NULL CHECK (btrim(provider_account_ref) <> ''),
    provider_event_id TEXT NOT NULL CHECK (btrim(provider_event_id) <> ''),
    delivery_id UUID NOT NULL,
    provider_message_ref TEXT NOT NULL CHECK (btrim(provider_message_ref) <> ''),
    provider_event_at TIMESTAMPTZ NOT NULL,
    received_at TIMESTAMPTZ NOT NULL,
    correlation_id UUID NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('delivered', 'failed', 'bounced')),
    order_class TEXT NOT NULL CHECK (order_class IN (
        'new', 'in_order', 'out_of_order', 'duplicate', 'conflicting_duplicate'
    )),
    event_fingerprint TEXT NOT NULL CHECK (length(event_fingerprint) = 64),
    raw_body_sha256 TEXT NOT NULL CHECK (length(raw_body_sha256) = 64),
    authentication_scheme TEXT NOT NULL CHECK (btrim(authentication_scheme) <> ''),
    verifier_key_ref TEXT NOT NULL CHECK (btrim(verifier_key_ref) <> ''),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (data_profile_key, provider_account_ref, provider_event_id)
);

CREATE INDEX IF NOT EXISTS marketing_provider_callback_delivery_idx
    ON erpx_holding.marketing_provider_callback_inbox
       (data_profile_key, delivery_id, provider_event_at DESC, provider_event_id DESC);

ALTER TABLE erpx_holding.marketing_provider_callback_inbox
    ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_provider_callback_inbox
    FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_provider_callback_inbox
    ON erpx_holding.marketing_provider_callback_inbox;
CREATE POLICY holding_scope_marketing_provider_callback_inbox
    ON erpx_holding.marketing_provider_callback_inbox
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Authoritative bank account, statement import, transaction, and
-- reconciliation facts for one routed holding. Provider credentials and
-- payment instruments are intentionally not represented.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.finance_bank_accounts (
    account_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    scope JSONB NOT NULL,
    currency CHAR(3) NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    account_reference TEXT NOT NULL CHECK (btrim(account_reference) <> ''),
    display_name TEXT NOT NULL CHECK (btrim(display_name) <> ''),
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (data_profile_key, account_reference)
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_bank_statement_imports (
    import_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    bank_account_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    scope JSONB NOT NULL,
    statement_reference TEXT NOT NULL CHECK (btrim(statement_reference) <> ''),
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL CHECK (ends_on >= starts_on),
    opening_balance_minor BIGINT NOT NULL,
    closing_balance_minor BIGINT NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    content_digest TEXT NOT NULL CHECK (length(content_digest) = 64),
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (data_profile_key, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_bank_transactions (
    transaction_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    import_id UUID NOT NULL,
    bank_account_id UUID NOT NULL,
    external_transaction_id TEXT NOT NULL CHECK (btrim(external_transaction_id) <> ''),
    posted_on DATE NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor <> 0),
    currency CHAR(3) NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    description TEXT NOT NULL,
    identity_key TEXT NOT NULL CHECK (btrim(identity_key) <> ''),
    content_digest TEXT NOT NULL CHECK (length(content_digest) = 64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (data_profile_key, identity_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_bank_reconciliations (
    reconciliation_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    bank_account_id UUID NOT NULL,
    statement_import_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    scope JSONB NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('draft', 'inprogress', 'exception', 'reconciled', 'closed')),
    version BIGINT NOT NULL CHECK (version > 0),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_banking_command_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    operation TEXT NOT NULL CHECK (btrim(operation) <> ''),
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS finance_bank_accounts_scope_idx
    ON erpx_holding.finance_bank_accounts (data_profile_key, company_id, account_id);
CREATE INDEX IF NOT EXISTS finance_bank_imports_scope_idx
    ON erpx_holding.finance_bank_statement_imports
        (data_profile_key, company_id, bank_account_id, ends_on DESC);
CREATE INDEX IF NOT EXISTS finance_bank_transactions_scope_idx
    ON erpx_holding.finance_bank_transactions
        (data_profile_key, bank_account_id, posted_on DESC, transaction_id);
CREATE INDEX IF NOT EXISTS finance_bank_reconciliations_scope_idx
    ON erpx_holding.finance_bank_reconciliations
        (data_profile_key, company_id, status, updated_at DESC);

ALTER TABLE erpx_holding.finance_bank_accounts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_bank_accounts FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_bank_statement_imports ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_bank_statement_imports FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_bank_transactions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_bank_transactions FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_bank_reconciliations ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_bank_reconciliations FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_banking_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_banking_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_bank_accounts
    ON erpx_holding.finance_bank_accounts;
CREATE POLICY holding_scope_finance_bank_accounts
    ON erpx_holding.finance_bank_accounts
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_finance_bank_imports
    ON erpx_holding.finance_bank_statement_imports;
CREATE POLICY holding_scope_finance_bank_imports
    ON erpx_holding.finance_bank_statement_imports
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_finance_bank_transactions
    ON erpx_holding.finance_bank_transactions;
CREATE POLICY holding_scope_finance_bank_transactions
    ON erpx_holding.finance_bank_transactions
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_finance_bank_reconciliations
    ON erpx_holding.finance_bank_reconciliations;
CREATE POLICY holding_scope_finance_bank_reconciliations
    ON erpx_holding.finance_bank_reconciliations
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_finance_banking_idempotency
    ON erpx_holding.finance_banking_command_idempotency;
CREATE POLICY holding_scope_finance_banking_idempotency
    ON erpx_holding.finance_banking_command_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Durable bank-to-GL matching proposals and decisions.  These rows are
-- holding-local workflow facts: posted journals remain immutable Finance
-- source records and are referenced by ID only.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.finance_bank_match_proposals (
    proposal_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    bank_account_id UUID NOT NULL,
    transaction_id UUID NOT NULL,
    scope JSONB NOT NULL,
    resolution JSONB NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (data_profile_key, proposal_id),
    UNIQUE (data_profile_key, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_bank_match_decisions (
    decision_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    proposal_id UUID NOT NULL,
    proposal_idempotency_key TEXT NOT NULL CHECK (btrim(proposal_idempotency_key) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    bank_account_id UUID NOT NULL,
    transaction_id UUID NOT NULL,
    scope JSONB NOT NULL,
    resolution JSONB NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (data_profile_key, decision_id),
    UNIQUE (data_profile_key, idempotency_key),
    UNIQUE (data_profile_key, proposal_id),
    FOREIGN KEY (data_profile_key, proposal_id)
        REFERENCES erpx_holding.finance_bank_match_proposals (data_profile_key, proposal_id)
);

CREATE INDEX IF NOT EXISTS finance_bank_match_proposals_scope_idx
    ON erpx_holding.finance_bank_match_proposals
        (data_profile_key, company_id, bank_account_id, transaction_id, created_at DESC);
CREATE INDEX IF NOT EXISTS finance_bank_match_decisions_scope_idx
    ON erpx_holding.finance_bank_match_decisions
        (data_profile_key, company_id, bank_account_id, transaction_id, created_at DESC);
CREATE INDEX IF NOT EXISTS finance_bank_match_decisions_proposal_idx
    ON erpx_holding.finance_bank_match_decisions
        (data_profile_key, proposal_id, created_at DESC);

ALTER TABLE erpx_holding.finance_bank_match_proposals ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_bank_match_proposals FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_bank_match_decisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_bank_match_decisions FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_bank_match_proposals
    ON erpx_holding.finance_bank_match_proposals;
CREATE POLICY holding_scope_finance_bank_match_proposals
    ON erpx_holding.finance_bank_match_proposals
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_finance_bank_match_decisions
    ON erpx_holding.finance_bank_match_decisions;
CREATE POLICY holding_scope_finance_bank_match_decisions
    ON erpx_holding.finance_bank_match_decisions
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Matching must compare against immutable persisted journal facts, not only
-- caller-supplied candidate values. Existing rows without these facts are
-- intentionally rejected by the matching adapter until backfilled by a
-- reviewed migration job.
ALTER TABLE erpx_holding.finance_posted_journals
    ADD COLUMN IF NOT EXISTS currency CHAR(3),
    ADD COLUMN IF NOT EXISTS total_minor BIGINT;
ALTER TABLE erpx_holding.finance_posted_journals
    DROP CONSTRAINT IF EXISTS finance_posted_journals_currency_check,
    ADD CONSTRAINT finance_posted_journals_currency_check
        CHECK (currency IS NULL OR currency ~ '^[A-Z]{3}$'),
    DROP CONSTRAINT IF EXISTS finance_posted_journals_total_check,
    ADD CONSTRAINT finance_posted_journals_total_check
        CHECK (total_minor IS NULL OR total_minor > 0);

CREATE OR REPLACE FUNCTION erpx_holding.reject_posted_journal_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        IF NEW.currency IS NULL OR NEW.total_minor IS NULL THEN
            RAISE EXCEPTION 'posted Finance journal match facts are required';
        END IF;
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'posted Finance journals are immutable';
END;
$$;

DROP TRIGGER IF EXISTS finance_posted_journals_immutable
    ON erpx_holding.finance_posted_journals;
CREATE TRIGGER finance_posted_journals_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON erpx_holding.finance_posted_journals
    FOR EACH ROW
    EXECUTE FUNCTION erpx_holding.reject_posted_journal_mutation();


-- Backfill match facts for legacy posted journals only when historical
-- payload evidence is complete. The chart's current currency is not a
-- historical fact and must never be used to rewrite an old journal.
--
-- This is a one-time holding-write migration. The migration role owns the
-- holding table, so it temporarily removes FORCE RLS for the bounded upgrade
-- transaction while the runtime policy remains enabled and is restored below.
-- Runtime repositories continue to require erpx.data_profile_key through RLS.
-- The exception handler also restores the named trigger and FORCE RLS when a
-- migration runner executes this file outside an enclosing transaction.
DO $migration$
BEGIN
    ALTER TABLE erpx_holding.finance_posted_journals
        NO FORCE ROW LEVEL SECURITY;
    ALTER TABLE erpx_holding.finance_posted_journals
        DISABLE TRIGGER finance_posted_journals_immutable;

    WITH source_journals AS (
        SELECT
            journal_id,
            payload #>> '{entry,currency}' AS source_currency,
            CASE
                WHEN jsonb_typeof(payload #> '{entry,lines}') = 'array'
                    THEN payload #> '{entry,lines}'
                ELSE '[]'::jsonb
            END AS lines
        FROM erpx_holding.finance_posted_journals
        WHERE currency IS NULL OR total_minor IS NULL
    ), line_values AS (
        SELECT
            source.journal_id,
            source.source_currency,
            line.value,
            CASE
                WHEN line.value IS NULL OR NOT (line.value ? 'debit_minor')
                    THEN 0::NUMERIC
                WHEN line.value->>'debit_minor' ~ '^[0-9]{1,19}$'
                    THEN (line.value->>'debit_minor')::NUMERIC
                ELSE NULL::NUMERIC
            END AS debit_minor,
            CASE
                WHEN line.value IS NULL OR NOT (line.value ? 'credit_minor')
                    THEN 0::NUMERIC
                WHEN line.value->>'credit_minor' ~ '^[0-9]{1,19}$'
                    THEN (line.value->>'credit_minor')::NUMERIC
                ELSE NULL::NUMERIC
            END AS credit_minor
        FROM source_journals AS source
        LEFT JOIN LATERAL jsonb_array_elements(source.lines) AS line(value)
            ON TRUE
    ), derived_facts AS (
        SELECT
            journal_id,
            MAX(source_currency) AS source_currency,
            COUNT(value) AS line_count,
            COUNT(*) FILTER (
                WHERE debit_minor IS NOT NULL
                  AND credit_minor IS NOT NULL
                  AND (
                      (debit_minor > 0 AND credit_minor = 0)
                      OR (debit_minor = 0 AND credit_minor > 0)
                  )
            ) AS valid_line_count,
            SUM(debit_minor) AS total_numeric,
            SUM(credit_minor) AS credit_numeric
        FROM line_values
        GROUP BY journal_id
    )
    UPDATE erpx_holding.finance_posted_journals AS journal
    SET currency = CASE
            WHEN journal.currency IS NULL
                 AND facts.source_currency ~ '^[A-Z]{3}$'
                THEN facts.source_currency
            ELSE journal.currency
        END,
        total_minor = CASE
            WHEN journal.total_minor IS NULL
                 AND facts.line_count > 0
                 AND facts.line_count = facts.valid_line_count
                 AND facts.total_numeric = facts.credit_numeric
                 AND facts.total_numeric BETWEEN 1 AND 9223372036854775807
                THEN facts.total_numeric::BIGINT
            ELSE journal.total_minor
        END
    FROM derived_facts AS facts
    WHERE journal.journal_id = facts.journal_id
      AND (
          (journal.currency IS NULL AND facts.source_currency ~ '^[A-Z]{3}$')
          OR (
              journal.total_minor IS NULL
              AND facts.line_count > 0
              AND facts.line_count = facts.valid_line_count
              AND facts.total_numeric = facts.credit_numeric
              AND facts.total_numeric BETWEEN 1 AND 9223372036854775807
          )
      );

    ALTER TABLE erpx_holding.finance_posted_journals
        ENABLE TRIGGER finance_posted_journals_immutable;
    ALTER TABLE erpx_holding.finance_posted_journals
        FORCE ROW LEVEL SECURITY;
EXCEPTION WHEN OTHERS THEN
    BEGIN
        ALTER TABLE erpx_holding.finance_posted_journals
            ENABLE TRIGGER finance_posted_journals_immutable;
    EXCEPTION WHEN OTHERS THEN
        NULL;
    END;
    BEGIN
        ALTER TABLE erpx_holding.finance_posted_journals
            FORCE ROW LEVEL SECURITY;
    EXCEPTION WHEN OTHERS THEN
        NULL;
    END;
    RAISE;
END;
$migration$;


-- Company-scoped Finance budgets and immutable commitment facts. Actual
-- journal amounts remain owned by finance_posted_journals; this context only
-- owns the approved plan and commitment references used for control checks.
CREATE TABLE IF NOT EXISTS erpx_holding.finance_budgets (
    budget_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    fiscal_year SMALLINT NOT NULL CHECK (fiscal_year BETWEEN 1900 AND 2200),
    currency CHAR(3) NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    status TEXT NOT NULL CHECK (status IN ('draft', 'submitted', 'approved', 'locked', 'closed')),
    version BIGINT NOT NULL CHECK (version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    approved_by TEXT,
    approval_limit_minor BIGINT CHECK (approval_limit_minor >= 0),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (data_profile_key, idempotency_key),
    UNIQUE (data_profile_key, budget_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_budget_commitments (
    commitment_id UUID PRIMARY KEY,
    budget_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    company_id UUID NOT NULL,
    account_id UUID NOT NULL,
    period SMALLINT NOT NULL CHECK (period BETWEEN 1 AND 12),
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    actual_minor BIGINT NOT NULL CHECK (actual_minor >= 0),
    decision_budget_minor BIGINT NOT NULL CHECK (decision_budget_minor >= 0),
    decision_new_commitment_minor BIGINT NOT NULL CHECK (decision_new_commitment_minor >= 0),
    decision_remaining_minor BIGINT NOT NULL CHECK (decision_remaining_minor >= 0),
    source_ref TEXT NOT NULL CHECK (btrim(source_ref) <> ''),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (data_profile_key, source_ref),
    UNIQUE (data_profile_key, idempotency_key),
    FOREIGN KEY (data_profile_key, budget_id)
        REFERENCES erpx_holding.finance_budgets (data_profile_key, budget_id)
);

CREATE INDEX IF NOT EXISTS finance_budgets_company_idx
    ON erpx_holding.finance_budgets (data_profile_key, company_id, fiscal_year, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS finance_budget_commitments_control_idx
    ON erpx_holding.finance_budget_commitments
        (data_profile_key, budget_id, account_id, period, created_at, commitment_id);

ALTER TABLE erpx_holding.finance_budgets ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_budgets FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_budget_commitments ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_budget_commitments FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_budgets ON erpx_holding.finance_budgets;
CREATE POLICY holding_scope_finance_budgets ON erpx_holding.finance_budgets
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_finance_budget_commitments
    ON erpx_holding.finance_budget_commitments;
CREATE POLICY holding_scope_finance_budget_commitments
    ON erpx_holding.finance_budget_commitments
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

CREATE OR REPLACE FUNCTION erpx_holding.reject_finance_budget_commitment_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'Finance budget commitments are immutable';
END;
$$;

DROP TRIGGER IF EXISTS finance_budget_commitments_immutable
    ON erpx_holding.finance_budget_commitments;
CREATE TRIGGER finance_budget_commitments_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.finance_budget_commitments
    FOR EACH ROW
    EXECUTE FUNCTION erpx_holding.reject_finance_budget_commitment_mutation();


-- Company-scoped Finance period-close reviews. The domain aggregate uses the
-- accounting period as its identity; one holding can therefore have one
-- review per period. Journal facts remain owned by Finance posting.
CREATE TABLE IF NOT EXISTS erpx_holding.finance_close_reviews (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    period_id UUID NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('open', 'review', 'closed')),
    version BIGINT NOT NULL CHECK (version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    opened_by TEXT NOT NULL CHECK (btrim(opened_by) <> ''),
    reviewed_by TEXT,
    closed_by TEXT,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, period_id),
    UNIQUE (data_profile_key, idempotency_key)
);

CREATE INDEX IF NOT EXISTS finance_close_reviews_company_idx
    ON erpx_holding.finance_close_reviews
        (data_profile_key, holding_id, company_id, status, updated_at DESC);

ALTER TABLE erpx_holding.finance_close_reviews ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_close_reviews FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_close_reviews
    ON erpx_holding.finance_close_reviews;
CREATE POLICY holding_scope_finance_close_reviews
    ON erpx_holding.finance_close_reviews
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

CREATE OR REPLACE FUNCTION erpx_holding.reject_closed_finance_close_review_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status = 'closed' THEN
        RAISE EXCEPTION 'closed Finance close reviews are immutable';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS finance_close_reviews_closed_immutable
    ON erpx_holding.finance_close_reviews;
CREATE TRIGGER finance_close_reviews_closed_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.finance_close_reviews
    FOR EACH ROW
    EXECUTE FUNCTION erpx_holding.reject_closed_finance_close_review_mutation();


-- Authoritative close-control results for one routed holding.  Close review
-- input may describe the controls that were observed, but it cannot create a
-- resolved result: every submitted flag must match one of these rows.
CREATE TABLE IF NOT EXISTS erpx_holding.finance_close_control_evidence (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    period_id UUID NOT NULL,
    control_code TEXT NOT NULL CHECK (btrim(control_code) <> ''),
    source_type TEXT NOT NULL CHECK (btrim(source_type) <> ''),
    source_aggregate_id UUID NOT NULL,
    evidence_ref TEXT NOT NULL CHECK (btrim(evidence_ref) <> ''),
    resolved BOOLEAN NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, company_id, period_id, control_code)
);

CREATE INDEX IF NOT EXISTS finance_close_control_evidence_scope_idx
    ON erpx_holding.finance_close_control_evidence
        (data_profile_key, holding_id, company_id, period_id, control_code);

ALTER TABLE erpx_holding.finance_close_control_evidence ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_close_control_evidence FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_close_control_evidence
    ON erpx_holding.finance_close_control_evidence;
CREATE POLICY holding_scope_finance_close_control_evidence
    ON erpx_holding.finance_close_control_evidence
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

CREATE OR REPLACE FUNCTION erpx_holding.reject_closed_finance_close_control_evidence_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF EXISTS (
            SELECT 1
            FROM erpx_holding.finance_close_reviews review
            WHERE review.data_profile_key = OLD.data_profile_key
              AND review.company_id = OLD.company_id
              AND review.period_id = OLD.period_id
              AND review.status = 'closed'
        ) THEN
            RAISE EXCEPTION 'close control evidence is immutable after period close';
        END IF;
        RETURN OLD;
    END IF;
    IF EXISTS (
        SELECT 1
        FROM erpx_holding.finance_close_reviews review
        WHERE (
            (review.data_profile_key = OLD.data_profile_key
             AND review.company_id = OLD.company_id
             AND review.period_id = OLD.period_id)
            OR (review.data_profile_key = NEW.data_profile_key
                AND review.company_id = NEW.company_id
                AND review.period_id = NEW.period_id)
        )
          AND review.status = 'closed'
    ) THEN
        RAISE EXCEPTION 'close control evidence is immutable after period close';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS finance_close_control_evidence_closed_immutable
    ON erpx_holding.finance_close_control_evidence;
CREATE TRIGGER finance_close_control_evidence_closed_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.finance_close_control_evidence
    FOR EACH ROW
    EXECUTE FUNCTION erpx_holding.reject_closed_finance_close_control_evidence_mutation();

CREATE OR REPLACE FUNCTION erpx_holding.reject_closed_finance_close_control_evidence_insert()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM erpx_holding.finance_close_reviews review
        WHERE review.data_profile_key = NEW.data_profile_key
          AND review.company_id = NEW.company_id
          AND review.period_id = NEW.period_id
          AND review.status = 'closed'
    ) THEN
        RAISE EXCEPTION 'close control evidence is immutable after period close';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS finance_close_control_evidence_closed_insert
    ON erpx_holding.finance_close_control_evidence;
CREATE TRIGGER finance_close_control_evidence_closed_insert
    BEFORE INSERT ON erpx_holding.finance_close_control_evidence
    FOR EACH ROW
    EXECUTE FUNCTION erpx_holding.reject_closed_finance_close_control_evidence_insert();


-- Marketing-to-CRM persistence boundary.
--
-- Marketing owns acquisition and qualification.  These tables only accept
-- the immutable handoff references and the CRM-owned opportunity lifecycle.
-- The holding write route plus the transaction-local erpx.data_profile_key setting
-- are the isolation boundary; the same rows are never used as a read model.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.crm_idempotency_bindings (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> ''),
    aggregate_id UUID NOT NULL,
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, idempotency_key),
    UNIQUE (data_profile_key, aggregate_type, aggregate_id, idempotency_key),
    UNIQUE (data_profile_key, aggregate_type, reference, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.crm_accepted_lead_handoffs (
    handoff_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
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
    PRIMARY KEY (data_profile_key, handoff_id),
    UNIQUE (data_profile_key, reference),
    UNIQUE (data_profile_key, handoff_id, reference),
    UNIQUE (data_profile_key, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.crm_opportunities (
    opportunity_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
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
    PRIMARY KEY (data_profile_key, opportunity_id),
    UNIQUE (data_profile_key, opportunity_id, reference),
    UNIQUE (data_profile_key, reference),
    UNIQUE (data_profile_key, idempotency_key),
    FOREIGN KEY (data_profile_key, accepted_lead_handoff_id, accepted_lead_reference)
        REFERENCES erpx_holding.crm_accepted_lead_handoffs
            (data_profile_key, handoff_id, reference)
);

CREATE TABLE IF NOT EXISTS erpx_holding.crm_opportunity_attribution_inputs (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    opportunity_id UUID NOT NULL,
    touch_id TEXT NOT NULL CHECK (btrim(touch_id) <> ''),
    source_id TEXT NOT NULL CHECK (btrim(source_id) <> ''),
    campaign_id TEXT,
    scope JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    cost_minor BIGINT NOT NULL CHECK (cost_minor >= 0),
    weight_bps INTEGER CHECK (weight_bps BETWEEN 0 AND 10000),
    PRIMARY KEY (data_profile_key, opportunity_id, touch_id),
    FOREIGN KEY (data_profile_key, opportunity_id)
        REFERENCES erpx_holding.crm_opportunities (data_profile_key, opportunity_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.crm_attribution_records (
    record_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
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
    PRIMARY KEY (data_profile_key, record_id),
    UNIQUE (data_profile_key, opportunity_id, model),
    UNIQUE (data_profile_key, idempotency_key),
    FOREIGN KEY (data_profile_key, opportunity_id)
        REFERENCES erpx_holding.crm_opportunities (data_profile_key, opportunity_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.crm_attribution_entries (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    record_id UUID NOT NULL,
    touch_id TEXT NOT NULL CHECK (btrim(touch_id) <> ''),
    source_id TEXT NOT NULL CHECK (btrim(source_id) <> ''),
    campaign_id TEXT,
    revenue_minor BIGINT NOT NULL CHECK (revenue_minor >= 0),
    cost_minor BIGINT NOT NULL CHECK (cost_minor >= 0),
    PRIMARY KEY (data_profile_key, record_id, touch_id),
    FOREIGN KEY (data_profile_key, record_id)
        REFERENCES erpx_holding.crm_attribution_records (data_profile_key, record_id)
);

CREATE INDEX IF NOT EXISTS crm_handoffs_scope_idx
    ON erpx_holding.crm_accepted_lead_handoffs
        (data_profile_key, holding_id, company_id, branch_id, accepted_at DESC);
CREATE INDEX IF NOT EXISTS crm_opportunities_scope_idx
    ON erpx_holding.crm_opportunities
        (data_profile_key, holding_id, company_id, branch_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS crm_attribution_opportunity_idx
    ON erpx_holding.crm_attribution_records
        (data_profile_key, opportunity_id, created_at DESC);

ALTER TABLE erpx_holding.crm_idempotency_bindings ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_idempotency_bindings FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_accepted_lead_handoffs ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_accepted_lead_handoffs FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_opportunities ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_opportunities FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_opportunity_attribution_inputs ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_opportunity_attribution_inputs FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_attribution_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_attribution_records FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_attribution_entries ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.crm_attribution_entries FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_crm_idempotency_bindings
    ON erpx_holding.crm_idempotency_bindings;
CREATE POLICY holding_scope_crm_idempotency_bindings
    ON erpx_holding.crm_idempotency_bindings
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_crm_accepted_lead_handoffs
    ON erpx_holding.crm_accepted_lead_handoffs;
CREATE POLICY holding_scope_crm_accepted_lead_handoffs
    ON erpx_holding.crm_accepted_lead_handoffs
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_crm_opportunities ON erpx_holding.crm_opportunities;
CREATE POLICY holding_scope_crm_opportunities
    ON erpx_holding.crm_opportunities
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_crm_opportunity_attribution_inputs
    ON erpx_holding.crm_opportunity_attribution_inputs;
CREATE POLICY holding_scope_crm_opportunity_attribution_inputs
    ON erpx_holding.crm_opportunity_attribution_inputs
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_crm_attribution_records
    ON erpx_holding.crm_attribution_records;
CREATE POLICY holding_scope_crm_attribution_records
    ON erpx_holding.crm_attribution_records
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_crm_attribution_entries
    ON erpx_holding.crm_attribution_entries;
CREATE POLICY holding_scope_crm_attribution_entries
    ON erpx_holding.crm_attribution_entries
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

CREATE OR REPLACE FUNCTION erpx_holding.reject_crm_immutable_row_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION '% rows are immutable', TG_TABLE_NAME;
END;
$$;

DROP TRIGGER IF EXISTS crm_handoffs_immutable
    ON erpx_holding.crm_accepted_lead_handoffs;
CREATE TRIGGER crm_handoffs_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.crm_accepted_lead_handoffs
    FOR EACH ROW
    EXECUTE FUNCTION erpx_holding.reject_crm_immutable_row_mutation();

DROP TRIGGER IF EXISTS crm_attribution_inputs_immutable
    ON erpx_holding.crm_opportunity_attribution_inputs;
CREATE TRIGGER crm_attribution_inputs_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.crm_opportunity_attribution_inputs
    FOR EACH ROW
    EXECUTE FUNCTION erpx_holding.reject_crm_immutable_row_mutation();

DROP TRIGGER IF EXISTS crm_attribution_records_immutable
    ON erpx_holding.crm_attribution_records;
CREATE TRIGGER crm_attribution_records_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.crm_attribution_records
    FOR EACH ROW
    EXECUTE FUNCTION erpx_holding.reject_crm_immutable_row_mutation();

DROP TRIGGER IF EXISTS crm_attribution_entries_immutable
    ON erpx_holding.crm_attribution_entries;
CREATE TRIGGER crm_attribution_entries_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.crm_attribution_entries
    FOR EACH ROW
    EXECUTE FUNCTION erpx_holding.reject_crm_immutable_row_mutation();

DROP TRIGGER IF EXISTS crm_idempotency_bindings_immutable
    ON erpx_holding.crm_idempotency_bindings;
CREATE TRIGGER crm_idempotency_bindings_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.crm_idempotency_bindings
    FOR EACH ROW
    EXECUTE FUNCTION erpx_holding.reject_crm_immutable_row_mutation();

CREATE OR REPLACE FUNCTION erpx_holding.reject_crm_opportunity_binding_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.data_profile_key IS DISTINCT FROM NEW.data_profile_key
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
    ON erpx_holding.crm_opportunities;
CREATE TRIGGER crm_opportunities_binding_immutable
    BEFORE UPDATE ON erpx_holding.crm_opportunities
    FOR EACH ROW
    EXECUTE FUNCTION erpx_holding.reject_crm_opportunity_binding_mutation();


-- Ordered source publication for close-control evidence. Existing rows are
-- the first observed source version; later source events must advance it.
ALTER TABLE erpx_holding.finance_close_control_evidence
    ADD COLUMN IF NOT EXISTS source_version BIGINT NOT NULL DEFAULT 1,
    ADD CONSTRAINT finance_close_control_evidence_source_version_check
        CHECK (source_version > 0);


-- Typed holding-write source state for HR, leave, expense, payroll, and
-- expired-role workflow reassignment.  Read projections are intentionally not
-- created here; these tables belong only to the holding write database.
CREATE TABLE IF NOT EXISTS erpx_holding.hr_command_idempotency (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    request_hash TEXT NOT NULL CHECK (btrim(request_hash) <> ''),
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> ''),
    aggregate_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.hr_employee_records (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    employee_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    employee_ref TEXT NOT NULL CHECK (btrim(employee_ref) <> ''),
    display_name TEXT NOT NULL CHECK (btrim(display_name) <> ''),
    status TEXT NOT NULL CHECK (status IN ('draft', 'active', 'on_leave', 'suspended', 'terminated')),
    hired_on DATE NOT NULL,
    terminated_on DATE,
    version BIGINT NOT NULL CHECK (version > 0),
    command_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, employee_id),
    UNIQUE (data_profile_key, employee_ref)
);

CREATE TABLE IF NOT EXISTS erpx_holding.hr_leave_requests (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    leave_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    employee_id UUID NOT NULL,
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    status TEXT NOT NULL CHECK (status IN ('draft', 'submitted', 'approved', 'rejected', 'cancelled', 'taken')),
    requested_by TEXT NOT NULL CHECK (btrim(requested_by) <> ''),
    approved_by TEXT,
    version BIGINT NOT NULL CHECK (version > 0),
    command_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, leave_id),
    CHECK (ends_on >= starts_on),
    UNIQUE (data_profile_key, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.hr_expense_claims (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    expense_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    employee_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (btrim(currency) <> '' AND char_length(currency) <= 3),
    amount_minor BIGINT NOT NULL CHECK (amount_minor >= 0),
    status TEXT NOT NULL CHECK (status IN ('draft', 'submitted', 'approved', 'rejected', 'reimbursed')),
    submitted_by TEXT NOT NULL CHECK (btrim(submitted_by) <> ''),
    approved_by TEXT,
    version BIGINT NOT NULL CHECK (version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, expense_id),
    UNIQUE (data_profile_key, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.hr_payroll_runs (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    payroll_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('draft', 'review', 'approved', 'posted', 'cancelled')),
    prepared_by TEXT NOT NULL CHECK (btrim(prepared_by) <> ''),
    reviewed_by TEXT,
    approved_by TEXT,
    total_gross_minor BIGINT NOT NULL CHECK (total_gross_minor >= 0),
    total_deductions_minor BIGINT NOT NULL CHECK (total_deductions_minor >= 0),
    total_net_minor BIGINT NOT NULL CHECK (total_net_minor >= 0),
    version BIGINT NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    PRIMARY KEY (data_profile_key, payroll_id),
    CHECK (ends_on >= starts_on),
    CHECK (total_gross_minor - total_deductions_minor = total_net_minor),
    UNIQUE (data_profile_key, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.hr_payroll_lines (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    payroll_id UUID NOT NULL,
    line_no INTEGER NOT NULL CHECK (line_no >= 0),
    employee_id UUID NOT NULL,
    gross_minor BIGINT NOT NULL CHECK (gross_minor >= 0),
    deductions_minor BIGINT NOT NULL CHECK (deductions_minor >= 0 AND deductions_minor <= gross_minor),
    net_minor BIGINT NOT NULL CHECK (net_minor = gross_minor - deductions_minor),
    PRIMARY KEY (data_profile_key, payroll_id, line_no)
);

CREATE TABLE IF NOT EXISTS erpx_holding.hr_expired_role_reassignments (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    workflow_ref TEXT NOT NULL CHECK (btrim(workflow_ref) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    expired_principal_ref TEXT NOT NULL CHECK (btrim(expired_principal_ref) <> ''),
    replacement_principal_ref TEXT NOT NULL CHECK (btrim(replacement_principal_ref) <> ''),
    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    reassigned_at TIMESTAMPTZ NOT NULL,
    command_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    version BIGINT NOT NULL CHECK (version > 0),
    PRIMARY KEY (data_profile_key, workflow_ref),
    UNIQUE (data_profile_key, idempotency_key),
    CHECK (expired_principal_ref <> replacement_principal_ref)
);

CREATE INDEX IF NOT EXISTS hr_employee_scope_idx
    ON erpx_holding.hr_employee_records (data_profile_key, holding_id, company_id, branch_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS hr_leave_scope_idx
    ON erpx_holding.hr_leave_requests (data_profile_key, holding_id, company_id, branch_id, status, starts_on);
CREATE INDEX IF NOT EXISTS hr_expense_scope_idx
    ON erpx_holding.hr_expense_claims (data_profile_key, holding_id, company_id, branch_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS hr_payroll_scope_idx
    ON erpx_holding.hr_payroll_runs (data_profile_key, holding_id, company_id, branch_id, status, starts_on);

ALTER TABLE erpx_holding.hr_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_command_idempotency FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_employee_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_employee_records FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_leave_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_leave_requests FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_expense_claims ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_expense_claims FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_payroll_runs ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_payroll_runs FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_payroll_lines ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_payroll_lines FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_expired_role_reassignments ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.hr_expired_role_reassignments FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_hr_command_idempotency ON erpx_holding.hr_command_idempotency;
CREATE POLICY holding_scope_hr_command_idempotency ON erpx_holding.hr_command_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_hr_employee_records ON erpx_holding.hr_employee_records;
CREATE POLICY holding_scope_hr_employee_records ON erpx_holding.hr_employee_records
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_hr_leave_requests ON erpx_holding.hr_leave_requests;
CREATE POLICY holding_scope_hr_leave_requests ON erpx_holding.hr_leave_requests
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_hr_expense_claims ON erpx_holding.hr_expense_claims;
CREATE POLICY holding_scope_hr_expense_claims ON erpx_holding.hr_expense_claims
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_hr_payroll_runs ON erpx_holding.hr_payroll_runs;
CREATE POLICY holding_scope_hr_payroll_runs ON erpx_holding.hr_payroll_runs
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_hr_payroll_lines ON erpx_holding.hr_payroll_lines;
CREATE POLICY holding_scope_hr_payroll_lines ON erpx_holding.hr_payroll_lines
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_hr_expired_role_reassignments ON erpx_holding.hr_expired_role_reassignments;
CREATE POLICY holding_scope_hr_expired_role_reassignments ON erpx_holding.hr_expired_role_reassignments
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

-- Terminal lifecycle states cannot be bypassed by writing a deserialized
-- aggregate directly.  Non-terminal transitions remain domain-owned.
CREATE OR REPLACE FUNCTION erpx_holding.reject_terminal_hr_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'HR source aggregates cannot be deleted';
    END IF;
    IF (TG_TABLE_NAME = 'hr_employee_records' AND OLD.status = 'terminated')
        OR (TG_TABLE_NAME = 'hr_leave_requests' AND OLD.status IN ('rejected', 'cancelled', 'taken'))
        OR (TG_TABLE_NAME = 'hr_expense_claims' AND OLD.status IN ('rejected', 'reimbursed'))
        OR (TG_TABLE_NAME = 'hr_payroll_runs' AND OLD.status IN ('posted', 'cancelled')) THEN
        RAISE EXCEPTION 'terminal HR source aggregates are immutable';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS hr_employee_terminal_immutable
    ON erpx_holding.hr_employee_records;
CREATE TRIGGER hr_employee_terminal_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.hr_employee_records
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_terminal_hr_mutation();
DROP TRIGGER IF EXISTS hr_leave_terminal_immutable
    ON erpx_holding.hr_leave_requests;
CREATE TRIGGER hr_leave_terminal_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.hr_leave_requests
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_terminal_hr_mutation();
DROP TRIGGER IF EXISTS hr_expense_terminal_immutable
    ON erpx_holding.hr_expense_claims;
CREATE TRIGGER hr_expense_terminal_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.hr_expense_claims
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_terminal_hr_mutation();
DROP TRIGGER IF EXISTS hr_payroll_terminal_immutable
    ON erpx_holding.hr_payroll_runs;
CREATE TRIGGER hr_payroll_terminal_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.hr_payroll_runs
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_terminal_hr_mutation();

CREATE OR REPLACE FUNCTION erpx_holding.reject_hr_reassignment_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'expired-role reassignment metadata is immutable';
END;
$$;

DROP TRIGGER IF EXISTS hr_expired_role_reassignment_immutable
    ON erpx_holding.hr_expired_role_reassignments;
CREATE TRIGGER hr_expired_role_reassignment_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.hr_expired_role_reassignments
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_hr_reassignment_mutation();


-- Typed POS source state for the holding write database.
--
-- The normalized tables are authoritative.  The existing outbox_events table
-- remains the transport envelope used by projectors; no POS aggregate JSON is
-- stored in these source tables.
CREATE TABLE IF NOT EXISTS erpx_holding.pos_sales (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    sale_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL CHECK (btrim(reference) <> '' AND char_length(reference) <= 128),
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
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    subtotal_minor BIGINT NOT NULL CHECK (subtotal_minor >= 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    total_minor BIGINT NOT NULL CHECK (total_minor = subtotal_minor + tax_minor),
    paid_minor BIGINT NOT NULL CHECK (paid_minor >= 0 AND paid_minor <= total_minor),
    status TEXT NOT NULL CHECK (status IN ('draft', 'open', 'held', 'completed', 'voided', 'refunded')),
    version BIGINT NOT NULL CHECK (version > 0),
    command_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> '' AND char_length(idempotency_key) <= 512),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_profile_key, sale_id),
    UNIQUE (data_profile_key, reference),
    CHECK ((customer_id IS NULL AND customer_scope_kind IS NULL
            AND customer_scope_holding_id IS NULL AND customer_scope_company_id IS NULL
            AND customer_scope_branch_id IS NULL)
        OR (customer_id IS NOT NULL AND customer_scope_kind IN ('holding', 'company', 'branch')
            AND customer_scope_holding_id IS NOT NULL)),
    CHECK ((register_id IS NULL AND register_scope_kind IS NULL
            AND register_scope_holding_id IS NULL AND register_scope_company_id IS NULL
            AND register_scope_branch_id IS NULL)
        OR (register_id IS NOT NULL AND register_scope_kind IN ('holding', 'company', 'branch')
            AND register_scope_holding_id IS NOT NULL))
);

CREATE TABLE IF NOT EXISTS erpx_holding.pos_sale_lines (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    sale_id UUID NOT NULL,
    line_id UUID NOT NULL,
    line_no INTEGER NOT NULL CHECK (line_no >= 0),
    item_reference TEXT NOT NULL CHECK (btrim(item_reference) <> '' AND char_length(item_reference) <= 128),
    description TEXT NOT NULL CHECK (btrim(description) <> '' AND char_length(description) <= 255),
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
    PRIMARY KEY (data_profile_key, sale_id, line_id),
    UNIQUE (data_profile_key, sale_id, line_no),
    CHECK ((product_id IS NULL AND product_scope_kind IS NULL
            AND product_scope_holding_id IS NULL AND product_scope_company_id IS NULL
            AND product_scope_branch_id IS NULL)
        OR (product_id IS NOT NULL AND product_scope_kind IN ('holding', 'company', 'branch')
            AND product_scope_holding_id IS NOT NULL)),
    FOREIGN KEY (data_profile_key, sale_id)
        REFERENCES erpx_holding.pos_sales (data_profile_key, sale_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.pos_sale_payments (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    sale_id UUID NOT NULL,
    payment_id UUID NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    method TEXT NOT NULL CHECK (method IN ('cash', 'card', 'bank_transfer', 'mobile_wallet', 'voucher', 'other')),
    received_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_profile_key, sale_id, payment_id),
    FOREIGN KEY (data_profile_key, sale_id)
        REFERENCES erpx_holding.pos_sales (data_profile_key, sale_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.pos_command_idempotency (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> '' AND char_length(idempotency_key) <= 512),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    command_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type = 'pos_sale'),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, idempotency_key)
);

CREATE INDEX IF NOT EXISTS pos_sales_scope_idx
    ON erpx_holding.pos_sales (data_profile_key, holding_id, company_id, branch_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS pos_sale_lines_sale_idx
    ON erpx_holding.pos_sale_lines (data_profile_key, sale_id, line_no);
CREATE INDEX IF NOT EXISTS pos_sale_payments_sale_idx
    ON erpx_holding.pos_sale_payments (data_profile_key, sale_id, received_at, payment_id);
CREATE INDEX IF NOT EXISTS pos_idempotency_sale_idx
    ON erpx_holding.pos_command_idempotency (data_profile_key, aggregate_id, aggregate_version);

ALTER TABLE erpx_holding.pos_sales ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_sales FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_sale_lines ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_sale_lines FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_sale_payments ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_sale_payments FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.pos_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_pos_sales ON erpx_holding.pos_sales;
CREATE POLICY holding_scope_pos_sales ON erpx_holding.pos_sales
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_pos_sale_lines ON erpx_holding.pos_sale_lines;
CREATE POLICY holding_scope_pos_sale_lines ON erpx_holding.pos_sale_lines
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_pos_sale_payments ON erpx_holding.pos_sale_payments;
CREATE POLICY holding_scope_pos_sale_payments ON erpx_holding.pos_sale_payments
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_pos_command_idempotency ON erpx_holding.pos_command_idempotency;
CREATE POLICY holding_scope_pos_command_idempotency ON erpx_holding.pos_command_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

CREATE OR REPLACE FUNCTION erpx_holding.reject_terminal_pos_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'POS sales cannot be deleted';
    END IF;
    IF OLD.status IN ('voided', 'refunded') THEN
        RAISE EXCEPTION 'terminal POS sales are immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION erpx_holding.reject_terminal_pos_child_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    sale_status TEXT;
    row_data_profile_key TEXT;
    row_sale_id UUID;
BEGIN
    IF TG_OP = 'DELETE' THEN
        row_data_profile_key := OLD.data_profile_key;
        row_sale_id := OLD.sale_id;
    ELSE
        row_data_profile_key := NEW.data_profile_key;
        row_sale_id := NEW.sale_id;
    END IF;
    SELECT status INTO sale_status
      FROM erpx_holding.pos_sales
     WHERE data_profile_key = row_data_profile_key AND sale_id = row_sale_id;
    IF sale_status IN ('voided', 'refunded') THEN
        RAISE EXCEPTION 'terminal POS sale children are immutable';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION erpx_holding.validate_pos_sale_reconciliation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    sale_row RECORD;
    line_subtotal BIGINT;
    line_tax BIGINT;
    line_total BIGINT;
    payment_total BIGINT;
    row_data_profile_key TEXT;
    row_sale_id UUID;
BEGIN
    IF TG_OP = 'DELETE' THEN
        row_data_profile_key := OLD.data_profile_key;
        row_sale_id := OLD.sale_id;
    ELSE
        row_data_profile_key := NEW.data_profile_key;
        row_sale_id := NEW.sale_id;
    END IF;
    SELECT * INTO sale_row
      FROM erpx_holding.pos_sales
     WHERE data_profile_key = row_data_profile_key AND sale_id = row_sale_id;
    IF NOT FOUND THEN
        IF TG_OP = 'DELETE' THEN
            RETURN OLD;
        END IF;
        RETURN NEW;
    END IF;

    SELECT COALESCE(sum(subtotal_minor), 0),
           COALESCE(sum(tax_minor), 0),
           COALESCE(sum(total_minor), 0)
      INTO line_subtotal, line_tax, line_total
      FROM erpx_holding.pos_sale_lines
     WHERE data_profile_key = sale_row.data_profile_key AND sale_id = sale_row.sale_id;
    SELECT COALESCE(sum(amount_minor), 0)
      INTO payment_total
      FROM erpx_holding.pos_sale_payments
     WHERE data_profile_key = sale_row.data_profile_key AND sale_id = sale_row.sale_id;

    IF line_subtotal <> sale_row.subtotal_minor
       OR line_tax <> sale_row.tax_minor
       OR line_total <> sale_row.total_minor
       OR payment_total <> sale_row.paid_minor
       OR sale_row.total_minor <> sale_row.subtotal_minor + sale_row.tax_minor
       OR sale_row.paid_minor > sale_row.total_minor THEN
        RAISE EXCEPTION 'POS sale totals and payments do not reconcile';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS pos_sales_terminal_immutable ON erpx_holding.pos_sales;
CREATE TRIGGER pos_sales_terminal_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.pos_sales
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_terminal_pos_mutation();

DROP TRIGGER IF EXISTS pos_sale_lines_terminal_immutable ON erpx_holding.pos_sale_lines;
CREATE TRIGGER pos_sale_lines_terminal_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON erpx_holding.pos_sale_lines
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_terminal_pos_child_mutation();
DROP TRIGGER IF EXISTS pos_sale_payments_terminal_immutable ON erpx_holding.pos_sale_payments;
CREATE TRIGGER pos_sale_payments_terminal_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON erpx_holding.pos_sale_payments
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_terminal_pos_child_mutation();

DROP TRIGGER IF EXISTS pos_sales_reconciliation ON erpx_holding.pos_sales;
CREATE CONSTRAINT TRIGGER pos_sales_reconciliation
    AFTER INSERT OR UPDATE ON erpx_holding.pos_sales
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.validate_pos_sale_reconciliation();
DROP TRIGGER IF EXISTS pos_sale_lines_reconciliation ON erpx_holding.pos_sale_lines;
CREATE CONSTRAINT TRIGGER pos_sale_lines_reconciliation
    AFTER INSERT OR UPDATE OR DELETE ON erpx_holding.pos_sale_lines
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.validate_pos_sale_reconciliation();
DROP TRIGGER IF EXISTS pos_sale_payments_reconciliation ON erpx_holding.pos_sale_payments;
CREATE CONSTRAINT TRIGGER pos_sale_payments_reconciliation
    AFTER INSERT OR UPDATE OR DELETE ON erpx_holding.pos_sale_payments
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.validate_pos_sale_reconciliation();


-- Typed, holding-scoped legal and document source state.  Raw document
-- content, credentials, and transport/provider state are intentionally not
-- represented in this migration; object_ref is an opaque bounded reference.

CREATE TABLE IF NOT EXISTS erpx_holding.legal_command_idempotency (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 512),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('legal_contract', 'legal_document', 'legal_access_history')),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.legal_contracts (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL CHECK (btrim(reference) <> '' AND octet_length(reference) <= 255),
    title TEXT NOT NULL CHECK (btrim(title) <> '' AND octet_length(title) <= 255),
    object_ref TEXT NOT NULL CHECK (btrim(object_ref) <> '' AND octet_length(object_ref) <= 255),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    retention_until TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('draft', 'review', 'approved', 'active', 'expired', 'terminated', 'archived')),
    created_by TEXT NOT NULL CHECK (btrim(created_by) <> '' AND octet_length(created_by) <= 255),
    approved_by TEXT CHECK (approved_by IS NULL OR (btrim(approved_by) <> '' AND octet_length(approved_by) <= 255)),
    version BIGINT NOT NULL CHECK (version > 0),
    last_command_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 512),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, contract_id),
    UNIQUE (data_profile_key, reference),
    CHECK (approved_by IS NULL OR approved_by <> created_by),
    CHECK ((status IN ('draft', 'review') AND approved_by IS NULL)
        OR (status IN ('approved', 'active', 'expired', 'terminated', 'archived') AND approved_by IS NOT NULL))
);

CREATE TABLE IF NOT EXISTS erpx_holding.legal_contract_parties (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    party_id UUID NOT NULL,
    party_role TEXT NOT NULL CHECK (btrim(party_role) <> '' AND octet_length(party_role) <= 255),
    PRIMARY KEY (data_profile_key, contract_id, party_id),
    FOREIGN KEY (data_profile_key, contract_id)
        REFERENCES erpx_holding.legal_contracts (data_profile_key, contract_id)
        ON DELETE RESTRICT,
    UNIQUE (data_profile_key, contract_id, party_id, party_role)
);

CREATE TABLE IF NOT EXISTS erpx_holding.legal_contract_approvals (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    approved_by TEXT NOT NULL CHECK (btrim(approved_by) <> '' AND octet_length(approved_by) <= 255),
    approved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    contract_version BIGINT NOT NULL CHECK (contract_version > 0),
    PRIMARY KEY (data_profile_key, contract_id),
    FOREIGN KEY (data_profile_key, contract_id)
        REFERENCES erpx_holding.legal_contracts (data_profile_key, contract_id)
        ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS erpx_holding.legal_contract_retention (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    contract_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    retention_until TIMESTAMPTZ NOT NULL,
    retention_state TEXT NOT NULL CHECK (retention_state IN ('active', 'ended')),
    contract_version BIGINT NOT NULL CHECK (contract_version > 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, contract_id),
    FOREIGN KEY (data_profile_key, contract_id)
        REFERENCES erpx_holding.legal_contracts (data_profile_key, contract_id)
        ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS erpx_holding.legal_documents (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    document_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    title TEXT NOT NULL CHECK (btrim(title) <> '' AND octet_length(title) <= 255),
    current_version BIGINT NOT NULL CHECK (current_version >= 0),
    archived BOOLEAN NOT NULL DEFAULT false,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 512),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, document_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.legal_document_versions (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    document_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    object_ref TEXT NOT NULL CHECK (btrim(object_ref) <> '' AND octet_length(object_ref) <= 255),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    created_at TIMESTAMPTZ NOT NULL,
    created_by TEXT NOT NULL CHECK (btrim(created_by) <> '' AND octet_length(created_by) <= 255),
    PRIMARY KEY (data_profile_key, document_id, version),
    FOREIGN KEY (data_profile_key, document_id)
        REFERENCES erpx_holding.legal_documents (data_profile_key, document_id)
        ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS erpx_holding.legal_access_history (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    entry_id UUID NOT NULL,
    document_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    document_version BIGINT NOT NULL CHECK (document_version > 0),
    actor_ref TEXT NOT NULL CHECK (btrim(actor_ref) <> '' AND octet_length(actor_ref) <= 255),
    action TEXT NOT NULL CHECK (btrim(action) <> '' AND octet_length(action) <= 255),
    purpose TEXT NOT NULL CHECK (btrim(purpose) <> '' AND octet_length(purpose) <= 512),
    observed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_profile_key, entry_id),
    FOREIGN KEY (data_profile_key, document_id, document_version)
        REFERENCES erpx_holding.legal_document_versions (data_profile_key, document_id, version)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS legal_contract_scope_idx
    ON erpx_holding.legal_contracts (data_profile_key, holding_id, company_id, branch_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS legal_document_scope_idx
    ON erpx_holding.legal_documents (data_profile_key, holding_id, company_id, branch_id, archived, updated_at DESC);
CREATE INDEX IF NOT EXISTS legal_document_versions_scope_idx
    ON erpx_holding.legal_document_versions (data_profile_key, document_id, version);
CREATE INDEX IF NOT EXISTS legal_access_history_scope_idx
    ON erpx_holding.legal_access_history (data_profile_key, document_id, observed_at DESC, entry_id);
CREATE INDEX IF NOT EXISTS legal_idempotency_aggregate_idx
    ON erpx_holding.legal_command_idempotency (data_profile_key, aggregate_type, aggregate_id, aggregate_version);

ALTER TABLE erpx_holding.legal_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_command_idempotency FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_contracts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_contracts FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_contract_parties ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_contract_parties FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_contract_approvals ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_contract_approvals FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_contract_retention ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_contract_retention FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_documents ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_documents FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_document_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_document_versions FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_access_history ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.legal_access_history FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_legal_command_idempotency ON erpx_holding.legal_command_idempotency;
CREATE POLICY holding_scope_legal_command_idempotency ON erpx_holding.legal_command_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_legal_contracts ON erpx_holding.legal_contracts;
CREATE POLICY holding_scope_legal_contracts ON erpx_holding.legal_contracts
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_legal_contract_parties ON erpx_holding.legal_contract_parties;
CREATE POLICY holding_scope_legal_contract_parties ON erpx_holding.legal_contract_parties
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_legal_contract_approvals ON erpx_holding.legal_contract_approvals;
CREATE POLICY holding_scope_legal_contract_approvals ON erpx_holding.legal_contract_approvals
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_legal_contract_retention ON erpx_holding.legal_contract_retention;
CREATE POLICY holding_scope_legal_contract_retention ON erpx_holding.legal_contract_retention
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_legal_documents ON erpx_holding.legal_documents;
CREATE POLICY holding_scope_legal_documents ON erpx_holding.legal_documents
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_legal_document_versions ON erpx_holding.legal_document_versions;
CREATE POLICY holding_scope_legal_document_versions ON erpx_holding.legal_document_versions
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_legal_access_history ON erpx_holding.legal_access_history;
CREATE POLICY holding_scope_legal_access_history ON erpx_holding.legal_access_history
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

CREATE OR REPLACE FUNCTION erpx_holding.reject_archived_legal_contract_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' OR OLD.status = 'archived' THEN
        RAISE EXCEPTION 'archived legal contracts are immutable';
    END IF;
    IF NEW.approved_by IS NOT NULL AND NEW.approved_by = NEW.created_by THEN
        RAISE EXCEPTION 'legal contract approval requires separation';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS legal_contract_terminal_immutable ON erpx_holding.legal_contracts;
CREATE TRIGGER legal_contract_terminal_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.legal_contracts
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_archived_legal_contract_mutation();

CREATE OR REPLACE FUNCTION erpx_holding.reject_archived_legal_party_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    contract_status TEXT;
BEGIN
    SELECT status INTO contract_status
      FROM erpx_holding.legal_contracts
     WHERE data_profile_key = COALESCE(NEW.data_profile_key, OLD.data_profile_key)
       AND contract_id = COALESCE(NEW.contract_id, OLD.contract_id);
    IF contract_status IS DISTINCT FROM 'draft' THEN
        RAISE EXCEPTION 'legal contract parties are immutable outside draft status';
    END IF;
    RETURN COALESCE(NEW, OLD);
END;
$$;

DROP TRIGGER IF EXISTS legal_contract_parties_terminal_immutable ON erpx_holding.legal_contract_parties;
CREATE TRIGGER legal_contract_parties_terminal_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON erpx_holding.legal_contract_parties
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_archived_legal_party_mutation();

CREATE OR REPLACE FUNCTION erpx_holding.reject_legal_approval_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'legal approvals are immutable';
END;
$$;

DROP TRIGGER IF EXISTS legal_contract_approvals_immutable ON erpx_holding.legal_contract_approvals;
CREATE TRIGGER legal_contract_approvals_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.legal_contract_approvals
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_legal_approval_mutation();

CREATE OR REPLACE FUNCTION erpx_holding.reject_legal_version_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'legal document versions are immutable';
END;
$$;

DROP TRIGGER IF EXISTS legal_document_versions_immutable ON erpx_holding.legal_document_versions;
CREATE TRIGGER legal_document_versions_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.legal_document_versions
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_legal_version_mutation();

CREATE OR REPLACE FUNCTION erpx_holding.reject_legal_access_history_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'legal access history is immutable';
END;
$$;

DROP TRIGGER IF EXISTS legal_access_history_immutable ON erpx_holding.legal_access_history;
CREATE TRIGGER legal_access_history_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.legal_access_history
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_legal_access_history_mutation();


-- Add the frozen shared event-envelope columns to the holding outbox.  Older
-- contexts may still have legacy rows; new source contexts must populate the
-- complete envelope before their events are accepted by their own adapters.
ALTER TABLE erpx_holding.outbox_events
    ADD COLUMN IF NOT EXISTS contract_version INTEGER,
    ADD COLUMN IF NOT EXISTS event_version INTEGER,
    ADD COLUMN IF NOT EXISTS active_scope JSONB,
    ADD COLUMN IF NOT EXISTS actor JSONB,
    ADD COLUMN IF NOT EXISTS correlation_id UUID,
    ADD COLUMN IF NOT EXISTS causation_id UUID,
    ADD COLUMN IF NOT EXISTS idempotency_key TEXT;

ALTER TABLE erpx_holding.outbox_events
    DROP CONSTRAINT IF EXISTS outbox_events_contract_version_check,
    ADD CONSTRAINT outbox_events_contract_version_check
        CHECK (contract_version IS NULL OR contract_version > 0),
    DROP CONSTRAINT IF EXISTS outbox_events_event_version_check,
    ADD CONSTRAINT outbox_events_event_version_check
        CHECK (event_version IS NULL OR event_version > 0),
    DROP CONSTRAINT IF EXISTS outbox_events_idempotency_key_check,
    ADD CONSTRAINT outbox_events_idempotency_key_check
        CHECK (idempotency_key IS NULL OR (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 512));

CREATE INDEX IF NOT EXISTS outbox_events_envelope_holding_idx
    ON erpx_holding.outbox_events (data_profile_key, occurred_at, event_id);


-- Normalized, authoritative Chart of Accounts source state. This is additive
-- to the early finance_charts JSON compatibility table; new integrations use
-- these rows and the complete shared outbox envelope.

CREATE TABLE IF NOT EXISTS erpx_holding.finance_chart_of_accounts (
    data_profile_key TEXT NOT NULL
        CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    chart_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    chart_name TEXT NOT NULL
        CHECK (btrim(chart_name) <> '' AND octet_length(chart_name) <= 255),
    version BIGINT NOT NULL CHECK (version > 0),
    last_command_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL
        CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 512),
    actor_ref TEXT NOT NULL
        CHECK (btrim(actor_ref) <> '' AND octet_length(actor_ref) <= 255),
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, chart_id),
    UNIQUE (data_profile_key, holding_id, company_id),
    UNIQUE (data_profile_key, chart_id, holding_id, company_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_chart_accounts (
    data_profile_key TEXT NOT NULL
        CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    chart_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    account_id UUID NOT NULL,
    account_code TEXT NOT NULL
        CHECK (
            btrim(account_code) <> ''
            AND octet_length(account_code) <= 64
            AND account_code ~ '^[A-Z0-9._-]+$'
        ),
    account_name TEXT NOT NULL
        CHECK (btrim(account_name) <> '' AND octet_length(account_name) <= 255),
    category TEXT NOT NULL
        CHECK (category IN ('asset', 'liability', 'equity', 'revenue', 'expense')),
    normal_balance TEXT NOT NULL
        CHECK (normal_balance IN ('debit', 'credit')),
    parent_account_id UUID,
    status TEXT NOT NULL CHECK (status IN ('active', 'inactive')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, chart_id, account_id),
    UNIQUE (data_profile_key, company_id, account_code),
    FOREIGN KEY (data_profile_key, chart_id, holding_id, company_id)
        REFERENCES erpx_holding.finance_chart_of_accounts
            (data_profile_key, chart_id, holding_id, company_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (data_profile_key, chart_id, parent_account_id)
        REFERENCES erpx_holding.finance_chart_accounts
            (data_profile_key, chart_id, account_id)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_chart_command_idempotency (
    data_profile_key TEXT NOT NULL
        CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    idempotency_key TEXT NOT NULL
        CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 512),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    chart_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, idempotency_key),
    FOREIGN KEY (data_profile_key, chart_id)
        REFERENCES erpx_holding.finance_chart_of_accounts (data_profile_key, chart_id)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS finance_chart_accounts_hierarchy_idx
    ON erpx_holding.finance_chart_accounts
        (data_profile_key, chart_id, parent_account_id, account_code);
CREATE INDEX IF NOT EXISTS finance_chart_idempotency_aggregate_idx
    ON erpx_holding.finance_chart_command_idempotency
        (data_profile_key, chart_id, aggregate_version);

ALTER TABLE erpx_holding.finance_chart_of_accounts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_chart_of_accounts FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_chart_accounts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_chart_accounts FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_chart_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_chart_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_chart_of_accounts
    ON erpx_holding.finance_chart_of_accounts;
CREATE POLICY holding_scope_finance_chart_of_accounts
    ON erpx_holding.finance_chart_of_accounts
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_finance_chart_accounts
    ON erpx_holding.finance_chart_accounts;
CREATE POLICY holding_scope_finance_chart_accounts
    ON erpx_holding.finance_chart_accounts
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_finance_chart_idempotency
    ON erpx_holding.finance_chart_command_idempotency;
CREATE POLICY holding_scope_finance_chart_idempotency
    ON erpx_holding.finance_chart_command_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

CREATE OR REPLACE FUNCTION erpx_holding.protect_finance_chart_identity()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Finance chart identities are immutable';
    END IF;
    IF NEW.data_profile_key <> OLD.data_profile_key
       OR NEW.chart_id <> OLD.chart_id
       OR NEW.holding_id <> OLD.holding_id
       OR NEW.company_id <> OLD.company_id THEN
        RAISE EXCEPTION 'Finance chart scope and identity are immutable';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS finance_chart_identity_immutable
    ON erpx_holding.finance_chart_of_accounts;
CREATE TRIGGER finance_chart_identity_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.finance_chart_of_accounts
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.protect_finance_chart_identity();

CREATE OR REPLACE FUNCTION erpx_holding.protect_finance_account_identity_and_usage()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Finance account identities are immutable';
    END IF;
    IF NEW.data_profile_key <> OLD.data_profile_key
       OR NEW.chart_id <> OLD.chart_id
       OR NEW.holding_id <> OLD.holding_id
       OR NEW.company_id <> OLD.company_id
       OR NEW.account_id <> OLD.account_id THEN
        RAISE EXCEPTION 'Finance account scope and identity are immutable';
    END IF;
    IF (
        NEW.account_code IS DISTINCT FROM OLD.account_code
        OR NEW.account_name IS DISTINCT FROM OLD.account_name
        OR NEW.category IS DISTINCT FROM OLD.category
        OR NEW.normal_balance IS DISTINCT FROM OLD.normal_balance
        OR NEW.parent_account_id IS DISTINCT FROM OLD.parent_account_id
    ) AND EXISTS (
        SELECT 1
          FROM erpx_holding.finance_posted_journals journal
          CROSS JOIN LATERAL jsonb_array_elements(
              COALESCE(journal.payload #> '{entry,lines}', '[]'::jsonb)
          ) AS line
         WHERE journal.data_profile_key = OLD.data_profile_key
           AND journal.company_id = OLD.company_id
           AND line ->> 'account_id' = OLD.account_id::text
    ) THEN
        RAISE EXCEPTION 'posted/used Finance accounts are structurally immutable';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS finance_chart_account_identity_and_usage
    ON erpx_holding.finance_chart_accounts;
CREATE TRIGGER finance_chart_account_identity_and_usage
    BEFORE UPDATE OR DELETE ON erpx_holding.finance_chart_accounts
    FOR EACH ROW
    EXECUTE FUNCTION erpx_holding.protect_finance_account_identity_and_usage();

CREATE OR REPLACE FUNCTION erpx_holding.reject_finance_chart_cycle()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    cycle_found BOOLEAN;
BEGIN
    WITH RECURSIVE ancestry AS (
        SELECT account_id,
               parent_account_id,
               ARRAY[account_id] AS path,
               FALSE AS cycle
          FROM erpx_holding.finance_chart_accounts
         WHERE data_profile_key = NEW.data_profile_key
           AND chart_id = NEW.chart_id
        UNION ALL
        SELECT parent.account_id,
               parent.parent_account_id,
               ancestry.path || parent.account_id,
               parent.account_id = ANY(ancestry.path)
          FROM ancestry
          JOIN erpx_holding.finance_chart_accounts parent
            ON parent.data_profile_key = NEW.data_profile_key
           AND parent.chart_id = NEW.chart_id
           AND parent.account_id = ancestry.parent_account_id
         WHERE NOT ancestry.cycle
    )
    SELECT EXISTS (SELECT 1 FROM ancestry WHERE cycle) INTO cycle_found;
    IF cycle_found THEN
        RAISE EXCEPTION 'Finance account hierarchy contains a cycle';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS finance_chart_hierarchy_acyclic
    ON erpx_holding.finance_chart_accounts;
CREATE CONSTRAINT TRIGGER finance_chart_hierarchy_acyclic
    AFTER INSERT OR UPDATE OF parent_account_id
    ON erpx_holding.finance_chart_accounts
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_finance_chart_cycle();


-- Journal posting records the exact normalized Chart of Accounts rows used by
-- each immutable journal. This is a write-side fact, never a read projection,
-- and is inserted in the same transaction as the journal and outbox event.
CREATE TABLE IF NOT EXISTS erpx_holding.finance_posted_journal_account_usage (
    data_profile_key TEXT NOT NULL
        CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    journal_id UUID NOT NULL
        REFERENCES erpx_holding.finance_posted_journals (journal_id)
        ON DELETE RESTRICT,
    chart_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    account_id UUID NOT NULL,
    chart_version BIGINT NOT NULL CHECK (chart_version > 0),
    line_count BIGINT NOT NULL CHECK (line_count > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, journal_id, account_id),
    FOREIGN KEY (data_profile_key, chart_id, account_id)
        REFERENCES erpx_holding.finance_chart_accounts
            (data_profile_key, chart_id, account_id)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS finance_journal_account_usage_account_idx
    ON erpx_holding.finance_posted_journal_account_usage
        (data_profile_key, chart_id, company_id, account_id, created_at);

ALTER TABLE erpx_holding.finance_posted_journal_account_usage ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_posted_journal_account_usage FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_journal_account_usage
    ON erpx_holding.finance_posted_journal_account_usage;
CREATE POLICY holding_scope_finance_journal_account_usage
    ON erpx_holding.finance_posted_journal_account_usage
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

CREATE OR REPLACE FUNCTION erpx_holding.reject_finance_journal_account_usage_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP <> 'INSERT' THEN
        RAISE EXCEPTION 'Finance journal account usage facts are immutable';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS finance_journal_account_usage_immutable
    ON erpx_holding.finance_posted_journal_account_usage;
CREATE TRIGGER finance_journal_account_usage_immutable
    BEFORE UPDATE OR DELETE ON erpx_holding.finance_posted_journal_account_usage
    FOR EACH ROW
    EXECUTE FUNCTION erpx_holding.reject_finance_journal_account_usage_mutation();

-- Extend the existing structural guard to use the normalized, durable usage
-- fact. Keep the original payload check for journals created before this
-- additive migration so historical rows remain protected during upgrade.
CREATE OR REPLACE FUNCTION erpx_holding.protect_finance_account_identity_and_usage()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Finance account identities are immutable';
    END IF;
    IF NEW.data_profile_key <> OLD.data_profile_key
       OR NEW.chart_id <> OLD.chart_id
       OR NEW.holding_id <> OLD.holding_id
       OR NEW.company_id <> OLD.company_id
       OR NEW.account_id <> OLD.account_id THEN
        RAISE EXCEPTION 'Finance account scope and identity are immutable';
    END IF;
    IF (
        NEW.account_code IS DISTINCT FROM OLD.account_code
        OR NEW.account_name IS DISTINCT FROM OLD.account_name
        OR NEW.category IS DISTINCT FROM OLD.category
        OR NEW.normal_balance IS DISTINCT FROM OLD.normal_balance
        OR NEW.parent_account_id IS DISTINCT FROM OLD.parent_account_id
    ) AND (
        EXISTS (
            SELECT 1
              FROM erpx_holding.finance_posted_journal_account_usage usage_fact
             WHERE usage_fact.data_profile_key = OLD.data_profile_key
               AND usage_fact.chart_id = OLD.chart_id
               AND usage_fact.account_id = OLD.account_id
        )
        OR EXISTS (
            SELECT 1
              FROM erpx_holding.finance_posted_journals journal
              CROSS JOIN LATERAL jsonb_array_elements(
                  COALESCE(journal.payload #> '{entry,lines}', '[]'::jsonb)
              ) AS line
             WHERE journal.data_profile_key = OLD.data_profile_key
               AND journal.company_id = OLD.company_id
               AND line ->> 'account_id' = OLD.account_id::text
        )
    ) THEN
        RAISE EXCEPTION 'posted/used Finance accounts are structurally immutable';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS finance_chart_account_identity_and_usage
    ON erpx_holding.finance_chart_accounts;
CREATE TRIGGER finance_chart_account_identity_and_usage
    BEFORE UPDATE OR DELETE ON erpx_holding.finance_chart_accounts
    FOR EACH ROW
    EXECUTE FUNCTION erpx_holding.protect_finance_account_identity_and_usage();


-- Marketing journeys are authoritative holding-write aggregates. The complete
-- domain value is retained for lossless reconstruction while bounded columns
-- support holding-scoped locking, optimistic concurrency, and operations.
CREATE TABLE IF NOT EXISTS erpx_holding.marketing_journeys (
    data_profile_key TEXT NOT NULL
        CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    journey_id UUID NOT NULL,
    scope JSONB NOT NULL,
    name TEXT NOT NULL
        CHECK (btrim(name) <> '' AND octet_length(name) <= 255),
    status TEXT NOT NULL
        CHECK (status IN ('draft', 'review', 'approved', 'scheduled',
                          'running', 'paused', 'completed', 'archived')),
    nodes JSONB NOT NULL,
    journey JSONB NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    last_command_id UUID NOT NULL,
    last_idempotency_key TEXT NOT NULL
        CHECK (btrim(last_idempotency_key) <> ''
               AND octet_length(last_idempotency_key) <= 255),
    actor JSONB NOT NULL,
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_profile_key, journey_id)
);

CREATE INDEX IF NOT EXISTS marketing_journeys_status_idx
    ON erpx_holding.marketing_journeys (data_profile_key, status, updated_at, journey_id);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_journey_command_idempotency (
    data_profile_key TEXT NOT NULL
        CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    idempotency_key TEXT NOT NULL
        CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 255),
    operation TEXT NOT NULL CHECK (operation IN ('create', 'transition')),
    request_hash TEXT NOT NULL
        CHECK (btrim(request_hash) <> '' AND octet_length(request_hash) <= 128),
    journey_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    event_id UUID NOT NULL,
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, idempotency_key)
);

CREATE INDEX IF NOT EXISTS marketing_journey_idempotency_journey_idx
    ON erpx_holding.marketing_journey_command_idempotency
        (data_profile_key, journey_id, created_at);

ALTER TABLE erpx_holding.marketing_journeys ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_journeys FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_journey_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_journey_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_journeys
    ON erpx_holding.marketing_journeys;
CREATE POLICY holding_scope_marketing_journeys
    ON erpx_holding.marketing_journeys
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_marketing_journey_idempotency
    ON erpx_holding.marketing_journey_command_idempotency;
CREATE POLICY holding_scope_marketing_journey_idempotency
    ON erpx_holding.marketing_journey_command_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Journey execution cursors are holding-write state. The initial cursor is a
-- durable seed at version zero; the first execution fact advances it to the
-- positive version required by the shared outbox contract.
CREATE TABLE IF NOT EXISTS erpx_holding.marketing_journey_execution_cursors (
    data_profile_key TEXT NOT NULL
        CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    run_id UUID NOT NULL,
    journey_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    scope JSONB NOT NULL,
    state TEXT NOT NULL
        CHECK (state IN ('ready', 'running', 'waiting', 'paused', 'completed')),
    next_node_id UUID,
    waiting_until TIMESTAMPTZ,
    version BIGINT NOT NULL CHECK (version >= 0),
    cursor JSONB NOT NULL,
    current_claim_id UUID,
    claimed_by UUID,
    claim_until TIMESTAMPTZ,
    next_attempt_at TIMESTAMPTZ NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error TEXT,
    dead_lettered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_profile_key, run_id)
);

CREATE INDEX IF NOT EXISTS marketing_journey_execution_due_idx
    ON erpx_holding.marketing_journey_execution_cursors
        (data_profile_key, state, next_attempt_at, waiting_until, claim_until, run_id);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_journey_execution_idempotency (
    data_profile_key TEXT NOT NULL
        CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    idempotency_key TEXT NOT NULL
        CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 255),
    run_id UUID NOT NULL,
    request_hash TEXT NOT NULL
        CHECK (btrim(request_hash) <> '' AND octet_length(request_hash) <= 128),
    version BIGINT NOT NULL CHECK (version > 0),
    event_id UUID NOT NULL,
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, idempotency_key)
);

CREATE INDEX IF NOT EXISTS marketing_journey_execution_idempotency_run_idx
    ON erpx_holding.marketing_journey_execution_idempotency
        (data_profile_key, run_id, created_at);

ALTER TABLE erpx_holding.marketing_journey_execution_cursors ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_journey_execution_cursors FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_journey_execution_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_journey_execution_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_journey_execution_cursors
    ON erpx_holding.marketing_journey_execution_cursors;
CREATE POLICY holding_scope_marketing_journey_execution_cursors
    ON erpx_holding.marketing_journey_execution_cursors
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

DROP POLICY IF EXISTS holding_scope_marketing_journey_execution_idempotency
    ON erpx_holding.marketing_journey_execution_idempotency;
CREATE POLICY holding_scope_marketing_journey_execution_idempotency
    ON erpx_holding.marketing_journey_execution_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Authoritative Marketing content and digital-experience write state.
-- Metadata and opaque object references live here; MinIO bytes never do.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_assets (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
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
    PRIMARY KEY (data_profile_key, asset_id)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_asset_versions (
    data_profile_key TEXT NOT NULL,
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
    PRIMARY KEY (data_profile_key, asset_id, version),
    UNIQUE (data_profile_key, asset_version_id),
    FOREIGN KEY (data_profile_key, asset_id)
        REFERENCES erpx_holding.marketing_content_assets (data_profile_key, asset_id)
        ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_templates (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
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
    PRIMARY KEY (data_profile_key, template_id),
    UNIQUE (data_profile_key, slug)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_template_versions (
    data_profile_key TEXT NOT NULL,
    template_id UUID NOT NULL,
    template_version_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    scope JSONB NOT NULL,
    state JSONB NOT NULL,
    created_by TEXT NOT NULL CHECK (btrim(created_by) <> '' AND octet_length(created_by) <= 255),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_profile_key, template_id, version),
    UNIQUE (data_profile_key, template_version_id),
    FOREIGN KEY (data_profile_key, template_id)
        REFERENCES erpx_holding.marketing_content_templates (data_profile_key, template_id)
        ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_previews (
    data_profile_key TEXT NOT NULL,
    preview_id UUID NOT NULL,
    template_id UUID NOT NULL,
    template_version BIGINT NOT NULL CHECK (template_version > 0),
    scope JSONB NOT NULL,
    token_hash TEXT NOT NULL CHECK (token_hash ~ '^[0-9A-Fa-f]{64}$'),
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    PRIMARY KEY (data_profile_key, preview_id),
    FOREIGN KEY (data_profile_key, template_id, template_version)
        REFERENCES erpx_holding.marketing_content_template_versions (data_profile_key, template_id, version)
        ON DELETE RESTRICT,
    CHECK (expires_at > issued_at),
    CHECK (revoked_at IS NULL OR revoked_at >= issued_at)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_forms (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
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
    PRIMARY KEY (data_profile_key, form_id),
    UNIQUE (data_profile_key, slug)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_form_submissions (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    submission_id UUID NOT NULL,
    form_id UUID NOT NULL,
    scope JSONB NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 255),
    values_json JSONB NOT NULL,
    received_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_profile_key, submission_id),
    UNIQUE (data_profile_key, form_id, idempotency_key),
    FOREIGN KEY (data_profile_key, form_id)
        REFERENCES erpx_holding.marketing_content_forms (data_profile_key, form_id)
        ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_domains (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
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
    PRIMARY KEY (data_profile_key, domain_id),
    UNIQUE (data_profile_key, hostname)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_domain_bindings (
    data_profile_key TEXT NOT NULL,
    domain_id UUID NOT NULL,
    template_id UUID NOT NULL,
    template_version BIGINT NOT NULL CHECK (template_version > 0),
    scope JSONB NOT NULL,
    bound_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_profile_key, domain_id),
    FOREIGN KEY (data_profile_key, domain_id)
        REFERENCES erpx_holding.marketing_content_domains (data_profile_key, domain_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (data_profile_key, template_id, template_version)
        REFERENCES erpx_holding.marketing_content_template_versions (data_profile_key, template_id, version)
        ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_approvals (
    data_profile_key TEXT NOT NULL,
    approval_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN ('template', 'form', 'domain')),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    approver TEXT NOT NULL CHECK (btrim(approver) <> '' AND octet_length(approver) <= 255),
    approved_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_profile_key, approval_id),
    UNIQUE (data_profile_key, aggregate_type, aggregate_id, aggregate_version)
);

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_content_command_idempotency (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 255),
    operation TEXT NOT NULL CHECK (btrim(operation) <> '' AND octet_length(operation) <= 64),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> '' AND octet_length(aggregate_type) <= 64),
    aggregate_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    event_id UUID NOT NULL,
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, idempotency_key)
);

CREATE INDEX IF NOT EXISTS marketing_content_assets_scope_idx
    ON erpx_holding.marketing_content_assets (data_profile_key, updated_at DESC, asset_id);
CREATE INDEX IF NOT EXISTS marketing_content_templates_status_idx
    ON erpx_holding.marketing_content_templates (data_profile_key, status, updated_at DESC, template_id);
CREATE INDEX IF NOT EXISTS marketing_content_forms_status_idx
    ON erpx_holding.marketing_content_forms (data_profile_key, status, updated_at DESC, form_id);
CREATE INDEX IF NOT EXISTS marketing_content_domains_status_idx
    ON erpx_holding.marketing_content_domains (data_profile_key, status, hostname);
CREATE INDEX IF NOT EXISTS marketing_content_submissions_form_idx
    ON erpx_holding.marketing_content_form_submissions (data_profile_key, form_id, received_at DESC);

ALTER TABLE erpx_holding.marketing_content_assets ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_assets FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_asset_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_asset_versions FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_templates ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_templates FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_template_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_template_versions FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_previews ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_previews FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_forms ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_forms FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_form_submissions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_form_submissions FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_domains ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_domains FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_domain_bindings ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_domain_bindings FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_approvals ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_approvals FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_content_command_idempotency FORCE ROW LEVEL SECURITY;

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
        EXECUTE format('DROP POLICY IF EXISTS holding_scope_%s ON erpx_holding.%I', table_name, table_name);
        EXECUTE format(
            'CREATE POLICY holding_scope_%s ON erpx_holding.%I USING (data_profile_key = current_setting(''erpx.data_profile_key'', true)) WITH CHECK (data_profile_key = current_setting(''erpx.data_profile_key'', true))',
            table_name, table_name
        );
    END LOOP;
END $$;

-- Version records, approvals, and source events are immutable audit facts.
CREATE OR REPLACE FUNCTION erpx_holding.reject_marketing_content_fact_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Marketing content version or approval facts are append-only';
END;
$$;

DROP TRIGGER IF EXISTS marketing_content_asset_version_append_only ON erpx_holding.marketing_content_asset_versions;
CREATE TRIGGER marketing_content_asset_version_append_only
    BEFORE UPDATE OR DELETE ON erpx_holding.marketing_content_asset_versions
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_marketing_content_fact_mutation();
DROP TRIGGER IF EXISTS marketing_content_template_version_append_only ON erpx_holding.marketing_content_template_versions;
CREATE TRIGGER marketing_content_template_version_append_only
    BEFORE UPDATE OR DELETE ON erpx_holding.marketing_content_template_versions
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_marketing_content_fact_mutation();
DROP TRIGGER IF EXISTS marketing_content_approval_append_only ON erpx_holding.marketing_content_approvals;
CREATE TRIGGER marketing_content_approval_append_only
    BEFORE UPDATE OR DELETE ON erpx_holding.marketing_content_approvals
    FOR EACH ROW EXECUTE FUNCTION erpx_holding.reject_marketing_content_fact_mutation();


-- Bounded retention and irreversible redaction for form submissions.
-- Submission identity and audit/outbox facts remain durable; submitted values
-- are replaced with an empty object when the retention deadline is reached.
ALTER TABLE erpx_holding.marketing_content_form_submissions
    ADD COLUMN IF NOT EXISTS retention_until TIMESTAMPTZ NOT NULL
        DEFAULT (now() + INTERVAL '365 days'),
    ADD COLUMN IF NOT EXISTS redacted_at TIMESTAMPTZ;

ALTER TABLE erpx_holding.marketing_content_form_submissions
    ALTER COLUMN retention_until DROP DEFAULT,
    ADD CONSTRAINT marketing_content_submission_retention_after_received
        CHECK (retention_until >= received_at),
    ADD CONSTRAINT marketing_content_submission_redacted_after_received
        CHECK (redacted_at IS NULL OR redacted_at >= received_at);

CREATE INDEX IF NOT EXISTS marketing_content_submissions_retention_idx
    ON erpx_holding.marketing_content_form_submissions
        (data_profile_key, retention_until, redacted_at, submission_id);


-- Profile erasure tombstones scrub direct contact fields while retaining the
-- profile identity and append-only consent/audit facts.
ALTER TABLE erpx_holding.marketing_profiles
    ADD COLUMN IF NOT EXISTS erased_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS erasure_reason TEXT;

ALTER TABLE erpx_holding.marketing_profiles
    ADD CONSTRAINT marketing_profile_erasure_reason_bound
        CHECK (erasure_reason IS NULL OR (btrim(erasure_reason) <> '' AND octet_length(erasure_reason) <= 512)),
    ADD CONSTRAINT marketing_profile_erased_at_pair
        CHECK ((erased_at IS NULL AND erasure_reason IS NULL)
            OR (erased_at IS NOT NULL AND erasure_reason IS NOT NULL));

ALTER TABLE erpx_holding.marketing_command_idempotency
    DROP CONSTRAINT IF EXISTS marketing_command_idempotency_operation_check,
    ADD CONSTRAINT marketing_command_idempotency_operation_check
        CHECK (operation IN ('append_consent', 'revoke_consent',
                             'upsert_suppression', 'upsert_audience_membership',
                             'erase_profile'));

CREATE INDEX IF NOT EXISTS marketing_profiles_erasure_idx
    ON erpx_holding.marketing_profiles (data_profile_key, erased_at, profile_id);


-- G13 target import ledger. This is holding write state, not a read-model or
-- source database cache. Rows are immutable for a run/source fence so replay
-- cannot silently relink a legacy identity to a different ERPX identity.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.g13_migration_import_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    run_id UUID NOT NULL,
    source_contract_hash TEXT NOT NULL CHECK (length(source_contract_hash) = 64),
    source_fence TEXT NOT NULL CHECK (btrim(source_fence) <> ''),
    source_kind TEXT NOT NULL CHECK (btrim(source_kind) <> ''),
    source_key TEXT NOT NULL CHECK (btrim(source_key) <> ''),
    target_id UUID NOT NULL,
    payload JSONB NOT NULL,
    imported_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, run_id, source_kind, source_key)
);

CREATE INDEX IF NOT EXISTS g13_migration_import_run_idx
    ON erpx_holding.g13_migration_import_rows (data_profile_key, run_id, source_kind, imported_at);

ALTER TABLE erpx_holding.g13_migration_import_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.g13_migration_import_rows FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS holding_scope_g13_migration_import_rows
    ON erpx_holding.g13_migration_import_rows;
CREATE POLICY holding_scope_g13_migration_import_rows
    ON erpx_holding.g13_migration_import_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

-- A rollback rehearsal is kept in a separate holding write database. It is
-- never used as a command source and is safe to discard after retention.
CREATE TABLE IF NOT EXISTS erpx_holding.g13_migration_rollback_rows (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    run_id UUID NOT NULL,
    source_contract_hash TEXT NOT NULL CHECK (length(source_contract_hash) = 64),
    source_fence TEXT NOT NULL CHECK (btrim(source_fence) <> ''),
    source_kind TEXT NOT NULL CHECK (btrim(source_kind) <> ''),
    source_key TEXT NOT NULL CHECK (btrim(source_key) <> ''),
    target_id UUID NOT NULL,
    payload JSONB NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, run_id, source_kind, source_key)
);

CREATE INDEX IF NOT EXISTS g13_migration_rollback_run_idx
    ON erpx_holding.g13_migration_rollback_rows (data_profile_key, run_id, source_kind, captured_at);

ALTER TABLE erpx_holding.g13_migration_rollback_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.g13_migration_rollback_rows FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS holding_scope_g13_migration_rollback_rows
    ON erpx_holding.g13_migration_rollback_rows;
CREATE POLICY holding_scope_g13_migration_rollback_rows
    ON erpx_holding.g13_migration_rollback_rows
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Durable, holding-scoped Operations SLA notification inbox.
-- The notification is created in the same write transaction as the
-- escalation event. Provider delivery is downstream; this table owns only
-- bounded claim, lease, acknowledgement, and expiry state.
CREATE TABLE IF NOT EXISTS erpx_holding.operations_sla_notifications (
    notification_key TEXT PRIMARY KEY CHECK (btrim(notification_key) <> '' AND octet_length(notification_key) <= 512),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    exception_id UUID NOT NULL,
    exception_version BIGINT NOT NULL CHECK (exception_version > 0),
    recipient_ref TEXT NOT NULL CHECK (btrim(recipient_ref) <> '' AND octet_length(recipient_ref) <= 255),
    kind TEXT NOT NULL CHECK (kind IN ('sla_escalated')),
    not_before TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'claimed', 'delivered', 'expired')),
    claimed_by UUID,
    claim_token UUID,
    claimed_at TIMESTAMPTZ,
    claim_expires_at TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (not_before < expires_at),
    CHECK ((status = 'claimed') = (claimed_by IS NOT NULL AND claim_token IS NOT NULL AND claimed_at IS NOT NULL AND claim_expires_at IS NOT NULL)),
    CHECK ((status = 'delivered') = (delivered_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS operations_sla_notifications_due_idx
    ON erpx_holding.operations_sla_notifications
       (data_profile_key, status, not_before, expires_at, notification_key);

ALTER TABLE erpx_holding.operations_sla_notifications ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.operations_sla_notifications FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_operations_sla_notifications
    ON erpx_holding.operations_sla_notifications;
CREATE POLICY holding_scope_operations_sla_notifications
    ON erpx_holding.operations_sla_notifications
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Explicit Thai statutory tax facts linked to immutable posted journal lines.
-- The fact and its outbox event are authoritative; reports are rebuildable.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.finance_thai_tax_facts (
    fact_id UUID PRIMARY KEY,
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    journal_id UUID NOT NULL,
    line_id UUID NOT NULL,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    tax_date DATE NOT NULL,
    tax_period CHAR(7) NOT NULL CHECK (tax_period ~ '^[0-9]{4}-[0-9]{2}$'),
    kind TEXT NOT NULL CHECK (kind IN ('output_vat', 'input_vat', 'withholding_payable')),
    currency CHAR(3) NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    base_minor BIGINT NOT NULL CHECK (base_minor > 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    tax_rate_basis_points INTEGER NOT NULL CHECK (tax_rate_basis_points BETWEEN 0 AND 10000),
    tax_identifier TEXT NOT NULL CHECK (btrim(tax_identifier) <> '' AND char_length(tax_identifier) <= 255),
    context JSONB NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS erpx_holding.finance_thai_tax_command_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    fact_id UUID NOT NULL REFERENCES erpx_holding.finance_thai_tax_facts(fact_id) ON DELETE RESTRICT,
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS finance_thai_tax_facts_report_idx
    ON erpx_holding.finance_thai_tax_facts
       (data_profile_key, company_id, tax_period, tax_date, fact_id);
CREATE INDEX IF NOT EXISTS finance_thai_tax_facts_journal_idx
    ON erpx_holding.finance_thai_tax_facts
       (data_profile_key, journal_id, line_id);

ALTER TABLE erpx_holding.finance_thai_tax_facts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_thai_tax_facts FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_thai_tax_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.finance_thai_tax_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_finance_thai_tax_facts
    ON erpx_holding.finance_thai_tax_facts;
CREATE POLICY holding_scope_finance_thai_tax_facts
    ON erpx_holding.finance_thai_tax_facts
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_finance_thai_tax_idempotency
    ON erpx_holding.finance_thai_tax_command_idempotency;
CREATE POLICY holding_scope_finance_thai_tax_idempotency
    ON erpx_holding.finance_thai_tax_command_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));

CREATE OR REPLACE FUNCTION erpx_holding.reject_finance_thai_tax_fact_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'Thai statutory tax facts are append-only';
END;
$$;

DROP TRIGGER IF EXISTS finance_thai_tax_facts_append_only
    ON erpx_holding.finance_thai_tax_facts;
CREATE TRIGGER finance_thai_tax_facts_append_only
    BEFORE UPDATE OR DELETE ON erpx_holding.finance_thai_tax_facts
    FOR EACH ROW
    EXECUTE FUNCTION erpx_holding.reject_finance_thai_tax_fact_mutation();


-- Authoritative Quality inspection aggregates and immutable source events.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.quality_inspections (
    inspection_id UUID PRIMARY KEY,
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    company_id UUID NOT NULL,
    branch_id UUID,
    reference TEXT NOT NULL CHECK (btrim(reference) <> ''),
    version BIGINT NOT NULL CHECK (version > 0),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    payload JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (data_profile_key, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_holding.quality_command_idempotency (
    idempotency_key TEXT PRIMARY KEY CHECK (btrim(idempotency_key) <> ''),
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    request_hash TEXT NOT NULL CHECK (length(request_hash) = 64),
    response JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS quality_inspections_scope_idx
    ON erpx_holding.quality_inspections
       (data_profile_key, company_id, branch_id, updated_at DESC, inspection_id);

ALTER TABLE erpx_holding.quality_inspections ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.quality_inspections FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.quality_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.quality_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_quality_inspections
    ON erpx_holding.quality_inspections;
CREATE POLICY holding_scope_quality_inspections
    ON erpx_holding.quality_inspections
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));
DROP POLICY IF EXISTS holding_scope_quality_idempotency
    ON erpx_holding.quality_command_idempotency;
CREATE POLICY holding_scope_quality_idempotency
    ON erpx_holding.quality_command_idempotency
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Immutable conversion facts produced by the Marketing journey worker.
-- The write row is authoritative; the matching read row is rebuilt from its
-- outbox event and never becomes a command source.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_journey_conversions (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    conversion_ref UUID NOT NULL,
    journey_id UUID NOT NULL,
    run_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    scope JSONB NOT NULL,
    revenue_minor BIGINT CHECK (revenue_minor >= 0),
    currency TEXT CHECK (currency IS NULL OR (length(currency) = 3 AND currency = upper(currency))),
    occurred_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL CHECK (version = 1),
    payload JSONB NOT NULL,
    payload_hash TEXT NOT NULL CHECK (length(payload_hash) = 64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, conversion_ref),
    UNIQUE (data_profile_key, run_id, journey_id, profile_id, conversion_ref)
);

CREATE INDEX IF NOT EXISTS marketing_journey_conversions_scope_idx
    ON erpx_holding.marketing_journey_conversions
       (data_profile_key, occurred_at DESC, conversion_ref);

ALTER TABLE erpx_holding.marketing_journey_conversions ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_journey_conversions FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_journey_conversions
    ON erpx_holding.marketing_journey_conversions;
CREATE POLICY holding_scope_marketing_journey_conversions
    ON erpx_holding.marketing_journey_conversions
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Durable internal owner-notification handoffs emitted by Marketing journeys.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_journey_owner_notifications (
    data_profile_key TEXT NOT NULL CHECK (btrim(data_profile_key) <> ''),
    notification_ref UUID NOT NULL,
    journey_id UUID NOT NULL,
    run_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    scope JSONB NOT NULL,
    owner_ref TEXT NOT NULL CHECK (btrim(owner_ref) <> '' AND octet_length(owner_ref) <= 160),
    occurred_at TIMESTAMPTZ NOT NULL,
    version BIGINT NOT NULL CHECK (version = 1),
    payload JSONB NOT NULL,
    payload_hash TEXT NOT NULL CHECK (length(payload_hash) = 64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_profile_key, notification_ref),
    UNIQUE (data_profile_key, run_id, journey_id, profile_id, notification_ref)
);

CREATE INDEX IF NOT EXISTS marketing_journey_owner_notifications_scope_idx
    ON erpx_holding.marketing_journey_owner_notifications
       (data_profile_key, occurred_at DESC, notification_ref);

ALTER TABLE erpx_holding.marketing_journey_owner_notifications ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_journey_owner_notifications FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_journey_owner_notifications
    ON erpx_holding.marketing_journey_owner_notifications;
CREATE POLICY holding_scope_marketing_journey_owner_notifications
    ON erpx_holding.marketing_journey_owner_notifications
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


-- Durable, profile-redacted handoffs from Marketing journeys to an
-- authoritative, holding-bound provider account. Provider endpoints and
-- credentials remain in the control-plane provider-account aggregate.
CREATE SCHEMA IF NOT EXISTS erpx_holding;

CREATE TABLE IF NOT EXISTS erpx_holding.marketing_journey_integration_invocations (
    data_profile_key TEXT NOT NULL
        CHECK (btrim(data_profile_key) <> '' AND octet_length(data_profile_key) <= 255),
    invocation_id UUID NOT NULL,
    integration_ref TEXT NOT NULL
        CHECK (btrim(integration_ref) <> '' AND octet_length(integration_ref) <= 160),
    journey_id UUID NOT NULL,
    run_id UUID NOT NULL,
    node_id UUID NOT NULL,
    profile_id UUID NOT NULL,
    scope JSONB NOT NULL,
    idempotency_key TEXT NOT NULL
        CHECK (btrim(idempotency_key) <> '' AND octet_length(idempotency_key) <= 255),
    state TEXT NOT NULL
        CHECK (state IN ('queued', 'sending', 'succeeded', 'retry_scheduled', 'failed')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    version BIGINT NOT NULL CHECK (version > 0),
    provider_reference TEXT
        CHECK (provider_reference IS NULL OR (btrim(provider_reference) <> '' AND octet_length(provider_reference) <= 255)),
    last_error TEXT
        CHECK (last_error IS NULL OR (btrim(last_error) <> '' AND octet_length(last_error) <= 128)),
    next_attempt_at TIMESTAMPTZ NOT NULL,
    claim_id UUID,
    claimed_by UUID,
    claim_until TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (data_profile_key, invocation_id),
    UNIQUE (data_profile_key, idempotency_key)
);

CREATE INDEX IF NOT EXISTS marketing_journey_integration_due_idx
    ON erpx_holding.marketing_journey_integration_invocations
       (data_profile_key, state, next_attempt_at, claim_until, invocation_id);

ALTER TABLE erpx_holding.marketing_journey_integration_invocations ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_holding.marketing_journey_integration_invocations FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS holding_scope_marketing_journey_integration_invocations
    ON erpx_holding.marketing_journey_integration_invocations;
CREATE POLICY holding_scope_marketing_journey_integration_invocations
    ON erpx_holding.marketing_journey_integration_invocations
    USING (data_profile_key = current_setting('erpx.data_profile_key', true))
    WITH CHECK (data_profile_key = current_setting('erpx.data_profile_key', true));


CREATE SCHEMA IF NOT EXISTS erpx_cqrs_write;

CREATE OR REPLACE FUNCTION erpx_cqrs_write.current_holding_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.holding_id', true), '')::uuid
$$;

CREATE TABLE erpx_cqrs_write.command_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    context_name TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash) = 32),
    correlation_id UUID NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('reserved', 'completed')),
    receipt JSONB,
    created_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, context_name, partition_key, idempotency_key),
    UNIQUE (holding_id, context_name, partition_key, command_id),
    CHECK (
        (status = 'reserved' AND receipt IS NULL AND completed_at IS NULL)
        OR (status = 'completed' AND receipt IS NOT NULL AND completed_at IS NOT NULL)
    )
);

CREATE TABLE erpx_cqrs_write.audit_log (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    audit_id UUID NOT NULL,
    context_name TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    active_scope JSONB NOT NULL,
    actor JSONB NOT NULL,
    audit_context JSONB NOT NULL,
    action TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    details JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, audit_id),
    UNIQUE (holding_id, context_name, partition_key, command_id, audit_id)
);

CREATE TABLE erpx_cqrs_write.outbox_events (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    global_sequence BIGSERIAL NOT NULL,
    event_id UUID NOT NULL,
    context_name TEXT NOT NULL,
    partition_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    event_version INTEGER NOT NULL CHECK (event_version > 0),
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    active_scope JSONB NOT NULL,
    actor JSONB NOT NULL,
    correlation_id UUID NOT NULL,
    causation_id UUID,
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    available_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ,
    publish_attempts INTEGER NOT NULL DEFAULT 0 CHECK (publish_attempts >= 0),
    publish_error TEXT,
    PRIMARY KEY (holding_id, global_sequence),
    UNIQUE (holding_id, event_id)
);

CREATE INDEX cqrs_outbox_projection_source
    ON erpx_cqrs_write.outbox_events (holding_id, global_sequence)
    WHERE published_at IS NULL;

CREATE INDEX cqrs_outbox_publish_source
    ON erpx_cqrs_write.outbox_events (holding_id, available_at, global_sequence)
    WHERE published_at IS NULL;

CREATE INDEX cqrs_outbox_command
    ON erpx_cqrs_write.outbox_events (holding_id, context_name, partition_key, command_id);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'command_receipts','audit_log','outbox_events'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_cqrs_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_cqrs_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY cqrs_write_holding_isolation ON erpx_cqrs_write.%I '
            'USING (holding_id = erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_write.current_holding_id())',
            table_name
        );
    END LOOP;
END;
$$;


-- Historical uniqueness and hierarchy guards. Migration 0081 promotes these
-- roots to typed relational command authority; receipts, audit and outbox
-- facts remain atomic in erpx_cqrs_write.

CREATE SCHEMA IF NOT EXISTS erpx_identity_access_write;

CREATE TABLE erpx_identity_access_write.role_code_guards (
    role_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    role_code TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, role_code)
);

CREATE TABLE erpx_identity_access_write.assignment_guards (
    assignment_id UUID PRIMARY KEY,
    principal_id UUID NOT NULL,
    role_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    risk_tier TEXT NOT NULL CHECK (risk_tier IN (
        'platform_root', 'administrator', 'high', 'standard', 'contractor',
        'support_diagnostic', 'support_mutation', 'emergency'
    )),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    principal_version BIGINT NOT NULL CHECK (principal_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (valid_until > valid_from)
);

CREATE UNIQUE INDEX identity_access_one_live_assignment
    ON erpx_identity_access_write.assignment_guards
        (principal_id, role_id, scope_key)
    WHERE revoked_at IS NULL;

CREATE INDEX identity_access_assignment_validity
    ON erpx_identity_access_write.assignment_guards
        (principal_id, valid_from, valid_until)
    WHERE revoked_at IS NULL;

CREATE SCHEMA IF NOT EXISTS erpx_organization_write;

CREATE TABLE erpx_organization_write.hierarchy_guards (
    node_id UUID PRIMARY KEY,
    node_kind TEXT NOT NULL CHECK (node_kind IN (
        'holding', 'company', 'headquarters', 'branch', 'department', 'team'
    )),
    parent_node_id UUID,
    lineage_node_ids UUID[] NOT NULL,
    holding_id UUID NOT NULL,
    company_id UUID,
    branch_id UUID,
    department_id UUID,
    node_code TEXT NOT NULL,
    node_status TEXT NOT NULL CHECK (node_status IN ('active', 'suspended', 'archived')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (NOT node_id = ANY(lineage_node_ids)),
    CHECK (
        (node_kind = 'holding' AND parent_node_id IS NULL AND company_id IS NULL)
        OR (node_kind <> 'holding' AND parent_node_id IS NOT NULL)
    )
);

CREATE UNIQUE INDEX organization_sibling_code
    ON erpx_organization_write.hierarchy_guards
        (holding_id, COALESCE(parent_node_id, holding_id), node_code)
    WHERE node_status <> 'archived';

CREATE INDEX organization_active_children
    ON erpx_organization_write.hierarchy_guards (parent_node_id, node_id)
    WHERE node_status = 'active';

CREATE INDEX organization_lineage_lookup
    ON erpx_organization_write.hierarchy_guards USING GIN (lineage_node_ids);

CREATE SCHEMA IF NOT EXISTS erpx_commercial_write;

CREATE TABLE erpx_commercial_write.catalog_plan_guards (
    catalog_plan_id UUID PRIMARY KEY,
    plan_key TEXT NOT NULL UNIQUE,
    plan_status TEXT NOT NULL CHECK (plan_status IN ('draft', 'published')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE erpx_commercial_write.subscription_guards (
    subscription_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    customer_id UUID NOT NULL,
    catalog_plan_id UUID NOT NULL,
    subscription_status TEXT NOT NULL CHECK (subscription_status IN (
        'trial', 'active', 'past_due', 'suspended', 'ended'
    )),
    last_provider_event_at TIMESTAMPTZ,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX commercial_customer_subscription
    ON erpx_commercial_write.subscription_guards
        (scope_key, customer_id, subscription_status);

CREATE TABLE erpx_commercial_write.entitlement_guards (
    entitlement_id UUID PRIMARY KEY,
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
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, subscription_id, capability)
);


-- Catalog plans are platform-scoped and authoritative in control write.
-- Holding databases retain only subscription and entitlement command guards.
DROP TABLE IF EXISTS erpx_commercial_write.catalog_plan_guards;



-- Wave 6 paired-context relational guards. Migration 0081 promotes these
-- roots to typed authority while receipts, audit and outbox facts remain
-- atomic through erpx_cqrs_write.

CREATE SCHEMA IF NOT EXISTS erpx_marketing_write;
CREATE TABLE erpx_marketing_write.lead_candidate_guards (
    lead_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    campaign_id UUID NOT NULL,
    audience_member_id UUID NOT NULL,
    lead_status TEXT NOT NULL CHECK (lead_status IN ('open', 'qualified', 'rejected')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX marketing_lead_scope_status
    ON erpx_marketing_write.lead_candidate_guards
        (scope_key, lead_status, updated_at DESC, lead_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_crm_write;
CREATE TABLE erpx_crm_write.lead_guards (
    lead_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    marketing_candidate_id UUID NOT NULL UNIQUE,
    customer_id UUID,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    accepted_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE erpx_crm_write.opportunity_guards (
    opportunity_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    accepted_lead_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    opportunity_stage TEXT NOT NULL CHECK (opportunity_stage IN (
        'qualified', 'discovery', 'proposal', 'negotiation', 'won', 'lost'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (accepted_lead_id)
        REFERENCES erpx_crm_write.lead_guards (lead_id)
);
CREATE INDEX crm_opportunity_scope_customer_stage
    ON erpx_crm_write.opportunity_guards
        (scope_key, customer_id, opportunity_stage, updated_at DESC, opportunity_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_procurement_write;
CREATE TABLE erpx_procurement_write.purchase_order_guards (
    purchase_order_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    order_code TEXT NOT NULL,
    supplier_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    order_status TEXT NOT NULL CHECK (order_status IN ('draft', 'released', 'cancelled')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, order_code)
);
CREATE INDEX procurement_order_scope_supplier_status
    ON erpx_procurement_write.purchase_order_guards
        (scope_key, supplier_id, order_status, updated_at DESC, purchase_order_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_inventory_write;
CREATE TABLE erpx_inventory_write.stock_item_guards (
    stock_item_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    warehouse_id UUID NOT NULL,
    item_code TEXT NOT NULL,
    on_hand BIGINT NOT NULL CHECK (on_hand >= 0),
    reserved BIGINT NOT NULL CHECK (reserved >= 0 AND reserved <= on_hand),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, warehouse_id, item_code)
);
CREATE TABLE erpx_inventory_write.movement_guards (
    movement_id UUID PRIMARY KEY,
    stock_item_id UUID NOT NULL,
    quantity_delta BIGINT NOT NULL CHECK (quantity_delta <> 0),
    procurement_line_id UUID,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (stock_item_id)
        REFERENCES erpx_inventory_write.stock_item_guards (stock_item_id)
        ON DELETE CASCADE
);
CREATE TABLE erpx_inventory_write.reservation_guards (
    reservation_id UUID PRIMARY KEY,
    stock_item_id UUID NOT NULL,
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    demand_reference TEXT NOT NULL,
    reservation_status TEXT NOT NULL CHECK (reservation_status IN ('active', 'released')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (stock_item_id)
        REFERENCES erpx_inventory_write.stock_item_guards (stock_item_id)
        ON DELETE CASCADE
);
CREATE INDEX inventory_stock_scope_warehouse_code
    ON erpx_inventory_write.stock_item_guards
        (scope_key, warehouse_id, item_code, updated_at DESC, stock_item_id DESC);
CREATE INDEX inventory_active_reservations
    ON erpx_inventory_write.reservation_guards (stock_item_id, reservation_id)
    WHERE reservation_status = 'active';

CREATE SCHEMA IF NOT EXISTS erpx_assets_write;
CREATE TABLE erpx_assets_write.asset_guards (
    asset_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    asset_code TEXT NOT NULL,
    location_id UUID NOT NULL,
    asset_status TEXT NOT NULL CHECK (asset_status IN (
        'draft', 'in_service', 'under_maintenance', 'retired'
    )),
    asset_condition TEXT NOT NULL CHECK (asset_condition IN (
        'operational', 'degraded', 'out_of_service', 'retired'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, asset_code)
);
CREATE INDEX assets_scope_location_status
    ON erpx_assets_write.asset_guards
        (scope_key, location_id, asset_status, updated_at DESC, asset_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_maintenance_write;
CREATE TABLE erpx_maintenance_write.asset_fact_inbox (
    asset_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_event_id UUID NOT NULL UNIQUE,
    holding_scope JSONB NOT NULL,
    asset_code TEXT NOT NULL,
    asset_name TEXT NOT NULL,
    location_id UUID NOT NULL,
    asset_condition TEXT NOT NULL CHECK (asset_condition IN (
        'operational', 'degraded', 'out_of_service', 'retired'
    )),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (asset_id, source_aggregate_version)
);
CREATE TABLE erpx_maintenance_write.work_order_guards (
    work_order_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    work_order_code TEXT NOT NULL,
    asset_id UUID NOT NULL,
    request_code TEXT NOT NULL,
    source_event_id UUID NOT NULL UNIQUE,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    work_order_status TEXT NOT NULL CHECK (work_order_status IN (
        'planned', 'scheduled', 'in_progress', 'blocked', 'completed', 'cancelled'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, work_order_code),
    FOREIGN KEY (asset_id, source_aggregate_version)
        REFERENCES erpx_maintenance_write.asset_fact_inbox
            (asset_id, source_aggregate_version)
);
CREATE TABLE erpx_maintenance_write.downtime_guards (
    downtime_id UUID PRIMARY KEY,
    work_order_id UUID NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ NOT NULL,
    reason TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    CHECK (ended_at > started_at),
    FOREIGN KEY (work_order_id)
        REFERENCES erpx_maintenance_write.work_order_guards (work_order_id)
        ON DELETE CASCADE
);
CREATE INDEX maintenance_scope_asset_status
    ON erpx_maintenance_write.work_order_guards
        (scope_key, asset_id, work_order_status, updated_at DESC, work_order_id DESC);


-- Wave 7 transactional-context guards. Migration 0081 promotes the Sales and
-- POS roots plus typed children to current command authority. Idempotency
-- receipts, audit records, and typed outbox events remain atomic in the shared
-- erpx_cqrs_write unit of work.

CREATE SCHEMA IF NOT EXISTS erpx_sales_write;
CREATE TABLE erpx_sales_write.quote_guards (
    quote_id UUID PRIMARY KEY,
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
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (valid_until > created_at),
    CHECK (updated_at >= created_at),
    UNIQUE (scope_key, quote_code)
);
CREATE TABLE erpx_sales_write.sales_order_guards (
    sales_order_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    quote_id UUID NOT NULL UNIQUE,
    opportunity_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    order_code TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    total_minor BIGINT NOT NULL CHECK (total_minor >= 0),
    order_status TEXT NOT NULL CHECK (order_status IN (
        'draft', 'confirmed', 'partially_fulfilled', 'fulfilled', 'cancelled'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    UNIQUE (scope_key, order_code),
    FOREIGN KEY (quote_id) REFERENCES erpx_sales_write.quote_guards (quote_id)
);
CREATE INDEX sales_quote_scope_customer_status
    ON erpx_sales_write.quote_guards
        (scope_key, customer_id, quote_status, updated_at DESC, quote_id DESC);
CREATE INDEX sales_order_scope_customer_status
    ON erpx_sales_write.sales_order_guards
        (scope_key, customer_id, order_status, updated_at DESC, sales_order_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_people_payroll_write;
CREATE TABLE erpx_people_payroll_write.employee_guards (
    employee_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    employee_code TEXT NOT NULL,
    hired_on DATE NOT NULL,
    terminated_on DATE,
    employee_status TEXT NOT NULL CHECK (employee_status IN (
        'draft', 'active', 'on_leave', 'suspended', 'terminated'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    CHECK (
        (employee_status = 'terminated' AND terminated_on IS NOT NULL AND terminated_on >= hired_on)
        OR (employee_status <> 'terminated' AND terminated_on IS NULL)
    ),
    UNIQUE (scope_key, employee_code)
);
CREATE TABLE erpx_people_payroll_write.leave_request_guards (
    leave_request_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    employee_id UUID NOT NULL,
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    leave_status TEXT NOT NULL CHECK (leave_status IN (
        'draft', 'submitted', 'approved', 'rejected', 'cancelled', 'taken'
    )),
    requested_by TEXT NOT NULL,
    approved_by TEXT,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (ends_on >= starts_on),
    CHECK (updated_at >= created_at),
    CHECK (approved_by IS NULL OR approved_by <> requested_by),
    CHECK (leave_status NOT IN ('approved', 'taken') OR approved_by IS NOT NULL),
    FOREIGN KEY (employee_id)
        REFERENCES erpx_people_payroll_write.employee_guards (employee_id)
);
CREATE TABLE erpx_people_payroll_write.expense_claim_guards (
    expense_claim_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    employee_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    amount_minor BIGINT NOT NULL CHECK (amount_minor >= 0),
    expense_status TEXT NOT NULL CHECK (expense_status IN (
        'draft', 'submitted', 'approved', 'rejected', 'reimbursed'
    )),
    claimed_by TEXT NOT NULL,
    approved_by TEXT,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    CHECK (approved_by IS NULL OR approved_by <> claimed_by),
    CHECK (expense_status NOT IN ('approved', 'reimbursed') OR approved_by IS NOT NULL),
    FOREIGN KEY (employee_id)
        REFERENCES erpx_people_payroll_write.employee_guards (employee_id)
);
CREATE TABLE erpx_people_payroll_write.payroll_run_guards (
    payroll_run_id UUID PRIMARY KEY,
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
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (ends_on >= starts_on),
    CHECK (total_gross_minor - total_deductions_minor = total_net_minor),
    CHECK (updated_at >= created_at),
    CHECK (reviewed_by IS NULL OR reviewed_by <> prepared_by),
    CHECK (approved_by IS NULL OR (
        approved_by <> prepared_by AND approved_by IS DISTINCT FROM reviewed_by
    )),
    CHECK (payroll_status <> 'approved' OR reviewed_by IS NOT NULL),
    CHECK (payroll_status <> 'posted' OR (
        reviewed_by IS NOT NULL AND approved_by IS NOT NULL
    )),
    UNIQUE (scope_key, period_code)
);
CREATE TABLE erpx_people_payroll_write.payroll_line_guards (
    payroll_run_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    gross_minor BIGINT NOT NULL CHECK (gross_minor >= 0),
    deductions_minor BIGINT NOT NULL CHECK (deductions_minor >= 0),
    net_minor BIGINT NOT NULL CHECK (net_minor >= 0),
    CHECK (gross_minor - deductions_minor = net_minor),
    PRIMARY KEY (payroll_run_id, employee_id),
    FOREIGN KEY (payroll_run_id)
        REFERENCES erpx_people_payroll_write.payroll_run_guards (payroll_run_id)
        ON DELETE CASCADE,
    FOREIGN KEY (employee_id)
        REFERENCES erpx_people_payroll_write.employee_guards (employee_id)
);
CREATE INDEX people_payroll_employee_scope_status
    ON erpx_people_payroll_write.employee_guards
        (scope_key, employee_status, updated_at DESC, employee_id DESC);
CREATE INDEX people_payroll_leave_scope_employee_status
    ON erpx_people_payroll_write.leave_request_guards
        (scope_key, employee_id, leave_status, updated_at DESC, leave_request_id DESC);
CREATE INDEX people_payroll_expense_scope_employee_status
    ON erpx_people_payroll_write.expense_claim_guards
        (scope_key, employee_id, expense_status, updated_at DESC, expense_claim_id DESC);
CREATE INDEX people_payroll_run_scope_status
    ON erpx_people_payroll_write.payroll_run_guards
        (scope_key, payroll_status, ends_on DESC, payroll_run_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_pos_write;
CREATE TABLE erpx_pos_write.sale_guards (
    sale_id UUID PRIMARY KEY,
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
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (subtotal_minor + tax_minor = total_minor),
    CHECK (paid_minor <= total_minor),
    CHECK (sale_status <> 'completed' OR paid_minor = total_minor),
    CHECK (updated_at >= created_at),
    UNIQUE (scope_key, receipt_code)
);
CREATE TABLE erpx_pos_write.sale_line_guards (
    sale_id UUID NOT NULL,
    line_id UUID NOT NULL,
    item_reference TEXT NOT NULL,
    description TEXT NOT NULL,
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    unit_price_minor BIGINT NOT NULL CHECK (unit_price_minor >= 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    PRIMARY KEY (sale_id, line_id),
    FOREIGN KEY (sale_id) REFERENCES erpx_pos_write.sale_guards (sale_id) ON DELETE CASCADE
);
CREATE TABLE erpx_pos_write.payment_guards (
    sale_id UUID NOT NULL,
    payment_id UUID NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    payment_method TEXT NOT NULL CHECK (payment_method IN (
        'cash', 'card', 'bank_transfer', 'mobile_wallet', 'voucher', 'other'
    )),
    received_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (sale_id, payment_id),
    FOREIGN KEY (sale_id) REFERENCES erpx_pos_write.sale_guards (sale_id) ON DELETE CASCADE
);
CREATE TABLE erpx_pos_write.refund_guards (
    refund_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    sale_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    total_amount_minor BIGINT NOT NULL CHECK (total_amount_minor > 0),
    reason TEXT NOT NULL,
    requester_principal_ref TEXT NOT NULL,
    reviewer_principal_ref TEXT,
    refund_status TEXT NOT NULL CHECK (refund_status IN (
        'requested', 'approved', 'rejected', 'posted', 'cancelled'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    requested_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= requested_at),
    CHECK (reviewer_principal_ref IS NULL OR reviewer_principal_ref <> requester_principal_ref),
    CHECK (refund_status NOT IN ('approved', 'posted') OR reviewer_principal_ref IS NOT NULL),
    FOREIGN KEY (sale_id) REFERENCES erpx_pos_write.sale_guards (sale_id)
);
CREATE TABLE erpx_pos_write.refund_line_guards (
    refund_id UUID NOT NULL,
    sale_line_id UUID NOT NULL,
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    PRIMARY KEY (refund_id, sale_line_id),
    FOREIGN KEY (refund_id) REFERENCES erpx_pos_write.refund_guards (refund_id) ON DELETE CASCADE
);
CREATE INDEX pos_sale_scope_register_status
    ON erpx_pos_write.sale_guards
        (scope_key, register_id, sale_status, updated_at DESC, sale_id DESC);
CREATE INDEX pos_refund_scope_sale_status
    ON erpx_pos_write.refund_guards
        (scope_key, sale_id, refund_status, updated_at DESC, refund_id DESC);


-- Wave 8 dependent-context guards. Migration 0081 promotes the Finance and
-- Legal roots plus typed children to current command authority. Idempotency
-- receipts, audit rows, and typed outbox facts remain atomic in erpx_cqrs_write.

CREATE SCHEMA IF NOT EXISTS erpx_finance_write;

CREATE TABLE erpx_finance_write.chart_guards (
    chart_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    chart_code TEXT NOT NULL,
    chart_name TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    UNIQUE (scope_key, chart_code)
);

CREATE TABLE erpx_finance_write.account_guards (
    chart_id UUID NOT NULL,
    account_id UUID NOT NULL,
    account_code TEXT NOT NULL,
    account_name TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN (
        'asset', 'liability', 'equity', 'revenue', 'expense'
    )),
    normal_balance TEXT NOT NULL CHECK (normal_balance IN ('debit', 'credit')),
    parent_account_id UUID,
    account_status TEXT NOT NULL CHECK (account_status IN ('active', 'inactive')),
    PRIMARY KEY (chart_id, account_id),
    UNIQUE (chart_id, account_code),
    FOREIGN KEY (chart_id)
        REFERENCES erpx_finance_write.chart_guards (chart_id) ON DELETE CASCADE,
    FOREIGN KEY (chart_id, parent_account_id)
        REFERENCES erpx_finance_write.account_guards (chart_id, account_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (parent_account_id IS NULL OR parent_account_id <> account_id)
);

CREATE TABLE erpx_finance_write.period_guards (
    period_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    period_code TEXT NOT NULL,
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    period_status TEXT NOT NULL CHECK (period_status IN ('open', 'closing', 'closed')),
    close_requested_by TEXT,
    closed_by TEXT,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (ends_on >= starts_on),
    CHECK (updated_at >= created_at),
    CHECK (close_requested_by IS NULL OR close_requested_by <> closed_by),
    CHECK (
        (period_status = 'open' AND close_requested_by IS NULL AND closed_by IS NULL)
        OR (period_status = 'closing' AND close_requested_by IS NOT NULL AND closed_by IS NULL)
        OR (period_status = 'closed' AND close_requested_by IS NOT NULL AND closed_by IS NOT NULL)
    ),
    UNIQUE (scope_key, period_code)
);

CREATE TABLE erpx_finance_write.journal_guards (
    journal_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    chart_id UUID NOT NULL REFERENCES erpx_finance_write.chart_guards (chart_id),
    period_id UUID NOT NULL REFERENCES erpx_finance_write.period_guards (period_id),
    journal_code TEXT NOT NULL,
    entry_date DATE NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    source_kind TEXT NOT NULL CHECK (source_kind IN (
        'sales_order', 'inventory_movement', 'payroll', 'pos_sale',
        'pos_refund', 'bank_statement', 'tax_adjustment', 'manual'
    )),
    source_reference TEXT NOT NULL,
    description TEXT NOT NULL,
    debit_total_minor BIGINT NOT NULL CHECK (debit_total_minor > 0),
    credit_total_minor BIGINT NOT NULL CHECK (credit_total_minor > 0),
    journal_status TEXT NOT NULL CHECK (journal_status IN ('draft', 'posted')),
    posted_at TIMESTAMPTZ,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (debit_total_minor = credit_total_minor),
    CHECK ((journal_status = 'posted') = (posted_at IS NOT NULL)),
    CHECK (updated_at >= created_at),
    UNIQUE (scope_key, journal_code),
    UNIQUE (journal_id, chart_id)
);

CREATE TABLE erpx_finance_write.journal_line_guards (
    journal_id UUID NOT NULL REFERENCES erpx_finance_write.journal_guards (journal_id)
        ON DELETE CASCADE,
    chart_id UUID NOT NULL,
    line_id UUID NOT NULL,
    account_id UUID NOT NULL,
    debit_minor BIGINT NOT NULL CHECK (debit_minor >= 0),
    credit_minor BIGINT NOT NULL CHECK (credit_minor >= 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    memo TEXT NOT NULL,
    PRIMARY KEY (journal_id, line_id),
    FOREIGN KEY (journal_id, chart_id)
        REFERENCES erpx_finance_write.journal_guards (journal_id, chart_id)
        ON DELETE CASCADE,
    FOREIGN KEY (chart_id, account_id)
        REFERENCES erpx_finance_write.account_guards (chart_id, account_id),
    CHECK ((debit_minor > 0) <> (credit_minor > 0))
);

CREATE TABLE erpx_finance_write.bank_transaction_facts (
    journal_id UUID NOT NULL REFERENCES erpx_finance_write.journal_guards (journal_id),
    import_id UUID NOT NULL,
    transaction_id UUID NOT NULL,
    reference TEXT NOT NULL,
    posted_on DATE NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor <> 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    PRIMARY KEY (journal_id, transaction_id),
    UNIQUE (journal_id),
    UNIQUE (import_id, transaction_id)
);

CREATE TABLE erpx_finance_write.statutory_tax_facts (
    journal_id UUID NOT NULL REFERENCES erpx_finance_write.journal_guards (journal_id),
    tax_fact_id UUID PRIMARY KEY,
    reference TEXT NOT NULL,
    tax_date DATE NOT NULL,
    tax_period TEXT NOT NULL,
    tax_kind TEXT NOT NULL CHECK (tax_kind IN ('output_vat', 'input_vat', 'withholding')),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    base_minor BIGINT NOT NULL CHECK (base_minor >= 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    tax_rate_basis_points INTEGER NOT NULL CHECK (
        tax_rate_basis_points >= 0 AND tax_rate_basis_points <= 10000
    ),
    tax_identifier TEXT NOT NULL,
    UNIQUE (journal_id)
);

CREATE OR REPLACE FUNCTION erpx_finance_write.reject_source_fact_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Finance source facts are append-only';
END
$$;
CREATE TRIGGER finance_bank_transaction_fact_immutable
    BEFORE UPDATE OR DELETE ON erpx_finance_write.bank_transaction_facts
    FOR EACH ROW EXECUTE FUNCTION erpx_finance_write.reject_source_fact_mutation();
CREATE TRIGGER finance_statutory_tax_fact_immutable
    BEFORE UPDATE OR DELETE ON erpx_finance_write.statutory_tax_facts
    FOR EACH ROW EXECUTE FUNCTION erpx_finance_write.reject_source_fact_mutation();

CREATE TABLE erpx_finance_write.subledger_guards (
    subledger_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    subledger_kind TEXT NOT NULL CHECK (subledger_kind IN ('receivable', 'payable')),
    party_id UUID NOT NULL,
    document_code TEXT NOT NULL,
    source_reference TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    original_minor BIGINT NOT NULL CHECK (original_minor > 0),
    outstanding_minor BIGINT NOT NULL CHECK (
        outstanding_minor >= 0 AND outstanding_minor <= original_minor
    ),
    subledger_status TEXT NOT NULL CHECK (subledger_status IN ('open', 'partially_settled', 'settled')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    CHECK (
        (subledger_status = 'open' AND outstanding_minor = original_minor)
        OR (subledger_status = 'partially_settled'
            AND outstanding_minor > 0 AND outstanding_minor < original_minor)
        OR (subledger_status = 'settled' AND outstanding_minor = 0)
    ),
    UNIQUE (scope_key, subledger_kind, source_reference)
);

CREATE INDEX finance_chart_scope_updated
    ON erpx_finance_write.chart_guards (scope_key, updated_at DESC, chart_id DESC);
CREATE INDEX finance_period_scope_status_dates
    ON erpx_finance_write.period_guards
        (scope_key, period_status, ends_on DESC, period_id DESC);
CREATE INDEX finance_journal_scope_period_status
    ON erpx_finance_write.journal_guards
        (scope_key, period_id, journal_status, entry_date DESC, journal_id DESC);
CREATE INDEX finance_subledger_scope_status
    ON erpx_finance_write.subledger_guards
        (scope_key, subledger_kind, subledger_status, updated_at DESC, subledger_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_legal_write;

CREATE TABLE erpx_legal_write.contract_guards (
    contract_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    reference TEXT NOT NULL CHECK (octet_length(reference) BETWEEN 1 AND 255),
    title TEXT NOT NULL CHECK (octet_length(title) BETWEEN 1 AND 255),
    object_ref TEXT NOT NULL CHECK (octet_length(object_ref) BETWEEN 1 AND 255),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^(sha256:)?[0-9A-Fa-f]{64}$'),
    retention_until TIMESTAMPTZ NOT NULL,
    contract_status TEXT NOT NULL CHECK (contract_status IN (
        'draft', 'review', 'approved', 'active', 'expired', 'terminated', 'archived'
    )),
    created_by TEXT NOT NULL CHECK (octet_length(created_by) BETWEEN 1 AND 255),
    approved_by TEXT CHECK (approved_by IS NULL OR octet_length(approved_by) BETWEEN 1 AND 255),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    CHECK (retention_until >= created_at),
    CHECK (retention_until <= created_at + INTERVAL '36525 days'),
    CHECK (approved_by IS NULL OR approved_by <> created_by),
    CHECK (
        contract_status NOT IN ('approved', 'active', 'expired', 'terminated', 'archived')
        OR approved_by IS NOT NULL
    ),
    UNIQUE (scope_key, reference)
);

CREATE TABLE erpx_legal_write.contract_party_guards (
    contract_id UUID NOT NULL REFERENCES erpx_legal_write.contract_guards (contract_id)
        ON DELETE CASCADE,
    party_id UUID NOT NULL,
    role TEXT NOT NULL CHECK (octet_length(role) BETWEEN 1 AND 255),
    PRIMARY KEY (contract_id, party_id)
);

CREATE TABLE erpx_legal_write.document_guards (
    document_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    title TEXT NOT NULL CHECK (octet_length(title) BETWEEN 1 AND 255),
    current_document_version BIGINT NOT NULL CHECK (
        current_document_version >= 0 AND current_document_version <= 1024
    ),
    archived BOOLEAN NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    CHECK (NOT archived OR current_document_version > 0)
);

CREATE TABLE erpx_legal_write.document_version_guards (
    document_id UUID NOT NULL REFERENCES erpx_legal_write.document_guards (document_id)
        ON DELETE CASCADE,
    document_version BIGINT NOT NULL CHECK (
        document_version > 0 AND document_version <= 1024
    ),
    object_ref TEXT NOT NULL CHECK (octet_length(object_ref) BETWEEN 1 AND 255),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^(sha256:)?[0-9A-Fa-f]{64}$'),
    created_at TIMESTAMPTZ NOT NULL,
    created_by TEXT NOT NULL CHECK (octet_length(created_by) BETWEEN 1 AND 255),
    PRIMARY KEY (document_id, document_version)
);

CREATE TABLE erpx_legal_write.access_entry_guards (
    access_entry_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    document_id UUID NOT NULL,
    document_version BIGINT NOT NULL,
    actor_ref TEXT NOT NULL CHECK (octet_length(actor_ref) BETWEEN 1 AND 255),
    action TEXT NOT NULL CHECK (octet_length(action) BETWEEN 1 AND 255),
    purpose TEXT NOT NULL CHECK (octet_length(purpose) BETWEEN 1 AND 512),
    observed_at TIMESTAMPTZ NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version = 1),
    FOREIGN KEY (document_id, document_version)
        REFERENCES erpx_legal_write.document_version_guards (document_id, document_version)
);

CREATE OR REPLACE FUNCTION erpx_legal_write.reject_append_only_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Legal evidence is append-only';
END
$$;
CREATE TRIGGER legal_document_version_immutable
    BEFORE UPDATE OR DELETE ON erpx_legal_write.document_version_guards
    FOR EACH ROW EXECUTE FUNCTION erpx_legal_write.reject_append_only_mutation();
CREATE TRIGGER legal_access_entry_immutable
    BEFORE UPDATE OR DELETE ON erpx_legal_write.access_entry_guards
    FOR EACH ROW EXECUTE FUNCTION erpx_legal_write.reject_append_only_mutation();

CREATE INDEX legal_contract_scope_status
    ON erpx_legal_write.contract_guards
        (scope_key, contract_status, updated_at DESC, contract_id DESC);
CREATE INDEX legal_document_scope_archived
    ON erpx_legal_write.document_guards
        (scope_key, archived, updated_at DESC, document_id DESC);
CREATE INDEX legal_access_scope_document_time
    ON erpx_legal_write.access_entry_guards
        (scope_key, document_id, observed_at DESC, access_entry_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_support_write;

CREATE TABLE erpx_support_write.ticket_guards (
    ticket_id UUID PRIMARY KEY,
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
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    UNIQUE (scope_key, case_code)
);

CREATE TABLE erpx_support_write.ticket_message_guards (
    ticket_id UUID NOT NULL,
    message_id UUID NOT NULL,
    author_ref TEXT NOT NULL,
    audience TEXT NOT NULL CHECK (audience IN ('customer_visible', 'internal')),
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (ticket_id, message_id),
    FOREIGN KEY (ticket_id)
        REFERENCES erpx_support_write.ticket_guards (ticket_id) ON DELETE CASCADE
);

CREATE TABLE erpx_support_write.incident_guards (
    incident_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    incident_code TEXT NOT NULL,
    severity TEXT NOT NULL,
    commander_ref TEXT NOT NULL,
    affected_services JSONB NOT NULL,
    affected_customer_accounts JSONB NOT NULL,
    timeline JSONB NOT NULL,
    broadcasts JSONB NOT NULL,
    incident_status TEXT NOT NULL CHECK (incident_status IN (
        'investigating', 'identified', 'monitoring', 'resolved', 'postmortem_complete'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (jsonb_typeof(affected_services) = 'array' AND jsonb_array_length(affected_services) > 0),
    CHECK (jsonb_typeof(affected_customer_accounts) = 'array'),
    CHECK (jsonb_typeof(timeline) = 'array'),
    CHECK (jsonb_typeof(broadcasts) = 'array'),
    CHECK (updated_at >= created_at),
    UNIQUE (scope_key, incident_code)
);

CREATE TABLE erpx_support_write.session_guards (
    session_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    ticket_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    agent_principal_id UUID NOT NULL,
    represented_principal_id UUID,
    support_mode TEXT NOT NULL CHECK (support_mode IN (
        'diagnostic_read', 'holding_mutation', 'impersonation'
    )),
    permissions JSONB NOT NULL,
    module TEXT NOT NULL,
    reason TEXT NOT NULL,
    session_status TEXT NOT NULL CHECK (session_status IN ('requested', 'approved', 'revoked')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    requested_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    approved_by TEXT,
    approved_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    revoked_reason TEXT,
    CHECK (jsonb_typeof(permissions) = 'array' AND jsonb_array_length(permissions) > 0),
    CHECK (expires_at > requested_at),
    CHECK ((support_mode = 'impersonation') = (represented_principal_id IS NOT NULL)),
    CHECK ((approved_by IS NULL) = (approved_at IS NULL)),
    CHECK (approved_by IS NULL OR approved_by <> agent_principal_id::TEXT),
    CHECK ((revoked_at IS NULL) = (revoked_reason IS NULL)),
    CHECK (
        (session_status = 'requested'
            AND approved_at IS NULL AND revoked_at IS NULL)
        OR (session_status = 'approved'
            AND approved_at IS NOT NULL AND revoked_at IS NULL)
        OR (session_status = 'revoked' AND revoked_at IS NOT NULL)
    ),
    FOREIGN KEY (ticket_id) REFERENCES erpx_support_write.ticket_guards (ticket_id),
    UNIQUE (
        session_id, ticket_id, customer_id, customer_account_id,
        agent_principal_id, scope_key, support_mode, module
    )
);

CREATE TABLE erpx_support_write.capability_guards (
    capability_id UUID PRIMARY KEY,
    session_id UUID NOT NULL,
    ticket_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    agent_principal_id UUID NOT NULL,
    device_ref TEXT NOT NULL,
    support_mode TEXT NOT NULL CHECK (support_mode IN (
        'diagnostic_read', 'holding_mutation', 'impersonation'
    )),
    permissions JSONB NOT NULL,
    approved_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    module TEXT NOT NULL,
    secret_hash_sha256 BYTEA NOT NULL CHECK (octet_length(secret_hash_sha256) = 32),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revocation_reason TEXT CHECK (revocation_reason IN (
        'customer_requested', 'agent_disabled', 'device_revoked',
        'session_revoked', 'ticket_closed', 'administrative'
    )),
    revoked_at TIMESTAMPTZ,
    CHECK (expires_at > issued_at),
    CHECK (jsonb_typeof(permissions) = 'array' AND jsonb_array_length(permissions) > 0),
    CHECK (jsonb_typeof(approved_scope) = 'object'),
    CHECK ((revocation_reason IS NULL) = (revoked_at IS NULL)),
    FOREIGN KEY (ticket_id) REFERENCES erpx_support_write.ticket_guards (ticket_id),
    FOREIGN KEY (
        session_id, ticket_id, customer_id, customer_account_id,
        agent_principal_id, scope_key, support_mode, module
    ) REFERENCES erpx_support_write.session_guards (
        session_id, ticket_id, customer_id, customer_account_id,
        agent_principal_id, scope_key, support_mode, module
    )
);

CREATE INDEX support_ticket_scope_customer_status
    ON erpx_support_write.ticket_guards
        (scope_key, customer_id, ticket_status, updated_at DESC, ticket_id DESC);
CREATE INDEX support_incident_scope_status
    ON erpx_support_write.incident_guards
        (scope_key, incident_status, updated_at DESC, incident_id DESC);
CREATE INDEX support_session_scope_ticket_status
    ON erpx_support_write.session_guards
        (scope_key, ticket_id, session_status, expires_at DESC, session_id DESC);
CREATE INDEX support_capability_scope_session_expiry
    ON erpx_support_write.capability_guards
        (scope_key, session_id, expires_at DESC, capability_id DESC);


-- Wave 9 searchable Operations guards. Migration 0081 promotes exception,
-- work-queue policy and all source-fact variants to typed relational command
-- authority; receipts, audit and outbox facts remain atomic.

CREATE SCHEMA IF NOT EXISTS erpx_operations_write;

CREATE TABLE erpx_operations_write.exception_guards (
    exception_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    reference TEXT NOT NULL,
    holding_scope JSONB NOT NULL,
    category TEXT NOT NULL,
    severity TEXT NOT NULL,
    source_context TEXT NOT NULL CHECK (source_context IN (
        'commercial', 'customer', 'marketing', 'crm', 'procurement', 'inventory',
        'assets', 'maintenance', 'quality', 'sales', 'people_payroll', 'pos',
        'finance', 'support'
    )),
    source_event_id UUID NOT NULL,
    source_event_type TEXT NOT NULL,
    source_event_version INTEGER NOT NULL CHECK (source_event_version > 0),
    source_partition_key TEXT NOT NULL,
    source_aggregate_type TEXT NOT NULL,
    source_aggregate_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    source_correlation_id UUID NOT NULL,
    policy_ref TEXT NOT NULL,
    assignee_id TEXT NOT NULL,
    manager_id TEXT NOT NULL,
    assignment_authority_ref TEXT NOT NULL,
    assignment_valid_from TIMESTAMPTZ NOT NULL,
    assignment_valid_until TIMESTAMPTZ NOT NULL,
    due_at TIMESTAMPTZ NOT NULL,
    escalation_after_minutes INTEGER NOT NULL CHECK (
        escalation_after_minutes BETWEEN 1 AND 10080
    ),
    exception_status TEXT NOT NULL CHECK (exception_status IN (
        'assigned', 'acknowledged', 'escalated', 'resolved', 'closed'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    CHECK (assignment_valid_until > assignment_valid_from),
    CHECK (assignee_id <> manager_id),
    UNIQUE (scope_key, reference),
    UNIQUE (source_context, source_event_id),
    UNIQUE (exception_id, source_context, source_event_id)
);

CREATE INDEX operations_exception_scope_status
    ON erpx_operations_write.exception_guards
        (scope_key, exception_status, updated_at DESC, exception_id DESC);
CREATE INDEX operations_exception_source_aggregate
    ON erpx_operations_write.exception_guards
        (scope_key, source_context, source_aggregate_id);

CREATE TABLE erpx_operations_write.source_receipts (
    source_event_id UUID PRIMARY KEY,
    source_context TEXT NOT NULL,
    source_fingerprint JSONB NOT NULL,
    disposition TEXT NOT NULL CHECK (disposition IN ('ignored', 'detected')),
    exception_id UUID,
    applied_at TIMESTAMPTZ NOT NULL,
    CHECK ((disposition = 'ignored') = (exception_id IS NULL)),
    FOREIGN KEY (exception_id, source_context, source_event_id)
        REFERENCES erpx_operations_write.exception_guards
            (exception_id, source_context, source_event_id)
);

CREATE UNIQUE INDEX operations_source_receipt_identity
    ON erpx_operations_write.source_receipts (source_context, source_event_id);

CREATE OR REPLACE FUNCTION erpx_operations_write.reject_source_receipt_mutation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'Operations source receipts are immutable';
END
$$;

CREATE TRIGGER operations_source_receipt_immutable
    BEFORE UPDATE OR DELETE ON erpx_operations_write.source_receipts
    FOR EACH ROW EXECUTE FUNCTION erpx_operations_write.reject_source_receipt_mutation();

CREATE TABLE erpx_operations_write.source_ingestion_checkpoints (
    source_database TEXT NOT NULL CHECK (source_database IN ('control', 'holding')),
    source_key TEXT NOT NULL CHECK (octet_length(source_key) BETWEEN 1 AND 255),
    source_partition TEXT NOT NULL,
    source_sequence BIGINT NOT NULL DEFAULT 0 CHECK (source_sequence >= 0),
    source_event_id UUID,
    source_occurred_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (source_database, source_key, source_partition),
    CHECK ((source_sequence = 0) = (source_event_id IS NULL))
);

CREATE FUNCTION erpx_operations_write.valid_source_context_array(values_to_check JSONB)
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

CREATE TABLE erpx_operations_write.work_queue_policy_guards (
    policy_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    holding_scope JSONB NOT NULL,
    policy_name TEXT NOT NULL,
    allowed_source_contexts JSONB NOT NULL CHECK (
        jsonb_typeof(allowed_source_contexts) = 'array'
        AND jsonb_array_length(allowed_source_contexts) BETWEEN 1 AND 16
        AND erpx_operations_write.valid_source_context_array(allowed_source_contexts)
    ),
    escalation_after_minutes INTEGER NOT NULL CHECK (
        escalation_after_minutes BETWEEN 1 AND 10080
    ),
    policy_status TEXT NOT NULL CHECK (policy_status IN ('active', 'paused', 'retired')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    UNIQUE (scope_key, policy_name)
);

CREATE INDEX operations_policy_scope_status
    ON erpx_operations_write.work_queue_policy_guards
        (scope_key, policy_status, updated_at DESC, policy_id DESC);


-- Sales-owned mixed-revenue arrangements. Migration 0081 replaces the JSON
-- compatibility representation as authority with typed dimension, obligation,
-- and fulfillment child relations.

CREATE TABLE erpx_sales_write.revenue_arrangement_guards (
    arrangement_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    sales_order_id UUID NOT NULL UNIQUE,
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
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    FOREIGN KEY (sales_order_id)
        REFERENCES erpx_sales_write.sales_order_guards (sales_order_id)
);

CREATE INDEX sales_revenue_arrangement_scope_customer_status
    ON erpx_sales_write.revenue_arrangement_guards
        (scope_key, customer_id, arrangement_status, updated_at DESC, arrangement_id DESC);


-- Finance-owned customer invoices created from immutable Sales billing facts.
-- Migration 0081 promotes typed billing and dimension columns to authority.
-- Posting is maker-checker and customer AR is created only from the resulting
-- finance.customer_invoice_posted fact.

CREATE TABLE erpx_finance_write.customer_invoice_guards (
    invoice_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    invoice_code TEXT NOT NULL,
    customer_id UUID NOT NULL,
    billing_event_id UUID NOT NULL UNIQUE,
    source_context TEXT NOT NULL CHECK (source_context = 'sales'),
    source_aggregate_kind TEXT NOT NULL CHECK (source_aggregate_kind = 'revenue_arrangement'),
    source_aggregate_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    net_minor BIGINT NOT NULL CHECK (net_minor > 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    receivable_minor BIGINT NOT NULL CHECK (receivable_minor = net_minor + tax_minor),
    eligible_on DATE NOT NULL,
    due_on DATE NOT NULL CHECK (due_on >= eligible_on),
    dimensions JSONB NOT NULL,
    prepared_by TEXT NOT NULL,
    posted_by TEXT,
    posted_at TIMESTAMPTZ,
    invoice_status TEXT NOT NULL CHECK (invoice_status IN ('draft', 'posted', 'cancelled')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    CHECK (
        (invoice_status = 'posted' AND posted_by IS NOT NULL AND posted_at IS NOT NULL
         AND posted_by <> prepared_by)
        OR (invoice_status <> 'posted' AND posted_by IS NULL AND posted_at IS NULL)
    ),
    UNIQUE (scope_key, invoice_code)
);

CREATE INDEX finance_customer_invoice_scope_customer_status
    ON erpx_finance_write.customer_invoice_guards
        (scope_key, customer_id, invoice_status, updated_at DESC, invoice_id DESC);


-- Finance-owned IFRS/TFRS 15 schedule created from a versioned Sales revenue
-- arrangement. Migration 0081 adds typed arrangement, obligation, dimension,
-- and recognition authority children.

CREATE TABLE erpx_finance_write.revenue_schedule_guards (
    schedule_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    arrangement_id UUID NOT NULL UNIQUE,
    sales_order_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    source_context TEXT NOT NULL CHECK (source_context = 'sales'),
    source_aggregate_kind TEXT NOT NULL CHECK (source_aggregate_kind = 'revenue_arrangement'),
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    last_source_version BIGINT NOT NULL CHECK (last_source_version >= source_aggregate_version),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    allocated_minor BIGINT NOT NULL CHECK (allocated_minor > 0),
    recognized_minor BIGINT NOT NULL CHECK (
        recognized_minor >= 0 AND recognized_minor <= allocated_minor
    ),
    schedule_status TEXT NOT NULL CHECK (schedule_status IN ('active', 'completed')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    CHECK (
        (schedule_status = 'completed' AND recognized_minor = allocated_minor)
        OR (schedule_status = 'active' AND recognized_minor < allocated_minor)
    )
);

CREATE INDEX finance_revenue_schedule_scope_customer_status
    ON erpx_finance_write.revenue_schedule_guards
        (scope_key, customer_id, schedule_status, updated_at DESC, schedule_id DESC);


-- CRM-owned forecast fields remain operational projections and never become
-- Finance revenue. The group is nullable only for pre-upgrade opportunities.

ALTER TABLE erpx_crm_write.opportunity_guards
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
    ADD CONSTRAINT crm_opportunity_forecast_complete CHECK (
        (account_id IS NULL AND owner_ref IS NULL AND pipeline_category IS NULL
         AND probability_basis_points IS NULL AND expected_close_on IS NULL)
        OR
        (account_id IS NOT NULL AND owner_ref IS NOT NULL AND pipeline_category IS NOT NULL
         AND probability_basis_points IS NOT NULL AND expected_close_on IS NOT NULL)
    ),
    ADD CONSTRAINT crm_opportunity_closed_forecast CHECK (
        (opportunity_stage IN ('won', 'lost')) = (pipeline_category = 'closed')
        OR pipeline_category IS NULL
    );

CREATE INDEX crm_opportunity_scope_forecast
    ON erpx_crm_write.opportunity_guards
        (scope_key, pipeline_category, expected_close_on, opportunity_id);


-- Legal owns the typed, versioned relationship graph. Migration 0081 promotes
-- exact scope and legal-hold columns to relational command authority; the JSON
-- columns below remain compatibility evidence only.

CREATE TABLE erpx_legal_write.matter_guards (
    matter_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    reference TEXT NOT NULL CHECK (octet_length(reference) BETWEEN 1 AND 255),
    title TEXT NOT NULL CHECK (octet_length(title) BETWEEN 1 AND 255),
    matter_status TEXT NOT NULL CHECK (matter_status IN ('open', 'closed')),
    legal_hold JSONB NOT NULL,
    created_by TEXT NOT NULL CHECK (octet_length(created_by) BETWEEN 1 AND 255),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    CHECK (updated_at >= created_at),
    UNIQUE (scope_key, reference)
);

CREATE TABLE erpx_legal_write.matter_link_guards (
    matter_id UUID NOT NULL REFERENCES erpx_legal_write.matter_guards (matter_id),
    link_ordinal BIGINT NOT NULL CHECK (link_ordinal BETWEEN 1 AND 1024),
    relation TEXT NOT NULL CHECK (relation IN (
        'governed_by', 'supports', 'billed_under', 'attributed_to',
        'disputes', 'settlement_for', 'supersedes', 'accounting_for'
    )),
    target_context TEXT NOT NULL CHECK (octet_length(target_context) BETWEEN 1 AND 64),
    target_aggregate_kind TEXT NOT NULL CHECK (
        octet_length(target_aggregate_kind) BETWEEN 1 AND 128
    ),
    target_aggregate_id UUID NOT NULL,
    target_aggregate_version BIGINT NOT NULL CHECK (target_aggregate_version > 0),
    target_scope JSONB NOT NULL,
    target_scope_key TEXT NOT NULL,
    target_digest TEXT NOT NULL CHECK (
        target_digest ~ '^(sha256:)?[0-9A-Fa-f]{64}$'
    ),
    linked_by TEXT NOT NULL CHECK (octet_length(linked_by) BETWEEN 1 AND 255),
    linked_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (matter_id, link_ordinal),
    UNIQUE (
        matter_id, relation, target_context, target_aggregate_kind,
        target_aggregate_id
    )
);

CREATE TRIGGER legal_matter_link_immutable
    BEFORE UPDATE OR DELETE ON erpx_legal_write.matter_link_guards
    FOR EACH ROW EXECUTE FUNCTION erpx_legal_write.reject_append_only_mutation();

CREATE INDEX legal_matter_scope_status
    ON erpx_legal_write.matter_guards
        (scope_key, matter_status, updated_at DESC, matter_id DESC);
CREATE INDEX legal_matter_link_target
    ON erpx_legal_write.matter_link_guards
        (target_context, target_aggregate_kind, target_aggregate_id, matter_id);


-- Marketing Campaign Budget command guards. Marketing owns planning,
-- allocation, and spend reservations; Procurement/Finance remain owners of
-- commitments, invoices, payments, and posted accounting amounts.

CREATE SCHEMA IF NOT EXISTS erpx_marketing_write;

CREATE TABLE erpx_marketing_write.campaign_budget_guards (
    budget_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL,
    campaign_id UUID NOT NULL,
    budget_code TEXT NOT NULL,
    budget_status TEXT NOT NULL CHECK (budget_status IN (
        'draft', 'review', 'approved', 'running', 'completed', 'cancelled'
    )),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    requested_minor BIGINT NOT NULL CHECK (requested_minor > 0),
    approved_minor BIGINT CHECK (approved_minor > 0),
    reserved_minor BIGINT NOT NULL CHECK (reserved_minor >= 0),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (scope_key, budget_code),
    CHECK (approved_minor IS NULL OR reserved_minor <= approved_minor)
);

CREATE INDEX marketing_budget_scope_campaign_status
    ON erpx_marketing_write.campaign_budget_guards
        (scope_key, campaign_id, budget_status, updated_at DESC, budget_id DESC);

CREATE TABLE erpx_marketing_write.campaign_spend_request_guards (
    request_id UUID PRIMARY KEY,
    budget_id UUID NOT NULL,
    allocation_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    expected_outcome TEXT NOT NULL,
    spend_status TEXT NOT NULL CHECK (spend_status IN ('active', 'released')),
    requested_at TIMESTAMPTZ NOT NULL,
    released_at TIMESTAMPTZ,
    budget_aggregate_version BIGINT NOT NULL CHECK (budget_aggregate_version > 0),
    FOREIGN KEY (budget_id)
        REFERENCES erpx_marketing_write.campaign_budget_guards (budget_id)
        ON DELETE RESTRICT,
    CHECK (
        (spend_status = 'active' AND released_at IS NULL)
        OR (spend_status = 'released' AND released_at >= requested_at)
    )
);

CREATE INDEX marketing_spend_request_budget_status
    ON erpx_marketing_write.campaign_spend_request_guards
        (budget_id, spend_status, requested_at, request_id);


CREATE TABLE erpx_marketing_write.approval_decision_checkpoints (
    consumer_name text PRIMARY KEY,
    last_global_sequence bigint NOT NULL CHECK (last_global_sequence > 0),
    last_event_id uuid NOT NULL,
    updated_at timestamptz NOT NULL,
    CONSTRAINT approval_decision_consumer_name_chk
        CHECK (consumer_name = 'marketing.operations-approval-decisions.v1')
);


-- Authoritative holding-write database generation marker.
ALTER TABLE erpx_cqrs_write.outbox_events
    ADD COLUMN claim_token UUID,
    ADD COLUMN claim_until TIMESTAMPTZ,
    ADD COLUMN dead_lettered_at TIMESTAMPTZ,
    ADD CONSTRAINT cqrs_holding_outbox_claim_consistency
        CHECK ((claim_token IS NULL) = (claim_until IS NULL)),
    ADD CONSTRAINT cqrs_holding_outbox_terminal_consistency
        CHECK (published_at IS NULL OR dead_lettered_at IS NULL);

CREATE INDEX cqrs_holding_outbox_expired_claims
    ON erpx_cqrs_write.outbox_events (claim_until, global_sequence)
    WHERE published_at IS NULL AND dead_lettered_at IS NULL;

CREATE INDEX cqrs_holding_outbox_due_v2
    ON erpx_cqrs_write.outbox_events (available_at, global_sequence)
    WHERE published_at IS NULL AND dead_lettered_at IS NULL;

CREATE SCHEMA erpx_meta;

CREATE TABLE erpx_meta.schema_manifest (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    schema_revision SMALLINT NOT NULL CHECK (schema_revision = 2),
    manifest_version SMALLINT NOT NULL CHECK (manifest_version = 1),
    database_role TEXT NOT NULL CHECK (database_role = 'holding_write'),
    migration_set TEXT NOT NULL CHECK (migration_set = 'holding-write'),
    installed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO erpx_meta.schema_manifest
    (singleton, schema_revision, manifest_version, database_role, migration_set)
VALUES (TRUE, 2, 1, 'holding_write', 'holding-write');


-- Collaboration authoritative records live in the same holding data plane as
-- their first-class anchor. Cross-scope inbox rows intentionally contain only
-- opaque identifiers; no comment body or business descriptor is replicated.

CREATE SCHEMA erpx_collaboration;

CREATE TABLE erpx_collaboration.threads (
    holding_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    anchor_record_id UUID NOT NULL,
    owner_context TEXT NOT NULL CHECK (octet_length(owner_context) BETWEEN 1 AND 64),
    record_type TEXT NOT NULL CHECK (octet_length(record_type) BETWEEN 1 AND 128),
    classification TEXT NOT NULL CHECK (
        classification IN ('public', 'internal', 'confidential', 'restricted')
    ),
    thread_state TEXT NOT NULL CHECK (thread_state IN ('open', 'resolved')),
    pinned BOOLEAN NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, thread_id),
    CHECK (updated_at >= created_at)
);

CREATE INDEX collaboration_thread_anchor
    ON erpx_collaboration.threads
       (holding_id, anchor_record_id, updated_at DESC, thread_id);

-- Concern-specific bounded documents are not serialized Thread aggregates.
CREATE TABLE erpx_collaboration.thread_policies (
    holding_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    policy_document JSONB NOT NULL,
    PRIMARY KEY (holding_id, thread_id),
    FOREIGN KEY (holding_id, thread_id)
        REFERENCES erpx_collaboration.threads (holding_id, thread_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.thread_anchors (
    holding_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    anchor_document JSONB NOT NULL,
    source_revision BIGINT,
    source_digest BYTEA CHECK (source_digest IS NULL OR octet_length(source_digest) = 32),
    stale BOOLEAN NOT NULL,
    PRIMARY KEY (holding_id, thread_id),
    FOREIGN KEY (holding_id, thread_id)
        REFERENCES erpx_collaboration.threads (holding_id, thread_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.thread_audiences (
    holding_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    audience_document JSONB NOT NULL,
    audience_realm TEXT NOT NULL CHECK (audience_realm IN ('internal','external_portal')),
    external_portal TEXT CHECK (
        external_portal IN ('customer','supplier','candidate','commerce','signatory')
    ),
    PRIMARY KEY (holding_id, thread_id),
    FOREIGN KEY (holding_id, thread_id)
        REFERENCES erpx_collaboration.threads (holding_id, thread_id) ON DELETE RESTRICT,
    CHECK ((audience_realm = 'internal') = (external_portal IS NULL))
);

CREATE TABLE erpx_collaboration.thread_retention (
    holding_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    retention_document JSONB NOT NULL,
    parent_retain_until TIMESTAMPTZ NOT NULL,
    comment_retain_until TIMESTAMPTZ NOT NULL,
    jurisdiction_retain_until TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, thread_id),
    FOREIGN KEY (holding_id, thread_id)
        REFERENCES erpx_collaboration.threads (holding_id, thread_id) ON DELETE RESTRICT
);

-- Opaque locator used only to discover an anchor identity before invoking the
-- record-policy projection. It carries no title, record type, comment body,
-- audience, count, or other protected business metadata.
CREATE TABLE erpx_collaboration.thread_anchor_authority_index (
    holding_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    anchor_record_id UUID NOT NULL,
    authorization_policy_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    PRIMARY KEY (holding_id, thread_id),
    FOREIGN KEY (holding_id, thread_id)
        REFERENCES erpx_collaboration.threads (holding_id, thread_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.comments (
    holding_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    comment_id UUID NOT NULL,
    author_principal_id UUID NOT NULL,
    author_kind TEXT NOT NULL CHECK (author_kind IN ('human','agent')),
    visibility TEXT NOT NULL CHECK (
        visibility IN ('visible','quarantined','hidden','redacted')
    ),
    tombstoned_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, thread_id, comment_id),
    FOREIGN KEY (holding_id, thread_id)
        REFERENCES erpx_collaboration.threads (holding_id, thread_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.comment_revisions (
    holding_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    comment_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    author_principal_id UUID NOT NULL,
    author_kind TEXT NOT NULL CHECK (author_kind IN ('human','agent')),
    document JSONB NOT NULL,
    plain_text TEXT NOT NULL CHECK (
        octet_length(plain_text) BETWEEN 1 AND 65536
    ),
    content_digest BYTEA NOT NULL CHECK (octet_length(content_digest) = 32),
    attachments JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, thread_id, comment_id, revision),
    FOREIGN KEY (holding_id, thread_id, comment_id)
        REFERENCES erpx_collaboration.comments (holding_id, thread_id, comment_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.mention_events (
    holding_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    mention_event_id UUID NOT NULL,
    comment_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    target JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, mention_event_id),
    FOREIGN KEY (holding_id, thread_id)
        REFERENCES erpx_collaboration.threads (holding_id, thread_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, thread_id, comment_id, revision)
        REFERENCES erpx_collaboration.comment_revisions
            (holding_id, thread_id, comment_id, revision)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.mention_withdrawals (
    holding_id UUID NOT NULL,
    mention_event_id UUID NOT NULL,
    withdrawn_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, mention_event_id),
    FOREIGN KEY (holding_id, mention_event_id)
        REFERENCES erpx_collaboration.mention_events (holding_id, mention_event_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.events (
    event_cursor BIGSERIAL NOT NULL,
    holding_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_type TEXT NOT NULL CHECK (
        event_type ~ '^collaboration[.][a-z0-9_]+$'
    ),
    event_version INTEGER NOT NULL CHECK (event_version = 1),
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    correlation_id UUID NOT NULL,
    PRIMARY KEY (holding_id, event_cursor),
    FOREIGN KEY (holding_id, thread_id)
        REFERENCES erpx_collaboration.threads (holding_id, thread_id)
        ON DELETE RESTRICT
);

CREATE INDEX collaboration_event_stream
    ON erpx_collaboration.events (holding_id, thread_id, event_cursor);

CREATE TABLE erpx_collaboration.command_receipts (
    holding_id UUID NOT NULL,
    idempotency_key TEXT NOT NULL CHECK (
        octet_length(idempotency_key) BETWEEN 1 AND 256
    ),
    command_id UUID NOT NULL,
    request_digest BYTEA NOT NULL CHECK (octet_length(request_digest) = 32),
    thread_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    last_event_cursor BIGINT NOT NULL CHECK (last_event_cursor > 0),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, idempotency_key),
    UNIQUE (holding_id, command_id),
    FOREIGN KEY (holding_id, thread_id)
        REFERENCES erpx_collaboration.threads (holding_id, thread_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, last_event_cursor)
        REFERENCES erpx_collaboration.events (holding_id, event_cursor)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.opaque_mention_inbox (
    inbox_cursor BIGSERIAL NOT NULL,
    event_id UUID NOT NULL,
    holding_id UUID NOT NULL,
    recipient_principal_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    mention_event_id UUID NOT NULL,
    anchor_record_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, inbox_cursor),
    UNIQUE (holding_id, event_id),
    UNIQUE (holding_id, recipient_principal_id, mention_event_id),
    FOREIGN KEY (holding_id, thread_id)
        REFERENCES erpx_collaboration.threads (holding_id, thread_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, mention_event_id)
        REFERENCES erpx_collaboration.mention_events (holding_id, mention_event_id)
        ON DELETE RESTRICT
);

CREATE INDEX collaboration_inbox_recipient
    ON erpx_collaboration.opaque_mention_inbox
       (holding_id, recipient_principal_id, inbox_cursor);

CREATE TABLE erpx_collaboration.encrypted_drafts (
    holding_id UUID NOT NULL,
    draft_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    owner_principal_id UUID NOT NULL,
    ciphertext BYTEA NOT NULL CHECK (
        octet_length(ciphertext) BETWEEN 1 AND 262144
    ),
    key_version INTEGER NOT NULL CHECK (key_version > 0),
    expires_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, draft_id),
    UNIQUE (holding_id, thread_id, owner_principal_id),
    FOREIGN KEY (holding_id, thread_id)
        REFERENCES erpx_collaboration.threads (holding_id, thread_id)
        ON DELETE CASCADE
);

CREATE INDEX collaboration_expired_drafts
    ON erpx_collaboration.encrypted_drafts (holding_id, expires_at);

-- Only sealed object identity and digest are stored here. The encrypted
-- original resides in governed object storage and is never projected.
CREATE TABLE erpx_collaboration.moderation_evidence (
    holding_id UUID NOT NULL,
    case_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    encrypted_object_id UUID NOT NULL,
    evidence_digest BYTEA NOT NULL CHECK (octet_length(evidence_digest) = 32),
    key_version INTEGER NOT NULL CHECK (key_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, case_id),
    FOREIGN KEY (holding_id, thread_id)
        REFERENCES erpx_collaboration.threads (holding_id, thread_id)
        ON DELETE RESTRICT
);

-- Mutable lifecycle entities are normalized by identity. Their bounded
-- documents contain one entity only and never contain comments or a Thread.
CREATE TABLE erpx_collaboration.thread_entities (
    holding_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    entity_kind TEXT NOT NULL CHECK (entity_kind IN (
        'reaction','follower','relation','moderation_case','promotion','action_card'
    )),
    entity_id TEXT NOT NULL CHECK (octet_length(entity_id) BETWEEN 1 AND 255),
    entity_document JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, thread_id, entity_kind, entity_id),
    FOREIGN KEY (holding_id, thread_id)
        REFERENCES erpx_collaboration.threads (holding_id, thread_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_collaboration.audit_log (
    holding_id UUID NOT NULL,
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    thread_id UUID NOT NULL,
    actor_principal_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    action TEXT NOT NULL CHECK (octet_length(action) BETWEEN 1 AND 128),
    evidence_digest BYTEA NOT NULL CHECK (octet_length(evidence_digest) = 32),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, audit_id),
    UNIQUE (holding_id, command_id),
    FOREIGN KEY (holding_id, thread_id)
        REFERENCES erpx_collaboration.threads (holding_id, thread_id) ON DELETE RESTRICT
);

CREATE FUNCTION erpx_collaboration.reject_append_only_change()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'collaboration evidence is append-only';
END;
$$;

CREATE TRIGGER collaboration_revision_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.comment_revisions
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();
CREATE TRIGGER collaboration_mention_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.mention_events
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();
CREATE TRIGGER collaboration_withdrawal_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.mention_withdrawals
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();
CREATE TRIGGER collaboration_event_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.events
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();
CREATE TRIGGER collaboration_inbox_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.opaque_mention_inbox
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();
CREATE TRIGGER collaboration_moderation_evidence_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.moderation_evidence
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();
CREATE TRIGGER collaboration_audit_append_only
    BEFORE UPDATE OR DELETE ON erpx_collaboration.audit_log
    FOR EACH ROW EXECUTE FUNCTION erpx_collaboration.reject_append_only_change();

DO $$
DECLARE
    table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'threads', 'thread_policies', 'thread_anchors', 'thread_audiences',
        'thread_retention', 'thread_anchor_authority_index', 'comments',
        'comment_revisions', 'mention_events',
        'mention_withdrawals', 'events', 'command_receipts',
        'opaque_mention_inbox', 'encrypted_drafts', 'moderation_evidence',
        'thread_entities', 'audit_log'
    ]
    LOOP
        EXECUTE format(
            'ALTER TABLE erpx_collaboration.%I ENABLE ROW LEVEL SECURITY',
            table_name
        );
        EXECUTE format(
            'ALTER TABLE erpx_collaboration.%I FORCE ROW LEVEL SECURITY',
            table_name
        );
        EXECUTE format(
            'CREATE POLICY collaboration_holding_isolation ON erpx_collaboration.%I '
            'USING (holding_id::text = current_setting(''app.holding_id'', true)) '
            'WITH CHECK (holding_id::text = current_setting(''app.holding_id'', true))',
            table_name
        );
    END LOOP;
END;
$$;


-- Agent Orchestration owns governed agent versions, durable assignments,
-- delegation lineage, private sessions, and budget/usage evidence. It never
-- owns or directly mutates records in another business context.

CREATE SCHEMA IF NOT EXISTS erpx_agent_write;

-- Every row is bound to the holding selected by the adapter transaction. A
-- missing holding setting evaluates to NULL, so inserts and RLS checks fail
-- closed instead of falling back to an unscoped data plane.
CREATE OR REPLACE FUNCTION erpx_agent_write.current_holding_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.holding_id', true), '')::uuid
$$;

CREATE TABLE erpx_agent_write.agent_definitions (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    agent_definition_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    holding_scope JSONB NOT NULL,
    agent_principal_ref TEXT NOT NULL,
    business_owner_ref TEXT NOT NULL,
    technical_owner_ref TEXT NOT NULL,
    agent_name TEXT NOT NULL,
    definition_status TEXT NOT NULL CHECK (definition_status IN (
        'draft', 'approved', 'suspended', 'retired'
    )),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, agent_definition_id),
    UNIQUE (holding_id, scope_key, agent_principal_ref),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_agent_write.agent_versions (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    agent_version_id UUID NOT NULL,
    agent_definition_id UUID NOT NULL,
    version_number INTEGER NOT NULL CHECK (version_number > 0),
    version_status TEXT NOT NULL CHECK (version_status IN (
        'draft', 'approved', 'suspended', 'retired'
    )),
    purpose TEXT NOT NULL,
    provider_ref TEXT NOT NULL,
    model_ref TEXT NOT NULL,
    use_case TEXT NOT NULL,
    prompt_digest TEXT NOT NULL CHECK (prompt_digest ~ '^[0-9A-Fa-f]{64}$'),
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    maximum_classification TEXT NOT NULL CHECK (maximum_classification IN (
        'public', 'internal', 'confidential', 'restricted'
    )),
    security_review_ref TEXT NOT NULL,
    privacy_review_ref TEXT NOT NULL,
    max_attempts INTEGER NOT NULL CHECK (max_attempts > 0),
    timeout_seconds INTEGER NOT NULL CHECK (timeout_seconds > 0),
    approved_by TEXT,
    approved_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, agent_version_id),
    UNIQUE (holding_id, agent_definition_id, version_number),
    FOREIGN KEY (holding_id, agent_definition_id) REFERENCES
        erpx_agent_write.agent_definitions (holding_id, agent_definition_id) ON DELETE RESTRICT,
    CHECK (
        (version_status = 'approved' AND approved_by IS NOT NULL AND approved_at IS NOT NULL)
        OR version_status <> 'approved'
    )
);

CREATE TABLE erpx_agent_write.agent_version_capabilities (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    agent_version_id UUID NOT NULL,
    capability TEXT NOT NULL,
    PRIMARY KEY (holding_id, agent_version_id, capability),
    FOREIGN KEY (holding_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (holding_id, agent_version_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_version_tools (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    agent_version_id UUID NOT NULL,
    tool_ref TEXT NOT NULL,
    PRIMARY KEY (holding_id, agent_version_id, tool_ref),
    FOREIGN KEY (holding_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (holding_id, agent_version_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_version_knowledge_scopes (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    agent_version_id UUID NOT NULL,
    knowledge_scope TEXT NOT NULL,
    PRIMARY KEY (holding_id, agent_version_id, knowledge_scope),
    FOREIGN KEY (holding_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (holding_id, agent_version_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_version_organization_scopes (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    agent_version_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    holding_scope JSONB NOT NULL,
    PRIMARY KEY (holding_id, agent_version_id, scope_key),
    FOREIGN KEY (holding_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (holding_id, agent_version_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_version_evaluations (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    agent_version_id UUID NOT NULL,
    evaluation_ref TEXT NOT NULL,
    PRIMARY KEY (holding_id, agent_version_id, evaluation_ref),
    FOREIGN KEY (holding_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (holding_id, agent_version_id) ON DELETE RESTRICT
);

-- Provider policy is holding-owned governance evidence, not provider secret
-- material. A worker accepts only one current approved policy whose digest is
-- pinned by the exact approved agent version and whose freshness observation
-- is still current. Revocation remains append-only evidence.
CREATE TABLE erpx_agent_write.agent_provider_policies (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    provider_policy_id UUID NOT NULL,
    provider_ref TEXT NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    policy_status TEXT NOT NULL CHECK (policy_status IN (
        'draft', 'approved', 'suspended', 'retired'
    )),
    is_current BOOLEAN NOT NULL DEFAULT FALSE,
    external_provider BOOLEAN NOT NULL,
    dpa_ref TEXT,
    maximum_classification TEXT NOT NULL CHECK (maximum_classification IN (
        'public', 'internal', 'confidential', 'restricted'
    )),
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    effective_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    freshness_checked_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, provider_policy_id),
    UNIQUE (holding_id, provider_ref, policy_version),
    UNIQUE (holding_id, provider_ref, policy_digest),
    CHECK (expires_at > effective_at),
    CHECK (freshness_checked_at BETWEEN effective_at AND updated_at),
    CHECK (NOT external_provider OR dpa_ref IS NOT NULL)
);

CREATE UNIQUE INDEX agent_provider_one_current_policy
    ON erpx_agent_write.agent_provider_policies (holding_id, provider_ref)
    WHERE is_current AND policy_status = 'approved';

CREATE TABLE erpx_agent_write.agent_provider_policy_models (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    provider_policy_id UUID NOT NULL,
    model_ref TEXT NOT NULL,
    PRIMARY KEY (holding_id, provider_policy_id, model_ref),
    FOREIGN KEY (holding_id, provider_policy_id) REFERENCES
        erpx_agent_write.agent_provider_policies (holding_id, provider_policy_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_provider_policy_use_cases (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    provider_policy_id UUID NOT NULL,
    use_case TEXT NOT NULL,
    PRIMARY KEY (holding_id, provider_policy_id, use_case),
    FOREIGN KEY (holding_id, provider_policy_id) REFERENCES
        erpx_agent_write.agent_provider_policies (holding_id, provider_policy_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_provider_policy_residencies (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    provider_policy_id UUID NOT NULL,
    residency TEXT NOT NULL,
    PRIMARY KEY (holding_id, provider_policy_id, residency),
    FOREIGN KEY (holding_id, provider_policy_id) REFERENCES
        erpx_agent_write.agent_provider_policies (holding_id, provider_policy_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_provider_policy_evaluations (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    provider_policy_id UUID NOT NULL,
    agent_version_id UUID NOT NULL,
    evaluation_ref TEXT NOT NULL,
    decision_status TEXT NOT NULL CHECK (decision_status IN ('approved', 'revoked')),
    evidence_digest TEXT NOT NULL CHECK (evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
    decided_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, provider_policy_id, agent_version_id, evaluation_ref),
    FOREIGN KEY (holding_id, provider_policy_id) REFERENCES
        erpx_agent_write.agent_provider_policies (holding_id, provider_policy_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (holding_id, agent_version_id) ON DELETE RESTRICT,
    CHECK (expires_at > decided_at)
);

CREATE TABLE erpx_agent_write.agent_provider_policy_revocations (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    provider_policy_id UUID NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    reason_digest TEXT NOT NULL CHECK (reason_digest ~ '^[0-9A-Fa-f]{64}$'),
    revoked_by TEXT NOT NULL,
    revoked_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, provider_policy_id),
    FOREIGN KEY (holding_id, provider_policy_id) REFERENCES
        erpx_agent_write.agent_provider_policies (holding_id, provider_policy_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_material_retention_policies (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    use_case TEXT NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    policy_status TEXT NOT NULL CHECK (policy_status IN ('approved', 'suspended', 'retired')),
    ttl_seconds BIGINT NOT NULL CHECK (ttl_seconds BETWEEN 60 AND 7776000),
    legal_hold_default BOOLEAN NOT NULL DEFAULT FALSE,
    effective_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    freshness_checked_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, use_case, policy_version),
    CHECK (expires_at > effective_at),
    CHECK (freshness_checked_at BETWEEN effective_at AND updated_at)
);

CREATE UNIQUE INDEX agent_material_one_current_retention
    ON erpx_agent_write.agent_material_retention_policies (holding_id, use_case)
    WHERE policy_status = 'approved';

CREATE TABLE erpx_agent_write.agent_budget_limits (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    layer_kind TEXT NOT NULL CHECK (layer_kind IN (
        'holding', 'workspace', 'agent', 'principal', 'root_assignment'
    )),
    subject_ref TEXT NOT NULL,
    remaining_units BIGINT NOT NULL CHECK (remaining_units >= 0),
    remaining_cost_micros BIGINT NOT NULL CHECK (remaining_cost_micros >= 0),
    active_concurrency INTEGER NOT NULL CHECK (active_concurrency >= 0),
    concurrency_limit INTEGER NOT NULL CHECK (concurrency_limit > 0),
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, layer_kind, subject_ref),
    CHECK (active_concurrency <= concurrency_limit)
);

CREATE TABLE erpx_agent_write.agent_budget_reservations (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    reservation_id UUID NOT NULL,
    assignment_id UUID,
    estimated_units BIGINT NOT NULL CHECK (estimated_units > 0),
    estimated_cost_micros BIGINT NOT NULL CHECK (estimated_cost_micros >= 0),
    stop_after_units BIGINT NOT NULL CHECK (stop_after_units >= estimated_units),
    queue_priority INTEGER NOT NULL CHECK (queue_priority BETWEEN 0 AND 65535),
    reservation_status TEXT NOT NULL CHECK (reservation_status IN (
        'reserved', 'reconciled', 'over_budget', 'released', 'cancelled'
    )),
    actual_units BIGINT CHECK (actual_units >= 0),
    actual_cost_micros BIGINT CHECK (actual_cost_micros >= 0),
    reserved_at TIMESTAMPTZ NOT NULL,
    reconciled_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, reservation_id),
    UNIQUE (holding_id, assignment_id)
);

CREATE TABLE erpx_agent_write.agent_budget_reservation_layers (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    reservation_id UUID NOT NULL,
    layer_kind TEXT NOT NULL CHECK (layer_kind IN (
        'holding', 'workspace', 'agent', 'principal', 'root_assignment'
    )),
    subject_ref TEXT NOT NULL,
    reserved_units BIGINT NOT NULL CHECK (reserved_units > 0),
    reserved_cost_micros BIGINT NOT NULL CHECK (reserved_cost_micros >= 0),
    PRIMARY KEY (holding_id, reservation_id, layer_kind),
    FOREIGN KEY (holding_id, reservation_id)
        REFERENCES erpx_agent_write.agent_budget_reservations (holding_id, reservation_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, layer_kind, subject_ref)
        REFERENCES erpx_agent_write.agent_budget_limits (holding_id, layer_kind, subject_ref)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_assignments (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    assignment_id UUID NOT NULL,
    root_assignment_id UUID NOT NULL,
    parent_assignment_id UUID,
    scope_key TEXT NOT NULL,
    holding_scope JSONB NOT NULL,
    root_action_ref TEXT NOT NULL,
    root_initiating_principal_ref TEXT NOT NULL,
    root_idempotency_key TEXT NOT NULL,
    trigger_kind TEXT NOT NULL CHECK (trigger_kind IN (
        'mention', 'assign', 'run', 'compare', 'delegation'
    )),
    requested_agent_ref TEXT NOT NULL,
    agent_definition_id UUID NOT NULL,
    agent_version_id UUID NOT NULL,
    provider_ref TEXT NOT NULL,
    model_ref TEXT NOT NULL,
    prompt_digest TEXT NOT NULL CHECK (prompt_digest ~ '^[0-9A-Fa-f]{64}$'),
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    encrypted_prompt_ref TEXT NOT NULL,
    payload_manifest JSONB NOT NULL,
    assignment_status TEXT NOT NULL CHECK (assignment_status IN (
        'queued', 'running', 'waiting', 'completed', 'failed', 'cancelled'
    )),
    depth INTEGER NOT NULL CHECK (depth >= 0),
    hops INTEGER NOT NULL CHECK (hops >= 0),
    attempt INTEGER NOT NULL CHECK (attempt >= 0),
    budget_reservation_id UUID NOT NULL,
    queue_priority INTEGER NOT NULL CHECK (queue_priority BETWEEN 0 AND 65535),
    not_before TIMESTAMPTZ,
    lease_token UUID,
    lease_owner TEXT,
    lease_acquired_at TIMESTAMPTZ,
    lease_expires_at TIMESTAMPTZ,
    output_digest TEXT CHECK (output_digest IS NULL OR output_digest ~ '^[0-9A-Fa-f]{64}$'),
    encrypted_output_ref TEXT,
    failure_code TEXT,
    failure_retryable BOOLEAN,
    failure_detail_digest TEXT CHECK (
        failure_detail_digest IS NULL OR failure_detail_digest ~ '^[0-9A-Fa-f]{64}$'
    ),
    failure_failed_at TIMESTAMPTZ,
    cancellation_reason TEXT,
    actual_units BIGINT CHECK (actual_units >= 0),
    actual_cost_micros BIGINT CHECK (actual_cost_micros >= 0),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, assignment_id),
    UNIQUE (holding_id, budget_reservation_id),
    UNIQUE (holding_id, scope_key, root_action_ref, requested_agent_ref),
    FOREIGN KEY (holding_id, agent_definition_id) REFERENCES
        erpx_agent_write.agent_definitions (holding_id, agent_definition_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (holding_id, agent_version_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, budget_reservation_id) REFERENCES
        erpx_agent_write.agent_budget_reservations (holding_id, reservation_id) ON DELETE RESTRICT,
    CHECK ((parent_assignment_id IS NULL) = (assignment_id = root_assignment_id)),
    CHECK ((lease_token IS NULL) = (lease_owner IS NULL)),
    CHECK ((lease_token IS NULL) = (lease_acquired_at IS NULL)),
    CHECK ((lease_token IS NULL) = (lease_expires_at IS NULL)),
    CHECK ((assignment_status = 'running') = (lease_token IS NOT NULL)),
    CHECK ((failure_code IS NULL) = (failure_retryable IS NULL)),
    CHECK ((failure_code IS NULL) = (failure_detail_digest IS NULL)),
    CHECK ((failure_code IS NULL) = (failure_failed_at IS NULL)),
    CHECK (cancellation_reason IS NULL OR assignment_status = 'cancelled'),
    CHECK (updated_at >= created_at)
);

ALTER TABLE erpx_agent_write.agent_budget_reservations
    ADD CONSTRAINT agent_budget_assignment_fk FOREIGN KEY (holding_id, assignment_id)
        REFERENCES erpx_agent_write.agent_assignments (holding_id, assignment_id)
        DEFERRABLE INITIALLY DEFERRED;

ALTER TABLE erpx_agent_write.agent_assignments
    ADD CONSTRAINT agent_assignment_root_fk FOREIGN KEY (holding_id, root_assignment_id)
        REFERENCES erpx_agent_write.agent_assignments (holding_id, assignment_id)
        DEFERRABLE INITIALLY DEFERRED,
    ADD CONSTRAINT agent_assignment_parent_fk FOREIGN KEY (holding_id, parent_assignment_id)
        REFERENCES erpx_agent_write.agent_assignments (holding_id, assignment_id)
        ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED;

CREATE INDEX agent_assignment_due_queue
    ON erpx_agent_write.agent_assignments
        (holding_id, queue_priority DESC, not_before, created_at, assignment_id)
    WHERE assignment_status = 'queued';

CREATE INDEX agent_assignment_expired_leases
    ON erpx_agent_write.agent_assignments (holding_id, lease_expires_at, assignment_id)
    WHERE assignment_status = 'running';

CREATE TABLE erpx_agent_write.assignment_delegations (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    child_assignment_id UUID NOT NULL,
    root_assignment_id UUID NOT NULL,
    parent_assignment_id UUID NOT NULL,
    parent_agent_definition_id UUID NOT NULL,
    child_agent_definition_id UUID NOT NULL,
    reason_digest TEXT NOT NULL CHECK (reason_digest ~ '^[0-9A-Fa-f]{64}$'),
    scope_key TEXT NOT NULL,
    depth INTEGER NOT NULL CHECK (depth > 0),
    hops INTEGER NOT NULL CHECK (hops > 0),
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, child_assignment_id),
    FOREIGN KEY (holding_id, child_assignment_id) REFERENCES
        erpx_agent_write.agent_assignments (holding_id, assignment_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, root_assignment_id) REFERENCES
        erpx_agent_write.agent_assignments (holding_id, assignment_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, parent_assignment_id) REFERENCES
        erpx_agent_write.agent_assignments (holding_id, assignment_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, parent_agent_definition_id) REFERENCES
        erpx_agent_write.agent_definitions (holding_id, agent_definition_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, child_agent_definition_id) REFERENCES
        erpx_agent_write.agent_definitions (holding_id, agent_definition_id) ON DELETE RESTRICT,
    CHECK (parent_agent_definition_id <> child_agent_definition_id)
);

CREATE INDEX agent_delegation_parent
    ON erpx_agent_write.assignment_delegations
        (holding_id, root_assignment_id, parent_assignment_id, child_assignment_id);

CREATE TABLE erpx_agent_write.assignment_checkpoints (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    checkpoint_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    step INTEGER NOT NULL CHECK (step > 0),
    state_digest TEXT NOT NULL CHECK (state_digest ~ '^[0-9A-Fa-f]{64}$'),
    retry_safe BOOLEAN NOT NULL,
    lease_token UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, checkpoint_id),
    UNIQUE (holding_id, assignment_id, step),
    FOREIGN KEY (holding_id, assignment_id) REFERENCES
        erpx_agent_write.agent_assignments (holding_id, assignment_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_pools (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    agent_pool_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    holding_scope JSONB NOT NULL,
    pool_name TEXT NOT NULL,
    pool_version BIGINT NOT NULL CHECK (pool_version > 0),
    enabled BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, agent_pool_id),
    UNIQUE (holding_id, scope_key, pool_name),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_agent_write.agent_pool_members (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    agent_pool_id UUID NOT NULL,
    agent_version_id UUID NOT NULL,
    PRIMARY KEY (holding_id, agent_pool_id, agent_version_id),
    FOREIGN KEY (holding_id, agent_pool_id) REFERENCES
        erpx_agent_write.agent_pools (holding_id, agent_pool_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (holding_id, agent_version_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_agent_write.agent_pool_routing_decisions (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    routing_decision_id UUID NOT NULL,
    agent_pool_id UUID NOT NULL,
    pool_version BIGINT NOT NULL CHECK (pool_version > 0),
    root_action_ref TEXT NOT NULL,
    capability TEXT NOT NULL,
    requested_scope JSONB NOT NULL,
    candidate_version_ids UUID[] NOT NULL,
    selected_agent_version_id UUID NOT NULL,
    selected_agent_version_ids UUID[] NOT NULL,
    decision_digest TEXT NOT NULL CHECK (decision_digest ~ '^[0-9A-Fa-f]{64}$'),
    compare_requested BOOLEAN NOT NULL,
    previewed_cost_micros BIGINT CHECK (previewed_cost_micros >= 0),
    initiating_human_ref TEXT,
    decided_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, routing_decision_id),
    UNIQUE (holding_id, agent_pool_id, root_action_ref),
    FOREIGN KEY (holding_id, agent_pool_id) REFERENCES
        erpx_agent_write.agent_pools (holding_id, agent_pool_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, selected_agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (holding_id, agent_version_id) ON DELETE RESTRICT,
    CHECK (cardinality(candidate_version_ids) > 0),
    CHECK (cardinality(selected_agent_version_ids) > 0),
    CHECK (
        (compare_requested AND previewed_cost_micros > 0 AND initiating_human_ref IS NOT NULL
            AND cardinality(selected_agent_version_ids) > 1)
        OR (NOT compare_requested AND previewed_cost_micros IS NULL
            AND initiating_human_ref IS NULL AND cardinality(selected_agent_version_ids) = 1)
    )
);

CREATE TABLE erpx_agent_write.agent_sessions (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    agent_session_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    holding_scope JSONB NOT NULL,
    owner_human_ref TEXT NOT NULL,
    record_id UUID,
    descriptor_version BIGINT CHECK (descriptor_version > 0),
    source_event_position BIGINT CHECK (source_event_position > 0),
    expires_at TIMESTAMPTZ NOT NULL,
    legal_hold BOOLEAN NOT NULL DEFAULT FALSE,
    expired_at TIMESTAMPTZ,
    session_version BIGINT NOT NULL CHECK (session_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, agent_session_id),
    CHECK (expires_at > created_at),
    CHECK (updated_at >= created_at),
    CHECK ((record_id IS NULL) = (descriptor_version IS NULL)),
    CHECK ((record_id IS NULL) = (source_event_position IS NULL))
);

CREATE INDEX agent_private_session_expiry
    ON erpx_agent_write.agent_sessions (holding_id, expires_at, agent_session_id)
    WHERE expired_at IS NULL AND legal_hold = FALSE;

CREATE TABLE erpx_agent_write.agent_session_messages (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    session_message_id UUID NOT NULL,
    agent_session_id UUID NOT NULL,
    author_kind TEXT NOT NULL CHECK (author_kind IN ('human', 'agent')),
    author_ref TEXT NOT NULL,
    encrypted_payload_ref TEXT NOT NULL,
    content_digest TEXT NOT NULL CHECK (content_digest ~ '^[0-9A-Fa-f]{64}$'),
    message_version BIGINT NOT NULL CHECK (message_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    published_at TIMESTAMPTZ,
    publishing_approver_human_ref TEXT,
    publication_anchor_ref UUID,
    PRIMARY KEY (holding_id, session_message_id),
    FOREIGN KEY (holding_id, agent_session_id) REFERENCES
        erpx_agent_write.agent_sessions (holding_id, agent_session_id) ON DELETE RESTRICT,
    CHECK (
        published_at IS NULL
        OR (author_kind = 'agent' AND publishing_approver_human_ref IS NOT NULL
            AND publication_anchor_ref IS NOT NULL)
    )
);

CREATE TABLE erpx_agent_write.agent_action_cards (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    action_card_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    agent_principal_ref TEXT NOT NULL,
    owner_context TEXT NOT NULL,
    command_type TEXT NOT NULL,
    payload_digest TEXT NOT NULL CHECK (payload_digest ~ '^[0-9A-Fa-f]{64}$'),
    action_status TEXT NOT NULL CHECK (action_status IN (
        'proposed', 'rejected', 'confirmed_for_handoff'
    )),
    confirmed_by_human_ref TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    resolved_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, action_card_id),
    FOREIGN KEY (holding_id, assignment_id) REFERENCES
        erpx_agent_write.agent_assignments (holding_id, assignment_id) ON DELETE RESTRICT,
    CHECK (
        (action_status = 'confirmed_for_handoff' AND confirmed_by_human_ref IS NOT NULL)
        OR action_status <> 'confirmed_for_handoff'
    )
);

CREATE TABLE erpx_agent_write.provider_invocation_evidence (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    invocation_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    assignment_version BIGINT NOT NULL CHECK (assignment_version > 0),
    attempt INTEGER NOT NULL CHECK (attempt > 0),
    agent_definition_id UUID NOT NULL,
    agent_version_id UUID NOT NULL,
    provider_ref TEXT NOT NULL,
    model_ref TEXT NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    payload_digest TEXT NOT NULL CHECK (payload_digest ~ '^[0-9A-Fa-f]{64}$'),
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    field_refs TEXT[] NOT NULL,
    document_refs UUID[] NOT NULL,
    classification TEXT NOT NULL CHECK (classification IN (
        'public', 'internal', 'confidential', 'restricted'
    )),
    residency TEXT NOT NULL,
    output_digest TEXT CHECK (output_digest ~ '^[0-9A-Fa-f]{64}$'),
    provider_units BIGINT CHECK (provider_units > 0),
    provider_cost_micros BIGINT CHECK (provider_cost_micros >= 0),
    evidence_version BIGINT NOT NULL CHECK (evidence_version > 0),
    invoked_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    outcome TEXT CHECK (outcome IN ('completed', 'failed', 'cancelled')),
    PRIMARY KEY (holding_id, invocation_id),
    UNIQUE (holding_id, assignment_id, attempt),
    FOREIGN KEY (holding_id, assignment_id) REFERENCES
        erpx_agent_write.agent_assignments (holding_id, assignment_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, agent_definition_id) REFERENCES
        erpx_agent_write.agent_definitions (holding_id, agent_definition_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, agent_version_id) REFERENCES
        erpx_agent_write.agent_versions (holding_id, agent_version_id) ON DELETE RESTRICT,
    CHECK (
        (outcome = 'completed' AND completed_at IS NOT NULL AND output_digest IS NOT NULL
            AND provider_units IS NOT NULL AND provider_cost_micros IS NOT NULL
            AND evidence_version = 2)
        OR (outcome IN ('failed', 'cancelled') AND completed_at IS NOT NULL
            AND output_digest IS NULL AND provider_units IS NULL
            AND provider_cost_micros IS NULL AND evidence_version = 2)
        OR (outcome IS NULL AND completed_at IS NULL AND output_digest IS NULL
            AND provider_units IS NULL AND provider_cost_micros IS NULL
            AND evidence_version = 1)
    )
);

CREATE TABLE erpx_agent_write.agent_governance_records (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    record_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    holding_scope JSONB NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('delegation_tree','budget_decision')),
    subject_record_id UUID NOT NULL,
    policy_digest TEXT NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    lifecycle_state TEXT NOT NULL CHECK (lifecycle_state IN ('proposed','approved','active','completed','denied','expired','cancelled')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,record_id),
    UNIQUE (holding_id,record_kind,subject_record_id,aggregate_version)
);

CREATE TABLE erpx_agent_write.agent_command_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_agent_write.current_holding_id(),
    scope_key TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_digest TEXT NOT NULL CHECK (request_digest ~ '^[0-9A-Fa-f]{64}$'),
    aggregate_type TEXT NOT NULL CHECK (aggregate_type IN (
        'agent_definition', 'assignment', 'agent_pool', 'pool_routing',
        'agent_session', 'provider_invocation', 'delegation_tree', 'budget_decision'
    )),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, scope_key, idempotency_key)
);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'agent_definitions','agent_versions','agent_version_capabilities',
        'agent_version_tools','agent_version_knowledge_scopes',
        'agent_version_organization_scopes','agent_version_evaluations',
        'agent_provider_policies','agent_provider_policy_models',
        'agent_provider_policy_use_cases','agent_provider_policy_residencies',
        'agent_provider_policy_evaluations','agent_provider_policy_revocations',
        'agent_material_retention_policies','agent_budget_limits',
        'agent_budget_reservations','agent_budget_reservation_layers',
        'agent_assignments','assignment_delegations',
        'assignment_checkpoints','agent_pools','agent_pool_members',
        'agent_pool_routing_decisions','agent_sessions','agent_session_messages',
        'agent_action_cards','provider_invocation_evidence','agent_governance_records','agent_command_receipts'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_agent_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_agent_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY agent_holding_isolation ON erpx_agent_write.%I '
            'USING (holding_id = erpx_agent_write.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_agent_write.current_holding_id())',
            table_name
        );
    END LOOP;
END;
$$;


-- Normalized Projects write authority. JSON columns below are bounded value
-- objects (holding scope or typed references), never aggregate snapshots.
CREATE SCHEMA IF NOT EXISTS erpx_projects_write;

CREATE OR REPLACE FUNCTION erpx_projects_write.current_holding_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.holding_id', true), '')::uuid
$$;

CREATE TABLE erpx_projects_write.project_guards (
    holding_id UUID NOT NULL DEFAULT erpx_projects_write.current_holding_id(),
    project_id UUID NOT NULL, holding_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    project_code TEXT NOT NULL CHECK (octet_length(project_code) BETWEEN 1 AND 128),
    project_name TEXT NOT NULL CHECK (octet_length(project_name) BETWEEN 1 AND 255),
    project_class TEXT NOT NULL CHECK (project_class IN ('client_services','internal','capital')),
    project_status TEXT NOT NULL CHECK (project_status IN ('proposed','approved','active','paused','completed','closed','cancelled','archived')),
    policy_ref TEXT NOT NULL CHECK (octet_length(policy_ref) BETWEEN 1 AND 255),
    workflow_template_id UUID NOT NULL, workflow_version BIGINT NOT NULL CHECK (workflow_version > 0),
    owner_id UUID NOT NULL, aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, project_id), UNIQUE (holding_id, scope_key, project_code),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_projects_write.project_node_guards (
    holding_id UUID NOT NULL DEFAULT erpx_projects_write.current_holding_id(),
    project_id UUID NOT NULL, node_id UUID NOT NULL, parent_node_id UUID,
    node_kind TEXT NOT NULL CHECK (node_kind IN ('phase','epic','task','milestone','deliverable','risk','issue','decision','change_request')),
    title TEXT NOT NULL CHECK (octet_length(title) BETWEEN 1 AND 255), owner_id UUID NOT NULL,
    planned_start DATE, planned_end DATE, actual_start DATE, actual_end DATE,
    workflow_state TEXT NOT NULL CHECK (octet_length(workflow_state) BETWEEN 1 AND 64),
    core_state TEXT NOT NULL CHECK (core_state IN ('proposed','open','in_progress','waiting','resolved','closed','cancelled')),
    promotion_thread_id UUID, promotion_comment_id UUID, promotion_revision BIGINT CHECK (promotion_revision > 0),
    promotion_digest TEXT CHECK (promotion_digest ~ '^(sha256:)?[0-9A-Fa-f]{64}$'),
    node_version BIGINT NOT NULL CHECK (node_version > 0), created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, project_id, node_id),
    FOREIGN KEY (holding_id, project_id) REFERENCES erpx_projects_write.project_guards(holding_id, project_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, project_id, parent_node_id) REFERENCES erpx_projects_write.project_node_guards(holding_id, project_id, node_id) DEFERRABLE INITIALLY DEFERRED,
    CHECK (parent_node_id IS NULL OR parent_node_id <> node_id),
    CHECK (planned_end IS NULL OR planned_start IS NULL OR planned_end >= planned_start),
    CHECK (actual_end IS NULL OR actual_start IS NULL OR actual_end >= actual_start),
    CHECK ((promotion_thread_id IS NULL) = (promotion_comment_id IS NULL)),
    CHECK ((promotion_thread_id IS NULL) = (promotion_revision IS NULL)),
    CHECK ((promotion_thread_id IS NULL) = (promotion_digest IS NULL)), CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_projects_write.project_node_assignees (
    holding_id UUID NOT NULL DEFAULT erpx_projects_write.current_holding_id(), project_id UUID NOT NULL,
    node_id UUID NOT NULL, assignee_id UUID NOT NULL, PRIMARY KEY (holding_id, project_id, node_id, assignee_id),
    FOREIGN KEY (holding_id, project_id, node_id) REFERENCES erpx_projects_write.project_node_guards(holding_id, project_id, node_id) ON DELETE CASCADE
);
CREATE TABLE erpx_projects_write.project_node_attachments (
    holding_id UUID NOT NULL DEFAULT erpx_projects_write.current_holding_id(), project_id UUID NOT NULL,
    node_id UUID NOT NULL, attachment_id UUID NOT NULL, object_ref TEXT NOT NULL,
    content_digest TEXT NOT NULL CHECK (content_digest ~ '^(sha256:)?[0-9A-Fa-f]{64}$'),
    PRIMARY KEY (holding_id, project_id, node_id, attachment_id),
    FOREIGN KEY (holding_id, project_id, node_id) REFERENCES erpx_projects_write.project_node_guards(holding_id, project_id, node_id) ON DELETE CASCADE
);
CREATE TABLE erpx_projects_write.project_node_references (
    holding_id UUID NOT NULL DEFAULT erpx_projects_write.current_holding_id(), project_id UUID NOT NULL,
    node_id UUID NOT NULL, record_id UUID NOT NULL, owner_context TEXT NOT NULL, record_type TEXT NOT NULL,
    reference_document JSONB NOT NULL, PRIMARY KEY (holding_id, project_id, node_id, record_id, owner_context, record_type),
    FOREIGN KEY (holding_id, project_id, node_id) REFERENCES erpx_projects_write.project_node_guards(holding_id, project_id, node_id) ON DELETE CASCADE
);
CREATE TABLE erpx_projects_write.project_time_facts (
    holding_id UUID NOT NULL DEFAULT erpx_projects_write.current_holding_id(), project_id UUID NOT NULL,
    node_id UUID NOT NULL, fact_id UUID NOT NULL, worker_id UUID NOT NULL, work_date DATE NOT NULL,
    minutes INTEGER NOT NULL CHECK (minutes BETWEEN 1 AND 1440), source_record JSONB NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0), PRIMARY KEY (holding_id, project_id, node_id, fact_id),
    FOREIGN KEY (holding_id, project_id, node_id) REFERENCES erpx_projects_write.project_node_guards(holding_id, project_id, node_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_projects_write.project_cost_facts (
    holding_id UUID NOT NULL DEFAULT erpx_projects_write.current_holding_id(), project_id UUID NOT NULL,
    node_id UUID NOT NULL, fact_id UUID NOT NULL, amount_minor BIGINT NOT NULL CHECK (amount_minor <> 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'), source_record JSONB NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0), reversed_fact_id UUID,
    PRIMARY KEY (holding_id, project_id, node_id, fact_id),
    FOREIGN KEY (holding_id, project_id, node_id) REFERENCES erpx_projects_write.project_node_guards(holding_id, project_id, node_id) ON DELETE RESTRICT
);

CREATE OR REPLACE FUNCTION erpx_projects_write.reject_project_node_cycle()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE cycle_found BOOLEAN;
BEGIN
    IF NEW.parent_node_id IS NULL THEN RETURN NEW; END IF;
    WITH RECURSIVE ancestors(node_id) AS (
        SELECT NEW.parent_node_id UNION ALL SELECT parent.parent_node_id
        FROM erpx_projects_write.project_node_guards parent JOIN ancestors current
          ON parent.holding_id=NEW.holding_id AND parent.project_id=NEW.project_id AND parent.node_id=current.node_id
        WHERE parent.parent_node_id IS NOT NULL
    ) SELECT EXISTS (SELECT 1 FROM ancestors WHERE node_id=NEW.node_id) INTO cycle_found;
    IF cycle_found THEN RAISE EXCEPTION 'project hierarchy cycle'; END IF; RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER projects_node_cycle_guard AFTER INSERT OR UPDATE OF parent_node_id
ON erpx_projects_write.project_node_guards DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
EXECUTE FUNCTION erpx_projects_write.reject_project_node_cycle();

CREATE TABLE erpx_projects_write.workflow_template_versions (
    holding_id UUID NOT NULL DEFAULT erpx_projects_write.current_holding_id(), template_id UUID NOT NULL,
    template_version BIGINT NOT NULL CHECK (template_version > 0), holding_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    template_name TEXT NOT NULL CHECK (octet_length(template_name) BETWEEN 1 AND 255),
    template_status TEXT NOT NULL CHECK (template_status IN ('active','withdrawn')),
    created_at TIMESTAMPTZ NOT NULL, published_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, template_id, template_version), CHECK (published_at >= created_at)
);
CREATE TABLE erpx_projects_write.workflow_states (
    holding_id UUID NOT NULL DEFAULT erpx_projects_write.current_holding_id(), template_id UUID NOT NULL,
    template_version BIGINT NOT NULL, state_code TEXT NOT NULL CHECK (octet_length(state_code) BETWEEN 1 AND 64),
    core_state TEXT NOT NULL CHECK (core_state IN ('proposed','open','in_progress','waiting','resolved','closed','cancelled')),
    PRIMARY KEY (holding_id, template_id, template_version, state_code),
    FOREIGN KEY (holding_id, template_id, template_version) REFERENCES erpx_projects_write.workflow_template_versions(holding_id, template_id, template_version) ON DELETE CASCADE
);
CREATE TABLE erpx_projects_write.workflow_transitions (
    holding_id UUID NOT NULL DEFAULT erpx_projects_write.current_holding_id(), template_id UUID NOT NULL,
    template_version BIGINT NOT NULL, from_state TEXT NOT NULL, to_state TEXT NOT NULL,
    PRIMARY KEY (holding_id, template_id, template_version, from_state, to_state),
    FOREIGN KEY (holding_id, template_id, template_version, from_state) REFERENCES erpx_projects_write.workflow_states(holding_id, template_id, template_version, state_code) ON DELETE CASCADE,
    FOREIGN KEY (holding_id, template_id, template_version, to_state) REFERENCES erpx_projects_write.workflow_states(holding_id, template_id, template_version, state_code) ON DELETE CASCADE,
    CHECK (from_state <> to_state)
);

CREATE TABLE erpx_projects_write.dependency_guards (
    holding_id UUID NOT NULL DEFAULT erpx_projects_write.current_holding_id(), dependency_id UUID NOT NULL,
    terms_version BIGINT NOT NULL CHECK (terms_version > 0), source_project_id UUID NOT NULL,
    source_project_version BIGINT NOT NULL CHECK (source_project_version > 0), source_node_id UUID NOT NULL,
    source_node_version BIGINT NOT NULL CHECK (source_node_version > 0), source_scope JSONB NOT NULL, source_owner_id UUID NOT NULL,
    target_project_id UUID NOT NULL, target_project_version BIGINT NOT NULL CHECK (target_project_version > 0), target_node_id UUID NOT NULL,
    target_node_version BIGINT NOT NULL CHECK (target_node_version > 0), target_scope JSONB NOT NULL, target_owner_id UUID NOT NULL,
    dependency_kind TEXT NOT NULL CHECK (dependency_kind IN ('finish_to_start','start_to_start','finish_to_finish')),
    dependency_status TEXT NOT NULL CHECK (dependency_status IN ('proposed','accepted','rejected','revoked','cancelled')),
    proposed_by UUID NOT NULL, aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    CHECK (source_project_id <> target_project_id), CHECK (proposed_by = source_owner_id), CHECK (updated_at >= created_at),
    PRIMARY KEY (holding_id, dependency_id),
    UNIQUE (holding_id, source_project_id, source_node_id, target_project_id, target_node_id, dependency_kind)
);
CREATE TABLE erpx_projects_write.dependency_acceptances (
    holding_id UUID NOT NULL DEFAULT erpx_projects_write.current_holding_id(), dependency_id UUID NOT NULL,
    dependency_side TEXT NOT NULL CHECK (dependency_side IN ('source','target')), owner_id UUID NOT NULL,
    terms_version BIGINT NOT NULL CHECK (terms_version > 0), accepted_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, dependency_id, dependency_side),
    FOREIGN KEY (holding_id, dependency_id) REFERENCES erpx_projects_write.dependency_guards(holding_id, dependency_id) ON DELETE CASCADE
);

CREATE TABLE erpx_projects_write.handoff_guards (
    holding_id UUID NOT NULL DEFAULT erpx_projects_write.current_holding_id(), handoff_id UUID NOT NULL,
    project_id UUID NOT NULL, item_id UUID NOT NULL, holding_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    handoff_kind TEXT NOT NULL, owner_context TEXT NOT NULL CHECK (owner_context IN ('finance','legal','operations','assets','sales','procurement')),
    handoff_status TEXT NOT NULL CHECK (handoff_status IN ('proposed','approved','ready','accepted','rejected','uncertain','cancelled')),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) BETWEEN 1 AND 255), correlation_id UUID NOT NULL,
    payload_digest TEXT NOT NULL CHECK (payload_digest ~ '^(sha256:)?[0-9A-Fa-f]{64}$'), proposed_by UUID NOT NULL,
    approved_by UUID, outcome_owner_context TEXT, outcome_receipt_id TEXT, outcome_record JSONB,
    outcome_accepted BOOLEAN, outcome_reason_code TEXT, reconciled_at TIMESTAMPTZ,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, handoff_id), UNIQUE (holding_id, scope_key, owner_context, idempotency_key),
    CHECK (updated_at >= created_at), CHECK (approved_by IS NULL OR approved_by <> proposed_by),
    CHECK ((outcome_receipt_id IS NULL) = (outcome_accepted IS NULL)),
    CHECK ((outcome_receipt_id IS NULL) = (outcome_reason_code IS NULL)),
    CHECK ((outcome_receipt_id IS NULL) = (reconciled_at IS NULL))
);

CREATE INDEX projects_node_parent ON erpx_projects_write.project_node_guards(holding_id, project_id, parent_node_id, node_id);
CREATE INDEX projects_dependency_active ON erpx_projects_write.dependency_guards(holding_id, source_project_id, target_project_id) WHERE dependency_status='accepted';
CREATE INDEX projects_handoff_reconciliation ON erpx_projects_write.handoff_guards(holding_id, scope_key, handoff_status, updated_at) WHERE handoff_status IN ('ready','uncertain');

DO $$ DECLARE table_name TEXT; BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'project_guards','project_node_guards','project_node_assignees','project_node_attachments',
        'project_node_references','project_time_facts','project_cost_facts','workflow_template_versions',
        'workflow_states','workflow_transitions','dependency_guards','dependency_acceptances','handoff_guards'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_projects_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_projects_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format('CREATE POLICY projects_holding_isolation ON erpx_projects_write.%I USING (holding_id=erpx_projects_write.current_holding_id()) WITH CHECK (holding_id=erpx_projects_write.current_holding_id())', table_name);
    END LOOP;
END; $$;


CREATE SCHEMA IF NOT EXISTS erpx_manufacturing_write;
CREATE SCHEMA IF NOT EXISTS erpx_planning_write;

CREATE TABLE erpx_manufacturing_write.aggregate_versions (
    holding_id uuid NOT NULL,
    aggregate_key text NOT NULL,
    aggregate_kind text NOT NULL CHECK (aggregate_kind IN ('specification', 'order', 'routing', 'batch', 'result')),
    aggregate_id uuid NOT NULL,
    authority_scope jsonb NOT NULL,
    aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (holding_id, aggregate_key),
    UNIQUE (holding_id, aggregate_kind, aggregate_id)
);

CREATE TABLE erpx_manufacturing_write.specifications (
    holding_id uuid NOT NULL, aggregate_key text NOT NULL, specification_id uuid NOT NULL,
    manufacturing_mode text NOT NULL CHECK (manufacturing_mode IN ('discrete','process')),
    output_item jsonb NOT NULL, output_quantity jsonb NOT NULL, material_lines jsonb NOT NULL,
    routing_id uuid NOT NULL, routing_steps jsonb NOT NULL,
    lifecycle_state text NOT NULL CHECK (lifecycle_state IN ('draft','released','retired')),
    PRIMARY KEY (holding_id,specification_id), UNIQUE (holding_id,aggregate_key),
    FOREIGN KEY (holding_id,aggregate_key) REFERENCES erpx_manufacturing_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT
);

CREATE TABLE erpx_manufacturing_write.production_orders (
    holding_id uuid NOT NULL, aggregate_key text NOT NULL, order_id uuid NOT NULL,
    specification_id uuid NOT NULL, specification_version bigint NOT NULL CHECK (specification_version > 0),
    manufacturing_mode text NOT NULL CHECK (manufacturing_mode IN ('discrete','process')),
    planned_output jsonb NOT NULL, lifecycle_state text NOT NULL,
    material_balances jsonb NOT NULL, material_fact_ids jsonb NOT NULL, production_results jsonb NOT NULL,
    labor_results jsonb NOT NULL, quality_dispositions jsonb NOT NULL, cost_facts jsonb NOT NULL,
    correlation_id uuid NOT NULL,
    PRIMARY KEY (holding_id,order_id), UNIQUE (holding_id,aggregate_key),
    FOREIGN KEY (holding_id,aggregate_key) REFERENCES erpx_manufacturing_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id,specification_id) REFERENCES erpx_manufacturing_write.specifications(holding_id,specification_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_manufacturing_write.governed_records (
    holding_id uuid NOT NULL, aggregate_key text NOT NULL, record_id uuid NOT NULL,
    record_kind text NOT NULL CHECK (record_kind IN ('routing','batch','result')),
    parent_record jsonb NOT NULL, evidence_digest text NOT NULL CHECK (evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
    lifecycle_state text NOT NULL CHECK (lifecycle_state IN ('proposed','active','completed','cancelled')),
    PRIMARY KEY (holding_id,record_id), UNIQUE (holding_id,aggregate_key),
    FOREIGN KEY (holding_id,aggregate_key) REFERENCES erpx_manufacturing_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT
);

CREATE TABLE erpx_manufacturing_write.material_fact_guards (
    holding_id uuid NOT NULL,
    order_id uuid NOT NULL,
    fact_id uuid NOT NULL,
    material_line integer NOT NULL CHECK (material_line > 0),
    quantity_units bigint NOT NULL CHECK (quantity_units > 0),
    quantity_scale smallint NOT NULL CHECK (quantity_scale BETWEEN 0 AND 9),
    occurred_at timestamptz NOT NULL,
    PRIMARY KEY (holding_id, fact_id)
);

CREATE TABLE erpx_manufacturing_write.genealogy_guards (
    holding_id uuid NOT NULL,
    order_id uuid NOT NULL,
    result_fact_id uuid NOT NULL,
    output_lot_or_serial text NOT NULL CHECK (length(btrim(output_lot_or_serial)) BETWEEN 1 AND 128),
    source_lot_or_serial text NOT NULL CHECK (length(btrim(source_lot_or_serial)) BETWEEN 1 AND 128),
    PRIMARY KEY (holding_id, result_fact_id, source_lot_or_serial)
);

CREATE TABLE erpx_manufacturing_write.command_receipts (
    holding_id uuid NOT NULL,
    idempotency_key text NOT NULL,
    command_id uuid NOT NULL,
    request_digest text NOT NULL CHECK (request_digest ~ '^[0-9A-Fa-f]{64}$'),
    aggregate_keys text[] NOT NULL,
    aggregate_versions bigint[] NOT NULL,
    committed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (holding_id, idempotency_key),
    UNIQUE (holding_id, command_id)
);

CREATE TABLE erpx_manufacturing_write.audit_log (
    holding_id uuid NOT NULL, command_id uuid NOT NULL, actor_principal_id uuid NOT NULL,
    action text NOT NULL, authority_scope jsonb NOT NULL, aggregate_key text NOT NULL,
    aggregate_version bigint NOT NULL CHECK (aggregate_version > 0), correlation_id uuid NOT NULL,
    evidence_digest text NOT NULL CHECK (evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(), PRIMARY KEY (holding_id,command_id)
);

CREATE TABLE erpx_manufacturing_write.outbox (
    position bigint GENERATED ALWAYS AS IDENTITY,
    holding_id uuid NOT NULL,
    aggregate_key text NOT NULL,
    aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),
    event_type text NOT NULL,
    event_version integer NOT NULL DEFAULT 1 CHECK (event_version = 1),
    correlation_id uuid NOT NULL,
    payload jsonb NOT NULL,
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    published_at timestamptz,
    PRIMARY KEY (holding_id,position), UNIQUE (holding_id,aggregate_key,aggregate_version,event_type)
);

CREATE VIEW erpx_manufacturing_write.aggregate_rehydration WITH (security_invoker=true) AS
SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',s.specification_id,'version',v.aggregate_version,'scope',v.authority_scope,
  'mode',s.manufacturing_mode,'output_item',s.output_item,'output_quantity',s.output_quantity,
  'materials',s.material_lines,'routing_id',s.routing_id,'routing',s.routing_steps,
  'status',s.lifecycle_state,'updated_at',v.updated_at) AS aggregate_document
FROM erpx_manufacturing_write.aggregate_versions v JOIN erpx_manufacturing_write.specifications s
 ON s.holding_id=v.holding_id AND s.aggregate_key=v.aggregate_key
UNION ALL
SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',o.order_id,'version',v.aggregate_version,'scope',v.authority_scope,
  'specification',jsonb_build_object('specification_id',o.specification_id,'version',o.specification_version),
  'mode',o.manufacturing_mode,'planned_output',o.planned_output,'status',o.lifecycle_state,
  'material',o.material_balances,'material_fact_ids',o.material_fact_ids,'results',o.production_results,
  'labor',o.labor_results,'dispositions',o.quality_dispositions,'costs',o.cost_facts,
  'correlation_id',o.correlation_id,'updated_at',v.updated_at) AS aggregate_document
FROM erpx_manufacturing_write.aggregate_versions v JOIN erpx_manufacturing_write.production_orders o
 ON o.holding_id=v.holding_id AND o.aggregate_key=v.aggregate_key
UNION ALL
SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',r.record_id,'version',v.aggregate_version,'scope',v.authority_scope,
  'kind',r.record_kind,'parent',r.parent_record,'evidence_digest',r.evidence_digest,
  'status',r.lifecycle_state,'updated_at',v.updated_at) AS aggregate_document
FROM erpx_manufacturing_write.aggregate_versions v JOIN erpx_manufacturing_write.governed_records r
 ON r.holding_id=v.holding_id AND r.aggregate_key=v.aggregate_key;

CREATE FUNCTION erpx_manufacturing_write.persist_normalized(
 p_holding_id uuid,p_aggregate_key text,p_kind text,p_id uuid,p_scope jsonb,p_version bigint,p_document jsonb
) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO erpx_manufacturing_write.aggregate_versions
  (holding_id,aggregate_key,aggregate_kind,aggregate_id,authority_scope,aggregate_version,updated_at)
 VALUES (p_holding_id,p_aggregate_key,p_kind,p_id,p_scope,p_version,(p_document->>'updated_at')::timestamptz)
 ON CONFLICT (holding_id,aggregate_key) DO UPDATE SET authority_scope=EXCLUDED.authority_scope,
  aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at
 WHERE erpx_manufacturing_write.aggregate_versions.aggregate_version < EXCLUDED.aggregate_version;
 IF p_kind='specification' THEN
  INSERT INTO erpx_manufacturing_write.specifications
   (holding_id,aggregate_key,specification_id,manufacturing_mode,output_item,output_quantity,material_lines,routing_id,routing_steps,lifecycle_state)
  VALUES (p_holding_id,p_aggregate_key,p_id,p_document->>'mode',p_document->'output_item',p_document->'output_quantity',
   p_document->'materials',(p_document->>'routing_id')::uuid,p_document->'routing',p_document->>'status')
  ON CONFLICT (holding_id,specification_id) DO UPDATE SET material_lines=EXCLUDED.material_lines,
   routing_steps=EXCLUDED.routing_steps,lifecycle_state=EXCLUDED.lifecycle_state;
 ELSIF p_kind='order' THEN
  INSERT INTO erpx_manufacturing_write.production_orders
   (holding_id,aggregate_key,order_id,specification_id,specification_version,manufacturing_mode,planned_output,lifecycle_state,
    material_balances,material_fact_ids,production_results,labor_results,quality_dispositions,cost_facts,correlation_id)
  VALUES (p_holding_id,p_aggregate_key,p_id,(p_document->'specification'->>'specification_id')::uuid,
   (p_document->'specification'->>'version')::bigint,p_document->>'mode',p_document->'planned_output',p_document->>'status',
   p_document->'material',p_document->'material_fact_ids',p_document->'results',p_document->'labor',
   p_document->'dispositions',p_document->'costs',(p_document->>'correlation_id')::uuid)
  ON CONFLICT (holding_id,order_id) DO UPDATE SET lifecycle_state=EXCLUDED.lifecycle_state,
   material_balances=EXCLUDED.material_balances,material_fact_ids=EXCLUDED.material_fact_ids,
   production_results=EXCLUDED.production_results,labor_results=EXCLUDED.labor_results,
   quality_dispositions=EXCLUDED.quality_dispositions,cost_facts=EXCLUDED.cost_facts;
 ELSIF p_kind IN ('routing','batch','result') THEN
  INSERT INTO erpx_manufacturing_write.governed_records
   (holding_id,aggregate_key,record_id,record_kind,parent_record,evidence_digest,lifecycle_state)
  VALUES (p_holding_id,p_aggregate_key,p_id,p_kind,p_document->'parent',p_document->>'evidence_digest',p_document->>'status')
  ON CONFLICT (holding_id,record_id) DO UPDATE SET parent_record=EXCLUDED.parent_record,
   evidence_digest=EXCLUDED.evidence_digest,lifecycle_state=EXCLUDED.lifecycle_state;
 ELSE RAISE EXCEPTION 'unsupported manufacturing aggregate kind'; END IF;
END $$;

CREATE TABLE erpx_planning_write.aggregate_versions (
    holding_id uuid NOT NULL,
    aggregate_key text NOT NULL,
    aggregate_kind text NOT NULL CHECK (aggregate_kind IN ('scenario', 'run', 'recommendation', 'demand_plan', 'supply_plan', 'capacity_plan')),
    aggregate_id uuid NOT NULL,
    authority_scope jsonb NOT NULL,
    aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (holding_id, aggregate_key),
    UNIQUE (holding_id, aggregate_kind, aggregate_id)
);

CREATE TABLE erpx_planning_write.scenarios (
    holding_id uuid NOT NULL,aggregate_key text NOT NULL,scenario_id uuid NOT NULL,planning_modes jsonb NOT NULL,
    input_position bigint NOT NULL CHECK (input_position > 0),input_refs jsonb NOT NULL,policy jsonb NOT NULL,
    demand_facts jsonb NOT NULL,supply_facts jsonb NOT NULL,capacity_buckets jsonb NOT NULL,capacity_requirements jsonb NOT NULL,
    lifecycle_state text NOT NULL,last_failure_reason text,
    PRIMARY KEY (holding_id,scenario_id),UNIQUE (holding_id,aggregate_key),
    FOREIGN KEY (holding_id,aggregate_key) REFERENCES erpx_planning_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT
);
CREATE TABLE erpx_planning_write.runs (
    holding_id uuid NOT NULL,aggregate_key text NOT NULL,run_id uuid NOT NULL,scenario_id uuid NOT NULL,
    scenario_version bigint NOT NULL CHECK (scenario_version > 0),policy_version bigint NOT NULL CHECK (policy_version > 0),
    input_position bigint NOT NULL CHECK (input_position > 0),input_digest text NOT NULL CHECK (input_digest ~ '^[0-9a-f]{64}$'),
    shortages jsonb NOT NULL,pegging jsonb NOT NULL,overloads jsonb NOT NULL,recommendations jsonb NOT NULL,completed_at timestamptz NOT NULL,
    PRIMARY KEY (holding_id,run_id),UNIQUE (holding_id,aggregate_key),
    FOREIGN KEY (holding_id,aggregate_key) REFERENCES erpx_planning_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id,scenario_id) REFERENCES erpx_planning_write.scenarios(holding_id,scenario_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_planning_write.recommendations (
    holding_id uuid NOT NULL,aggregate_key text NOT NULL,recommendation_id uuid NOT NULL,recommendation_kind text NOT NULL,
    item_ref jsonb NOT NULL,destination_ref jsonb NOT NULL,source_ref jsonb,quantity jsonb NOT NULL,required_on date NOT NULL,
    demand_id uuid NOT NULL,lifecycle_state text NOT NULL,override_evidence jsonb NOT NULL,release_id uuid,
    PRIMARY KEY (holding_id,recommendation_id),UNIQUE (holding_id,aggregate_key),
    FOREIGN KEY (holding_id,aggregate_key) REFERENCES erpx_planning_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT
);

CREATE TABLE erpx_planning_write.plans (
    holding_id uuid NOT NULL,aggregate_key text NOT NULL,plan_id uuid NOT NULL,
    plan_kind text NOT NULL CHECK (plan_kind IN ('demand_plan','supply_plan','capacity_plan')),
    scenario_ref jsonb NOT NULL,input_position bigint NOT NULL CHECK (input_position > 0),
    policy_digest text NOT NULL CHECK (policy_digest ~ '^[0-9A-Fa-f]{64}$'),
    lifecycle_state text NOT NULL CHECK (lifecycle_state IN ('draft','frozen','published','superseded','cancelled')),
    PRIMARY KEY (holding_id,plan_id),UNIQUE (holding_id,aggregate_key),
    FOREIGN KEY (holding_id,aggregate_key) REFERENCES erpx_planning_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT
);

CREATE TABLE erpx_planning_write.input_pins (
    holding_id uuid NOT NULL,
    scenario_id uuid NOT NULL,
    input_record_id uuid NOT NULL,
    input_version bigint NOT NULL CHECK (input_version > 0),
    source_position bigint NOT NULL CHECK (source_position > 0),
    policy_id uuid NOT NULL,
    policy_version bigint NOT NULL CHECK (policy_version > 0),
    PRIMARY KEY (holding_id, scenario_id, input_record_id),
    FOREIGN KEY (holding_id,scenario_id) REFERENCES erpx_planning_write.scenarios(holding_id,scenario_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_planning_write.run_guards (
    holding_id uuid NOT NULL,
    run_id uuid NOT NULL,
    scenario_id uuid NOT NULL,
    scenario_version bigint NOT NULL CHECK (scenario_version > 0),
    policy_version bigint NOT NULL CHECK (policy_version > 0),
    input_position bigint NOT NULL CHECK (input_position > 0),
    input_digest text NOT NULL CHECK (input_digest ~ '^[0-9a-f]{64}$'),
    completed_at timestamptz NOT NULL,
    PRIMARY KEY (holding_id, run_id),
    UNIQUE (holding_id, scenario_id, scenario_version, input_digest),
    FOREIGN KEY (holding_id,run_id) REFERENCES erpx_planning_write.runs(holding_id,run_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_planning_write.release_guards (
    holding_id uuid NOT NULL,
    release_id uuid NOT NULL,
    recommendation_id uuid NOT NULL,
    recommendation_version bigint NOT NULL CHECK (recommendation_version > 0),
    owner_correlation_id uuid NOT NULL,
    accepted boolean NOT NULL,
    rejection_reason text,
    decided_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (holding_id, release_id),
    UNIQUE (holding_id, recommendation_id, recommendation_version),
    FOREIGN KEY (holding_id,recommendation_id) REFERENCES erpx_planning_write.recommendations(holding_id,recommendation_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_planning_write.command_receipts (
    holding_id uuid NOT NULL,
    idempotency_key text NOT NULL,
    command_id uuid NOT NULL,
    request_digest text NOT NULL CHECK (request_digest ~ '^[0-9A-Fa-f]{64}$'),
    aggregate_keys text[] NOT NULL,
    aggregate_versions bigint[] NOT NULL,
    committed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (holding_id, idempotency_key),
    UNIQUE (holding_id, command_id)
);

CREATE TABLE erpx_planning_write.audit_log (
    holding_id uuid NOT NULL,command_id uuid NOT NULL,actor_principal_id uuid NOT NULL,
    action text NOT NULL,authority_scope jsonb NOT NULL,aggregate_keys text[] NOT NULL,aggregate_versions bigint[] NOT NULL,
    correlation_id uuid NOT NULL,evidence_digest text NOT NULL CHECK (evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY (holding_id,command_id)
);

CREATE TABLE erpx_planning_write.outbox (
    position bigint GENERATED ALWAYS AS IDENTITY,
    holding_id uuid NOT NULL,
    aggregate_key text NOT NULL,
    aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),
    event_type text NOT NULL,
    event_version integer NOT NULL DEFAULT 1 CHECK (event_version = 1),
    correlation_id uuid NOT NULL,
    payload jsonb NOT NULL,
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    published_at timestamptz,
    PRIMARY KEY (holding_id,position),UNIQUE (holding_id,aggregate_key,aggregate_version,event_type)
);

CREATE VIEW erpx_planning_write.aggregate_rehydration WITH (security_invoker=true) AS
SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',s.scenario_id,'version',v.aggregate_version,'scope',v.authority_scope,'modes',s.planning_modes,
  'input_position',s.input_position,'inputs',s.input_refs,'policy',s.policy,'demands',s.demand_facts,
  'supplies',s.supply_facts,'capacity',s.capacity_buckets,'capacity_requirements',s.capacity_requirements,
  'status',s.lifecycle_state,'last_failure_reason',s.last_failure_reason,'updated_at',v.updated_at) AS aggregate_document
FROM erpx_planning_write.aggregate_versions v JOIN erpx_planning_write.scenarios s
 ON s.holding_id=v.holding_id AND s.aggregate_key=v.aggregate_key
UNION ALL SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',r.run_id,'scenario_id',r.scenario_id,'scenario_version',r.scenario_version,
  'policy_version',r.policy_version,'input_position',r.input_position,'input_digest',r.input_digest,
  'shortages',r.shortages,'pegging',r.pegging,'overloads',r.overloads,'recommendations',r.recommendations,'completed_at',r.completed_at)
FROM erpx_planning_write.aggregate_versions v JOIN erpx_planning_write.runs r ON r.holding_id=v.holding_id AND r.aggregate_key=v.aggregate_key
UNION ALL SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',r.recommendation_id,'version',v.aggregate_version,'kind',r.recommendation_kind,'item',r.item_ref,
  'destination',r.destination_ref,'source',r.source_ref,'quantity',r.quantity,'required_on',r.required_on,
  'demand_id',r.demand_id,'status',r.lifecycle_state,'overrides',r.override_evidence,'release_id',r.release_id)
FROM erpx_planning_write.aggregate_versions v JOIN erpx_planning_write.recommendations r
 ON r.holding_id=v.holding_id AND r.aggregate_key=v.aggregate_key
UNION ALL SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',p.plan_id,'version',v.aggregate_version,'scope',v.authority_scope,'kind',p.plan_kind,
  'scenario',p.scenario_ref,'input_position',p.input_position,'policy_digest',p.policy_digest,
  'status',p.lifecycle_state,'updated_at',v.updated_at)
FROM erpx_planning_write.aggregate_versions v JOIN erpx_planning_write.plans p
 ON p.holding_id=v.holding_id AND p.aggregate_key=v.aggregate_key;

CREATE FUNCTION erpx_planning_write.persist_normalized(
 p_holding_id uuid,p_aggregate_key text,p_kind text,p_id uuid,p_scope jsonb,p_version bigint,p_document jsonb
) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO erpx_planning_write.aggregate_versions
  (holding_id,aggregate_key,aggregate_kind,aggregate_id,authority_scope,aggregate_version,updated_at)
 VALUES (p_holding_id,p_aggregate_key,p_kind,p_id,p_scope,p_version,
  COALESCE((p_document->>'updated_at')::timestamptz,(p_document->>'completed_at')::timestamptz,clock_timestamp()))
 ON CONFLICT (holding_id,aggregate_key) DO UPDATE SET authority_scope=EXCLUDED.authority_scope,
  aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at
 WHERE erpx_planning_write.aggregate_versions.aggregate_version < EXCLUDED.aggregate_version;
 IF p_kind='scenario' THEN
  INSERT INTO erpx_planning_write.scenarios
   (holding_id,aggregate_key,scenario_id,planning_modes,input_position,input_refs,policy,demand_facts,supply_facts,capacity_buckets,capacity_requirements,lifecycle_state,last_failure_reason)
  VALUES (p_holding_id,p_aggregate_key,p_id,p_document->'modes',(p_document->>'input_position')::bigint,
   p_document->'inputs',p_document->'policy',p_document->'demands',p_document->'supplies',p_document->'capacity',
   p_document->'capacity_requirements',p_document->>'status',p_document->>'last_failure_reason')
  ON CONFLICT (holding_id,scenario_id) DO UPDATE SET lifecycle_state=EXCLUDED.lifecycle_state,last_failure_reason=EXCLUDED.last_failure_reason;
 ELSIF p_kind='run' THEN
  INSERT INTO erpx_planning_write.runs
   (holding_id,aggregate_key,run_id,scenario_id,scenario_version,policy_version,input_position,input_digest,shortages,pegging,overloads,recommendations,completed_at)
  VALUES (p_holding_id,p_aggregate_key,p_id,(p_document->>'scenario_id')::uuid,(p_document->>'scenario_version')::bigint,
   (p_document->>'policy_version')::bigint,(p_document->>'input_position')::bigint,p_document->>'input_digest',
   p_document->'shortages',p_document->'pegging',p_document->'overloads',p_document->'recommendations',(p_document->>'completed_at')::timestamptz)
  ON CONFLICT (holding_id,run_id) DO NOTHING;
  INSERT INTO erpx_planning_write.run_guards
   (holding_id,run_id,scenario_id,scenario_version,policy_version,input_position,input_digest,completed_at)
  VALUES (p_holding_id,p_id,(p_document->>'scenario_id')::uuid,(p_document->>'scenario_version')::bigint,
   (p_document->>'policy_version')::bigint,(p_document->>'input_position')::bigint,p_document->>'input_digest',
   (p_document->>'completed_at')::timestamptz) ON CONFLICT (holding_id,run_id) DO NOTHING;
 ELSIF p_kind='recommendation' THEN
  INSERT INTO erpx_planning_write.recommendations
   (holding_id,aggregate_key,recommendation_id,recommendation_kind,item_ref,destination_ref,source_ref,quantity,required_on,demand_id,lifecycle_state,override_evidence,release_id)
  VALUES (p_holding_id,p_aggregate_key,p_id,p_document->>'kind',p_document->'item',p_document->'destination',p_document->'source',
   p_document->'quantity',(p_document->>'required_on')::date,(p_document->>'demand_id')::uuid,p_document->>'status',
   p_document->'overrides',(p_document->>'release_id')::uuid)
  ON CONFLICT (holding_id,recommendation_id) DO UPDATE SET quantity=EXCLUDED.quantity,
   lifecycle_state=EXCLUDED.lifecycle_state,override_evidence=EXCLUDED.override_evidence,release_id=EXCLUDED.release_id;
 ELSIF p_kind IN ('demand_plan','supply_plan','capacity_plan') THEN
  INSERT INTO erpx_planning_write.plans
   (holding_id,aggregate_key,plan_id,plan_kind,scenario_ref,input_position,policy_digest,lifecycle_state)
  VALUES (p_holding_id,p_aggregate_key,p_id,p_kind,p_document->'scenario',(p_document->>'input_position')::bigint,
   p_document->>'policy_digest',p_document->>'status')
  ON CONFLICT (holding_id,plan_id) DO UPDATE SET scenario_ref=EXCLUDED.scenario_ref,
   input_position=EXCLUDED.input_position,policy_digest=EXCLUDED.policy_digest,lifecycle_state=EXCLUDED.lifecycle_state;
 ELSE RAISE EXCEPTION 'unsupported planning aggregate kind'; END IF;
END $$;

DO $$
DECLARE relation_name text;
BEGIN
  FOREACH relation_name IN ARRAY ARRAY[
    'erpx_manufacturing_write.aggregate_versions','erpx_manufacturing_write.specifications','erpx_manufacturing_write.production_orders','erpx_manufacturing_write.governed_records',
    'erpx_manufacturing_write.material_fact_guards','erpx_manufacturing_write.genealogy_guards','erpx_manufacturing_write.command_receipts',
    'erpx_manufacturing_write.audit_log','erpx_manufacturing_write.outbox','erpx_planning_write.aggregate_versions',
    'erpx_planning_write.scenarios','erpx_planning_write.runs','erpx_planning_write.recommendations','erpx_planning_write.plans',
    'erpx_planning_write.input_pins', 'erpx_planning_write.run_guards',
    'erpx_planning_write.release_guards','erpx_planning_write.command_receipts','erpx_planning_write.audit_log','erpx_planning_write.outbox'
  ] LOOP
    EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', relation_name);
    EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', relation_name);
    EXECUTE format(
      'CREATE POLICY holding_isolation ON %s USING (holding_id = nullif(current_setting(''app.holding_id'', true), '''')::uuid) WITH CHECK (holding_id = nullif(current_setting(''app.holding_id'', true), '''')::uuid)',
      relation_name
    );
  END LOOP;
END $$;


-- Normalized E-commerce and Recruiting & Talent write authority. JSON columns
-- hold named value objects/evidence only; no generic aggregate snapshot exists.
CREATE SCHEMA IF NOT EXISTS erpx_ecommerce_write;
CREATE SCHEMA IF NOT EXISTS erpx_talent_write;

CREATE OR REPLACE FUNCTION erpx_ecommerce_write.current_holding_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.holding_id', true), '')::uuid
$$;
CREATE OR REPLACE FUNCTION erpx_talent_write.current_holding_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.holding_id', true), '')::uuid
$$;

CREATE TABLE erpx_ecommerce_write.channels (
    holding_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_holding_id(),
    channel_id UUID NOT NULL, scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL,
    channel_code TEXT NOT NULL, channel_status TEXT NOT NULL,
    configuration JSONB NOT NULL, proposed_by UUID NOT NULL, approved_by UUID,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, channel_id), UNIQUE (holding_id, scope_key, channel_code)
);
CREATE TABLE erpx_ecommerce_write.listings (
    holding_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_holding_id(),
    listing_id UUID NOT NULL, scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL,
    channel_id UUID NOT NULL, product_ref JSONB NOT NULL, source_versions JSONB NOT NULL,
    unit_price_minor BIGINT NOT NULL CHECK (unit_price_minor >= 0), unit_price_currency TEXT NOT NULL,
    available BOOLEAN NOT NULL, listing_status TEXT NOT NULL,
    proposed_by UUID NOT NULL, approved_by UUID,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, listing_id),
    FOREIGN KEY (holding_id, channel_id) REFERENCES erpx_ecommerce_write.channels(holding_id, channel_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_ecommerce_write.carts (
    holding_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_holding_id(),
    cart_id UUID NOT NULL, scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL,
    channel_id UUID NOT NULL, owner_realm TEXT NOT NULL CHECK (owner_realm IN ('guest','customer')),
    owner_principal_id UUID NOT NULL, verified_customer_ref JSONB,
    marketing_consent BOOLEAN NOT NULL, cart_status TEXT NOT NULL, lines JSONB NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, cart_id),
    FOREIGN KEY (holding_id, channel_id) REFERENCES erpx_ecommerce_write.channels(holding_id, channel_id) ON DELETE RESTRICT,
    CHECK (owner_realm <> 'guest' OR verified_customer_ref IS NULL)
);
CREATE TABLE erpx_ecommerce_write.checkouts (
    holding_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_holding_id(),
    checkout_id UUID NOT NULL, scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL,
    idempotency_key TEXT NOT NULL, cart_id UUID NOT NULL, cart_version BIGINT NOT NULL CHECK (cart_version > 0),
    owner_realm TEXT NOT NULL CHECK (owner_realm IN ('guest','customer')), owner_principal_id UUID NOT NULL,
    checkout_snapshot JSONB NOT NULL, revalidation JSONB NOT NULL, reservation JSONB NOT NULL, payment JSONB NOT NULL,
    order_receipt TEXT, order_ref JSONB, checkout_status TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, checkout_id), UNIQUE (holding_id, scope_key, idempotency_key),
    FOREIGN KEY (holding_id, cart_id) REFERENCES erpx_ecommerce_write.carts(holding_id, cart_id) ON DELETE RESTRICT,
    CHECK ((order_receipt IS NULL) = (order_ref IS NULL))
);
CREATE TABLE erpx_ecommerce_write.returns (
    holding_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_holding_id(),
    return_id UUID NOT NULL, scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL,
    checkout_id UUID NOT NULL, order_ref JSONB NOT NULL,
    owner_realm TEXT NOT NULL CHECK (owner_realm IN ('guest','customer')), owner_principal_id UUID NOT NULL,
    owner_ref JSONB NOT NULL, reason TEXT NOT NULL, evidence_digest TEXT NOT NULL,
    handoff_receipt TEXT, refund_receipt TEXT, return_status TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, return_id),
    FOREIGN KEY (holding_id, checkout_id) REFERENCES erpx_ecommerce_write.checkouts(holding_id, checkout_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_ecommerce_write.fulfillment_requests (
    holding_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_holding_id(),
    fulfillment_request_id UUID NOT NULL, scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL,
    owner_realm TEXT NOT NULL CHECK (owner_realm IN ('guest','customer')), owner_principal_id UUID NOT NULL,
    owner_ref JSONB NOT NULL, checkout_ref JSONB NOT NULL, order_ref JSONB NOT NULL,
    reservation_digest TEXT NOT NULL CHECK (reservation_digest ~ '^[0-9A-Fa-f]{64}$'),
    fulfillment_status TEXT NOT NULL CHECK (fulfillment_status IN ('proposed','accepted','dispatched','completed','rejected','cancelled')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,fulfillment_request_id)
);
CREATE TABLE erpx_ecommerce_write.command_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_holding_id(),
    idempotency_key TEXT NOT NULL, command_id UUID NOT NULL, request_digest TEXT NOT NULL,
    aggregate_type TEXT NOT NULL, aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    committed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (holding_id, idempotency_key), UNIQUE (holding_id, command_id)
);
CREATE TABLE erpx_ecommerce_write.audit_log (
    holding_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_holding_id(),
    audit_id UUID NOT NULL, command_id UUID NOT NULL, actor_principal_id UUID NOT NULL,
    action TEXT NOT NULL, aggregate_type TEXT NOT NULL, aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), scope_key TEXT NOT NULL,
    correlation_id UUID NOT NULL, evidence_digest TEXT NOT NULL, occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, audit_id), UNIQUE (holding_id, command_id)
);
CREATE TABLE erpx_ecommerce_write.outbox (
    position BIGINT GENERATED ALWAYS AS IDENTITY, holding_id UUID NOT NULL DEFAULT erpx_ecommerce_write.current_holding_id(),
    event_id UUID NOT NULL, aggregate_type TEXT NOT NULL, aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), event_type TEXT NOT NULL,
    correlation_id UUID NOT NULL, payload JSONB NOT NULL, occurred_at TIMESTAMPTZ NOT NULL, published_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, position), UNIQUE (holding_id, event_id), UNIQUE (holding_id, aggregate_id, aggregate_version)
);

CREATE VIEW erpx_ecommerce_write.aggregate_rehydration WITH (security_invoker = true) AS
SELECT holding_id,'commerce_channel'::text AS aggregate_type,channel_id AS aggregate_id,aggregate_version,
       jsonb_build_object('id',channel_id,'version',aggregate_version,'scope',holding_scope,'code',channel_code,
       'locales',configuration->'locales','currencies',configuration->'currencies','status',channel_status,
       'proposed_by',proposed_by,'approved_by',approved_by,'updated_at',updated_at) AS aggregate_document
FROM erpx_ecommerce_write.channels
UNION ALL SELECT holding_id,'commerce_listing',listing_id,aggregate_version,
       jsonb_build_object('id',listing_id,'version',aggregate_version,'scope',holding_scope,'channel_id',channel_id,
       'product',product_ref,'source_versions',source_versions,'unit_price',jsonb_build_object('minor_units',unit_price_minor,'currency',unit_price_currency),
       'available',available,'status',listing_status,'proposed_by',proposed_by,'approved_by',approved_by,'updated_at',updated_at)
FROM erpx_ecommerce_write.listings
UNION ALL SELECT holding_id,'commerce_cart',cart_id,aggregate_version,
       jsonb_build_object('id',cart_id,'version',aggregate_version,'scope',holding_scope,'channel_id',channel_id,
       'owner',jsonb_build_object('realm',owner_realm,'principal_id',owner_principal_id,'verified_customer',verified_customer_ref),
       'marketing_consent',marketing_consent,'lines',lines,'status',cart_status,'updated_at',updated_at)
FROM erpx_ecommerce_write.carts
UNION ALL SELECT holding_id,'commerce_checkout',checkout_id,aggregate_version,
       jsonb_build_object('id',checkout_id,'version',aggregate_version,'scope',holding_scope,'idempotency_key',idempotency_key,
       'snapshot',checkout_snapshot,'revalidation',revalidation,'reservation',reservation,'payment',payment,
       'order_receipt',order_receipt,'order_ref',order_ref,'status',checkout_status,'updated_at',updated_at)
FROM erpx_ecommerce_write.checkouts
UNION ALL SELECT holding_id,'commerce_return',return_id,aggregate_version,
       jsonb_build_object('id',return_id,'version',aggregate_version,'scope',holding_scope,'checkout_id',checkout_id,
       'order_ref',order_ref,'owner',owner_ref,'reason',reason,'evidence_digest',evidence_digest,
       'handoff_receipt',handoff_receipt,'refund_receipt',refund_receipt,'status',return_status,'updated_at',updated_at)
FROM erpx_ecommerce_write.returns
UNION ALL SELECT holding_id,'commerce_fulfillment_request',fulfillment_request_id,aggregate_version,
       jsonb_build_object('id',fulfillment_request_id,'version',aggregate_version,'scope',holding_scope,'owner',owner_ref,
       'checkout',checkout_ref,'order',order_ref,'reservation_digest',reservation_digest,
       'status',fulfillment_status,'updated_at',updated_at)
FROM erpx_ecommerce_write.fulfillment_requests;

CREATE FUNCTION erpx_ecommerce_write.persist_normalized(
    p_holding_id UUID, p_aggregate_type TEXT, p_aggregate_id UUID,
    p_scope_key TEXT, p_holding_scope JSONB, s JSONB
) RETURNS VOID LANGUAGE plpgsql AS $$
BEGIN
    IF p_aggregate_type = 'commerce_channel' THEN
        INSERT INTO erpx_ecommerce_write.channels
            (holding_id,channel_id,scope_key,holding_scope,channel_code,channel_status,configuration,proposed_by,approved_by,aggregate_version,updated_at)
        VALUES (p_holding_id,p_aggregate_id,p_scope_key,p_holding_scope,s->>'code',s->>'status',
            jsonb_build_object('locales',s->'locales','currencies',s->'currencies'),
            (s->>'proposed_by')::uuid,(s->>'approved_by')::uuid,(s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (holding_id,channel_id) DO UPDATE SET channel_status=EXCLUDED.channel_status,
            configuration=EXCLUDED.configuration,approved_by=EXCLUDED.approved_by,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'commerce_listing' THEN
        INSERT INTO erpx_ecommerce_write.listings
            (holding_id,listing_id,scope_key,holding_scope,channel_id,product_ref,source_versions,unit_price_minor,unit_price_currency,available,listing_status,proposed_by,approved_by,aggregate_version,updated_at)
        VALUES (p_holding_id,p_aggregate_id,p_scope_key,p_holding_scope,(s->>'channel_id')::uuid,s->'product',s->'source_versions',
            (s->'unit_price'->>'minor_units')::bigint,s->'unit_price'->>'currency',(s->>'available')::boolean,s->>'status',
            (s->>'proposed_by')::uuid,(s->>'approved_by')::uuid,(s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (holding_id,listing_id) DO UPDATE SET source_versions=EXCLUDED.source_versions,
            available=EXCLUDED.available,listing_status=EXCLUDED.listing_status,approved_by=EXCLUDED.approved_by,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'commerce_cart' THEN
        INSERT INTO erpx_ecommerce_write.carts
            (holding_id,cart_id,scope_key,holding_scope,channel_id,owner_realm,owner_principal_id,verified_customer_ref,marketing_consent,cart_status,lines,aggregate_version,updated_at)
        VALUES (p_holding_id,p_aggregate_id,p_scope_key,p_holding_scope,(s->>'channel_id')::uuid,s->'owner'->>'realm',
            (s->'owner'->>'principal_id')::uuid,s->'owner'->'verified_customer',(s->>'marketing_consent')::boolean,
            s->>'status',s->'lines',(s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (holding_id,cart_id) DO UPDATE SET owner_realm=EXCLUDED.owner_realm,
            owner_principal_id=EXCLUDED.owner_principal_id,verified_customer_ref=EXCLUDED.verified_customer_ref,
            marketing_consent=EXCLUDED.marketing_consent,cart_status=EXCLUDED.cart_status,lines=EXCLUDED.lines,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'commerce_checkout' THEN
        INSERT INTO erpx_ecommerce_write.checkouts
            (holding_id,checkout_id,scope_key,holding_scope,idempotency_key,cart_id,cart_version,owner_realm,owner_principal_id,checkout_snapshot,revalidation,reservation,payment,order_receipt,order_ref,checkout_status,aggregate_version,updated_at)
        VALUES (p_holding_id,p_aggregate_id,p_scope_key,p_holding_scope,s->>'idempotency_key',(s->'snapshot'->>'cart_id')::uuid,
            (s->'snapshot'->>'cart_version')::bigint,s->'snapshot'->'owner'->>'realm',(s->'snapshot'->'owner'->>'principal_id')::uuid,
            s->'snapshot',s->'revalidation',s->'reservation',s->'payment',s->>'order_receipt',s->'order_ref',s->>'status',
            (s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (holding_id,checkout_id) DO UPDATE SET revalidation=EXCLUDED.revalidation,
            reservation=EXCLUDED.reservation,payment=EXCLUDED.payment,order_receipt=EXCLUDED.order_receipt,
            order_ref=EXCLUDED.order_ref,checkout_status=EXCLUDED.checkout_status,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'commerce_return' THEN
        INSERT INTO erpx_ecommerce_write.returns
            (holding_id,return_id,scope_key,holding_scope,checkout_id,order_ref,owner_realm,owner_principal_id,owner_ref,reason,evidence_digest,handoff_receipt,refund_receipt,return_status,aggregate_version,updated_at)
        VALUES (p_holding_id,p_aggregate_id,p_scope_key,p_holding_scope,(s->>'checkout_id')::uuid,s->'order_ref',s->'owner'->>'realm',
            (s->'owner'->>'principal_id')::uuid,s->'owner',s->>'reason',s->>'evidence_digest',s->>'handoff_receipt',
            s->>'refund_receipt',s->>'status',(s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (holding_id,return_id) DO UPDATE SET handoff_receipt=EXCLUDED.handoff_receipt,
            refund_receipt=EXCLUDED.refund_receipt,return_status=EXCLUDED.return_status,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'commerce_fulfillment_request' THEN
        INSERT INTO erpx_ecommerce_write.fulfillment_requests
            (holding_id,fulfillment_request_id,scope_key,holding_scope,owner_realm,owner_principal_id,owner_ref,
             checkout_ref,order_ref,reservation_digest,fulfillment_status,aggregate_version,updated_at)
        VALUES (p_holding_id,p_aggregate_id,p_scope_key,p_holding_scope,s->'owner'->>'realm',(s->'owner'->>'principal_id')::uuid,
            s->'owner',s->'checkout',s->'order',s->>'reservation_digest',s->>'status',(s->>'version')::bigint,
            (s->>'updated_at')::timestamptz)
        ON CONFLICT (holding_id,fulfillment_request_id) DO UPDATE SET fulfillment_status=EXCLUDED.fulfillment_status,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSE
        RAISE EXCEPTION 'unsupported commerce aggregate type';
    END IF;
END;
$$;

CREATE TABLE erpx_talent_write.requisitions (
    holding_id UUID NOT NULL DEFAULT erpx_talent_write.current_holding_id(),
    requisition_id UUID NOT NULL, scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL,
    title TEXT NOT NULL, position_ref JSONB NOT NULL, position_version BIGINT NOT NULL,
    budget_ref JSONB NOT NULL, budget_version BIGINT NOT NULL, publication_evidence JSONB NOT NULL,
    proposed_by UUID NOT NULL, approved_by UUID, requisition_status TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, requisition_id)
);
CREATE TABLE erpx_talent_write.candidates (
    holding_id UUID NOT NULL DEFAULT erpx_talent_write.current_holding_id(),
    candidate_id UUID NOT NULL, scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL,
    authority_realm TEXT NOT NULL CHECK (authority_realm = 'candidate'), candidate_principal_id UUID NOT NULL,
    profile_digest TEXT NOT NULL, attachment_digests JSONB NOT NULL,
    recruiting_consent BOOLEAN NOT NULL, marketing_consent BOOLEAN NOT NULL, legal_hold BOOLEAN NOT NULL,
    retention_policy_version BIGINT NOT NULL, candidate_status TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, candidate_id), UNIQUE (holding_id, candidate_principal_id)
);
CREATE TABLE erpx_talent_write.applications (
    holding_id UUID NOT NULL DEFAULT erpx_talent_write.current_holding_id(),
    application_id UUID NOT NULL, scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL,
    authority_realm TEXT NOT NULL CHECK (authority_realm = 'candidate'),
    candidate_id UUID NOT NULL, candidate_principal_id UUID NOT NULL, profile_version BIGINT NOT NULL,
    requisition_id UUID NOT NULL, requisition_version BIGINT NOT NULL, submission_idempotency_key TEXT NOT NULL,
    interviews JSONB NOT NULL, selection_evidence JSONB, application_status TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, application_id),
    UNIQUE (holding_id, candidate_id, requisition_id, submission_idempotency_key),
    FOREIGN KEY (holding_id, candidate_id) REFERENCES erpx_talent_write.candidates(holding_id, candidate_id) ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, requisition_id) REFERENCES erpx_talent_write.requisitions(holding_id, requisition_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_talent_write.offers (
    holding_id UUID NOT NULL DEFAULT erpx_talent_write.current_holding_id(),
    offer_id UUID NOT NULL, scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL,
    application_id UUID NOT NULL, application_version BIGINT NOT NULL,
    candidate_principal_id UUID NOT NULL, terms_digest TEXT NOT NULL, terms_version BIGINT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL, proposed_by UUID NOT NULL, approved_by UUID,
    acceptance_receipt TEXT, offer_status TEXT NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, offer_id),
    FOREIGN KEY (holding_id, application_id) REFERENCES erpx_talent_write.applications(holding_id, application_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_talent_write.hire_handoffs (
    holding_id UUID NOT NULL DEFAULT erpx_talent_write.current_holding_id(),
    handoff_id UUID NOT NULL, scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL,
    offer_id UUID NOT NULL, offer_version BIGINT NOT NULL, idempotency_key TEXT NOT NULL,
    evidence_digest TEXT NOT NULL, handoff_status TEXT NOT NULL,
    owner_receipt TEXT, employee_ref JSONB,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, handoff_id), UNIQUE (holding_id, scope_key, idempotency_key),
    FOREIGN KEY (holding_id, offer_id) REFERENCES erpx_talent_write.offers(holding_id, offer_id) ON DELETE RESTRICT,
    CHECK (handoff_status <> 'accepted' OR (owner_receipt IS NOT NULL AND employee_ref IS NOT NULL))
);
CREATE TABLE erpx_talent_write.proposals (
    holding_id UUID NOT NULL DEFAULT erpx_talent_write.current_holding_id(),
    proposal_id UUID NOT NULL, scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL,
    authority_realm TEXT NOT NULL CHECK (authority_realm = 'workforce'),
    proposal_kind TEXT NOT NULL, employee_ref JSONB NOT NULL,
    employee_version BIGINT NOT NULL, cycle_version BIGINT NOT NULL, evidence_digest TEXT NOT NULL,
    amount JSONB, proposed_by UUID NOT NULL, approved_by UUID, proposal_status TEXT NOT NULL, owner_receipt TEXT,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, proposal_id), CHECK ((proposal_kind = 'compensation') = (amount IS NOT NULL))
);
CREATE TABLE erpx_talent_write.governed_records (
    holding_id UUID NOT NULL DEFAULT erpx_talent_write.current_holding_id(),
    record_id UUID NOT NULL, scope_key TEXT NOT NULL, holding_scope JSONB NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('talent_interview','talent_review_cycle')),
    authority_realm TEXT NOT NULL CHECK (authority_realm IN ('candidate','workforce')),
    candidate_principal_id UUID, subject_ref JSONB NOT NULL,
    evidence_digest TEXT NOT NULL CHECK (evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
    lifecycle_state TEXT NOT NULL CHECK (lifecycle_state IN ('draft','scheduled','in_progress','completed','closed','cancelled')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,record_id),
    CHECK ((record_kind='talent_interview' AND authority_realm='candidate' AND candidate_principal_id IS NOT NULL)
        OR (record_kind='talent_review_cycle' AND authority_realm='workforce' AND candidate_principal_id IS NULL))
);
CREATE TABLE erpx_talent_write.command_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_talent_write.current_holding_id(),
    idempotency_key TEXT NOT NULL, command_id UUID NOT NULL, request_digest TEXT NOT NULL,
    aggregate_type TEXT NOT NULL, aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    committed_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (holding_id, idempotency_key), UNIQUE (holding_id, command_id)
);
CREATE TABLE erpx_talent_write.audit_log (
    holding_id UUID NOT NULL DEFAULT erpx_talent_write.current_holding_id(),
    audit_id UUID NOT NULL, command_id UUID NOT NULL, actor_principal_id UUID NOT NULL,
    actor_realm TEXT NOT NULL CHECK (actor_realm IN ('candidate','workforce')),
    action TEXT NOT NULL, aggregate_type TEXT NOT NULL, aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), scope_key TEXT NOT NULL,
    correlation_id UUID NOT NULL, evidence_digest TEXT NOT NULL, occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, audit_id), UNIQUE (holding_id, command_id)
);
CREATE TABLE erpx_talent_write.outbox (
    position BIGINT GENERATED ALWAYS AS IDENTITY, holding_id UUID NOT NULL DEFAULT erpx_talent_write.current_holding_id(),
    event_id UUID NOT NULL, aggregate_type TEXT NOT NULL, aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0), event_type TEXT NOT NULL,
    correlation_id UUID NOT NULL, payload JSONB NOT NULL, occurred_at TIMESTAMPTZ NOT NULL, published_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, position), UNIQUE (holding_id, event_id), UNIQUE (holding_id, aggregate_id, aggregate_version)
);

CREATE VIEW erpx_talent_write.aggregate_rehydration WITH (security_invoker = true) AS
SELECT holding_id,'talent_requisition'::text AS aggregate_type,requisition_id AS aggregate_id,aggregate_version,
       jsonb_build_object('id',requisition_id,'version',aggregate_version,'scope',holding_scope,'title',title,
       'position',position_ref,'position_version',position_version,'budget',budget_ref,'budget_version',budget_version,
       'public_content_digest',publication_evidence->>'public_content_digest','privacy_review_version',publication_evidence->'privacy_review_version',
       'proposed_by',proposed_by,'approved_by',approved_by,'status',requisition_status,'updated_at',updated_at) AS aggregate_document
FROM erpx_talent_write.requisitions
UNION ALL SELECT holding_id,'talent_candidate',candidate_id,aggregate_version,
       jsonb_build_object('id',candidate_id,'version',aggregate_version,'scope',holding_scope,
       'principal',jsonb_build_object('realm',authority_realm,'principal_id',candidate_principal_id),
       'profile_digest',profile_digest,'attachment_digests',attachment_digests,'recruiting_consent',recruiting_consent,
       'marketing_consent',marketing_consent,'legal_hold',legal_hold,'retention_policy_version',retention_policy_version,
       'status',candidate_status,'updated_at',updated_at)
FROM erpx_talent_write.candidates
UNION ALL SELECT holding_id,'talent_application',application_id,aggregate_version,
       jsonb_build_object('id',application_id,'version',aggregate_version,'scope',holding_scope,'candidate_id',candidate_id,
       'candidate_principal_id',candidate_principal_id,'profile_version',profile_version,'requisition_id',requisition_id,
       'requisition_version',requisition_version,'idempotency_key',submission_idempotency_key,'interviews',interviews,
       'selection',selection_evidence,'status',application_status,'updated_at',updated_at)
FROM erpx_talent_write.applications
UNION ALL SELECT holding_id,'talent_offer',offer_id,aggregate_version,
       jsonb_build_object('id',offer_id,'version',aggregate_version,'scope',holding_scope,'application_id',application_id,
       'application_version',application_version,'candidate_principal_id',candidate_principal_id,'terms_digest',terms_digest,
       'terms_version',terms_version,'expires_at',expires_at,'proposed_by',proposed_by,'approved_by',approved_by,
       'acceptance_receipt',acceptance_receipt,'status',offer_status,'updated_at',updated_at)
FROM erpx_talent_write.offers
UNION ALL SELECT holding_id,'talent_hire_handoff',handoff_id,aggregate_version,
       jsonb_build_object('id',handoff_id,'version',aggregate_version,'scope',holding_scope,'offer_id',offer_id,
       'offer_version',offer_version,'idempotency_key',idempotency_key,'evidence_digest',evidence_digest,
       'status',handoff_status,'owner_receipt',owner_receipt,'employee_ref',employee_ref,'updated_at',updated_at)
FROM erpx_talent_write.hire_handoffs
UNION ALL SELECT holding_id,'talent_proposal',proposal_id,aggregate_version,
       jsonb_build_object('id',proposal_id,'version',aggregate_version,'scope',holding_scope,'kind',proposal_kind,
       'employee',employee_ref,'employee_version',employee_version,'cycle_version',cycle_version,'evidence_digest',evidence_digest,
       'amount',amount,'proposed_by',proposed_by,'approved_by',approved_by,'status',proposal_status,
       'owner_receipt',owner_receipt,'updated_at',updated_at)
FROM erpx_talent_write.proposals
UNION ALL SELECT holding_id,record_kind,record_id,aggregate_version,
       jsonb_build_object('id',record_id,'version',aggregate_version,'scope',holding_scope,
       'kind',CASE record_kind WHEN 'talent_interview' THEN 'interview' ELSE 'review_cycle' END,
       'authority_realm',authority_realm,'candidate_principal_id',candidate_principal_id,'subject',subject_ref,
       'evidence_digest',evidence_digest,'status',lifecycle_state,'updated_at',updated_at)
FROM erpx_talent_write.governed_records;

CREATE FUNCTION erpx_talent_write.persist_normalized(
    p_holding_id UUID, p_aggregate_type TEXT, p_aggregate_id UUID,
    p_scope_key TEXT, p_holding_scope JSONB, s JSONB
) RETURNS VOID LANGUAGE plpgsql AS $$
BEGIN
    IF p_aggregate_type = 'talent_requisition' THEN
        INSERT INTO erpx_talent_write.requisitions
            (holding_id,requisition_id,scope_key,holding_scope,title,position_ref,position_version,budget_ref,budget_version,
             publication_evidence,proposed_by,approved_by,requisition_status,aggregate_version,updated_at)
        VALUES (p_holding_id,p_aggregate_id,p_scope_key,p_holding_scope,s->>'title',s->'position',(s->>'position_version')::bigint,
            s->'budget',(s->>'budget_version')::bigint,jsonb_build_object('public_content_digest',s->'public_content_digest',
            'privacy_review_version',s->'privacy_review_version'),(s->>'proposed_by')::uuid,(s->>'approved_by')::uuid,
            s->>'status',(s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (holding_id,requisition_id) DO UPDATE SET approved_by=EXCLUDED.approved_by,
            requisition_status=EXCLUDED.requisition_status,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'talent_candidate' THEN
        INSERT INTO erpx_talent_write.candidates
            (holding_id,candidate_id,scope_key,holding_scope,authority_realm,candidate_principal_id,profile_digest,attachment_digests,
             recruiting_consent,marketing_consent,legal_hold,retention_policy_version,candidate_status,aggregate_version,updated_at)
        VALUES (p_holding_id,p_aggregate_id,p_scope_key,p_holding_scope,s->'principal'->>'realm',(s->'principal'->>'principal_id')::uuid,
            s->>'profile_digest',s->'attachment_digests',(s->>'recruiting_consent')::boolean,(s->>'marketing_consent')::boolean,
            (s->>'legal_hold')::boolean,(s->>'retention_policy_version')::bigint,s->>'status',(s->>'version')::bigint,
            (s->>'updated_at')::timestamptz)
        ON CONFLICT (holding_id,candidate_id) DO UPDATE SET profile_digest=EXCLUDED.profile_digest,
            attachment_digests=EXCLUDED.attachment_digests,recruiting_consent=EXCLUDED.recruiting_consent,
            marketing_consent=EXCLUDED.marketing_consent,legal_hold=EXCLUDED.legal_hold,candidate_status=EXCLUDED.candidate_status,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'talent_application' THEN
        INSERT INTO erpx_talent_write.applications
            (holding_id,application_id,scope_key,holding_scope,authority_realm,candidate_id,candidate_principal_id,profile_version,
             requisition_id,requisition_version,submission_idempotency_key,interviews,selection_evidence,application_status,aggregate_version,updated_at)
        VALUES (p_holding_id,p_aggregate_id,p_scope_key,p_holding_scope,'candidate',(s->>'candidate_id')::uuid,
            (s->>'candidate_principal_id')::uuid,(s->>'profile_version')::bigint,(s->>'requisition_id')::uuid,
            (s->>'requisition_version')::bigint,s->>'idempotency_key',s->'interviews',s->'selection',s->>'status',
            (s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (holding_id,application_id) DO UPDATE SET interviews=EXCLUDED.interviews,
            selection_evidence=EXCLUDED.selection_evidence,application_status=EXCLUDED.application_status,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'talent_offer' THEN
        INSERT INTO erpx_talent_write.offers
            (holding_id,offer_id,scope_key,holding_scope,application_id,application_version,candidate_principal_id,terms_digest,
             terms_version,expires_at,proposed_by,approved_by,acceptance_receipt,offer_status,aggregate_version,updated_at)
        VALUES (p_holding_id,p_aggregate_id,p_scope_key,p_holding_scope,(s->>'application_id')::uuid,(s->>'application_version')::bigint,
            (s->>'candidate_principal_id')::uuid,s->>'terms_digest',(s->>'terms_version')::bigint,(s->>'expires_at')::timestamptz,
            (s->>'proposed_by')::uuid,(s->>'approved_by')::uuid,s->>'acceptance_receipt',s->>'status',
            (s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (holding_id,offer_id) DO UPDATE SET approved_by=EXCLUDED.approved_by,
            acceptance_receipt=EXCLUDED.acceptance_receipt,offer_status=EXCLUDED.offer_status,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'talent_hire_handoff' THEN
        INSERT INTO erpx_talent_write.hire_handoffs
            (holding_id,handoff_id,scope_key,holding_scope,offer_id,offer_version,idempotency_key,evidence_digest,
             handoff_status,owner_receipt,employee_ref,aggregate_version,updated_at)
        VALUES (p_holding_id,p_aggregate_id,p_scope_key,p_holding_scope,(s->>'offer_id')::uuid,(s->>'offer_version')::bigint,
            s->>'idempotency_key',s->>'evidence_digest',s->>'status',s->>'owner_receipt',s->'employee_ref',
            (s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (holding_id,handoff_id) DO UPDATE SET handoff_status=EXCLUDED.handoff_status,
            owner_receipt=EXCLUDED.owner_receipt,employee_ref=EXCLUDED.employee_ref,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type = 'talent_proposal' THEN
        INSERT INTO erpx_talent_write.proposals
            (holding_id,proposal_id,scope_key,holding_scope,authority_realm,proposal_kind,employee_ref,employee_version,
             cycle_version,evidence_digest,amount,proposed_by,approved_by,proposal_status,owner_receipt,aggregate_version,updated_at)
        VALUES (p_holding_id,p_aggregate_id,p_scope_key,p_holding_scope,'workforce',s->>'kind',s->'employee',
            (s->>'employee_version')::bigint,(s->>'cycle_version')::bigint,s->>'evidence_digest',s->'amount',
            (s->>'proposed_by')::uuid,(s->>'approved_by')::uuid,s->>'status',s->>'owner_receipt',
            (s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (holding_id,proposal_id) DO UPDATE SET approved_by=EXCLUDED.approved_by,
            proposal_status=EXCLUDED.proposal_status,owner_receipt=EXCLUDED.owner_receipt,
            aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSIF p_aggregate_type IN ('talent_interview','talent_review_cycle') THEN
        INSERT INTO erpx_talent_write.governed_records
            (holding_id,record_id,scope_key,holding_scope,record_kind,authority_realm,candidate_principal_id,
             subject_ref,evidence_digest,lifecycle_state,aggregate_version,updated_at)
        VALUES(p_holding_id,p_aggregate_id,p_scope_key,p_holding_scope,p_aggregate_type,s->>'authority_realm',
            (s->>'candidate_principal_id')::uuid,s->'subject',s->>'evidence_digest',s->>'status',
            (s->>'version')::bigint,(s->>'updated_at')::timestamptz)
        ON CONFLICT (holding_id,record_id) DO UPDATE SET lifecycle_state=EXCLUDED.lifecycle_state,
            evidence_digest=EXCLUDED.evidence_digest,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at;
    ELSE
        RAISE EXCEPTION 'unsupported talent aggregate type';
    END IF;
END;
$$;

CREATE INDEX ecommerce_scope_status ON erpx_ecommerce_write.checkouts(holding_id, scope_key, checkout_status, updated_at DESC);
CREATE INDEX ecommerce_uncertain_checkout ON erpx_ecommerce_write.checkouts(holding_id, scope_key, payment) WHERE payment->>'status' = 'uncertain';
CREATE INDEX talent_candidate_owner ON erpx_talent_write.applications(holding_id, candidate_principal_id, application_id);
CREATE INDEX talent_handoff_uncertain ON erpx_talent_write.hire_handoffs(holding_id, scope_key, handoff_status) WHERE handoff_status = 'uncertain';

DO $$
DECLARE schema_name TEXT; table_name TEXT;
BEGIN
    FOR schema_name, table_name IN SELECT * FROM (VALUES
        ('erpx_ecommerce_write','channels'),('erpx_ecommerce_write','listings'),
        ('erpx_ecommerce_write','carts'),('erpx_ecommerce_write','checkouts'),
        ('erpx_ecommerce_write','returns'),('erpx_ecommerce_write','fulfillment_requests'),('erpx_ecommerce_write','command_receipts'),
        ('erpx_ecommerce_write','audit_log'),('erpx_ecommerce_write','outbox'),
        ('erpx_talent_write','requisitions'),('erpx_talent_write','candidates'),
        ('erpx_talent_write','applications'),('erpx_talent_write','offers'),
        ('erpx_talent_write','hire_handoffs'),('erpx_talent_write','proposals'),('erpx_talent_write','governed_records'),
        ('erpx_talent_write','command_receipts'),('erpx_talent_write','audit_log'),
        ('erpx_talent_write','outbox')
    ) AS protected(schema_name, table_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY', schema_name, table_name);
        EXECUTE format(
            'CREATE POLICY holding_isolation ON %I.%I USING (holding_id = %I.current_holding_id()) '
            'WITH CHECK (holding_id = %I.current_holding_id())',
            schema_name, table_name, schema_name, schema_name
        );
    END LOOP;
END;
$$;


CREATE SCHEMA IF NOT EXISTS erpx_learning_write;
CREATE SCHEMA IF NOT EXISTS erpx_benefits_write;
CREATE SCHEMA IF NOT EXISTS erpx_field_service_write;

CREATE TABLE erpx_learning_write.aggregate_versions (
  holding_id uuid NOT NULL, aggregate_key text NOT NULL, aggregate_kind text NOT NULL CHECK (aggregate_kind IN ('course','enrollment','assignment','completion')),
  aggregate_id uuid NOT NULL, authority_scope jsonb NOT NULL, aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),
  updated_at timestamptz NOT NULL DEFAULT clock_timestamp(), PRIMARY KEY (holding_id, aggregate_key), UNIQUE (holding_id, aggregate_kind, aggregate_id)
);
CREATE TABLE erpx_learning_write.courses (
  holding_id uuid NOT NULL,aggregate_key text NOT NULL,course_id uuid NOT NULL,title text NOT NULL,
  passing_basis_points integer NOT NULL CHECK (passing_basis_points BETWEEN 0 AND 10000),
  required_attendance_minutes integer NOT NULL CHECK (required_attendance_minutes > 0),provider_ref jsonb,
  lifecycle_state text NOT NULL CHECK (lifecycle_state IN ('draft','published','retired')),
  PRIMARY KEY (holding_id,course_id),UNIQUE (holding_id,aggregate_key),
  FOREIGN KEY (holding_id,aggregate_key) REFERENCES erpx_learning_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT
);
CREATE TABLE erpx_learning_write.enrollments (
  holding_id uuid NOT NULL,aggregate_key text NOT NULL,enrollment_id uuid NOT NULL,assignment_id uuid NOT NULL,
  learner_ref jsonb NOT NULL,learner_principal_id uuid NOT NULL,course_id uuid NOT NULL,course_version bigint NOT NULL,
  due_on date NOT NULL,lifecycle_state text NOT NULL,attendance_facts jsonb NOT NULL,provider_results jsonb NOT NULL,
  waiver_evidence jsonb,completion_evidence jsonb,
  PRIMARY KEY (holding_id,enrollment_id),UNIQUE (holding_id,aggregate_key),
  FOREIGN KEY (holding_id,aggregate_key) REFERENCES erpx_learning_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT,
  FOREIGN KEY (holding_id,course_id) REFERENCES erpx_learning_write.courses(holding_id,course_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_learning_write.governed_records (
  holding_id uuid NOT NULL,aggregate_key text NOT NULL,record_id uuid NOT NULL,
  record_kind text NOT NULL CHECK(record_kind IN ('assignment','completion')),subject_ref jsonb NOT NULL,
  evidence_digest text NOT NULL CHECK(evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
  lifecycle_state text NOT NULL CHECK(lifecycle_state IN ('proposed','assigned','in_progress','completed','waived','cancelled')),
  PRIMARY KEY(holding_id,record_id),UNIQUE(holding_id,aggregate_key),
  FOREIGN KEY(holding_id,aggregate_key) REFERENCES erpx_learning_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT
);
CREATE TABLE erpx_learning_write.attendance_guards (
  holding_id uuid NOT NULL, enrollment_id uuid NOT NULL, attendance_fact_id uuid NOT NULL, learner_principal_id uuid NOT NULL,
  recorded_by_principal_id uuid NOT NULL CHECK (learner_principal_id <> recorded_by_principal_id), evidence_record_id uuid NOT NULL,
  evidence_version bigint NOT NULL CHECK (evidence_version > 0), attended_minutes integer NOT NULL CHECK (attended_minutes > 0), PRIMARY KEY (holding_id, attendance_fact_id),
  FOREIGN KEY (holding_id,enrollment_id) REFERENCES erpx_learning_write.enrollments(holding_id,enrollment_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_learning_write.provider_result_guards (
  holding_id uuid NOT NULL, enrollment_id uuid NOT NULL, result_id uuid NOT NULL, provider_attempt_id text NOT NULL,
  evidence_record_id uuid NOT NULL, evidence_version bigint NOT NULL CHECK (evidence_version > 0), score_basis_points integer NOT NULL CHECK (score_basis_points BETWEEN 0 AND 10000), passed boolean NOT NULL,
  PRIMARY KEY (holding_id, result_id), UNIQUE (holding_id, provider_attempt_id),
  FOREIGN KEY (holding_id,enrollment_id) REFERENCES erpx_learning_write.enrollments(holding_id,enrollment_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_learning_write.completion_facts (
  holding_id uuid NOT NULL, enrollment_id uuid NOT NULL, enrollment_version bigint NOT NULL CHECK (enrollment_version > 0),
  employee_record_id uuid NOT NULL, course_id uuid NOT NULL, course_version bigint NOT NULL CHECK (course_version > 0), completed_at timestamptz NOT NULL,
  PRIMARY KEY (holding_id, enrollment_id, enrollment_version),
  FOREIGN KEY (holding_id,enrollment_id) REFERENCES erpx_learning_write.enrollments(holding_id,enrollment_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_benefits_write.aggregate_versions (
  holding_id uuid NOT NULL, aggregate_key text NOT NULL, aggregate_kind text NOT NULL CHECK (aggregate_kind IN ('plan','enrollment','window','handoff')),
  aggregate_id uuid NOT NULL, authority_scope jsonb NOT NULL, aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),
  updated_at timestamptz NOT NULL DEFAULT clock_timestamp(), PRIMARY KEY (holding_id, aggregate_key), UNIQUE (holding_id, aggregate_kind, aggregate_id)
);
CREATE TABLE erpx_benefits_write.plans (
  holding_id uuid NOT NULL,aggregate_key text NOT NULL,plan_id uuid NOT NULL,plan_name text NOT NULL,currency char(3) NOT NULL,
  eligibility_class text NOT NULL,minimum_tenure_days integer NOT NULL,contribution_rule jsonb NOT NULL,
  lifecycle_state text NOT NULL CHECK (lifecycle_state IN ('draft','published','retired')),
  PRIMARY KEY (holding_id,plan_id),UNIQUE (holding_id,aggregate_key),
  FOREIGN KEY (holding_id,aggregate_key) REFERENCES erpx_benefits_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT
);
CREATE TABLE erpx_benefits_write.enrollments (
  holding_id uuid NOT NULL,aggregate_key text NOT NULL,enrollment_id uuid NOT NULL,plan_id uuid NOT NULL,plan_version bigint NOT NULL,
  window_id uuid NOT NULL,employee_ref jsonb NOT NULL,eligibility_decision jsonb NOT NULL,life_event jsonb,
  dependents jsonb NOT NULL,election jsonb,provider_acknowledgements jsonb NOT NULL,payroll_handoff_id uuid,
  lifecycle_state text NOT NULL,
  PRIMARY KEY (holding_id,enrollment_id),UNIQUE (holding_id,aggregate_key),
  FOREIGN KEY (holding_id,aggregate_key) REFERENCES erpx_benefits_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT,
  FOREIGN KEY (holding_id,plan_id) REFERENCES erpx_benefits_write.plans(holding_id,plan_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_benefits_write.governed_records (
  holding_id uuid NOT NULL,aggregate_key text NOT NULL,record_id uuid NOT NULL,
  record_kind text NOT NULL CHECK(record_kind IN ('window','handoff')),subject_ref jsonb NOT NULL,
  evidence_digest text NOT NULL CHECK(evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
  lifecycle_state text NOT NULL CHECK(lifecycle_state IN ('draft','open','closed','submitted','accepted','rejected','cancelled')),
  PRIMARY KEY(holding_id,record_id),UNIQUE(holding_id,aggregate_key),
  FOREIGN KEY(holding_id,aggregate_key) REFERENCES erpx_benefits_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT
);
CREATE TABLE erpx_benefits_write.eligibility_guards (
  holding_id uuid NOT NULL, enrollment_id uuid NOT NULL, employee_record_id uuid NOT NULL, employee_version bigint NOT NULL CHECK (employee_version > 0),
  source_record_id uuid NOT NULL, source_version bigint NOT NULL CHECK (source_version > 0), eligible boolean NOT NULL, evaluated_at timestamptz NOT NULL,
  PRIMARY KEY (holding_id, enrollment_id),
  FOREIGN KEY (holding_id,enrollment_id) REFERENCES erpx_benefits_write.enrollments(holding_id,enrollment_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_benefits_write.provider_acknowledgement_guards (
  holding_id uuid NOT NULL, enrollment_id uuid NOT NULL, acknowledgement_id uuid NOT NULL, request_key text NOT NULL,
  accepted boolean NOT NULL, provider_reference text NOT NULL, rejection_reason text, received_at timestamptz NOT NULL,
  PRIMARY KEY (holding_id, acknowledgement_id), UNIQUE (holding_id, request_key),
  FOREIGN KEY (holding_id,enrollment_id) REFERENCES erpx_benefits_write.enrollments(holding_id,enrollment_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_benefits_write.payroll_handoff_guards (
  holding_id uuid NOT NULL, handoff_id uuid NOT NULL, enrollment_id uuid NOT NULL, enrollment_version bigint NOT NULL CHECK (enrollment_version > 0),
  owner_correlation_id uuid NOT NULL, accepted boolean NOT NULL, rejection_reason text, decided_at timestamptz NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (holding_id, handoff_id), UNIQUE (holding_id, enrollment_id, enrollment_version),
  FOREIGN KEY (holding_id,enrollment_id) REFERENCES erpx_benefits_write.enrollments(holding_id,enrollment_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_field_service_write.aggregate_versions (
  holding_id uuid NOT NULL, aggregate_key text NOT NULL, aggregate_kind text NOT NULL CHECK (aggregate_kind IN ('agreement','order','request','dispatch','visit','handoff')),
  aggregate_id uuid NOT NULL, authority_scope jsonb NOT NULL, aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),
  updated_at timestamptz NOT NULL DEFAULT clock_timestamp(), PRIMARY KEY (holding_id, aggregate_key), UNIQUE (holding_id, aggregate_kind, aggregate_id)
);
CREATE TABLE erpx_field_service_write.agreements (
  holding_id uuid NOT NULL,aggregate_key text NOT NULL,agreement_id uuid NOT NULL,customer_ref jsonb NOT NULL,
  covered_assets jsonb NOT NULL,covered_locations jsonb NOT NULL,service_codes jsonb NOT NULL,
  valid_from date NOT NULL,valid_through date NOT NULL,active boolean NOT NULL,
  PRIMARY KEY (holding_id,agreement_id),UNIQUE (holding_id,aggregate_key),
  FOREIGN KEY (holding_id,aggregate_key) REFERENCES erpx_field_service_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT
);
CREATE TABLE erpx_field_service_write.service_orders (
  holding_id uuid NOT NULL,aggregate_key text NOT NULL,service_order_id uuid NOT NULL,request_id uuid NOT NULL,request_version bigint NOT NULL,
  agreement_id uuid NOT NULL,agreement_version bigint NOT NULL,customer_ref jsonb NOT NULL,asset_ref jsonb NOT NULL,location_ref jsonb NOT NULL,
  service_code text NOT NULL,required_skills jsonb NOT NULL,scheduled_on date,technician_qualification jsonb,dispatch_id uuid,visit_id uuid,
  part_reservations jsonb NOT NULL,part_fact_ids jsonb NOT NULL,visit_evidence jsonb NOT NULL,customer_signature jsonb,
  exceptions jsonb NOT NULL,billing_handoff_id uuid,lifecycle_state text NOT NULL,
  PRIMARY KEY (holding_id,service_order_id),UNIQUE (holding_id,aggregate_key),
  FOREIGN KEY (holding_id,aggregate_key) REFERENCES erpx_field_service_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT,
  FOREIGN KEY (holding_id,agreement_id) REFERENCES erpx_field_service_write.agreements(holding_id,agreement_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_field_service_write.governed_records (
  holding_id uuid NOT NULL,aggregate_key text NOT NULL,record_id uuid NOT NULL,
  record_kind text NOT NULL CHECK(record_kind IN ('request','dispatch','visit','handoff')),subject_ref jsonb NOT NULL,
  evidence_digest text NOT NULL CHECK(evidence_digest ~ '^[0-9A-Fa-f]{64}$'),
  lifecycle_state text NOT NULL CHECK(lifecycle_state IN ('proposed','accepted','scheduled','in_progress','completed','reconciled','rejected','cancelled')),
  PRIMARY KEY(holding_id,record_id),UNIQUE(holding_id,aggregate_key),
  FOREIGN KEY(holding_id,aggregate_key) REFERENCES erpx_field_service_write.aggregate_versions(holding_id,aggregate_key) ON DELETE RESTRICT
);
CREATE TABLE erpx_field_service_write.part_issue_guards (
  holding_id uuid NOT NULL, service_order_id uuid NOT NULL, part_fact_id uuid NOT NULL, part_record_id uuid NOT NULL,
  quantity_units bigint NOT NULL CHECK (quantity_units > 0), quantity_scale smallint NOT NULL CHECK (quantity_scale BETWEEN 0 AND 9), occurred_at timestamptz NOT NULL,
  PRIMARY KEY (holding_id, part_fact_id),
  FOREIGN KEY (holding_id,service_order_id) REFERENCES erpx_field_service_write.service_orders(holding_id,service_order_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_field_service_write.visit_evidence_guards (
  holding_id uuid NOT NULL, service_order_id uuid NOT NULL, evidence_record_id uuid NOT NULL, evidence_version bigint NOT NULL CHECK (evidence_version > 0),
  evidence_kind text NOT NULL CHECK (evidence_kind IN ('visit','customer_signature','signature_refusal','mismatch_exception')), PRIMARY KEY (holding_id, service_order_id, evidence_record_id, evidence_version),
  FOREIGN KEY (holding_id,service_order_id) REFERENCES erpx_field_service_write.service_orders(holding_id,service_order_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_field_service_write.exception_guards (
  holding_id uuid NOT NULL, exception_id uuid NOT NULL, service_order_id uuid NOT NULL,
  exception_kind text NOT NULL CHECK (exception_kind IN ('customer_mismatch','asset_mismatch','signature_refused','coverage_mismatch','inventory_mismatch')),
  reason text NOT NULL CHECK (length(btrim(reason)) BETWEEN 8 AND 500), evidence_record_id uuid NOT NULL, evidence_version bigint NOT NULL CHECK (evidence_version > 0),
  PRIMARY KEY (holding_id, exception_id),
  FOREIGN KEY (holding_id,service_order_id) REFERENCES erpx_field_service_write.service_orders(holding_id,service_order_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_field_service_write.billing_handoff_guards (
  holding_id uuid NOT NULL, handoff_id uuid NOT NULL, service_order_id uuid NOT NULL, service_order_version bigint NOT NULL CHECK (service_order_version > 0),
  owner_correlation_id uuid NOT NULL, accepted boolean NOT NULL, rejection_reason text, decided_at timestamptz NOT NULL DEFAULT clock_timestamp(),
  PRIMARY KEY (holding_id, handoff_id), UNIQUE (holding_id, service_order_id, service_order_version),
  FOREIGN KEY (holding_id,service_order_id) REFERENCES erpx_field_service_write.service_orders(holding_id,service_order_id) ON DELETE RESTRICT
);

DO $$
DECLARE schema_name text;
BEGIN
  FOREACH schema_name IN ARRAY ARRAY['erpx_learning_write','erpx_benefits_write','erpx_field_service_write'] LOOP
    EXECUTE format('CREATE TABLE %I.command_receipts (holding_id uuid NOT NULL,idempotency_key text NOT NULL,command_id uuid NOT NULL,request_digest text NOT NULL CHECK (request_digest ~ ''^[0-9A-Fa-f]{64}$''),aggregate_key text NOT NULL,aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),committed_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY (holding_id,idempotency_key),UNIQUE (holding_id,command_id))', schema_name);
    EXECUTE format('CREATE TABLE %I.audit_log (holding_id uuid NOT NULL,command_id uuid NOT NULL,actor_principal_id uuid NOT NULL,action text NOT NULL,authority_scope jsonb NOT NULL,aggregate_key text NOT NULL,aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),correlation_id uuid NOT NULL,evidence_digest text NOT NULL CHECK (evidence_digest ~ ''^[0-9A-Fa-f]{64}$''),occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY (holding_id,command_id))', schema_name);
    EXECUTE format('CREATE TABLE %I.outbox (position bigint GENERATED ALWAYS AS IDENTITY,holding_id uuid NOT NULL,aggregate_key text NOT NULL,aggregate_version bigint NOT NULL CHECK (aggregate_version > 0),event_type text NOT NULL,event_version integer NOT NULL DEFAULT 1 CHECK (event_version = 1),correlation_id uuid NOT NULL,payload jsonb NOT NULL,occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),published_at timestamptz,PRIMARY KEY (holding_id,position),UNIQUE (holding_id,aggregate_key,aggregate_version,event_type))', schema_name);
  END LOOP;
END $$;

CREATE VIEW erpx_learning_write.aggregate_rehydration WITH (security_invoker=true) AS
SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',c.course_id,'version',v.aggregate_version,'scope',v.authority_scope,'title',c.title,
  'passing_basis_points',c.passing_basis_points,'required_attendance_minutes',c.required_attendance_minutes,
  'provider',c.provider_ref,'status',c.lifecycle_state,'updated_at',v.updated_at) AS aggregate_document
FROM erpx_learning_write.aggregate_versions v JOIN erpx_learning_write.courses c ON c.holding_id=v.holding_id AND c.aggregate_key=v.aggregate_key
UNION ALL SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',e.enrollment_id,'version',v.aggregate_version,'scope',v.authority_scope,'assignment_id',e.assignment_id,
  'learner',e.learner_ref,'learner_principal',e.learner_principal_id,
  'course',jsonb_build_object('course_id',e.course_id,'version',e.course_version),'due_on',e.due_on,'status',e.lifecycle_state,
  'attendance',e.attendance_facts,'provider_results',e.provider_results,'waiver',e.waiver_evidence,
  'completion',e.completion_evidence,'updated_at',v.updated_at)
FROM erpx_learning_write.aggregate_versions v JOIN erpx_learning_write.enrollments e ON e.holding_id=v.holding_id AND e.aggregate_key=v.aggregate_key
UNION ALL SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',r.record_id,'version',v.aggregate_version,'scope',v.authority_scope,'kind',r.record_kind,
 'subject',r.subject_ref,'evidence_digest',r.evidence_digest,'status',r.lifecycle_state,'updated_at',v.updated_at)
FROM erpx_learning_write.aggregate_versions v JOIN erpx_learning_write.governed_records r ON r.holding_id=v.holding_id AND r.aggregate_key=v.aggregate_key;

CREATE FUNCTION erpx_learning_write.persist_normalized(uuid,text,text,uuid,jsonb,bigint,jsonb) RETURNS void LANGUAGE plpgsql AS $$
DECLARE t alias for $1; k alias for $2; kind alias for $3; rid alias for $4; scope alias for $5; ver alias for $6; d alias for $7;
BEGIN
 INSERT INTO erpx_learning_write.aggregate_versions VALUES(t,k,kind,rid,scope,ver,(d->>'updated_at')::timestamptz)
 ON CONFLICT (holding_id,aggregate_key) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at
 WHERE erpx_learning_write.aggregate_versions.aggregate_version<EXCLUDED.aggregate_version;
 IF kind='course' THEN
  INSERT INTO erpx_learning_write.courses VALUES(t,k,rid,d->>'title',(d->>'passing_basis_points')::integer,(d->>'required_attendance_minutes')::integer,d->'provider',d->>'status')
  ON CONFLICT (holding_id,course_id) DO UPDATE SET lifecycle_state=EXCLUDED.lifecycle_state;
 ELSIF kind='enrollment' THEN
  INSERT INTO erpx_learning_write.enrollments VALUES(t,k,rid,(d->>'assignment_id')::uuid,d->'learner',(d->>'learner_principal')::uuid,
   (d->'course'->>'course_id')::uuid,(d->'course'->>'version')::bigint,(d->>'due_on')::date,d->>'status',
   d->'attendance',d->'provider_results',d->'waiver',d->'completion')
  ON CONFLICT (holding_id,enrollment_id) DO UPDATE SET lifecycle_state=EXCLUDED.lifecycle_state,attendance_facts=EXCLUDED.attendance_facts,
   provider_results=EXCLUDED.provider_results,waiver_evidence=EXCLUDED.waiver_evidence,completion_evidence=EXCLUDED.completion_evidence;
 ELSIF kind IN ('assignment','completion') THEN
  INSERT INTO erpx_learning_write.governed_records VALUES(t,k,rid,kind,d->'subject',d->>'evidence_digest',d->>'status')
  ON CONFLICT(holding_id,record_id) DO UPDATE SET evidence_digest=EXCLUDED.evidence_digest,lifecycle_state=EXCLUDED.lifecycle_state;
 ELSE RAISE EXCEPTION 'unsupported learning aggregate kind'; END IF;
END $$;

CREATE VIEW erpx_benefits_write.aggregate_rehydration WITH (security_invoker=true) AS
SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',p.plan_id,'version',v.aggregate_version,'scope',v.authority_scope,'name',p.plan_name,'currency',p.currency,
  'eligibility_class',p.eligibility_class,'minimum_tenure_days',p.minimum_tenure_days,'contribution',p.contribution_rule,
  'status',p.lifecycle_state,'updated_at',v.updated_at) AS aggregate_document
FROM erpx_benefits_write.aggregate_versions v JOIN erpx_benefits_write.plans p ON p.holding_id=v.holding_id AND p.aggregate_key=v.aggregate_key
UNION ALL SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',e.enrollment_id,'version',v.aggregate_version,'scope',v.authority_scope,
  'plan',jsonb_build_object('plan_id',e.plan_id,'version',e.plan_version),'window',e.window_id,'employee',e.employee_ref,
  'eligibility',e.eligibility_decision,'life_event',e.life_event,'dependents',e.dependents,'election',e.election,
  'acknowledgements',e.provider_acknowledgements,'payroll_handoff_id',e.payroll_handoff_id,
  'status',e.lifecycle_state,'updated_at',v.updated_at)
FROM erpx_benefits_write.aggregate_versions v JOIN erpx_benefits_write.enrollments e ON e.holding_id=v.holding_id AND e.aggregate_key=v.aggregate_key
UNION ALL SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',r.record_id,'version',v.aggregate_version,'scope',v.authority_scope,'kind',r.record_kind,
 'subject',r.subject_ref,'evidence_digest',r.evidence_digest,'status',r.lifecycle_state,'updated_at',v.updated_at)
FROM erpx_benefits_write.aggregate_versions v JOIN erpx_benefits_write.governed_records r ON r.holding_id=v.holding_id AND r.aggregate_key=v.aggregate_key;

CREATE FUNCTION erpx_benefits_write.persist_normalized(uuid,text,text,uuid,jsonb,bigint,jsonb) RETURNS void LANGUAGE plpgsql AS $$
DECLARE t alias for $1; k alias for $2; kind alias for $3; rid alias for $4; scope alias for $5; ver alias for $6; d alias for $7;
BEGIN
 INSERT INTO erpx_benefits_write.aggregate_versions VALUES(t,k,kind,rid,scope,ver,(d->>'updated_at')::timestamptz)
 ON CONFLICT (holding_id,aggregate_key) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at
 WHERE erpx_benefits_write.aggregate_versions.aggregate_version<EXCLUDED.aggregate_version;
 IF kind='plan' THEN
  INSERT INTO erpx_benefits_write.plans VALUES(t,k,rid,d->>'name',d->>'currency',d->>'eligibility_class',
   (d->>'minimum_tenure_days')::integer,d->'contribution',d->>'status')
  ON CONFLICT (holding_id,plan_id) DO UPDATE SET lifecycle_state=EXCLUDED.lifecycle_state;
 ELSIF kind='enrollment' THEN
  INSERT INTO erpx_benefits_write.enrollments VALUES(t,k,rid,(d->'plan'->>'plan_id')::uuid,(d->'plan'->>'version')::bigint,
   (d->>'window')::uuid,d->'employee',d->'eligibility',d->'life_event',d->'dependents',d->'election',d->'acknowledgements',
   (d->>'payroll_handoff_id')::uuid,d->>'status')
  ON CONFLICT (holding_id,enrollment_id) DO UPDATE SET life_event=EXCLUDED.life_event,dependents=EXCLUDED.dependents,
   election=EXCLUDED.election,provider_acknowledgements=EXCLUDED.provider_acknowledgements,
   payroll_handoff_id=EXCLUDED.payroll_handoff_id,lifecycle_state=EXCLUDED.lifecycle_state;
 ELSIF kind IN ('window','handoff') THEN
  INSERT INTO erpx_benefits_write.governed_records VALUES(t,k,rid,kind,d->'subject',d->>'evidence_digest',d->>'status')
  ON CONFLICT(holding_id,record_id) DO UPDATE SET evidence_digest=EXCLUDED.evidence_digest,lifecycle_state=EXCLUDED.lifecycle_state;
 ELSE RAISE EXCEPTION 'unsupported benefits aggregate kind'; END IF;
END $$;

CREATE VIEW erpx_field_service_write.aggregate_rehydration WITH (security_invoker=true) AS
SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',a.agreement_id,'version',v.aggregate_version,'scope',v.authority_scope,'customer',a.customer_ref,
  'covered_assets',a.covered_assets,'covered_locations',a.covered_locations,'service_codes',a.service_codes,
  'valid_from',a.valid_from,'valid_through',a.valid_through,'active',a.active) AS aggregate_document
FROM erpx_field_service_write.aggregate_versions v JOIN erpx_field_service_write.agreements a ON a.holding_id=v.holding_id AND a.aggregate_key=v.aggregate_key
UNION ALL SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',o.service_order_id,'version',v.aggregate_version,'scope',v.authority_scope,'request',o.request_id,
  'request_version',o.request_version,'agreement',o.agreement_id,'agreement_version',o.agreement_version,'customer',o.customer_ref,
  'asset',o.asset_ref,'location',o.location_ref,'service_code',o.service_code,'required_skills',o.required_skills,
  'scheduled_on',o.scheduled_on,'technician',o.technician_qualification,'dispatch_id',o.dispatch_id,'visit_id',o.visit_id,
  'parts',o.part_reservations,'part_fact_ids',o.part_fact_ids,'evidence',o.visit_evidence,'signature',o.customer_signature,
  'exceptions',o.exceptions,'billing_handoff_id',o.billing_handoff_id,'status',o.lifecycle_state,'updated_at',v.updated_at)
FROM erpx_field_service_write.aggregate_versions v JOIN erpx_field_service_write.service_orders o
 ON o.holding_id=v.holding_id AND o.aggregate_key=v.aggregate_key
UNION ALL SELECT v.holding_id,v.aggregate_key,v.aggregate_kind,v.aggregate_id,v.authority_scope,v.aggregate_version,
 jsonb_build_object('id',r.record_id,'version',v.aggregate_version,'scope',v.authority_scope,'kind',r.record_kind,
 'subject',r.subject_ref,'evidence_digest',r.evidence_digest,'status',r.lifecycle_state,'updated_at',v.updated_at)
FROM erpx_field_service_write.aggregate_versions v JOIN erpx_field_service_write.governed_records r ON r.holding_id=v.holding_id AND r.aggregate_key=v.aggregate_key;

CREATE FUNCTION erpx_field_service_write.persist_normalized(uuid,text,text,uuid,jsonb,bigint,jsonb) RETURNS void LANGUAGE plpgsql AS $$
DECLARE t alias for $1; k alias for $2; kind alias for $3; rid alias for $4; scope alias for $5; ver alias for $6; d alias for $7;
BEGIN
 INSERT INTO erpx_field_service_write.aggregate_versions VALUES(t,k,kind,rid,scope,ver,COALESCE((d->>'updated_at')::timestamptz,clock_timestamp()))
 ON CONFLICT (holding_id,aggregate_key) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at
 WHERE erpx_field_service_write.aggregate_versions.aggregate_version<EXCLUDED.aggregate_version;
 IF kind='agreement' THEN
  INSERT INTO erpx_field_service_write.agreements VALUES(t,k,rid,d->'customer',d->'covered_assets',d->'covered_locations',d->'service_codes',
   (d->>'valid_from')::date,(d->>'valid_through')::date,(d->>'active')::boolean)
  ON CONFLICT (holding_id,agreement_id) DO UPDATE SET active=EXCLUDED.active;
 ELSIF kind='order' THEN
  INSERT INTO erpx_field_service_write.service_orders VALUES(t,k,rid,(d->>'request')::uuid,(d->>'request_version')::bigint,
   (d->>'agreement')::uuid,(d->>'agreement_version')::bigint,d->'customer',d->'asset',d->'location',d->>'service_code',
   d->'required_skills',(d->>'scheduled_on')::date,d->'technician',(d->>'dispatch_id')::uuid,(d->>'visit_id')::uuid,
   d->'parts',d->'part_fact_ids',d->'evidence',d->'signature',d->'exceptions',(d->>'billing_handoff_id')::uuid,d->>'status')
  ON CONFLICT (holding_id,service_order_id) DO UPDATE SET scheduled_on=EXCLUDED.scheduled_on,
   technician_qualification=EXCLUDED.technician_qualification,dispatch_id=EXCLUDED.dispatch_id,visit_id=EXCLUDED.visit_id,
   part_reservations=EXCLUDED.part_reservations,part_fact_ids=EXCLUDED.part_fact_ids,visit_evidence=EXCLUDED.visit_evidence,
   customer_signature=EXCLUDED.customer_signature,exceptions=EXCLUDED.exceptions,billing_handoff_id=EXCLUDED.billing_handoff_id,
   lifecycle_state=EXCLUDED.lifecycle_state;
 ELSIF kind IN ('request','dispatch','visit','handoff') THEN
  INSERT INTO erpx_field_service_write.governed_records VALUES(t,k,rid,kind,d->'subject',d->>'evidence_digest',d->>'status')
  ON CONFLICT(holding_id,record_id) DO UPDATE SET evidence_digest=EXCLUDED.evidence_digest,lifecycle_state=EXCLUDED.lifecycle_state;
 ELSE RAISE EXCEPTION 'unsupported field-service aggregate kind'; END IF;
END $$;

DO $$
DECLARE relation_name text;
BEGIN
  FOREACH relation_name IN ARRAY ARRAY[
    'erpx_learning_write.aggregate_versions','erpx_learning_write.courses','erpx_learning_write.enrollments','erpx_learning_write.governed_records','erpx_learning_write.attendance_guards','erpx_learning_write.provider_result_guards','erpx_learning_write.completion_facts','erpx_learning_write.command_receipts','erpx_learning_write.audit_log','erpx_learning_write.outbox',
    'erpx_benefits_write.aggregate_versions','erpx_benefits_write.plans','erpx_benefits_write.enrollments','erpx_benefits_write.governed_records','erpx_benefits_write.eligibility_guards','erpx_benefits_write.provider_acknowledgement_guards','erpx_benefits_write.payroll_handoff_guards','erpx_benefits_write.command_receipts','erpx_benefits_write.audit_log','erpx_benefits_write.outbox',
    'erpx_field_service_write.aggregate_versions','erpx_field_service_write.agreements','erpx_field_service_write.service_orders','erpx_field_service_write.governed_records','erpx_field_service_write.part_issue_guards','erpx_field_service_write.visit_evidence_guards','erpx_field_service_write.exception_guards','erpx_field_service_write.billing_handoff_guards','erpx_field_service_write.command_receipts','erpx_field_service_write.audit_log','erpx_field_service_write.outbox'
  ] LOOP
    EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', relation_name);
    EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', relation_name);
    EXECUTE format('CREATE POLICY holding_isolation ON %s USING (holding_id = nullif(current_setting(''app.holding_id'', true), '''')::uuid) WITH CHECK (holding_id = nullif(current_setting(''app.holding_id'', true), '''')::uuid)', relation_name);
  END LOOP;
END $$;


-- Shared enterprise-service authority. Source-context business records remain
-- authoritative. JSON columns below are bounded documents/evidence only; no
-- table stores a serialized aggregate snapshot as write authority.

CREATE SCHEMA IF NOT EXISTS erpx_shared_write;

CREATE OR REPLACE FUNCTION erpx_shared_write.current_holding_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.holding_id', true), '')::uuid
$$;

CREATE TABLE erpx_shared_write.approval_sessions (
    holding_id UUID NOT NULL DEFAULT erpx_shared_write.current_holding_id(),
    session_id UUID NOT NULL,
    scope_key TEXT NOT NULL CHECK (octet_length(scope_key) BETWEEN 1 AND 2048),
    source_record_id UUID NOT NULL,
    source_record_version BIGINT NOT NULL CHECK (source_record_version > 0),
    policy_id UUID NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    policy_sha256 TEXT NOT NULL CHECK (policy_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    session_status TEXT NOT NULL CHECK (session_status IN (
        'pending','approved','rejected','changes_requested','invalidated','expired'
    )),
    current_gate_id TEXT,
    due_at TIMESTAMPTZ NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    -- Evidence for evaluated gates; never an approval-session snapshot.
    gate_evidence JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, session_id),
    UNIQUE (holding_id, session_id, aggregate_version)
);

CREATE TABLE erpx_shared_write.approval_decisions (
    holding_id UUID NOT NULL DEFAULT erpx_shared_write.current_holding_id(),
    session_id UUID NOT NULL,
    decision_sequence BIGINT NOT NULL CHECK (decision_sequence > 0),
    gate_id TEXT NOT NULL,
    actor_ref TEXT NOT NULL,
    represented_actor_ref TEXT,
    outcome TEXT NOT NULL CHECK (outcome IN ('approve','reject','request_changes')),
    authority_version BIGINT NOT NULL CHECK (authority_version > 0),
    authority_sha256 TEXT NOT NULL CHECK (authority_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    -- Immutable versions/digests used by the decision, not mutable business state.
    evidence_versions JSONB NOT NULL,
    decided_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, session_id, decision_sequence),
    FOREIGN KEY (holding_id, session_id)
        REFERENCES erpx_shared_write.approval_sessions(holding_id, session_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_shared_write.notifications (
    holding_id UUID NOT NULL DEFAULT erpx_shared_write.current_holding_id(),
    notification_id UUID NOT NULL,
    scope_key TEXT NOT NULL CHECK (octet_length(scope_key) BETWEEN 1 AND 2048),
    holding_scope JSONB NOT NULL,
    source_id TEXT NOT NULL CHECK (octet_length(source_id) BETWEEN 1 AND 255),
    source_record_ref TEXT NOT NULL CHECK (octet_length(source_record_ref) BETWEEN 1 AND 255),
    recipient_ref TEXT NOT NULL CHECK (octet_length(recipient_ref) BETWEEN 1 AND 255),
    classification TEXT NOT NULL CHECK (classification IN (
        'public','internal','confidential','restricted'
    )),
    preview TEXT CHECK (octet_length(preview) <= 1024),
    inbox_state TEXT NOT NULL CHECK (inbox_state IN (
        'unread','read','acknowledged','action_required','withdrawn'
    )),
    mandatory_policy_version TEXT,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    delivery_version BIGINT NOT NULL CHECK (delivery_version > 0),
    evidence_sha256 TEXT NOT NULL CHECK (evidence_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, notification_id),
    UNIQUE (holding_id, notification_id, aggregate_version)
);

CREATE TABLE erpx_shared_write.notification_delivery_attempts (
    holding_id UUID NOT NULL DEFAULT erpx_shared_write.current_holding_id(),
    notification_id UUID NOT NULL,
    recipient_ref TEXT NOT NULL,
    channel TEXT NOT NULL CHECK (channel IN ('in_app','email','sms','push','webhook')),
    delivery_version BIGINT NOT NULL CHECK (delivery_version > 0),
    dedupe_key TEXT NOT NULL,
    delivery_status TEXT NOT NULL CHECK (delivery_status IN (
        'queued','accepted','delivered','failed','uncertain','suppressed'
    )),
    provider_message_ref TEXT,
    last_provider_sequence BIGINT NOT NULL DEFAULT 0 CHECK (last_provider_sequence >= 0),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error_code TEXT,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, notification_id, recipient_ref, channel, delivery_version),
    UNIQUE (holding_id, dedupe_key),
    FOREIGN KEY (holding_id, notification_id)
        REFERENCES erpx_shared_write.notifications(holding_id, notification_id)
        ON DELETE CASCADE
);

CREATE TABLE erpx_shared_write.notification_delivery_callbacks (
    holding_id UUID NOT NULL DEFAULT erpx_shared_write.current_holding_id(),
    notification_id UUID NOT NULL,
    recipient_ref TEXT NOT NULL,
    channel TEXT NOT NULL,
    delivery_version BIGINT NOT NULL,
    callback_id TEXT NOT NULL,
    callback_ordinal INTEGER NOT NULL CHECK (callback_ordinal > 0),
    processed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, notification_id, recipient_ref, channel, delivery_version, callback_id),
    UNIQUE (holding_id, notification_id, recipient_ref, channel, delivery_version, callback_ordinal),
    FOREIGN KEY (holding_id, notification_id, recipient_ref, channel, delivery_version)
        REFERENCES erpx_shared_write.notification_delivery_attempts(
            holding_id, notification_id, recipient_ref, channel, delivery_version
        ) ON DELETE CASCADE
);

CREATE TABLE erpx_shared_write.signature_envelopes (
    holding_id UUID NOT NULL DEFAULT erpx_shared_write.current_holding_id(),
    envelope_id UUID NOT NULL,
    scope_key TEXT NOT NULL CHECK (octet_length(scope_key) BETWEEN 1 AND 2048),
    holding_scope JSONB NOT NULL,
    document_id UUID NOT NULL,
    document_version BIGINT NOT NULL CHECK (document_version > 0),
    document_sha256 TEXT NOT NULL CHECK (document_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    provider_key TEXT NOT NULL CHECK (octet_length(provider_key) BETWEEN 1 AND 255),
    provider_policy_version TEXT NOT NULL CHECK (
        octet_length(provider_policy_version) BETWEEN 1 AND 255
    ),
    signature_order TEXT NOT NULL CHECK (signature_order IN ('parallel','serial')),
    retention_until TIMESTAMPTZ NOT NULL,
    envelope_status TEXT NOT NULL CHECK (envelope_status IN (
        'draft','submitted','in_progress','completed','failed','cancelled','expired'
    )),
    provider_envelope_ref TEXT,
    last_provider_sequence BIGINT NOT NULL DEFAULT 0 CHECK (last_provider_sequence >= 0),
    evidence_sha256 TEXT CHECK (evidence_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    mutation_evidence_sha256 TEXT NOT NULL CHECK (
        mutation_evidence_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'
    ),
    created_by TEXT NOT NULL CHECK (octet_length(created_by) BETWEEN 1 AND 255),
    cancelled_by TEXT,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, envelope_id),
    UNIQUE (holding_id, envelope_id, aggregate_version),
    UNIQUE (holding_id, provider_envelope_ref)
);

CREATE TABLE erpx_shared_write.signature_recipients (
    holding_id UUID NOT NULL DEFAULT erpx_shared_write.current_holding_id(),
    envelope_id UUID NOT NULL,
    recipient_id UUID NOT NULL,
    recipient_order INTEGER NOT NULL CHECK (recipient_order > 0),
    subject_realm TEXT NOT NULL CHECK (subject_realm IN ('internal','external_one_time')),
    principal_ref TEXT,
    external_subject_ref TEXT,
    access_grant_sha256 TEXT CHECK (access_grant_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    access_expires_at TIMESTAMPTZ,
    signed_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, envelope_id, recipient_id),
    FOREIGN KEY (holding_id, envelope_id)
        REFERENCES erpx_shared_write.signature_envelopes(holding_id, envelope_id)
        ON DELETE CASCADE,
    CHECK (
        (subject_realm = 'internal' AND principal_ref IS NOT NULL
            AND external_subject_ref IS NULL AND access_grant_sha256 IS NULL
            AND access_expires_at IS NULL)
        OR
        (subject_realm = 'external_one_time' AND principal_ref IS NULL
            AND external_subject_ref IS NOT NULL AND access_grant_sha256 IS NOT NULL
            AND access_expires_at IS NOT NULL)
    )
);

CREATE TABLE erpx_shared_write.signature_provider_events (
    holding_id UUID NOT NULL DEFAULT erpx_shared_write.current_holding_id(),
    envelope_id UUID NOT NULL,
    provider_event_id TEXT NOT NULL,
    event_ordinal INTEGER NOT NULL CHECK (event_ordinal > 0),
    processed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, envelope_id, provider_event_id),
    UNIQUE (holding_id, envelope_id, event_ordinal),
    FOREIGN KEY (holding_id, envelope_id)
        REFERENCES erpx_shared_write.signature_envelopes(holding_id, envelope_id)
        ON DELETE CASCADE
);

CREATE TABLE erpx_shared_write.report_export_manifests (
    holding_id UUID NOT NULL DEFAULT erpx_shared_write.current_holding_id(),
    export_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    definition_id UUID NOT NULL,
    definition_version BIGINT NOT NULL CHECK (definition_version > 0),
    manifest_sha256 TEXT NOT NULL CHECK (manifest_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    classification TEXT NOT NULL CHECK (classification IN (
        'public','internal','confidential','restricted'
    )),
    status TEXT NOT NULL CHECK (status IN (
        'queued','running','completed','failed','cancelled','expired','deletion_pending','deleted'
    )),
    expires_at TIMESTAMPTZ NOT NULL,
    -- Signed delivery manifest; it is not a report-job aggregate snapshot.
    manifest JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, export_id),
    UNIQUE (holding_id, export_id, manifest_sha256)
);

CREATE TABLE erpx_shared_write.import_row_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_shared_write.current_holding_id(),
    batch_id UUID NOT NULL,
    row_number INTEGER NOT NULL CHECK (row_number > 0),
    command_id UUID NOT NULL,
    source_receipt_ref TEXT,
    rejection_code TEXT,
    reconciled_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, batch_id, row_number),
    UNIQUE (holding_id, command_id),
    CHECK ((source_receipt_ref IS NULL) <> (rejection_code IS NULL))
);

CREATE TABLE erpx_shared_write.extension_schema_versions (
    holding_id UUID NOT NULL DEFAULT erpx_shared_write.current_holding_id(),
    extension_id UUID NOT NULL,
    extension_version BIGINT NOT NULL CHECK (extension_version > 0),
    scope_key TEXT NOT NULL,
    owner_ref TEXT NOT NULL,
    affected_context TEXT NOT NULL,
    extension_kind TEXT NOT NULL CHECK (
        extension_kind IN ('custom_field','form','workflow_template')
    ),
    approval_sha256 TEXT CHECK (approval_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    active BOOLEAN NOT NULL,
    -- Versioned field/form/workflow schema, never executable code or SQL.
    definition JSONB NOT NULL,
    published_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, extension_id, extension_version)
);

CREATE TABLE erpx_shared_write.signature_callback_inbox (
    holding_id UUID NOT NULL DEFAULT erpx_shared_write.current_holding_id(),
    provider_key TEXT NOT NULL,
    provider_event_id TEXT NOT NULL,
    provider_envelope_ref TEXT NOT NULL,
    provider_sequence BIGINT NOT NULL CHECK (provider_sequence > 0),
    callback_sha256 TEXT NOT NULL CHECK (callback_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    verified BOOLEAN NOT NULL,
    received_at TIMESTAMPTZ NOT NULL,
    applied_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, provider_key, provider_event_id),
    UNIQUE (holding_id, provider_key, provider_envelope_ref, provider_sequence)
);

CREATE INDEX shared_approval_due
    ON erpx_shared_write.approval_sessions(holding_id, scope_key, due_at)
    WHERE session_status = 'pending';
CREATE INDEX shared_notification_recipient
    ON erpx_shared_write.notifications(holding_id, recipient_ref, inbox_state, updated_at DESC);
CREATE INDEX shared_notification_retry
    ON erpx_shared_write.notification_delivery_attempts(holding_id, delivery_status, updated_at)
    WHERE delivery_status IN ('queued','uncertain');
CREATE INDEX shared_signature_retention
    ON erpx_shared_write.signature_envelopes(holding_id, retention_until, envelope_status);
CREATE INDEX shared_report_expiry
    ON erpx_shared_write.report_export_manifests(holding_id, expires_at)
    WHERE status IN ('completed','deletion_pending');

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'approval_sessions','approval_decisions','notifications',
        'notification_delivery_attempts','notification_delivery_callbacks',
        'signature_envelopes','signature_recipients','signature_provider_events',
        'report_export_manifests','import_row_receipts',
        'extension_schema_versions','signature_callback_inbox'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_shared_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_shared_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY shared_holding_isolation ON erpx_shared_write.%I '
            'USING (holding_id = erpx_shared_write.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_shared_write.current_holding_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- Final current role marker: erpx_meta.schema_manifest,
-- schema_revision=2, database_role=holding_write, migration_set=holding-write.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_revision = 2
  AND database_role = 'holding_write' AND migration_set = 'holding-write';


-- Normalized authority evidence for IAM and Organization. These tables are
-- command-side guards/evidence, not serialized aggregate snapshots. Every
-- mutation occurs inside the owning context command transaction.

CREATE SCHEMA IF NOT EXISTS erpx_authority_write;

CREATE OR REPLACE FUNCTION erpx_authority_write.current_holding_id()
RETURNS UUID LANGUAGE SQL STABLE AS $$
    SELECT nullif(current_setting('app.holding_id', true), '')::uuid
$$;

CREATE TABLE erpx_authority_write.principals (
    holding_id UUID NOT NULL DEFAULT erpx_authority_write.current_holding_id(),
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
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, principal_id),
    UNIQUE (holding_id, principal_id, principal_version)
);

CREATE TABLE erpx_authority_write.roles (
    holding_id UUID NOT NULL DEFAULT erpx_authority_write.current_holding_id(),
    role_id UUID NOT NULL,
    role_scope JSONB NOT NULL,
    role_version BIGINT NOT NULL CHECK (role_version > 0),
    permissions TEXT[] NOT NULL CHECK (cardinality(permissions) BETWEEN 1 AND 256),
    classification_ceiling TEXT NOT NULL CHECK (
        classification_ceiling IN ('public','internal','confidential','restricted')
    ),
    active BOOLEAN NOT NULL,
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, role_id),
    UNIQUE (holding_id, role_id, role_version)
);

CREATE TABLE erpx_authority_write.assignments (
    holding_id UUID NOT NULL DEFAULT erpx_authority_write.current_holding_id(),
    assignment_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    role_id UUID NOT NULL,
    assignment_scope JSONB NOT NULL,
    assignment_version BIGINT NOT NULL CHECK (assignment_version > 0),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, assignment_id),
    UNIQUE (holding_id, assignment_id, assignment_version),
    FOREIGN KEY (holding_id, principal_id)
        REFERENCES erpx_authority_write.principals(holding_id, principal_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (holding_id, role_id)
        REFERENCES erpx_authority_write.roles(holding_id, role_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (valid_until > valid_from)
);

CREATE TABLE erpx_authority_write.group_memberships (
    holding_id UUID NOT NULL DEFAULT erpx_authority_write.current_holding_id(),
    group_id UUID NOT NULL,
    member_principal_id UUID NOT NULL,
    group_kind TEXT NOT NULL CHECK (group_kind IN ('human_group','agent_pool')),
    group_version BIGINT NOT NULL CHECK (group_version > 0),
    membership_version BIGINT NOT NULL CHECK (membership_version > 0),
    membership_scope JSONB NOT NULL,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, group_id, member_principal_id),
    UNIQUE (holding_id, group_id, member_principal_id, membership_version),
    FOREIGN KEY (holding_id, group_id)
        REFERENCES erpx_authority_write.principals(holding_id, principal_id)
        ON DELETE CASCADE,
    FOREIGN KEY (holding_id, member_principal_id)
        REFERENCES erpx_authority_write.principals(holding_id, principal_id)
        ON DELETE CASCADE,
    CHECK (group_id <> member_principal_id),
    CHECK (valid_until > valid_from)
);

CREATE TABLE erpx_authority_write.organization_sharing_agreements (
    holding_id UUID NOT NULL DEFAULT erpx_authority_write.current_holding_id(),
    sharing_agreement_id UUID NOT NULL,
    agreement_version BIGINT NOT NULL CHECK (agreement_version > 0),
    source_scope JSONB NOT NULL,
    target_scope JSONB NOT NULL,
    direction TEXT NOT NULL CHECK (direction IN ('source_to_target','mutual')),
    capabilities TEXT[] NOT NULL CHECK (cardinality(capabilities) BETWEEN 1 AND 4),
    reason TEXT NOT NULL CHECK (octet_length(reason) BETWEEN 1 AND 256),
    proposed_by TEXT NOT NULL CHECK (octet_length(proposed_by) BETWEEN 1 AND 256),
    source_approved_by TEXT,
    target_approved_by TEXT,
    rejected_by TEXT,
    rejected_at TIMESTAMPTZ,
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    sharing_status TEXT NOT NULL CHECK (
        sharing_status IN ('proposed','active','rejected','revoked','expired')
    ),
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, sharing_agreement_id),
    UNIQUE (holding_id, sharing_agreement_id, agreement_version),
    CHECK (valid_until > valid_from),
    CHECK ((rejected_by IS NULL) = (rejected_at IS NULL)),
    CHECK (sharing_status <> 'active' OR (
        source_approved_by IS NOT NULL AND target_approved_by IS NOT NULL
        AND 'open_record' = ANY(capabilities)
    ))
);

CREATE TABLE erpx_authority_write.sharing_command_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_authority_write.current_holding_id(),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) BETWEEN 1 AND 256),
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash) = 32),
    sharing_agreement_id UUID NOT NULL,
    agreement_version BIGINT NOT NULL CHECK (agreement_version > 0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, idempotency_key),
    UNIQUE (holding_id, command_id),
    FOREIGN KEY (holding_id, sharing_agreement_id)
        REFERENCES erpx_authority_write.organization_sharing_agreements
            (holding_id, sharing_agreement_id)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE erpx_authority_write.sharing_audit_log (
    holding_id UUID NOT NULL DEFAULT erpx_authority_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    actor JSONB NOT NULL,
    authority_scope JSONB NOT NULL,
    sharing_agreement_id UUID NOT NULL,
    agreement_version BIGINT NOT NULL CHECK (agreement_version > 0),
    action TEXT NOT NULL CHECK (action IN (
        'propose','approve_source','approve_target','reject','revoke','expire'
    )),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, audit_id),
    UNIQUE (holding_id, command_id),
    FOREIGN KEY (holding_id, sharing_agreement_id)
        REFERENCES erpx_authority_write.organization_sharing_agreements
            (holding_id, sharing_agreement_id)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE erpx_authority_write.customer_portal_principal_bindings (
    holding_id UUID NOT NULL DEFAULT erpx_authority_write.current_holding_id(),
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
    verified_at TIMESTAMPTZ NOT NULL,
    permitted_use_cases TEXT[] NOT NULL CHECK (
        cardinality(permitted_use_cases) = 1
        AND permitted_use_cases <@ ARRAY['field_service_record_access']::TEXT[]
    ),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    proposed_by UUID NOT NULL,
    approved_by UUID,
    rejected_by UUID,
    revoked_by UUID,
    ended_at TIMESTAMPTZ,
    decision_evidence_digest TEXT CHECK (
        decision_evidence_digest IS NULL
        OR decision_evidence_digest ~ '^[0-9A-Fa-f]{64}$'
    ),
    binding_status TEXT NOT NULL CHECK (
        binding_status IN ('proposed','active','rejected','revoked','expired')
    ),
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, binding_id),
    UNIQUE (holding_id, binding_id, binding_version),
    FOREIGN KEY (holding_id, portal_principal_id)
        REFERENCES erpx_authority_write.principals(holding_id, principal_id)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (holding_id, proposed_by)
        REFERENCES erpx_authority_write.principals(holding_id, principal_id)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (holding_id, approved_by)
        REFERENCES erpx_authority_write.principals(holding_id, principal_id)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (holding_id, rejected_by)
        REFERENCES erpx_authority_write.principals(holding_id, principal_id)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (holding_id, revoked_by)
        REFERENCES erpx_authority_write.principals(holding_id, principal_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (valid_until > valid_from),
    CHECK (binding_status <> 'active' OR (
        approved_by IS NOT NULL AND approved_by <> proposed_by
        AND ended_at IS NULL AND decision_evidence_digest IS NOT NULL
    )),
    CHECK (binding_status <> 'rejected' OR (
        rejected_by IS NOT NULL AND rejected_by <> proposed_by
        AND ended_at IS NOT NULL AND decision_evidence_digest IS NOT NULL
    )),
    CHECK (binding_status <> 'revoked' OR (
        approved_by IS NOT NULL AND revoked_by IS NOT NULL
        AND ended_at IS NOT NULL AND decision_evidence_digest IS NOT NULL
    )),
    CHECK (binding_status <> 'expired' OR ended_at IS NOT NULL)
);

CREATE TABLE erpx_authority_write.portal_binding_command_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_authority_write.current_holding_id(),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) BETWEEN 1 AND 256),
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash) = 32),
    binding_id UUID NOT NULL,
    binding_version BIGINT NOT NULL CHECK (binding_version > 0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, idempotency_key),
    UNIQUE (holding_id, command_id),
    FOREIGN KEY (holding_id, binding_id)
        REFERENCES erpx_authority_write.customer_portal_principal_bindings
            (holding_id, binding_id)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE erpx_authority_write.portal_binding_audit_log (
    holding_id UUID NOT NULL DEFAULT erpx_authority_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    actor JSONB NOT NULL,
    authority_scope JSONB NOT NULL,
    binding_id UUID NOT NULL,
    binding_version BIGINT NOT NULL CHECK (binding_version > 0),
    customer_record_id UUID NOT NULL,
    portal_principal_id UUID NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('propose','approve','reject','revoke','expire')),
    evidence_digest TEXT,
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, audit_id),
    UNIQUE (holding_id, command_id),
    FOREIGN KEY (holding_id, binding_id)
        REFERENCES erpx_authority_write.customer_portal_principal_bindings
            (holding_id, binding_id)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE INDEX authority_assignments_effective
    ON erpx_authority_write.assignments
        (holding_id, principal_id, role_id, valid_from, valid_until)
    WHERE revoked_at IS NULL;
CREATE INDEX authority_memberships_effective
    ON erpx_authority_write.group_memberships
        (holding_id, group_id, member_principal_id, valid_from, valid_until)
    WHERE revoked_at IS NULL;
CREATE INDEX authority_sharing_effective
    ON erpx_authority_write.organization_sharing_agreements
        (holding_id, sharing_status, valid_from, valid_until)
    WHERE sharing_status = 'active';
CREATE UNIQUE INDEX authority_portal_binding_one_current
    ON erpx_authority_write.customer_portal_principal_bindings
        (holding_id, customer_record_id, portal_principal_id)
    WHERE binding_status IN ('proposed','active');
CREATE INDEX authority_portal_binding_effective
    ON erpx_authority_write.customer_portal_principal_bindings
        (holding_id, portal_principal_id, customer_record_id, valid_from, valid_until)
    WHERE binding_status = 'active';

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'principals','roles','assignments','group_memberships',
        'organization_sharing_agreements','sharing_command_receipts','sharing_audit_log',
        'customer_portal_principal_bindings','portal_binding_command_receipts',
        'portal_binding_audit_log'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_authority_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_authority_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY authority_write_holding_isolation ON erpx_authority_write.%I '
            'USING (holding_id = erpx_authority_write.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_authority_write.current_holding_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- Final current role marker: erpx_meta.schema_manifest,
-- schema_revision=2, database_role=holding_write, migration_set=holding-write.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_revision = 2
  AND database_role = 'holding_write' AND migration_set = 'holding-write';


-- current completion for current-context first-class records.
-- This migration is additive and never removes an existing database or row.

-- CRM owns the durable, normalized copy of Marketing's published qualified
-- lead evidence. Commands resolve this inbox by holding and candidate rather
-- than trusting a client-supplied fact or reading Marketing's repository.
ALTER TABLE erpx_crm_write.lead_guards
    ADD COLUMN holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    ADD COLUMN campaign_id UUID NOT NULL,
    ADD COLUMN audience_member_id UUID NOT NULL,
    ADD COLUMN marketing_source_version BIGINT NOT NULL
        CHECK (marketing_source_version > 0);
ALTER TABLE erpx_crm_write.opportunity_guards
    ADD COLUMN holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id();

ALTER TABLE erpx_crm_write.opportunity_guards
    DROP CONSTRAINT opportunity_guards_accepted_lead_id_fkey,
    DROP CONSTRAINT opportunity_guards_pkey;
ALTER TABLE erpx_crm_write.lead_guards
    DROP CONSTRAINT lead_guards_marketing_candidate_id_key,
    DROP CONSTRAINT lead_guards_pkey;
ALTER TABLE erpx_crm_write.lead_guards
    ADD PRIMARY KEY (holding_id, lead_id),
    ADD UNIQUE (holding_id, marketing_candidate_id),
    ADD UNIQUE (holding_id, lead_id, marketing_candidate_id, marketing_source_version);
ALTER TABLE erpx_crm_write.opportunity_guards
    ADD PRIMARY KEY (holding_id, opportunity_id),
    ADD FOREIGN KEY (holding_id, accepted_lead_id)
        REFERENCES erpx_crm_write.lead_guards (holding_id, lead_id);

CREATE TABLE erpx_crm_write.marketing_qualified_lead_evidence (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    marketing_candidate_id UUID NOT NULL,
    campaign_id UUID NOT NULL,
    audience_member_id UUID NOT NULL,
    customer_id UUID,
    source_code TEXT NOT NULL CHECK (octet_length(source_code) BETWEEN 1 AND 255),
    qualification_score_basis_points INTEGER NOT NULL
        CHECK (qualification_score_basis_points BETWEEN 1 AND 10000),
    marketing_source_version BIGINT NOT NULL CHECK (marketing_source_version > 0),
    source_event_id UUID NOT NULL,
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    source_occurred_at TIMESTAMPTZ NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_id, marketing_candidate_id),
    UNIQUE (holding_id, source_event_id),
    UNIQUE (holding_id, marketing_candidate_id, marketing_source_version)
);
CREATE INDEX crm_qualified_lead_evidence_scope
    ON erpx_crm_write.marketing_qualified_lead_evidence
        (holding_id, scope_key, marketing_source_version, marketing_candidate_id);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'lead_guards','opportunity_guards','marketing_qualified_lead_evidence'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_crm_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_crm_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY crm_write_holding_isolation ON erpx_crm_write.%I '
            'USING (holding_id = erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_write.current_holding_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- Identity governance roots are normalized separately from principal/role
-- snapshots so an access review or emergency session has its own lifecycle,
-- receipt, audit evidence, and first-class record identity.
CREATE TABLE erpx_authority_write.access_reviews (
    holding_id UUID NOT NULL DEFAULT erpx_authority_write.current_holding_id(),
    access_review_id UUID NOT NULL,
    review_version BIGINT NOT NULL CHECK (review_version > 0),
    review_scope JSONB NOT NULL,
    reviewer_ref TEXT NOT NULL CHECK (octet_length(reviewer_ref) BETWEEN 1 AND 256),
    due_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, access_review_id),
    UNIQUE (holding_id, access_review_id, review_version)
);
CREATE TABLE erpx_authority_write.access_review_items (
    holding_id UUID NOT NULL DEFAULT erpx_authority_write.current_holding_id(),
    access_review_id UUID NOT NULL,
    assignment_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    principal_kind TEXT NOT NULL CHECK (
        principal_kind IN ('human','service','agent','human_group','agent_pool')
    ),
    owner_ref TEXT NOT NULL CHECK (octet_length(owner_ref) BETWEEN 1 AND 256),
    grantor_ref TEXT NOT NULL CHECK (octet_length(grantor_ref) BETWEEN 1 AND 256),
    decision TEXT CHECK (decision IN ('retain','revoke')),
    decided_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, access_review_id, assignment_id),
    FOREIGN KEY (holding_id, access_review_id)
        REFERENCES erpx_authority_write.access_reviews (holding_id, access_review_id)
        ON DELETE CASCADE,
    CHECK ((decision IS NULL) = (decided_at IS NULL))
);
CREATE TABLE erpx_authority_write.break_glass_sessions (
    holding_id UUID NOT NULL DEFAULT erpx_authority_write.current_holding_id(),
    break_glass_session_id UUID NOT NULL,
    session_version BIGINT NOT NULL CHECK (session_version > 0),
    principal_id UUID NOT NULL,
    session_scope JSONB NOT NULL,
    reason TEXT NOT NULL CHECK (octet_length(reason) BETWEEN 1 AND 256),
    requester_ref TEXT NOT NULL CHECK (octet_length(requester_ref) BETWEEN 1 AND 256),
    approver_ref TEXT,
    requested_at TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    activated_at TIMESTAMPTZ,
    closed_at TIMESTAMPTZ,
    after_reviewed_at TIMESTAMPTZ,
    session_status TEXT NOT NULL CHECK (
        session_status IN ('requested','active','closed','expired')
    ),
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, break_glass_session_id),
    UNIQUE (holding_id, break_glass_session_id, session_version),
    CHECK (valid_until > requested_at)
);
CREATE TABLE erpx_authority_write.identity_governance_command_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_authority_write.current_holding_id(),
    idempotency_key TEXT NOT NULL CHECK (octet_length(idempotency_key) BETWEEN 1 AND 256),
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash) = 32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, idempotency_key),
    UNIQUE (holding_id, command_id)
);
CREATE TABLE erpx_authority_write.identity_governance_audit_log (
    holding_id UUID NOT NULL DEFAULT erpx_authority_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    actor JSONB NOT NULL,
    authority_scope JSONB NOT NULL,
    record_id UUID NOT NULL,
    record_type TEXT NOT NULL CHECK (
        record_type IN ('identity.access-review','identity.break-glass-session')
    ),
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    action TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, audit_id),
    UNIQUE (holding_id, command_id)
);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'access_reviews','access_review_items','break_glass_sessions',
        'identity_governance_command_receipts','identity_governance_audit_log'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_authority_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_authority_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY identity_governance_holding_isolation ON erpx_authority_write.%I '
            'USING (holding_id = erpx_authority_write.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_authority_write.current_holding_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch B/F: People & Payroll owns employment and payslip lifecycle rows.
-- These are normalized child authorities derived inside the same guarded UoW
-- as the employee/payroll aggregate; they are not aggregate JSON aliases.
CREATE TABLE erpx_people_payroll_write.employment_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    employment_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    source_employee_version BIGINT NOT NULL CHECK (source_employee_version > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    hired_on DATE NOT NULL,
    terminated_on DATE,
    employment_status TEXT NOT NULL CHECK (employment_status IN (
        'draft','active','on_leave','suspended','terminated'
    )),
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, employment_id),
    UNIQUE (holding_id, employee_id),
    UNIQUE (holding_id, employment_id, source_employee_version),
    CHECK (
        (employment_status = 'terminated' AND terminated_on IS NOT NULL
            AND terminated_on >= hired_on)
        OR (employment_status <> 'terminated' AND terminated_on IS NULL)
    )
);
CREATE TABLE erpx_people_payroll_write.payslip_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    payslip_id UUID NOT NULL,
    payroll_run_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    source_payroll_version BIGINT NOT NULL CHECK (source_payroll_version > 0),
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
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, payslip_id),
    UNIQUE (holding_id, payroll_run_id, employee_id),
    UNIQUE (holding_id, payslip_id, source_payroll_version),
    CHECK (ends_on >= starts_on),
    CHECK (gross_minor - deductions_minor = net_minor)
);
CREATE INDEX employment_records_scope_status
    ON erpx_people_payroll_write.employment_records
        (holding_id, scope_key, employment_status, employment_id);
CREATE INDEX payslip_records_scope_status
    ON erpx_people_payroll_write.payslip_records
        (holding_id, scope_key, payslip_status, payroll_run_id, employee_id);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY['employment_records','payslip_records'] LOOP
        EXECUTE format('ALTER TABLE erpx_people_payroll_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_people_payroll_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY people_completion_holding_isolation ON erpx_people_payroll_write.%I '
            'USING (holding_id = erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id = erpx_cqrs_write.current_holding_id())',
            table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch B/F: Legal hold is a governed entity inside LegalMatter. The
-- distinct opaque identity and normalized row follow place/release atomically.
CREATE TABLE erpx_legal_write.legal_hold_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    legal_hold_id UUID NOT NULL,
    matter_id UUID NOT NULL,
    source_matter_version BIGINT NOT NULL CHECK (source_matter_version > 0),
    holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL,
    hold_active BOOLEAN NOT NULL,
    reason TEXT NOT NULL,
    placed_by TEXT NOT NULL,
    placed_at TIMESTAMPTZ NOT NULL,
    released_by TEXT,
    released_at TIMESTAMPTZ,
    changed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, legal_hold_id),
    UNIQUE (holding_id, matter_id),
    UNIQUE (holding_id, legal_hold_id, source_matter_version),
    CHECK (
        (hold_active AND released_by IS NULL AND released_at IS NULL)
        OR (NOT hold_active AND released_by IS NOT NULL
            AND released_at IS NOT NULL AND released_at >= placed_at)
    )
);
CREATE INDEX legal_hold_records_scope_active
    ON erpx_legal_write.legal_hold_records
        (holding_id, scope_key, hold_active, legal_hold_id);
ALTER TABLE erpx_legal_write.legal_hold_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_legal_write.legal_hold_records FORCE ROW LEVEL SECURITY;
CREATE POLICY legal_hold_record_holding_isolation
    ON erpx_legal_write.legal_hold_records
    USING (holding_id = erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id = erpx_cqrs_write.current_holding_id());

-- D13 Batch G: Marketing-owned first-class campaign, content, audience
-- activation and consent-purpose lifecycles.
CREATE TABLE erpx_marketing_write.business_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'campaign','content','audience_activation','consent_purpose'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','proposed','approved','active','paused','running','published',
        'completed','cancelled','retired'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,record_id),
    UNIQUE (holding_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_marketing_write.business_record_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,idempotency_key),
    UNIQUE (holding_id,command_id)
);
CREATE TABLE erpx_marketing_write.business_record_audit (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,audit_id),
    UNIQUE (holding_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_marketing_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_marketing_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY marketing_record_holding_isolation ON erpx_marketing_write.%I '
            'USING (holding_id=erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch G: CRM-owned first-class activity and forecast lifecycles.
CREATE TABLE erpx_crm_write.business_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'activity','forecast'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','planned','in_progress','submitted','approved','completed',
        'cancelled','superseded'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,record_id),
    UNIQUE (holding_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_crm_write.business_record_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,idempotency_key),
    UNIQUE (holding_id,command_id)
);
CREATE TABLE erpx_crm_write.business_record_audit (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,audit_id),
    UNIQUE (holding_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_crm_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_crm_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY crm_record_holding_isolation ON erpx_crm_write.%I '
            'USING (holding_id=erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())', table_name
        );
    END LOOP;
END;
$$;



-- D13 Batch G: Finance-owned first-class activity and forecast lifecycles.
CREATE TABLE erpx_finance_write.business_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'credit_note','debit_note','payment','reconciliation'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','pending','open','posted','authorized','matched','applied',
        'settled','closed','failed','cancelled','void'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,record_id),
    UNIQUE (holding_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_finance_write.business_record_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,idempotency_key),
    UNIQUE (holding_id,command_id)
);
CREATE TABLE erpx_finance_write.business_record_audit (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,audit_id),
    UNIQUE (holding_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_finance_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_finance_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY finance_record_holding_isolation ON erpx_finance_write.%I '
            'USING (holding_id=erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())', table_name
        );
    END LOOP;
END;
$$;





-- D13 Batch G: Procurement-owned first-class activity and forecast lifecycles.
CREATE TABLE erpx_procurement_write.business_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'requisition','rfq','receipt','supplier_return'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','expected','proposed','submitted','approved','rejected','issued',
        'received','accepted','authorized','shipped','closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,record_id),
    UNIQUE (holding_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_procurement_write.business_record_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,idempotency_key),
    UNIQUE (holding_id,command_id)
);
CREATE TABLE erpx_procurement_write.business_record_audit (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,audit_id),
    UNIQUE (holding_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_procurement_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_procurement_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY procurement_record_holding_isolation ON erpx_procurement_write.%I '
            'USING (holding_id=erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())', table_name
        );
    END LOOP;
END;
$$;







-- D13 Batch B/F2: genuine owner lifecycles for the remaining current families.
CREATE SCHEMA IF NOT EXISTS erpx_customer_authority_write;
CREATE SCHEMA IF NOT EXISTS erpx_customer_360_write;

CREATE TABLE erpx_customer_authority_write.business_authority_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    record_id UUID NOT NULL, record_kind TEXT NOT NULL CHECK (record_kind IN ('authority_grant','support_access')),
    record_version BIGINT NOT NULL CHECK (record_version>0), customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL, support_case_id UUID, holding_scope JSONB NOT NULL,
    scope_key TEXT NOT NULL, evidence_sha256 TEXT NOT NULL CHECK (evidence_sha256~'^sha256:[0-9A-Fa-f]{64}$'),
    record_status TEXT NOT NULL CHECK (record_status IN ('proposed','active','revoked','expired')),
    valid_until TIMESTAMPTZ NOT NULL, created_by TEXT NOT NULL, revoked_by TEXT,
    created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(holding_id,record_id), UNIQUE(holding_id,record_id,record_version),
    CHECK ((record_kind='authority_grant' AND support_case_id IS NULL) OR (record_kind='support_access' AND support_case_id IS NOT NULL)),
    CHECK (updated_at>=created_at AND valid_until>created_at)
);
CREATE INDEX business_authority_record_scope ON erpx_customer_authority_write.business_authority_records(holding_id,scope_key,record_kind,record_status,record_id);

CREATE TABLE erpx_customer_360_write.owned_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(), record_id UUID NOT NULL,
    record_kind TEXT NOT NULL CHECK(record_kind IN ('profile','note','merge')),
    record_version BIGINT NOT NULL CHECK(record_version>0), holding_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    customer_account_id UUID NOT NULL, related_customer_account_id UUID, source_version BIGINT NOT NULL CHECK(source_version>0),
    title TEXT NOT NULL CHECK(octet_length(title) BETWEEN 1 AND 255), content_object_ref TEXT,
    content_sha256 TEXT NOT NULL CHECK(content_sha256~'^sha256:[0-9A-Fa-f]{64}$'),
    record_status TEXT NOT NULL CHECK(record_status IN ('draft','active','closed','cancelled')),
    created_by TEXT NOT NULL, closed_by TEXT, created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(holding_id,record_id), UNIQUE(holding_id,record_id,record_version),
    CHECK ((record_kind='merge' AND related_customer_account_id IS NOT NULL AND related_customer_account_id<>customer_account_id)
        OR (record_kind<>'merge' AND related_customer_account_id IS NULL)), CHECK(updated_at>=created_at)
);
CREATE INDEX customer_360_owned_record_scope ON erpx_customer_360_write.owned_records(holding_id,scope_key,record_kind,record_status,record_id);

CREATE TABLE erpx_support_write.platform_cases (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(), case_id UUID NOT NULL,
    case_version BIGINT NOT NULL CHECK(case_version>0), holding_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    case_code TEXT NOT NULL, title TEXT NOT NULL, source_record_id UUID, source_record_type TEXT,
    source_version BIGINT CHECK(source_version>0), evidence_sha256 TEXT NOT NULL CHECK(evidence_sha256~'^sha256:[0-9A-Fa-f]{64}$'),
    case_status TEXT NOT NULL CHECK(case_status IN ('open','investigating','waiting','resolved','closed','cancelled')),
    opened_by TEXT NOT NULL, resolved_by TEXT, created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(holding_id,case_id), UNIQUE(holding_id,case_id,case_version), UNIQUE(holding_id,case_code),
    CHECK((source_record_id IS NULL)=(source_version IS NULL)), CHECK((source_record_id IS NULL)=(source_record_type IS NULL)), CHECK(updated_at>=created_at)
);
CREATE INDEX platform_case_scope_status ON erpx_support_write.platform_cases(holding_id,scope_key,case_status,case_id);

CREATE TABLE erpx_people_payroll_write.time_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(), time_record_id UUID NOT NULL,
    time_record_version BIGINT NOT NULL CHECK(time_record_version>0), holding_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    employee_id UUID NOT NULL, employment_record_id UUID NOT NULL, work_date DATE NOT NULL,
    minutes INTEGER NOT NULL CHECK(minutes BETWEEN 1 AND 1440), cost_center TEXT NOT NULL,
    evidence_sha256 TEXT NOT NULL CHECK(evidence_sha256~'^sha256:[0-9A-Fa-f]{64}$'),
    time_status TEXT NOT NULL CHECK(time_status IN ('draft','submitted','approved','posted','voided')),
    submitted_by TEXT NOT NULL, approved_by TEXT, posted_by TEXT, created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(holding_id,time_record_id), UNIQUE(holding_id,time_record_id,time_record_version), CHECK(updated_at>=created_at)
);
CREATE INDEX time_record_employee_date ON erpx_people_payroll_write.time_records(holding_id,scope_key,employee_id,work_date,time_record_id);

CREATE TABLE erpx_legal_write.legal_cases (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(), case_id UUID NOT NULL,
    case_version BIGINT NOT NULL CHECK(case_version>0), holding_scope JSONB NOT NULL, scope_key TEXT NOT NULL,
    case_reference TEXT NOT NULL, title TEXT NOT NULL, related_matter_id UUID, source_version BIGINT CHECK(source_version>0),
    evidence_sha256 TEXT NOT NULL CHECK(evidence_sha256~'^sha256:[0-9A-Fa-f]{64}$'),
    case_status TEXT NOT NULL CHECK(case_status IN ('proposed','open','on-hold','resolved','closed','cancelled')),
    owner_ref TEXT NOT NULL, closed_by TEXT, retention_until TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(holding_id,case_id), UNIQUE(holding_id,case_id,case_version), UNIQUE(holding_id,case_reference),
    CHECK((related_matter_id IS NULL)=(source_version IS NULL)), CHECK(updated_at>=created_at AND retention_until>created_at)
);
CREATE INDEX legal_case_scope_status ON erpx_legal_write.legal_cases(holding_id,scope_key,case_status,case_id);

DO $$ DECLARE item RECORD; BEGIN
    FOR item IN SELECT * FROM (VALUES
        ('erpx_customer_authority_write','business_authority_records','customer_authority_completion_holding_isolation'),
        ('erpx_customer_360_write','owned_records','customer_360_owned_holding_isolation'),
        ('erpx_support_write','platform_cases','support_platform_case_holding_isolation'),
        ('erpx_people_payroll_write','time_records','people_time_record_holding_isolation'),
        ('erpx_legal_write','legal_cases','legal_case_holding_isolation')) AS v(schema_name,table_name,policy_name)
    LOOP
        EXECUTE format('ALTER TABLE %I.%I ENABLE ROW LEVEL SECURITY',item.schema_name,item.table_name);
        EXECUTE format('ALTER TABLE %I.%I FORCE ROW LEVEL SECURITY',item.schema_name,item.table_name);
        EXECUTE format('CREATE POLICY %I ON %I.%I USING (holding_id=erpx_cqrs_write.current_holding_id()) WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())',item.policy_name,item.schema_name,item.table_name);
    END LOOP;
END $$;

-- D13 Batch H: Assets-owned transfer, depreciation, and disposal lifecycles.
CREATE TABLE erpx_assets_write.business_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'transfer','depreciation_run','disposal'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','proposed','calculated','approved','rejected','in_transit',
        'received','posted','disposed','closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,record_id),
    UNIQUE (holding_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_assets_write.business_record_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,idempotency_key),
    UNIQUE (holding_id,command_id)
);
CREATE TABLE erpx_assets_write.business_record_audit (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,audit_id),
    UNIQUE (holding_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_assets_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_assets_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY assets_record_holding_isolation ON erpx_assets_write.%I '
            'USING (holding_id=erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch I: Maintenance-owned plan, inspection, and request lifecycles.
CREATE TABLE erpx_maintenance_write.business_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'plan','inspection','request'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','scheduled','open','triaged','approved','rejected','active',
        'suspended','retired','in_progress','passed','failed','converted','closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,record_id),
    UNIQUE (holding_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_maintenance_write.business_record_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,idempotency_key),
    UNIQUE (holding_id,command_id)
);
CREATE TABLE erpx_maintenance_write.business_record_audit (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,audit_id),
    UNIQUE (holding_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_maintenance_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_maintenance_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY maintenance_record_holding_isolation ON erpx_maintenance_write.%I '
            'USING (holding_id=erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch J: Quality-owned plan, nonconformance, and corrective-action lifecycles.
-- Quality's earlier holding-write guard migration predates the context-owned
-- schema name used by the normalized authority. Fresh current
-- databases therefore create the schema here before its first relation.
CREATE SCHEMA IF NOT EXISTS erpx_quality_write;

CREATE TABLE erpx_quality_write.business_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'plan','nonconformance','corrective_action'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','proposed','open','approved','rejected','active','suspended',
        'retired','investigating','dispositioned','in_progress','verified',
        'verification_failed','closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,record_id),
    UNIQUE (holding_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_quality_write.business_record_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,idempotency_key),
    UNIQUE (holding_id,command_id)
);
CREATE TABLE erpx_quality_write.business_record_audit (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,audit_id),
    UNIQUE (holding_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_quality_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_quality_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY quality_record_holding_isolation ON erpx_quality_write.%I '
            'USING (holding_id=erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch K: Operations-owned operation, case, and approval-work lifecycles.
CREATE TABLE erpx_operations_write.business_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'operation','case','approval_work'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'planned','proposed','open','scheduled','pending','approved','rejected',
        'investigating','in_progress','waiting','resolved','completed','expired',
        'closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,record_id),
    UNIQUE (holding_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_operations_write.business_record_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,idempotency_key),
    UNIQUE (holding_id,command_id)
);
CREATE TABLE erpx_operations_write.business_record_audit (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,audit_id),
    UNIQUE (holding_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_operations_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_operations_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY operations_record_holding_isolation ON erpx_operations_write.%I '
            'USING (holding_id=erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch L: Commercial-owned account and adjustment lifecycles.
CREATE TABLE erpx_commercial_write.business_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'account','adjustment'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'proposed','pending','active','suspended','approved','rejected','applied',
        'closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,record_id),
    UNIQUE (holding_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_commercial_write.business_record_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,idempotency_key),
    UNIQUE (holding_id,command_id)
);
CREATE TABLE erpx_commercial_write.business_record_audit (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,audit_id),
    UNIQUE (holding_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_commercial_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_commercial_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY commercial_record_holding_isolation ON erpx_commercial_write.%I '
            'USING (holding_id=erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch M: POS-owned register-session and cash-movement lifecycles.
CREATE TABLE erpx_pos_write.business_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'register_session','cash_movement'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'opening','open','suspended','reconciling','draft','approved','rejected',
        'posted','reversed','closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,record_id),
    UNIQUE (holding_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_pos_write.business_record_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,idempotency_key),
    UNIQUE (holding_id,command_id)
);
CREATE TABLE erpx_pos_write.business_record_audit (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,audit_id),
    UNIQUE (holding_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_pos_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_pos_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY pos_record_holding_isolation ON erpx_pos_write.%I '
            'USING (holding_id=erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch N: Sales-owned return and fulfillment-handoff lifecycles.
CREATE TABLE erpx_sales_write.business_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'return','fulfillment_handoff'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'requested','proposed','accepted','approved','authorized','rejected',
        'received','inspected','in_progress','completed','failed','closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,record_id),
    UNIQUE (holding_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_sales_write.business_record_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,idempotency_key),
    UNIQUE (holding_id,command_id)
);
CREATE TABLE erpx_sales_write.business_record_audit (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,audit_id),
    UNIQUE (holding_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_sales_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_sales_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY sales_record_holding_isolation ON erpx_sales_write.%I '
            'USING (holding_id=erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())', table_name
        );
    END LOOP;
END;
$$;

-- D13 Batch O: Inventory-owned document, transfer, count, lot, and serial lifecycles.
CREATE TABLE erpx_inventory_write.business_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN (
        'document','transfer','count','lot','serial'
    )),
    record_scope JSONB NOT NULL,
    record_title TEXT NOT NULL CHECK (octet_length(record_title) BETWEEN 1 AND 255),
    record_status TEXT NOT NULL CHECK (record_status IN (
        'draft','proposed','planned','active','registered','approved','rejected',
        'posted','reversed','in_transit','received','in_progress','reconciled',
        'quarantined','expired','in_stock','allocated','shipped','returned',
        'retired','closed','cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,record_id),
    UNIQUE (holding_id,record_id,record_version),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_inventory_write.business_record_receipts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    idempotency_key TEXT NOT NULL,
    command_id UUID NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash)=32),
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    correlation_id UUID NOT NULL,
    receipt JSONB NOT NULL,
    committed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,idempotency_key),
    UNIQUE (holding_id,command_id)
);
CREATE TABLE erpx_inventory_write.business_record_audit (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    audit_id UUID NOT NULL,
    command_id UUID NOT NULL,
    correlation_id UUID NOT NULL,
    record_id UUID NOT NULL,
    record_version BIGINT NOT NULL CHECK (record_version>0),
    actor JSONB NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('create','transition')),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,audit_id),
    UNIQUE (holding_id,command_id)
);
DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'business_records','business_record_receipts','business_record_audit'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_inventory_write.%I ENABLE ROW LEVEL SECURITY', table_name);
        EXECUTE format('ALTER TABLE erpx_inventory_write.%I FORCE ROW LEVEL SECURITY', table_name);
        EXECUTE format(
            'CREATE POLICY inventory_record_holding_isolation ON erpx_inventory_write.%I '
            'USING (holding_id=erpx_cqrs_write.current_holding_id()) '
            'WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())', table_name
        );
    END LOOP;
END;
$$;

-- Final current role marker: erpx_meta.schema_manifest,
-- D13 Batch O2: holding-bound derived authority for StockItem-owned reservations.
CREATE TABLE erpx_inventory_write.reservation_records (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    reservation_id UUID NOT NULL,
    stock_item_id UUID NOT NULL,
    source_item_version BIGINT NOT NULL CHECK (source_item_version > 0),
    holding_scope JSONB NOT NULL,
    warehouse_id UUID NOT NULL,
    item_code TEXT NOT NULL,
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    demand_reference TEXT NOT NULL,
    reservation_status TEXT NOT NULL CHECK (reservation_status IN ('active','released')),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,reservation_id),
    UNIQUE (holding_id,reservation_id,source_item_version)
);
CREATE INDEX inventory_reservation_record_scope
    ON erpx_inventory_write.reservation_records
        (holding_id,stock_item_id,reservation_status,reservation_id);
ALTER TABLE erpx_inventory_write.reservation_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_inventory_write.reservation_records FORCE ROW LEVEL SECURITY;
CREATE POLICY inventory_reservation_record_holding_isolation
    ON erpx_inventory_write.reservation_records
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());

-- Final current role marker: erpx_meta.schema_manifest,
-- schema_revision=2, database_role=holding_write, migration_set=holding-write.
UPDATE erpx_meta.schema_manifest
SET installed_at = now()
WHERE singleton AND schema_revision = 2
  AND database_role = 'holding_write' AND migration_set = 'holding-write';


-- Typed relational authority for current-context aggregates migrated away
-- from the shared aggregate snapshot store. Additional context sections may
-- be appended here before the final generation marker.

CREATE TABLE erpx_quality_write.inspection_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    inspection_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    reference TEXT NOT NULL CHECK (octet_length(reference) BETWEEN 1 AND 255),
    company_id UUID NOT NULL,
    branch_id UUID,
    inspection_kind TEXT NOT NULL CHECK (inspection_kind IN (
        'incoming','in_process','final'
    )),
    product_id UUID NOT NULL,
    lot_reference TEXT CHECK (
        lot_reference IS NULL OR octet_length(lot_reference) BETWEEN 1 AND 255
    ),
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    passed_quantity BIGINT NOT NULL CHECK (passed_quantity >= 0),
    failed_quantity BIGINT NOT NULL CHECK (failed_quantity >= 0),
    inspection_status TEXT NOT NULL CHECK (inspection_status IN (
        'draft','in_progress','passed','failed','released','cancelled'
    )),
    disposition_note TEXT CHECK (
        disposition_note IS NULL OR octet_length(disposition_note) BETWEEN 1 AND 255
    ),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, inspection_id),
    UNIQUE (holding_id, inspection_id, aggregate_version),
    UNIQUE NULLS NOT DISTINCT (
        holding_id, company_id, branch_id, reference
    ),
    CHECK (passed_quantity + failed_quantity <= quantity),
    CHECK (updated_at >= created_at),
    CHECK (
        (inspection_status IN ('draft','in_progress')
            AND passed_quantity=0 AND failed_quantity=0)
        OR (inspection_status IN ('passed','released')
            AND passed_quantity=quantity AND failed_quantity=0)
        OR (inspection_status='failed'
            AND failed_quantity>0
            AND passed_quantity + failed_quantity=quantity
            AND disposition_note IS NOT NULL)
        OR inspection_status='cancelled'
    )
);

CREATE INDEX quality_inspection_authority_scope
    ON erpx_quality_write.inspection_authority
        (holding_id, company_id, branch_id, inspection_status,
         updated_at DESC, inspection_id DESC);

ALTER TABLE erpx_quality_write.inspection_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_quality_write.inspection_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY quality_inspection_authority_holding_isolation
    ON erpx_quality_write.inspection_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());

-- Organization hierarchy authority. This replaces the generation-one
-- hierarchy guard plus shared aggregate snapshot combination with one typed,
-- holding-composite authoritative relation.
CREATE TABLE erpx_organization_write.organization_node_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    node_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    node_kind TEXT NOT NULL CHECK (node_kind IN (
        'holding','company','headquarters','branch','department','team'
    )),
    parent_node_id UUID,
    lineage_node_ids UUID[] NOT NULL,
    company_id UUID,
    branch_id UUID,
    department_id UUID,
    node_code TEXT NOT NULL CHECK (octet_length(node_code) BETWEEN 1 AND 128),
    node_name TEXT NOT NULL CHECK (octet_length(node_name) BETWEEN 1 AND 256),
    node_status TEXT NOT NULL CHECK (node_status IN ('active','suspended','archived')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,node_id),
    UNIQUE (holding_id,node_id,aggregate_version),
    FOREIGN KEY (holding_id,parent_node_id)
        REFERENCES erpx_organization_write.organization_node_authority(holding_id,node_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (NOT node_id=ANY(lineage_node_ids)),
    CHECK (updated_at >= created_at),
    CHECK (
        (node_kind='holding' AND parent_node_id IS NULL
            AND holding_id=node_id AND company_id IS NULL
            AND branch_id IS NULL AND department_id IS NULL)
        OR (node_kind='company' AND parent_node_id=holding_id
            AND company_id=node_id AND branch_id IS NULL AND department_id IS NULL)
        OR (node_kind='headquarters' AND parent_node_id=company_id
            AND company_id IS NOT NULL AND branch_id IS NULL AND department_id IS NULL)
        OR (node_kind='branch' AND parent_node_id=company_id
            AND company_id IS NOT NULL AND branch_id=node_id AND department_id IS NULL)
        OR (node_kind='department' AND parent_node_id=COALESCE(branch_id,company_id)
            AND company_id IS NOT NULL AND department_id=node_id)
        OR (node_kind='team' AND parent_node_id=department_id
            AND company_id IS NOT NULL AND department_id IS NOT NULL)
    )
);

CREATE UNIQUE INDEX organization_node_authority_sibling_code
    ON erpx_organization_write.organization_node_authority
        (holding_id,COALESCE(parent_node_id,holding_id),node_code)
    WHERE node_status <> 'archived';
CREATE INDEX organization_node_authority_children
    ON erpx_organization_write.organization_node_authority
        (holding_id,parent_node_id,node_status,node_id);
CREATE INDEX organization_node_authority_lineage
    ON erpx_organization_write.organization_node_authority USING GIN (lineage_node_ids);

ALTER TABLE erpx_organization_write.organization_node_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_organization_write.organization_node_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY organization_node_authority_holding_isolation
    ON erpx_organization_write.organization_node_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());

-- Assets write authority: scalar asset identity, scope and lifecycle state.
CREATE TABLE erpx_assets_write.asset_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    asset_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    asset_code TEXT NOT NULL CHECK (octet_length(asset_code) BETWEEN 1 AND 128),
    asset_name TEXT NOT NULL CHECK (octet_length(asset_name) BETWEEN 1 AND 256),
    location_id UUID NOT NULL,
    asset_status TEXT NOT NULL CHECK (asset_status IN (
        'draft','in_service','under_maintenance','retired'
    )),
    asset_condition TEXT NOT NULL CHECK (asset_condition IN (
        'operational','degraded','out_of_service','retired'
    )),
    placed_in_service_at TIMESTAMPTZ,
    retired_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,asset_id),
    UNIQUE (holding_id,asset_id,aggregate_version),
    UNIQUE (holding_id,scope_key,asset_code),
    CHECK (updated_at >= created_at),
    CHECK (placed_in_service_at IS NULL OR placed_in_service_at BETWEEN created_at AND updated_at),
    CHECK (retired_at IS NULL OR retired_at BETWEEN created_at AND updated_at),
    CHECK (retired_at IS NULL OR placed_in_service_at IS NULL OR retired_at >= placed_in_service_at),
    CHECK (
        (asset_status='draft' AND asset_condition='out_of_service'
            AND placed_in_service_at IS NULL AND retired_at IS NULL)
        OR (asset_status='in_service' AND asset_condition IN ('operational','degraded')
            AND placed_in_service_at IS NOT NULL AND retired_at IS NULL)
        OR (asset_status='under_maintenance' AND asset_condition='out_of_service'
            AND placed_in_service_at IS NOT NULL AND retired_at IS NULL)
        OR (asset_status='retired' AND asset_condition='retired' AND retired_at IS NOT NULL)
    )
);

CREATE INDEX asset_authority_scope_location_status
    ON erpx_assets_write.asset_authority
        (holding_id,company_id,branch_id,location_id,asset_status,updated_at DESC);
ALTER TABLE erpx_assets_write.asset_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_assets_write.asset_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY asset_authority_holding_isolation ON erpx_assets_write.asset_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());

-- Procurement purchase-order authority with relational line ownership.
CREATE TABLE erpx_procurement_write.purchase_order_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    purchase_order_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    order_code TEXT NOT NULL CHECK (octet_length(order_code) BETWEEN 1 AND 128),
    supplier_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    order_status TEXT NOT NULL CHECK (order_status IN ('draft','released','cancelled')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,purchase_order_id),
    UNIQUE (holding_id,purchase_order_id,aggregate_version),
    UNIQUE (holding_id,scope_key,order_code),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_procurement_write.purchase_order_line_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    purchase_order_id UUID NOT NULL,
    line_id UUID NOT NULL,
    line_position INTEGER NOT NULL CHECK (line_position >= 0 AND line_position < 10000),
    item_code TEXT NOT NULL CHECK (octet_length(item_code) BETWEEN 1 AND 128),
    ordered_quantity BIGINT NOT NULL CHECK (ordered_quantity > 0),
    unit_price_minor BIGINT NOT NULL CHECK (unit_price_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    destination_code TEXT NOT NULL CHECK (octet_length(destination_code) BETWEEN 1 AND 128),
    PRIMARY KEY (holding_id,purchase_order_id,line_id),
    UNIQUE (holding_id,purchase_order_id,line_position),
    FOREIGN KEY (holding_id,purchase_order_id)
        REFERENCES erpx_procurement_write.purchase_order_authority(holding_id,purchase_order_id)
        ON DELETE CASCADE
);

CREATE INDEX purchase_order_authority_scope_supplier_status
    ON erpx_procurement_write.purchase_order_authority
        (holding_id,company_id,branch_id,supplier_id,order_status,updated_at DESC);
ALTER TABLE erpx_procurement_write.purchase_order_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_procurement_write.purchase_order_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY purchase_order_authority_holding_isolation
    ON erpx_procurement_write.purchase_order_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_procurement_write.purchase_order_line_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_procurement_write.purchase_order_line_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY purchase_order_line_authority_holding_isolation
    ON erpx_procurement_write.purchase_order_line_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());

-- Inventory stock authority and child movement/reservation ledgers.
CREATE TABLE erpx_inventory_write.stock_item_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    stock_item_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    warehouse_id UUID NOT NULL,
    item_code TEXT NOT NULL CHECK (octet_length(item_code) BETWEEN 1 AND 128),
    on_hand BIGINT NOT NULL CHECK (on_hand >= 0),
    reserved BIGINT NOT NULL CHECK (reserved >= 0 AND reserved <= on_hand),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,stock_item_id),
    UNIQUE (holding_id,stock_item_id,aggregate_version),
    UNIQUE (holding_id,scope_key,warehouse_id,item_code),
    CHECK (updated_at >= created_at)
);
CREATE TABLE erpx_inventory_write.movement_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    movement_id UUID NOT NULL,
    stock_item_id UUID NOT NULL,
    quantity_delta BIGINT NOT NULL CHECK (quantity_delta <> 0),
    procurement_line_id UUID,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,movement_id),
    UNIQUE (holding_id,stock_item_id,aggregate_version,movement_id),
    FOREIGN KEY (holding_id,stock_item_id)
        REFERENCES erpx_inventory_write.stock_item_authority(holding_id,stock_item_id)
        ON DELETE RESTRICT
);
CREATE TABLE erpx_inventory_write.reservation_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    reservation_id UUID NOT NULL,
    stock_item_id UUID NOT NULL,
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    demand_reference TEXT NOT NULL CHECK (octet_length(demand_reference) BETWEEN 1 AND 128),
    reservation_status TEXT NOT NULL CHECK (reservation_status IN ('active','released')),
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,reservation_id),
    UNIQUE (holding_id,stock_item_id,reservation_id),
    FOREIGN KEY (holding_id,stock_item_id)
        REFERENCES erpx_inventory_write.stock_item_authority(holding_id,stock_item_id)
        ON DELETE CASCADE
);
CREATE INDEX stock_item_authority_scope_warehouse_code
    ON erpx_inventory_write.stock_item_authority
        (holding_id,company_id,branch_id,warehouse_id,item_code);
CREATE INDEX reservation_authority_active
    ON erpx_inventory_write.reservation_authority(holding_id,stock_item_id,reservation_id)
    WHERE reservation_status='active';
ALTER TABLE erpx_inventory_write.stock_item_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_inventory_write.stock_item_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY stock_item_authority_holding_isolation
    ON erpx_inventory_write.stock_item_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_inventory_write.movement_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_inventory_write.movement_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY movement_authority_holding_isolation ON erpx_inventory_write.movement_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_inventory_write.reservation_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_inventory_write.reservation_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY reservation_authority_holding_isolation
    ON erpx_inventory_write.reservation_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());

-- CRM accepted-lead and opportunity authority, including exact Marketing
-- provenance and complete forecast state.
CREATE TABLE erpx_crm_write.lead_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    lead_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    marketing_candidate_id UUID NOT NULL,
    campaign_id UUID NOT NULL,
    audience_member_id UUID NOT NULL,
    marketing_source_version BIGINT NOT NULL CHECK (marketing_source_version > 0),
    customer_id UUID,
    source TEXT NOT NULL CHECK (octet_length(source) BETWEEN 1 AND 255),
    qualification_score_basis_points INTEGER NOT NULL
        CHECK (qualification_score_basis_points BETWEEN 1 AND 10000),
    accepted_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,lead_id),
    UNIQUE (holding_id,lead_id,aggregate_version),
    UNIQUE (holding_id,marketing_candidate_id)
);
CREATE TABLE erpx_crm_write.opportunity_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    opportunity_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    accepted_lead_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    opportunity_name TEXT NOT NULL CHECK (octet_length(opportunity_name) BETWEEN 1 AND 255),
    expected_value_minor BIGINT NOT NULL CHECK (expected_value_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    opportunity_stage TEXT NOT NULL CHECK (opportunity_stage IN (
        'qualified','discovery','proposal','negotiation','won','lost'
    )),
    account_id UUID,
    owner_ref TEXT CHECK (owner_ref IS NULL OR octet_length(owner_ref) BETWEEN 1 AND 255),
    pipeline_category TEXT CHECK (pipeline_category IN ('pipeline','best_case','commit','closed')),
    probability_basis_points INTEGER CHECK (probability_basis_points BETWEEN 0 AND 10000),
    expected_close_on DATE,
    originating_campaign_id UUID,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,opportunity_id),
    UNIQUE (holding_id,opportunity_id,aggregate_version),
    FOREIGN KEY (holding_id,accepted_lead_id)
        REFERENCES erpx_crm_write.lead_authority(holding_id,lead_id) ON DELETE RESTRICT,
    CHECK (updated_at >= created_at),
    CHECK (
        (account_id IS NULL AND owner_ref IS NULL AND pipeline_category IS NULL
            AND probability_basis_points IS NULL AND expected_close_on IS NULL)
        OR (account_id IS NOT NULL AND owner_ref IS NOT NULL AND pipeline_category IS NOT NULL
            AND probability_basis_points IS NOT NULL AND expected_close_on IS NOT NULL)
    )
);
CREATE INDEX crm_lead_authority_scope
    ON erpx_crm_write.lead_authority(holding_id,company_id,branch_id,accepted_at DESC);
CREATE INDEX crm_opportunity_authority_scope_stage
    ON erpx_crm_write.opportunity_authority
        (holding_id,company_id,branch_id,customer_id,opportunity_stage,updated_at DESC);
ALTER TABLE erpx_crm_write.lead_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_crm_write.lead_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY crm_lead_authority_holding_isolation ON erpx_crm_write.lead_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_crm_write.opportunity_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_crm_write.opportunity_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY crm_opportunity_authority_holding_isolation
    ON erpx_crm_write.opportunity_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());

-- Marketing lead and campaign-budget authority. Dimensions, allocations and
-- spend reservations are typed columns/child rows; no aggregate document is
-- authoritative.
CREATE TABLE erpx_marketing_write.lead_candidate_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    lead_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    company_id UUID NOT NULL,
    branch_id UUID,
    campaign_id UUID NOT NULL,
    audience_member_id UUID NOT NULL,
    source TEXT NOT NULL CHECK (octet_length(source) BETWEEN 1 AND 255),
    existing_customer_id UUID,
    qualification_score_basis_points INTEGER CHECK (
        qualification_score_basis_points BETWEEN 1 AND 10000
    ),
    lead_status TEXT NOT NULL CHECK (lead_status IN ('open','qualified','rejected')),
    engagement_count BIGINT NOT NULL CHECK (engagement_count >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,lead_id),
    UNIQUE (holding_id,lead_id,aggregate_version),
    CHECK (updated_at >= created_at),
    CHECK ((lead_status='qualified') = (qualification_score_basis_points IS NOT NULL))
);
CREATE INDEX marketing_lead_authority_scope
    ON erpx_marketing_write.lead_candidate_authority
       (holding_id,company_id,branch_id,lead_status,updated_at DESC);

CREATE TABLE erpx_marketing_write.campaign_budget_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    budget_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    company_id UUID NOT NULL,
    branch_id UUID,
    campaign_id UUID NOT NULL,
    budget_code TEXT NOT NULL CHECK (octet_length(budget_code) BETWEEN 1 AND 255),
    budget_name TEXT NOT NULL CHECK (octet_length(budget_name) BETWEEN 1 AND 255),
    budget_period TEXT NOT NULL CHECK (budget_period IN ('annual','quarterly','campaign')),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    requested_minor BIGINT NOT NULL CHECK (requested_minor > 0),
    approved_minor BIGINT CHECK (approved_minor > 0),
    dimension_business_unit_id UUID,
    dimension_cost_center_id UUID,
    dimension_location_id UUID,
    dimension_project_id UUID,
    dimension_channel_id UUID,
    dimension_party_id UUID,
    dimension_employee_id UUID,
    dimension_asset_id UUID,
    dimension_legal_matter_id UUID,
    dimension_intercompany_partner_id UUID,
    approval_request_id UUID,
    approval_subject_version BIGINT CHECK (approval_subject_version > 0),
    approval_requested_by TEXT CHECK (
        approval_requested_by IS NULL OR octet_length(approval_requested_by) BETWEEN 1 AND 255
    ),
    approval_requested_at TIMESTAMPTZ,
    budget_status TEXT NOT NULL CHECK (
        budget_status IN ('draft','review','approved','running','completed','cancelled')
    ),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,budget_id),
    UNIQUE (holding_id,budget_id,aggregate_version),
    UNIQUE (holding_id,company_id,branch_id,budget_code),
    CHECK (updated_at >= created_at),
    CHECK (approved_minor IS NULL OR approved_minor = requested_minor),
    CHECK (
        (approval_request_id IS NULL AND approval_subject_version IS NULL
            AND approval_requested_by IS NULL AND approval_requested_at IS NULL)
        OR (approval_request_id IS NOT NULL AND approval_subject_version IS NOT NULL
            AND approval_requested_by IS NOT NULL AND approval_requested_at IS NOT NULL)
    )
);
CREATE INDEX marketing_budget_authority_scope
    ON erpx_marketing_write.campaign_budget_authority
       (holding_id,company_id,branch_id,campaign_id,budget_status,updated_at DESC);

CREATE TABLE erpx_marketing_write.budget_allocation_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    budget_id UUID NOT NULL,
    allocation_id UUID NOT NULL,
    tactic_id UUID NOT NULL,
    channel_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    planned_minor BIGINT NOT NULL CHECK (planned_minor > 0),
    budget_aggregate_version BIGINT NOT NULL CHECK (budget_aggregate_version > 0),
    PRIMARY KEY (holding_id,budget_id,allocation_id),
    UNIQUE (holding_id,budget_id,allocation_id,budget_aggregate_version),
    FOREIGN KEY (holding_id,budget_id)
        REFERENCES erpx_marketing_write.campaign_budget_authority(holding_id,budget_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_marketing_write.spend_reservation_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    request_id UUID NOT NULL,
    budget_id UUID NOT NULL,
    allocation_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    expected_outcome TEXT NOT NULL CHECK (octet_length(expected_outcome) BETWEEN 1 AND 255),
    spend_status TEXT NOT NULL CHECK (spend_status IN ('active','released')),
    requested_at TIMESTAMPTZ NOT NULL,
    released_at TIMESTAMPTZ,
    budget_aggregate_version BIGINT NOT NULL CHECK (budget_aggregate_version > 0),
    PRIMARY KEY (holding_id,request_id),
    UNIQUE (holding_id,budget_id,request_id),
    FOREIGN KEY (holding_id,budget_id,allocation_id)
        REFERENCES erpx_marketing_write.budget_allocation_authority
            (holding_id,budget_id,allocation_id) ON DELETE RESTRICT,
    CHECK (
        (spend_status='active' AND released_at IS NULL)
        OR (spend_status='released' AND released_at >= requested_at)
    )
);

ALTER TABLE erpx_marketing_write.lead_candidate_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_marketing_write.lead_candidate_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY marketing_lead_candidate_authority_holding_isolation
    ON erpx_marketing_write.lead_candidate_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_marketing_write.campaign_budget_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_marketing_write.campaign_budget_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY marketing_campaign_budget_authority_holding_isolation
    ON erpx_marketing_write.campaign_budget_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_marketing_write.budget_allocation_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_marketing_write.budget_allocation_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY marketing_budget_allocation_authority_holding_isolation
    ON erpx_marketing_write.budget_allocation_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_marketing_write.spend_reservation_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_marketing_write.spend_reservation_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY marketing_spend_reservation_authority_holding_isolation
    ON erpx_marketing_write.spend_reservation_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());

-- Commercial holding-plane subscription and entitlement authority. Catalog
-- plans remain in the separately normalized control-plane authority.
CREATE TABLE erpx_commercial_write.subscription_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    subscription_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    customer_id UUID NOT NULL,
    catalog_plan_id UUID NOT NULL,
    subscription_status TEXT NOT NULL CHECK (
        subscription_status IN ('trial','active','past_due','suspended','ended')
    ),
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    last_provider_event_id TEXT CHECK (
        last_provider_event_id IS NULL OR octet_length(last_provider_event_id) BETWEEN 1 AND 255
    ),
    last_provider_event_at TIMESTAMPTZ,
    grace_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,subscription_id),
    UNIQUE (holding_id,subscription_id,aggregate_version),
    CHECK (updated_at >= created_at),
    CHECK ((last_provider_event_id IS NULL) = (last_provider_event_at IS NULL)),
    CHECK ((subscription_status='past_due') = (grace_expires_at IS NOT NULL)),
    CHECK (last_provider_event_at IS NULL OR last_provider_event_at <= updated_at)
);
CREATE INDEX commercial_subscription_authority_scope
    ON erpx_commercial_write.subscription_authority
       (holding_id,customer_id,subscription_status,updated_at DESC);

CREATE TABLE erpx_commercial_write.entitlement_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    entitlement_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    subscription_id UUID NOT NULL,
    capability TEXT NOT NULL CHECK (octet_length(capability) BETWEEN 1 AND 255),
    enabled BOOLEAN NOT NULL,
    seat_capacity BIGINT NOT NULL CHECK (seat_capacity > 0),
    company_capacity BIGINT NOT NULL CHECK (company_capacity > 0),
    branch_capacity BIGINT NOT NULL CHECK (branch_capacity > 0),
    allocated_seats BIGINT NOT NULL CHECK (
        allocated_seats >= 0 AND allocated_seats <= seat_capacity
    ),
    allocated_companies BIGINT NOT NULL CHECK (
        allocated_companies >= 0 AND allocated_companies <= company_capacity
    ),
    allocated_branches BIGINT NOT NULL CHECK (
        allocated_branches >= 0 AND allocated_branches <= branch_capacity
    ),
    last_provider_event_id TEXT CHECK (
        last_provider_event_id IS NULL OR octet_length(last_provider_event_id) BETWEEN 1 AND 255
    ),
    last_provider_event_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,entitlement_id),
    UNIQUE (holding_id,entitlement_id,aggregate_version),
    UNIQUE (holding_id,subscription_id,capability),
    FOREIGN KEY (holding_id,subscription_id)
        REFERENCES erpx_commercial_write.subscription_authority(holding_id,subscription_id)
        ON DELETE RESTRICT,
    CHECK (updated_at >= created_at),
    CHECK ((last_provider_event_id IS NULL) = (last_provider_event_at IS NULL)),
    CHECK (last_provider_event_at IS NULL OR last_provider_event_at <= updated_at)
);
CREATE INDEX commercial_entitlement_authority_scope
    ON erpx_commercial_write.entitlement_authority
       (holding_id,subscription_id,enabled,updated_at DESC);

ALTER TABLE erpx_commercial_write.subscription_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_commercial_write.subscription_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY commercial_subscription_authority_holding_isolation
    ON erpx_commercial_write.subscription_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_commercial_write.entitlement_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_commercial_write.entitlement_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY commercial_entitlement_authority_holding_isolation
    ON erpx_commercial_write.entitlement_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());

-- Maintenance work-order authority and immutable owned downtime intervals.
CREATE TABLE erpx_maintenance_write.work_order_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    work_order_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    work_order_code TEXT NOT NULL CHECK (octet_length(work_order_code) BETWEEN 1 AND 128),
    company_id UUID NOT NULL,
    branch_id UUID,
    source_event_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    asset_id UUID NOT NULL,
    request_code TEXT NOT NULL CHECK (octet_length(request_code) BETWEEN 1 AND 255),
    asset_code TEXT NOT NULL CHECK (octet_length(asset_code) BETWEEN 1 AND 255),
    asset_name TEXT NOT NULL CHECK (octet_length(asset_name) BETWEEN 1 AND 255),
    asset_location_id UUID NOT NULL,
    asset_condition TEXT NOT NULL CHECK (
        asset_condition IN ('operational','degraded','out_of_service','retired')
    ),
    title TEXT NOT NULL CHECK (octet_length(title) BETWEEN 1 AND 255),
    work_order_status TEXT NOT NULL CHECK (
        work_order_status IN ('planned','scheduled','in_progress','blocked','completed','cancelled')
    ),
    scheduled_for TIMESTAMPTZ,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,work_order_id),
    UNIQUE (holding_id,work_order_id,aggregate_version),
    UNIQUE (holding_id,source_event_id),
    UNIQUE (holding_id,company_id,branch_id,work_order_code),
    CHECK (updated_at >= created_at),
    CHECK (asset_condition <> 'retired'),
    CHECK (scheduled_for IS NULL OR scheduled_for >= created_at),
    CHECK (started_at IS NULL OR (scheduled_for IS NOT NULL AND started_at >= scheduled_for)),
    CHECK (completed_at IS NULL OR (started_at IS NOT NULL AND completed_at > started_at))
);
CREATE INDEX maintenance_work_order_authority_scope
    ON erpx_maintenance_write.work_order_authority
       (holding_id,company_id,branch_id,asset_id,work_order_status,updated_at DESC);

CREATE TABLE erpx_maintenance_write.downtime_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    work_order_id UUID NOT NULL,
    downtime_id UUID NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    ended_at TIMESTAMPTZ NOT NULL,
    reason TEXT NOT NULL CHECK (octet_length(reason) BETWEEN 1 AND 1024),
    work_order_aggregate_version BIGINT NOT NULL CHECK (work_order_aggregate_version > 0),
    PRIMARY KEY (holding_id,work_order_id,downtime_id),
    UNIQUE (holding_id,downtime_id),
    FOREIGN KEY (holding_id,work_order_id)
        REFERENCES erpx_maintenance_write.work_order_authority(holding_id,work_order_id)
        ON DELETE RESTRICT,
    CHECK (ended_at > started_at)
);

ALTER TABLE erpx_maintenance_write.work_order_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_maintenance_write.work_order_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY maintenance_work_order_authority_holding_isolation
    ON erpx_maintenance_write.work_order_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_maintenance_write.downtime_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_maintenance_write.downtime_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY maintenance_downtime_authority_holding_isolation
    ON erpx_maintenance_write.downtime_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());

-- BEGIN FINANCE_SALES_POS_LEGAL_NORMALIZED_AUTHORITY
-- Context-owned command authority for the Finance, Sales, Point of Sale, and
-- Legal aggregates. JSON columns retained by earlier migrations are derived
-- compatibility evidence only; command rehydration reads these typed columns.

ALTER TABLE erpx_sales_write.quote_guards
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID;
ALTER TABLE erpx_sales_write.sales_order_guards
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID;
ALTER TABLE erpx_sales_write.revenue_arrangement_guards
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID;

CREATE TABLE erpx_sales_write.quote_line_authority (
    quote_id UUID NOT NULL REFERENCES erpx_sales_write.quote_guards(quote_id) ON DELETE CASCADE,
    line_id UUID NOT NULL,
    item_code TEXT NOT NULL,
    description TEXT NOT NULL,
    quantity BIGINT NOT NULL,
    unit_price_minor BIGINT NOT NULL,
    tax_minor BIGINT NOT NULL,
    currency TEXT NOT NULL,
    PRIMARY KEY (quote_id,line_id)
);
CREATE TABLE erpx_sales_write.sales_order_line_authority (
    sales_order_id UUID NOT NULL REFERENCES erpx_sales_write.sales_order_guards(sales_order_id) ON DELETE CASCADE,
    line_id UUID NOT NULL,
    item_code TEXT NOT NULL,
    description TEXT NOT NULL,
    quantity BIGINT NOT NULL,
    unit_price_minor BIGINT NOT NULL,
    tax_minor BIGINT NOT NULL,
    currency TEXT NOT NULL,
    PRIMARY KEY (sales_order_id,line_id)
);
CREATE TABLE erpx_sales_write.arrangement_dimension_authority (
    arrangement_id UUID PRIMARY KEY REFERENCES erpx_sales_write.revenue_arrangement_guards(arrangement_id) ON DELETE CASCADE,
    legal_entity_id UUID NOT NULL,
    branch_dimension_id UUID,
    business_unit_id UUID,
    cost_center_id UUID,
    location_id UUID,
    project_id UUID,
    campaign_id UUID,
    channel_id UUID,
    party_id UUID,
    employee_id UUID,
    asset_id UUID,
    legal_matter_id UUID,
    intercompany_partner_id UUID
);
CREATE TABLE erpx_sales_write.arrangement_obligation_authority (
    arrangement_id UUID NOT NULL REFERENCES erpx_sales_write.revenue_arrangement_guards(arrangement_id) ON DELETE CASCADE,
    obligation_id UUID NOT NULL,
    line_id UUID NOT NULL,
    revenue_model TEXT NOT NULL CHECK (revenue_model IN ('goods','service_milestone','recurring')),
    allocated_minor BIGINT NOT NULL,
    allocated_tax_minor BIGINT NOT NULL,
    accepted_minor BIGINT NOT NULL,
    accepted_tax_minor BIGINT NOT NULL,
    service_starts_on DATE,
    service_ends_on DATE,
    PRIMARY KEY (arrangement_id,obligation_id),
    UNIQUE (arrangement_id,line_id)
);
CREATE TABLE erpx_sales_write.arrangement_fulfillment_authority (
    arrangement_id UUID NOT NULL REFERENCES erpx_sales_write.revenue_arrangement_guards(arrangement_id) ON DELETE CASCADE,
    fulfillment_id UUID NOT NULL,
    billing_event_id UUID NOT NULL,
    obligation_id UUID NOT NULL,
    fulfillment_kind TEXT NOT NULL CHECK (fulfillment_kind IN ('goods_delivery','service_milestone','recurring_period')),
    accepted_minor BIGINT NOT NULL,
    accepted_tax_minor BIGINT NOT NULL,
    accepted_at TIMESTAMPTZ NOT NULL,
    eligible_on DATE NOT NULL,
    PRIMARY KEY (arrangement_id,fulfillment_id),
    UNIQUE (arrangement_id,billing_event_id),
    FOREIGN KEY (arrangement_id,obligation_id)
        REFERENCES erpx_sales_write.arrangement_obligation_authority(arrangement_id,obligation_id)
);

ALTER TABLE erpx_pos_write.sale_guards
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID;
ALTER TABLE erpx_pos_write.refund_guards
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID;

ALTER TABLE erpx_finance_write.chart_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_finance_write.period_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_finance_write.journal_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_finance_write.subledger_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_finance_write.customer_invoice_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_finance_write.revenue_schedule_guards
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID,
    ADD COLUMN legal_contract_id UUID,
    ADD COLUMN order_code TEXT;

CREATE TABLE erpx_finance_write.customer_invoice_dimension_authority (
    invoice_id UUID PRIMARY KEY REFERENCES erpx_finance_write.customer_invoice_guards(invoice_id) ON DELETE CASCADE,
    fulfillment_id UUID NOT NULL,
    arrangement_id UUID NOT NULL,
    sales_order_id UUID NOT NULL,
    order_code TEXT NOT NULL,
    legal_entity_id UUID NOT NULL,
    branch_dimension_id UUID,
    business_unit_id UUID,
    cost_center_id UUID,
    location_id UUID,
    project_id UUID,
    campaign_id UUID,
    channel_id UUID,
    party_id UUID,
    employee_id UUID,
    asset_id UUID,
    legal_matter_id UUID,
    intercompany_partner_id UUID
);
CREATE TABLE erpx_finance_write.revenue_schedule_dimension_authority (
    schedule_id UUID PRIMARY KEY REFERENCES erpx_finance_write.revenue_schedule_guards(schedule_id) ON DELETE CASCADE,
    legal_entity_id UUID NOT NULL,
    branch_dimension_id UUID,
    business_unit_id UUID,
    cost_center_id UUID,
    location_id UUID,
    project_id UUID,
    campaign_id UUID,
    channel_id UUID,
    party_id UUID,
    employee_id UUID,
    asset_id UUID,
    legal_matter_id UUID,
    intercompany_partner_id UUID
);
CREATE TABLE erpx_finance_write.revenue_schedule_obligation_authority (
    schedule_id UUID NOT NULL REFERENCES erpx_finance_write.revenue_schedule_guards(schedule_id) ON DELETE CASCADE,
    obligation_id UUID NOT NULL,
    line_id UUID NOT NULL,
    revenue_model TEXT NOT NULL CHECK (revenue_model IN ('goods','service_milestone','recurring')),
    allocated_minor BIGINT NOT NULL,
    recognized_minor BIGINT NOT NULL,
    service_starts_on DATE,
    service_ends_on DATE,
    PRIMARY KEY (schedule_id,obligation_id)
);
CREATE TABLE erpx_finance_write.revenue_schedule_recognition_authority (
    schedule_id UUID NOT NULL REFERENCES erpx_finance_write.revenue_schedule_guards(schedule_id) ON DELETE CASCADE,
    recognition_ordinal BIGINT NOT NULL,
    fulfillment_id UUID NOT NULL,
    obligation_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL,
    recognized_minor BIGINT NOT NULL,
    recognized_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (schedule_id,recognition_ordinal),
    UNIQUE (schedule_id,fulfillment_id),
    FOREIGN KEY (schedule_id,obligation_id)
        REFERENCES erpx_finance_write.revenue_schedule_obligation_authority(schedule_id,obligation_id)
);

CREATE TABLE erpx_finance_write.journal_source_authority (
    journal_id UUID PRIMARY KEY REFERENCES erpx_finance_write.journal_guards(journal_id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL,
    sales_order_id UUID,
    sales_order_code TEXT,
    sales_customer_id UUID,
    sales_order_status TEXT,
    sales_total_minor BIGINT,
    inventory_movement_id UUID,
    inventory_stock_item_id UUID,
    inventory_warehouse_id UUID,
    inventory_item_code TEXT,
    inventory_quantity_units BIGINT,
    inventory_quantity_scale SMALLINT,
    inventory_procurement_line_id UUID,
    payroll_run_id UUID,
    payroll_period_code TEXT,
    payroll_finalized_at TIMESTAMPTZ,
    payroll_gross_minor BIGINT,
    payroll_deduction_minor BIGINT,
    payroll_net_minor BIGINT,
    pos_transaction_id UUID,
    pos_original_sale_id UUID,
    pos_register_id UUID,
    pos_receipt_code TEXT,
    pos_customer_id UUID,
    pos_amount_minor BIGINT,
    pos_occurred_at TIMESTAMPTZ,
    manual_reference TEXT
);
CREATE TABLE erpx_finance_write.journal_sales_line_authority (
    journal_id UUID NOT NULL REFERENCES erpx_finance_write.journal_source_authority(journal_id) ON DELETE CASCADE,
    line_id UUID NOT NULL,
    item_code TEXT NOT NULL,
    quantity_units BIGINT NOT NULL,
    quantity_scale SMALLINT NOT NULL,
    line_value_minor BIGINT NOT NULL,
    PRIMARY KEY (journal_id,line_id)
);

ALTER TABLE erpx_legal_write.contract_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_legal_write.document_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_legal_write.access_entry_guards ADD COLUMN holding_id UUID, ADD COLUMN company_id UUID, ADD COLUMN branch_id UUID;
ALTER TABLE erpx_legal_write.matter_guards
    ADD COLUMN holding_id UUID,
    ADD COLUMN company_id UUID,
    ADD COLUMN branch_id UUID,
    ADD COLUMN hold_active BOOLEAN,
    ADD COLUMN hold_reason TEXT,
    ADD COLUMN hold_placed_by TEXT,
    ADD COLUMN hold_placed_at TIMESTAMPTZ,
    ADD COLUMN hold_released_by TEXT,
    ADD COLUMN hold_released_at TIMESTAMPTZ;
ALTER TABLE erpx_legal_write.matter_link_guards
    ADD COLUMN target_holding_id UUID,
    ADD COLUMN target_company_id UUID,
    ADD COLUMN target_branch_id UUID;

-- Backfill exact scope keys without consulting generic aggregate snapshots.
DO $$
DECLARE relation_name TEXT;
BEGIN
    FOREACH relation_name IN ARRAY ARRAY[
      'erpx_sales_write.quote_guards','erpx_sales_write.sales_order_guards',
      'erpx_sales_write.revenue_arrangement_guards','erpx_finance_write.chart_guards',
      'erpx_finance_write.period_guards','erpx_finance_write.journal_guards',
      'erpx_finance_write.subledger_guards','erpx_finance_write.customer_invoice_guards',
      'erpx_finance_write.revenue_schedule_guards'
    ] LOOP
      EXECUTE format(
        'UPDATE %s SET holding_id=split_part(scope_key,'':'',2)::uuid,
                       company_id=split_part(scope_key,'':'',3)::uuid,
                       branch_id=NULLIF(split_part(scope_key,'':'',4),'''')::uuid
         WHERE holding_id IS NULL OR company_id IS NULL', relation_name);
    END LOOP;
    FOREACH relation_name IN ARRAY ARRAY[
      'erpx_legal_write.contract_guards','erpx_legal_write.document_guards',
      'erpx_legal_write.access_entry_guards','erpx_legal_write.matter_guards'
    ] LOOP
      EXECUTE format(
        'UPDATE %s SET holding_id=split_part(split_part(scope_key,''h:'',2),''/c:'',1)::uuid,
                       company_id=split_part(split_part(scope_key,''/c:'',2),''/b:'',1)::uuid,
                       branch_id=NULLIF(split_part(scope_key,''/b:'',2),'''')::uuid
         WHERE holding_id IS NULL OR company_id IS NULL', relation_name);
    END LOOP;
END $$;
UPDATE erpx_pos_write.sale_guards SET
    holding_id=split_part(split_part(scope_key,'holding:',2),'/company:',1)::uuid,
    company_id=split_part(split_part(scope_key,'/company:',2),'/branch:',1)::uuid,
    branch_id=NULLIF(split_part(scope_key,'/branch:',2),'')::uuid;
UPDATE erpx_pos_write.refund_guards SET
    holding_id=split_part(split_part(scope_key,'holding:',2),'/company:',1)::uuid,
    company_id=split_part(split_part(scope_key,'/company:',2),'/branch:',1)::uuid,
    branch_id=NULLIF(split_part(scope_key,'/branch:',2),'')::uuid;
UPDATE erpx_legal_write.matter_link_guards SET
    target_holding_id=split_part(split_part(target_scope_key,'h:',2),'/c:',1)::uuid,
    target_company_id=split_part(split_part(target_scope_key,'/c:',2),'/b:',1)::uuid,
    target_branch_id=NULLIF(split_part(target_scope_key,'/b:',2),'')::uuid;

CREATE INDEX sales_quote_typed_authority ON erpx_sales_write.quote_guards(holding_id,company_id,branch_id,quote_id);
CREATE INDEX sales_order_typed_authority ON erpx_sales_write.sales_order_guards(holding_id,company_id,branch_id,sales_order_id);
CREATE INDEX sales_arrangement_typed_authority ON erpx_sales_write.revenue_arrangement_guards(holding_id,company_id,branch_id,arrangement_id);
CREATE INDEX pos_sale_typed_authority ON erpx_pos_write.sale_guards(holding_id,company_id,branch_id,sale_id);
CREATE INDEX pos_refund_typed_authority ON erpx_pos_write.refund_guards(holding_id,company_id,branch_id,refund_id);
CREATE INDEX legal_contract_typed_authority ON erpx_legal_write.contract_guards(holding_id,company_id,branch_id,contract_id);
CREATE INDEX legal_document_typed_authority ON erpx_legal_write.document_guards(holding_id,company_id,branch_id,document_id);
CREATE INDEX legal_access_typed_authority ON erpx_legal_write.access_entry_guards(holding_id,company_id,branch_id,access_entry_id);
CREATE INDEX legal_matter_typed_authority ON erpx_legal_write.matter_guards(holding_id,company_id,branch_id,matter_id);
-- END FINANCE_SALES_POS_LEGAL_NORMALIZED_AUTHORITY

-- Identity & Access principal/role authority. Organizational scopes are
-- decomposed into typed scalar/array columns; assignments and memberships are
-- owned relational children.
CREATE TABLE erpx_identity_access_write.principal_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    principal_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    scope_mode TEXT NOT NULL CHECK (scope_mode IN ('platform','holding','company','branch','subtree')),
    company_id UUID,
    branch_id UUID,
    company_ids UUID[] NOT NULL DEFAULT '{}',
    branch_ids UUID[] NOT NULL DEFAULT '{}',
    display_name TEXT NOT NULL CHECK (octet_length(display_name) BETWEEN 1 AND 256),
    principal_kind TEXT NOT NULL CHECK (
        principal_kind IN ('human','service','agent','human_group','agent_pool')
    ),
    principal_realm TEXT NOT NULL CHECK (
        principal_realm IN ('internal_workforce','platform_workforce','customer_portal')
    ),
    governance_owner_ref TEXT,
    governance_purpose TEXT,
    governance_workload_binding TEXT,
    governance_credential_reference TEXT,
    principal_status TEXT NOT NULL CHECK (
        principal_status IN ('invited','active','suspended','disabled')
    ),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,principal_id),
    UNIQUE (holding_id,principal_id,aggregate_version),
    CHECK (updated_at >= created_at),
    CHECK (
        (scope_mode='platform' AND holding_id IS NULL AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_mode='holding' AND holding_id IS NOT NULL AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_mode='company' AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NULL)
        OR (scope_mode='branch' AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NOT NULL)
        OR (scope_mode='subtree' AND holding_id IS NOT NULL AND company_id IS NULL AND branch_id IS NULL)
    ),
    CHECK (
        (principal_kind='human' AND governance_owner_ref IS NULL AND governance_purpose IS NULL
            AND governance_workload_binding IS NULL AND governance_credential_reference IS NULL)
        OR (principal_kind<>'human' AND governance_owner_ref IS NOT NULL
            AND governance_purpose IS NOT NULL AND governance_workload_binding IS NOT NULL
            AND governance_credential_reference IS NOT NULL)
    )
);
CREATE INDEX identity_principal_authority_scope
    ON erpx_identity_access_write.principal_authority
       (holding_id,company_id,branch_id,principal_status,updated_at DESC);

CREATE TABLE erpx_identity_access_write.role_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    role_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    scope_mode TEXT NOT NULL CHECK (scope_mode IN ('platform','holding','company','branch','subtree')),
    company_id UUID,
    branch_id UUID,
    company_ids UUID[] NOT NULL DEFAULT '{}',
    branch_ids UUID[] NOT NULL DEFAULT '{}',
    role_code TEXT NOT NULL CHECK (octet_length(role_code) BETWEEN 1 AND 256),
    display_name TEXT NOT NULL CHECK (octet_length(display_name) BETWEEN 1 AND 256),
    risk_tier TEXT NOT NULL CHECK (risk_tier IN (
        'platform_root','administrator','high','standard','contractor',
        'support_diagnostic','support_mutation','emergency'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,role_id),
    UNIQUE (holding_id,role_id,aggregate_version),
    UNIQUE (holding_id,scope_key,role_code),
    CHECK (updated_at >= created_at),
    CHECK (
        (scope_mode='platform' AND holding_id IS NULL AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_mode='holding' AND holding_id IS NOT NULL AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_mode='company' AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NULL)
        OR (scope_mode='branch' AND holding_id IS NOT NULL AND company_id IS NOT NULL AND branch_id IS NOT NULL)
        OR (scope_mode='subtree' AND holding_id IS NOT NULL AND company_id IS NULL AND branch_id IS NULL)
    )
);

CREATE TABLE erpx_identity_access_write.role_permission_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    role_id UUID NOT NULL,
    permission_key TEXT NOT NULL CHECK (octet_length(permission_key) BETWEEN 1 AND 256),
    role_version BIGINT NOT NULL CHECK (role_version > 0),
    PRIMARY KEY (holding_id,role_id,permission_key),
    FOREIGN KEY (holding_id,role_id)
        REFERENCES erpx_identity_access_write.role_authority(holding_id,role_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_identity_access_write.assignment_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    assignment_id UUID NOT NULL,
    principal_id UUID NOT NULL,
    role_id UUID NOT NULL,
    scope_key TEXT NOT NULL,
    scope_mode TEXT NOT NULL CHECK (scope_mode IN ('platform','holding','company','branch','subtree')),
    company_id UUID,
    branch_id UUID,
    company_ids UUID[] NOT NULL DEFAULT '{}',
    branch_ids UUID[] NOT NULL DEFAULT '{}',
    risk_tier TEXT NOT NULL CHECK (risk_tier IN (
        'platform_root','administrator','high','standard','contractor',
        'support_diagnostic','support_mutation','emergency'
    )),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    grant_reason TEXT NOT NULL CHECK (octet_length(grant_reason) BETWEEN 1 AND 256),
    ticket_reference TEXT CHECK (
        ticket_reference IS NULL OR octet_length(ticket_reference) BETWEEN 1 AND 256
    ),
    grantor_ref TEXT NOT NULL CHECK (octet_length(grantor_ref) BETWEEN 1 AND 256),
    approver_ref TEXT CHECK (approver_ref IS NULL OR octet_length(approver_ref) BETWEEN 1 AND 256),
    revoked_at TIMESTAMPTZ,
    principal_version BIGINT NOT NULL CHECK (principal_version > 0),
    PRIMARY KEY (holding_id,assignment_id),
    UNIQUE (holding_id,principal_id,assignment_id),
    FOREIGN KEY (holding_id,principal_id)
        REFERENCES erpx_identity_access_write.principal_authority(holding_id,principal_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (holding_id,role_id)
        REFERENCES erpx_identity_access_write.role_authority(holding_id,role_id)
        ON DELETE RESTRICT,
    CHECK (valid_until > valid_from),
    CHECK (revoked_at IS NULL OR revoked_at >= valid_from)
);
CREATE UNIQUE INDEX identity_active_assignment_authority
    ON erpx_identity_access_write.assignment_authority
       (holding_id,principal_id,role_id,scope_key)
    WHERE revoked_at IS NULL;

CREATE TABLE erpx_identity_access_write.group_membership_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    group_id UUID NOT NULL,
    member_principal_id UUID NOT NULL,
    membership_version BIGINT NOT NULL CHECK (membership_version BETWEEN 1 AND 2),
    valid_from TIMESTAMPTZ NOT NULL,
    valid_until TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    principal_version BIGINT NOT NULL CHECK (principal_version > 0),
    PRIMARY KEY (holding_id,group_id,member_principal_id),
    FOREIGN KEY (holding_id,group_id)
        REFERENCES erpx_identity_access_write.principal_authority(holding_id,principal_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (holding_id,member_principal_id)
        REFERENCES erpx_identity_access_write.principal_authority(holding_id,principal_id)
        ON DELETE RESTRICT,
    CHECK (group_id <> member_principal_id),
    CHECK (valid_until > valid_from),
    CHECK ((membership_version=1) = (revoked_at IS NULL)),
    CHECK (revoked_at IS NULL OR revoked_at >= valid_from)
);

ALTER TABLE erpx_identity_access_write.principal_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_identity_access_write.principal_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY identity_principal_authority_holding_isolation
    ON erpx_identity_access_write.principal_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_identity_access_write.role_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_identity_access_write.role_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY identity_role_authority_holding_isolation
    ON erpx_identity_access_write.role_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_identity_access_write.role_permission_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_identity_access_write.role_permission_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY identity_role_permission_authority_holding_isolation
    ON erpx_identity_access_write.role_permission_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_identity_access_write.assignment_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_identity_access_write.assignment_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY identity_assignment_authority_holding_isolation
    ON erpx_identity_access_write.assignment_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());
ALTER TABLE erpx_identity_access_write.group_membership_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_identity_access_write.group_membership_authority FORCE ROW LEVEL SECURITY;
CREATE POLICY identity_group_membership_authority_holding_isolation
    ON erpx_identity_access_write.group_membership_authority
    USING (holding_id=erpx_cqrs_write.current_holding_id())
    WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id());

-- BEGIN PEOPLE_PAYROLL_NORMALIZED_AUTHORITY
-- People & Payroll owner state. Exact organizational scope is decomposed into
-- scalar columns; payroll lines and calculation evidence are relational
-- children rather than a serialized aggregate document.
CREATE TABLE erpx_people_payroll_write.employee_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    employee_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    employee_code TEXT NOT NULL CHECK (octet_length(employee_code) BETWEEN 1 AND 128),
    display_name TEXT NOT NULL CHECK (octet_length(display_name) BETWEEN 1 AND 255),
    hired_on DATE NOT NULL,
    terminated_on DATE,
    employee_status TEXT NOT NULL CHECK (
        employee_status IN ('draft','active','on_leave','suspended','terminated')
    ),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,employee_id),
    UNIQUE (holding_id,scope_key,employee_code),
    CHECK (updated_at >= created_at),
    CHECK (
        (employee_status='terminated' AND terminated_on IS NOT NULL AND terminated_on >= hired_on)
        OR (employee_status<>'terminated' AND terminated_on IS NULL)
    )
);

CREATE TABLE erpx_people_payroll_write.leave_request_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    leave_request_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    employee_id UUID NOT NULL,
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    reason TEXT NOT NULL CHECK (octet_length(reason) BETWEEN 1 AND 1024),
    leave_status TEXT NOT NULL CHECK (
        leave_status IN ('draft','submitted','approved','rejected','cancelled','taken')
    ),
    requested_by TEXT NOT NULL CHECK (octet_length(requested_by) BETWEEN 1 AND 128),
    approved_by TEXT CHECK (approved_by IS NULL OR octet_length(approved_by) BETWEEN 1 AND 128),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,leave_request_id),
    FOREIGN KEY (holding_id,employee_id)
        REFERENCES erpx_people_payroll_write.employee_authority(holding_id,employee_id)
        ON DELETE RESTRICT,
    CHECK (ends_on >= starts_on),
    CHECK (updated_at >= created_at),
    CHECK (approved_by IS NULL OR approved_by <> requested_by),
    CHECK (leave_status NOT IN ('approved','taken') OR approved_by IS NOT NULL)
);

CREATE TABLE erpx_people_payroll_write.expense_claim_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    expense_claim_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    employee_id UUID NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    amount_minor BIGINT NOT NULL CHECK (amount_minor >= 0),
    reason TEXT NOT NULL CHECK (octet_length(reason) BETWEEN 1 AND 1024),
    expense_status TEXT NOT NULL CHECK (
        expense_status IN ('draft','submitted','approved','rejected','reimbursed')
    ),
    claimed_by TEXT NOT NULL CHECK (octet_length(claimed_by) BETWEEN 1 AND 128),
    approved_by TEXT CHECK (approved_by IS NULL OR octet_length(approved_by) BETWEEN 1 AND 128),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,expense_claim_id),
    FOREIGN KEY (holding_id,employee_id)
        REFERENCES erpx_people_payroll_write.employee_authority(holding_id,employee_id)
        ON DELETE RESTRICT,
    CHECK (updated_at >= created_at),
    CHECK (approved_by IS NULL OR approved_by <> claimed_by),
    CHECK (expense_status NOT IN ('approved','reimbursed') OR approved_by IS NOT NULL)
);

CREATE TABLE erpx_people_payroll_write.payroll_run_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    payroll_run_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    period_code TEXT NOT NULL CHECK (octet_length(period_code) BETWEEN 1 AND 128),
    starts_on DATE NOT NULL,
    ends_on DATE NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    payroll_status TEXT NOT NULL CHECK (
        payroll_status IN ('draft','review','approved','posted','cancelled')
    ),
    prepared_by TEXT NOT NULL CHECK (octet_length(prepared_by) BETWEEN 1 AND 128),
    reviewed_by TEXT CHECK (reviewed_by IS NULL OR octet_length(reviewed_by) BETWEEN 1 AND 128),
    approved_by TEXT CHECK (approved_by IS NULL OR octet_length(approved_by) BETWEEN 1 AND 128),
    total_gross_minor BIGINT NOT NULL CHECK (total_gross_minor >= 0),
    total_deductions_minor BIGINT NOT NULL CHECK (total_deductions_minor >= 0),
    total_net_minor BIGINT NOT NULL CHECK (total_net_minor >= 0),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,payroll_run_id),
    UNIQUE (holding_id,scope_key,period_code),
    CHECK (ends_on >= starts_on),
    CHECK (updated_at >= created_at),
    CHECK (total_gross_minor-total_deductions_minor=total_net_minor),
    CHECK (reviewed_by IS NULL OR reviewed_by<>prepared_by),
    CHECK (approved_by IS NULL OR (approved_by<>prepared_by AND approved_by IS DISTINCT FROM reviewed_by)),
    CHECK (payroll_status NOT IN ('approved','posted') OR reviewed_by IS NOT NULL),
    CHECK (payroll_status<>'posted' OR approved_by IS NOT NULL)
);

CREATE TABLE erpx_people_payroll_write.payroll_line_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    payroll_run_id UUID NOT NULL,
    employee_id UUID NOT NULL,
    gross_minor BIGINT NOT NULL CHECK (gross_minor >= 0),
    deductions_minor BIGINT NOT NULL CHECK (deductions_minor >= 0),
    net_minor BIGINT NOT NULL CHECK (net_minor >= 0),
    payroll_version BIGINT NOT NULL CHECK (payroll_version > 0),
    PRIMARY KEY (holding_id,payroll_run_id,employee_id),
    FOREIGN KEY (holding_id,payroll_run_id)
        REFERENCES erpx_people_payroll_write.payroll_run_authority(holding_id,payroll_run_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (holding_id,employee_id)
        REFERENCES erpx_people_payroll_write.employee_authority(holding_id,employee_id)
        ON DELETE RESTRICT,
    CHECK (gross_minor-deductions_minor=net_minor)
);

CREATE TABLE erpx_people_payroll_write.payroll_calculation_evidence_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    payroll_run_id UUID NOT NULL,
    policy_version TEXT NOT NULL CHECK (octet_length(policy_version) BETWEEN 1 AND 128),
    rate_table_version TEXT NOT NULL CHECK (octet_length(rate_table_version) BETWEEN 1 AND 128),
    calculation_sha256 TEXT NOT NULL CHECK (calculation_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    captured_at TIMESTAMPTZ NOT NULL,
    payroll_version BIGINT NOT NULL CHECK (payroll_version > 0),
    PRIMARY KEY (holding_id,payroll_run_id),
    FOREIGN KEY (holding_id,payroll_run_id)
        REFERENCES erpx_people_payroll_write.payroll_run_authority(holding_id,payroll_run_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_people_payroll_write.payroll_input_evidence_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    payroll_run_id UUID NOT NULL,
    input_ordinal INTEGER NOT NULL CHECK (input_ordinal >= 0),
    source_record_id UUID NOT NULL,
    source_owner_context TEXT NOT NULL CHECK (octet_length(source_owner_context) BETWEEN 1 AND 64),
    source_record_type TEXT NOT NULL CHECK (octet_length(source_record_type) BETWEEN 1 AND 128),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^sha256:[0-9A-Fa-f]{64}$'),
    payroll_version BIGINT NOT NULL CHECK (payroll_version > 0),
    PRIMARY KEY (holding_id,payroll_run_id,input_ordinal),
    UNIQUE (holding_id,payroll_run_id,source_record_id,source_version),
    FOREIGN KEY (holding_id,payroll_run_id)
        REFERENCES erpx_people_payroll_write.payroll_run_authority(holding_id,payroll_run_id)
        ON DELETE RESTRICT
);

CREATE INDEX people_employee_authority_scope
    ON erpx_people_payroll_write.employee_authority
       (holding_id,company_id,branch_id,employee_status,updated_at DESC);
CREATE INDEX people_leave_authority_scope
    ON erpx_people_payroll_write.leave_request_authority
       (holding_id,company_id,branch_id,leave_status,updated_at DESC);
CREATE INDEX people_expense_authority_scope
    ON erpx_people_payroll_write.expense_claim_authority
       (holding_id,company_id,branch_id,expense_status,updated_at DESC);
CREATE INDEX payroll_run_authority_scope
    ON erpx_people_payroll_write.payroll_run_authority
       (holding_id,company_id,branch_id,payroll_status,updated_at DESC);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'employee_authority','leave_request_authority','expense_claim_authority',
        'payroll_run_authority','payroll_line_authority',
        'payroll_calculation_evidence_authority','payroll_input_evidence_authority'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_people_payroll_write.%I ENABLE ROW LEVEL SECURITY',table_name);
        EXECUTE format('ALTER TABLE erpx_people_payroll_write.%I FORCE ROW LEVEL SECURITY',table_name);
        EXECUTE format(
            'CREATE POLICY people_payroll_%I_holding_isolation ON erpx_people_payroll_write.%I USING (holding_id=erpx_cqrs_write.current_holding_id()) WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())',
            table_name,table_name
        );
    END LOOP;
END $$;
-- END PEOPLE_PAYROLL_NORMALIZED_AUTHORITY

-- BEGIN OPERATIONS_NORMALIZED_AUTHORITY
-- Operations exception and policy authority. The inbound SourceFact union is
-- normalized as one discriminator plus exactly one source-specific relation;
-- list-valued facts use owned child rows.
CREATE TABLE erpx_operations_write.exception_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    exception_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    scope_mode TEXT NOT NULL CHECK (scope_mode IN ('holding','company','branch')),
    company_id UUID,
    branch_id UUID,
    reference TEXT NOT NULL CHECK (octet_length(reference) BETWEEN 1 AND 256),
    source_context TEXT NOT NULL,
    source_event_id UUID NOT NULL,
    source_event_type TEXT NOT NULL CHECK (octet_length(source_event_type) BETWEEN 1 AND 256),
    source_aggregate_type TEXT NOT NULL CHECK (octet_length(source_aggregate_type) BETWEEN 1 AND 256),
    source_aggregate_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_correlation_id UUID NOT NULL,
    source_occurred_at TIMESTAMPTZ NOT NULL,
    policy_ref TEXT NOT NULL CHECK (octet_length(policy_ref) BETWEEN 1 AND 128),
    category TEXT NOT NULL CHECK (octet_length(category) BETWEEN 1 AND 128),
    severity TEXT NOT NULL CHECK (octet_length(severity) BETWEEN 1 AND 128),
    assignee_id UUID NOT NULL,
    manager_id UUID NOT NULL,
    assignment_authority_ref TEXT NOT NULL CHECK (octet_length(assignment_authority_ref) BETWEEN 1 AND 256),
    assignment_valid_from TIMESTAMPTZ NOT NULL,
    assignment_valid_until TIMESTAMPTZ NOT NULL,
    due_at TIMESTAMPTZ NOT NULL,
    escalation_after_minutes INTEGER NOT NULL CHECK (escalation_after_minutes BETWEEN 1 AND 10080),
    exception_status TEXT NOT NULL CHECK (exception_status IN ('assigned','acknowledged','escalated','resolved','closed')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    UNIQUE (holding_id,scope_key,reference),
    UNIQUE (holding_id,source_context,source_event_id),
    CHECK (updated_at >= created_at),
    CHECK (assignment_valid_until > assignment_valid_from),
    CHECK (assignee_id <> manager_id),
    CHECK (
        (scope_mode='holding' AND company_id IS NULL AND branch_id IS NULL)
        OR (scope_mode='company' AND company_id IS NOT NULL AND branch_id IS NULL)
        OR (scope_mode='branch' AND company_id IS NOT NULL AND branch_id IS NOT NULL)
    )
);

CREATE TABLE erpx_operations_write.exception_source_fact_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    exception_id UUID NOT NULL,
    fact_kind TEXT NOT NULL CHECK (fact_kind IN (
        'commercial','customer','marketing','marketing_budget_approval','crm','procurement',
        'inventory','assets','maintenance','quality','sales','people_payroll','pos_sale',
        'pos_refund','finance','legal','support','support_incident'
    )),
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id)
        REFERENCES erpx_operations_write.exception_authority(holding_id,exception_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_operations_write.source_commercial_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    subscription_id UUID NOT NULL, customer_id UUID NOT NULL, plan_id UUID NOT NULL,
    subscription_status TEXT NOT NULL, recurring_currency TEXT NOT NULL CHECK (recurring_currency ~ '^[A-Z]{3}$'),
    recurring_minor BIGINT NOT NULL, effective_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_customer_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    customer_id UUID NOT NULL, account_id UUID NOT NULL, customer_code TEXT NOT NULL,
    display_name TEXT NOT NULL, owning_company_id UUID NOT NULL, customer_status TEXT NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_marketing_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    campaign_id UUID NOT NULL, audience_member_id UUID NOT NULL,
    channel_code TEXT NOT NULL, action_code TEXT NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_marketing_approval_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    budget_id UUID NOT NULL, campaign_id UUID NOT NULL, approval_request_id UUID NOT NULL,
    subject_record_id UUID NOT NULL, subject_context TEXT NOT NULL,
    subject_aggregate_kind TEXT NOT NULL, subject_aggregate_version BIGINT NOT NULL CHECK (subject_aggregate_version > 0),
    policy_code TEXT NOT NULL, amount_currency TEXT CHECK (amount_currency IS NULL OR amount_currency ~ '^[A-Z]{3}$'),
    amount_minor BIGINT, requested_by TEXT NOT NULL, requested_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT,
    CHECK ((amount_currency IS NULL)=(amount_minor IS NULL))
);
CREATE TABLE erpx_operations_write.source_crm_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    opportunity_id UUID NOT NULL, customer_id UUID NOT NULL, opportunity_stage TEXT NOT NULL,
    expected_currency TEXT NOT NULL CHECK (expected_currency ~ '^[A-Z]{3}$'), expected_minor BIGINT NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_procurement_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    purchase_order_id UUID NOT NULL, supplier_id UUID NOT NULL, order_code TEXT NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_procurement_line_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL, line_ordinal INTEGER NOT NULL CHECK (line_ordinal>=0),
    line_id UUID NOT NULL, item_code TEXT NOT NULL, quantity_units BIGINT NOT NULL,
    quantity_scale SMALLINT NOT NULL CHECK (quantity_scale BETWEEN 0 AND 9),
    line_currency TEXT NOT NULL CHECK (line_currency ~ '^[A-Z]{3}$'), line_minor BIGINT NOT NULL,
    destination_code TEXT NOT NULL,
    PRIMARY KEY (holding_id,exception_id,line_ordinal),
    UNIQUE (holding_id,exception_id,line_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.source_procurement_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_inventory_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    movement_id UUID NOT NULL, stock_item_id UUID NOT NULL, warehouse_id UUID NOT NULL,
    item_code TEXT NOT NULL, quantity_units BIGINT NOT NULL,
    quantity_scale SMALLINT NOT NULL CHECK (quantity_scale BETWEEN 0 AND 9), procurement_line_id UUID,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_assets_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    asset_id UUID NOT NULL, asset_code TEXT NOT NULL, asset_name TEXT NOT NULL,
    location_id UUID NOT NULL, asset_condition TEXT NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_maintenance_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    work_order_id UUID NOT NULL, work_order_code TEXT NOT NULL, asset_id UUID NOT NULL,
    work_order_status TEXT NOT NULL, scheduled_for TIMESTAMPTZ,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_quality_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    inspection_id UUID NOT NULL, subject_id UUID NOT NULL, inspection_code TEXT NOT NULL,
    inspection_outcome TEXT NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_sales_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    sales_order_id UUID NOT NULL, order_code TEXT NOT NULL, customer_id UUID NOT NULL,
    sales_status TEXT NOT NULL, total_currency TEXT NOT NULL CHECK (total_currency ~ '^[A-Z]{3}$'),
    total_minor BIGINT NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_sales_line_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL, line_ordinal INTEGER NOT NULL CHECK (line_ordinal>=0),
    line_id UUID NOT NULL, item_code TEXT NOT NULL, quantity_units BIGINT NOT NULL,
    quantity_scale SMALLINT NOT NULL CHECK (quantity_scale BETWEEN 0 AND 9),
    line_currency TEXT NOT NULL CHECK (line_currency ~ '^[A-Z]{3}$'), line_minor BIGINT NOT NULL,
    PRIMARY KEY (holding_id,exception_id,line_ordinal),
    UNIQUE (holding_id,exception_id,line_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.source_sales_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_people_payroll_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    payroll_run_id UUID NOT NULL, period_code TEXT NOT NULL, finalized_at TIMESTAMPTZ NOT NULL,
    gross_currency TEXT NOT NULL CHECK (gross_currency ~ '^[A-Z]{3}$'), gross_minor BIGINT NOT NULL,
    deduction_currency TEXT NOT NULL CHECK (deduction_currency ~ '^[A-Z]{3}$'), deduction_minor BIGINT NOT NULL,
    net_currency TEXT NOT NULL CHECK (net_currency ~ '^[A-Z]{3}$'), net_minor BIGINT NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_pos_sale_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    transaction_id UUID NOT NULL, register_id UUID NOT NULL, receipt_code TEXT NOT NULL,
    customer_id UUID, gross_currency TEXT NOT NULL CHECK (gross_currency ~ '^[A-Z]{3}$'),
    gross_minor BIGINT NOT NULL, completed_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_pos_refund_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    refund_id UUID NOT NULL, original_sale_id UUID NOT NULL,
    amount_currency TEXT NOT NULL CHECK (amount_currency ~ '^[A-Z]{3}$'), amount_minor BIGINT NOT NULL,
    approved_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_finance_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    journal_id UUID NOT NULL, journal_code TEXT NOT NULL, journal_source_context TEXT NOT NULL,
    source_reference TEXT NOT NULL,
    debit_currency TEXT NOT NULL CHECK (debit_currency ~ '^[A-Z]{3}$'), debit_minor BIGINT NOT NULL,
    credit_currency TEXT NOT NULL CHECK (credit_currency ~ '^[A-Z]{3}$'), credit_minor BIGINT NOT NULL,
    posted_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_legal_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    obligation_id UUID NOT NULL, legal_case_id UUID NOT NULL, customer_id UUID,
    obligation_code TEXT NOT NULL, obligation_title TEXT NOT NULL, obligation_status TEXT NOT NULL,
    due_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_support_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    support_case_id UUID NOT NULL, case_code TEXT NOT NULL, customer_id UUID NOT NULL,
    support_status TEXT NOT NULL, category_code TEXT NOT NULL, source_updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_support_incident_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL,
    incident_id UUID NOT NULL, incident_code TEXT NOT NULL, incident_status TEXT NOT NULL,
    severity_code TEXT NOT NULL, source_updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,exception_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.exception_source_fact_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_support_service_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL, service_ordinal INTEGER NOT NULL CHECK (service_ordinal>=0),
    service_code TEXT NOT NULL,
    PRIMARY KEY (holding_id,exception_id,service_ordinal),
    UNIQUE (holding_id,exception_id,service_code),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.source_support_incident_authority(holding_id,exception_id) ON DELETE RESTRICT
);
CREATE TABLE erpx_operations_write.source_support_customer_authority (
    holding_id UUID NOT NULL, exception_id UUID NOT NULL, customer_ordinal INTEGER NOT NULL CHECK (customer_ordinal>=0),
    customer_account_id UUID NOT NULL,
    PRIMARY KEY (holding_id,exception_id,customer_ordinal),
    UNIQUE (holding_id,exception_id,customer_account_id),
    FOREIGN KEY (holding_id,exception_id) REFERENCES erpx_operations_write.source_support_incident_authority(holding_id,exception_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_operations_write.work_queue_policy_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    policy_id UUID NOT NULL, aggregate_version BIGINT NOT NULL CHECK (aggregate_version>0),
    scope_key TEXT NOT NULL, policy_name TEXT NOT NULL,
    escalation_after_minutes INTEGER NOT NULL CHECK (escalation_after_minutes BETWEEN 1 AND 10080),
    policy_status TEXT NOT NULL CHECK (policy_status IN ('active','paused','retired')),
    created_at TIMESTAMPTZ NOT NULL, updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,policy_id), UNIQUE (holding_id,scope_key,policy_name),
    CHECK (updated_at>=created_at)
);
CREATE TABLE erpx_operations_write.work_queue_policy_source_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    policy_id UUID NOT NULL, source_context TEXT NOT NULL, policy_version BIGINT NOT NULL CHECK (policy_version>0),
    PRIMARY KEY (holding_id,policy_id,source_context),
    FOREIGN KEY (holding_id,policy_id) REFERENCES erpx_operations_write.work_queue_policy_authority(holding_id,policy_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_operations_write.source_receipt_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    source_event_id UUID NOT NULL,
    source_context TEXT NOT NULL,
    source_fingerprint BYTEA NOT NULL CHECK (octet_length(source_fingerprint)=32),
    disposition TEXT NOT NULL CHECK (disposition IN ('ignored','detected')),
    exception_id UUID,
    applied_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,source_event_id),
    FOREIGN KEY (holding_id,exception_id)
        REFERENCES erpx_operations_write.exception_authority(holding_id,exception_id)
        ON DELETE RESTRICT,
    CHECK ((disposition='ignored')=(exception_id IS NULL))
);
CREATE TABLE erpx_operations_write.source_checkpoint_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    source_database TEXT NOT NULL CHECK (source_database IN ('control','holding')),
    source_key TEXT NOT NULL CHECK (octet_length(source_key) BETWEEN 1 AND 255),
    source_partition TEXT NOT NULL,
    source_sequence BIGINT NOT NULL CHECK (source_sequence >= 0),
    source_event_id UUID,
    source_occurred_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,source_database,source_key,source_partition),
    CHECK ((source_sequence=0)=(source_event_id IS NULL))
);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'exception_authority','exception_source_fact_authority','source_commercial_authority',
        'source_customer_authority','source_marketing_authority','source_marketing_approval_authority',
        'source_crm_authority','source_procurement_authority','source_procurement_line_authority',
        'source_inventory_authority','source_assets_authority','source_maintenance_authority',
        'source_quality_authority','source_sales_authority','source_sales_line_authority',
        'source_people_payroll_authority','source_pos_sale_authority','source_pos_refund_authority',
        'source_finance_authority','source_legal_authority','source_support_authority',
        'source_support_incident_authority','source_support_service_authority',
        'source_support_customer_authority','work_queue_policy_authority',
        'work_queue_policy_source_authority','source_receipt_authority',
        'source_checkpoint_authority'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_operations_write.%I ENABLE ROW LEVEL SECURITY',table_name);
        EXECUTE format('ALTER TABLE erpx_operations_write.%I FORCE ROW LEVEL SECURITY',table_name);
        EXECUTE format(
            'CREATE POLICY operations_%I_holding_isolation ON erpx_operations_write.%I USING (holding_id=erpx_cqrs_write.current_holding_id()) WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())',
            table_name,table_name
        );
    END LOOP;
END $$;
-- END OPERATIONS_NORMALIZED_AUTHORITY

-- BEGIN SUPPORT_NORMALIZED_AUTHORITY
-- Support customer-case, incident, access-session, and capability command
-- authority. Earlier JSON guard columns remain generation-one evidence only;
-- current command rehydration uses these typed roots and child relations.

CREATE TABLE erpx_support_write.ticket_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    ticket_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    case_code TEXT NOT NULL CHECK (octet_length(case_code) BETWEEN 1 AND 128),
    category TEXT NOT NULL CHECK (octet_length(category) BETWEEN 1 AND 128),
    severity TEXT NOT NULL CHECK (octet_length(severity) BETWEEN 1 AND 128),
    description TEXT NOT NULL CHECK (octet_length(description) BETWEEN 1 AND 16000),
    ticket_status TEXT NOT NULL CHECK (ticket_status IN (
        'new','triaged','waiting_for_customer','access_requested','approved',
        'in_progress','resolved','customer_confirmed','closed'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,ticket_id),
    UNIQUE (holding_id,ticket_id,aggregate_version),
    UNIQUE (holding_id,scope_key,case_code),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_support_write.ticket_message_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    ticket_id UUID NOT NULL,
    message_ordinal INTEGER NOT NULL CHECK (message_ordinal >= 0),
    message_id UUID NOT NULL,
    author_ref TEXT NOT NULL CHECK (octet_length(author_ref) BETWEEN 1 AND 256),
    audience TEXT NOT NULL CHECK (audience IN ('customer_visible','internal')),
    body TEXT NOT NULL CHECK (octet_length(body) BETWEEN 1 AND 16000),
    created_at TIMESTAMPTZ NOT NULL,
    ticket_version BIGINT NOT NULL CHECK (ticket_version > 0),
    PRIMARY KEY (holding_id,ticket_id,message_ordinal),
    UNIQUE (holding_id,ticket_id,message_id),
    FOREIGN KEY (holding_id,ticket_id)
        REFERENCES erpx_support_write.ticket_authority(holding_id,ticket_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_support_write.incident_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    incident_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    incident_code TEXT NOT NULL CHECK (octet_length(incident_code) BETWEEN 1 AND 128),
    severity TEXT NOT NULL CHECK (octet_length(severity) BETWEEN 1 AND 128),
    commander_ref TEXT NOT NULL CHECK (octet_length(commander_ref) BETWEEN 1 AND 256),
    incident_status TEXT NOT NULL CHECK (incident_status IN (
        'investigating','identified','monitoring','resolved','postmortem_complete'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id,incident_id),
    UNIQUE (holding_id,incident_id,aggregate_version),
    UNIQUE (holding_id,scope_key,incident_code),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_support_write.incident_service_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    incident_id UUID NOT NULL,
    service_ordinal INTEGER NOT NULL CHECK (service_ordinal >= 0),
    service_ref TEXT NOT NULL CHECK (octet_length(service_ref) BETWEEN 1 AND 256),
    incident_version BIGINT NOT NULL CHECK (incident_version > 0),
    PRIMARY KEY (holding_id,incident_id,service_ordinal),
    UNIQUE (holding_id,incident_id,service_ref),
    FOREIGN KEY (holding_id,incident_id)
        REFERENCES erpx_support_write.incident_authority(holding_id,incident_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_support_write.incident_customer_account_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    incident_id UUID NOT NULL,
    account_ordinal INTEGER NOT NULL CHECK (account_ordinal >= 0),
    customer_account_id UUID NOT NULL,
    incident_version BIGINT NOT NULL CHECK (incident_version > 0),
    PRIMARY KEY (holding_id,incident_id,account_ordinal),
    FOREIGN KEY (holding_id,incident_id)
        REFERENCES erpx_support_write.incident_authority(holding_id,incident_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_support_write.incident_timeline_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    incident_id UUID NOT NULL,
    timeline_ordinal INTEGER NOT NULL CHECK (timeline_ordinal >= 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    actor_ref TEXT NOT NULL CHECK (octet_length(actor_ref) BETWEEN 1 AND 256),
    message TEXT NOT NULL CHECK (octet_length(message) BETWEEN 1 AND 2000),
    incident_version BIGINT NOT NULL CHECK (incident_version > 0),
    PRIMARY KEY (holding_id,incident_id,timeline_ordinal),
    FOREIGN KEY (holding_id,incident_id)
        REFERENCES erpx_support_write.incident_authority(holding_id,incident_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_support_write.incident_broadcast_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    incident_id UUID NOT NULL,
    broadcast_ordinal INTEGER NOT NULL CHECK (broadcast_ordinal >= 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    actor_ref TEXT NOT NULL CHECK (octet_length(actor_ref) BETWEEN 1 AND 256),
    audience TEXT NOT NULL CHECK (audience IN ('customer_visible','internal')),
    message TEXT NOT NULL CHECK (octet_length(message) BETWEEN 1 AND 2000),
    incident_version BIGINT NOT NULL CHECK (incident_version > 0),
    PRIMARY KEY (holding_id,incident_id,broadcast_ordinal),
    FOREIGN KEY (holding_id,incident_id)
        REFERENCES erpx_support_write.incident_authority(holding_id,incident_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_support_write.session_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    session_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    ticket_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    agent_principal_id UUID NOT NULL,
    represented_principal_id UUID,
    support_mode TEXT NOT NULL CHECK (support_mode IN (
        'diagnostic_read','holding_mutation','impersonation'
    )),
    module TEXT NOT NULL CHECK (octet_length(module) BETWEEN 1 AND 128),
    reason TEXT NOT NULL CHECK (octet_length(reason) BETWEEN 1 AND 2000),
    session_status TEXT NOT NULL CHECK (session_status IN ('requested','approved','revoked')),
    requested_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    approved_by TEXT CHECK (approved_by IS NULL OR octet_length(approved_by) BETWEEN 1 AND 256),
    approved_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    revoked_reason TEXT CHECK (
        revoked_reason IS NULL OR octet_length(revoked_reason) BETWEEN 1 AND 2000
    ),
    PRIMARY KEY (holding_id,session_id),
    UNIQUE (holding_id,session_id,aggregate_version),
    FOREIGN KEY (holding_id,ticket_id)
        REFERENCES erpx_support_write.ticket_authority(holding_id,ticket_id)
        ON DELETE RESTRICT,
    CHECK (expires_at > requested_at),
    CHECK ((support_mode='impersonation') = (represented_principal_id IS NOT NULL)),
    CHECK ((approved_by IS NULL) = (approved_at IS NULL)),
    CHECK (approved_by IS NULL OR approved_by <> agent_principal_id::TEXT),
    CHECK ((revoked_at IS NULL) = (revoked_reason IS NULL)),
    CHECK (
        (session_status='requested' AND aggregate_version=1
            AND approved_at IS NULL AND revoked_at IS NULL)
        OR (session_status='approved' AND aggregate_version=2
            AND approved_at IS NOT NULL AND revoked_at IS NULL)
        OR (session_status='revoked' AND aggregate_version IN (2,3)
            AND revoked_at IS NOT NULL)
    )
);

CREATE TABLE erpx_support_write.session_permission_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    session_id UUID NOT NULL,
    permission_ordinal INTEGER NOT NULL CHECK (permission_ordinal >= 0),
    permission TEXT NOT NULL CHECK (octet_length(permission) BETWEEN 1 AND 256),
    session_version BIGINT NOT NULL CHECK (session_version > 0),
    PRIMARY KEY (holding_id,session_id,permission_ordinal),
    UNIQUE (holding_id,session_id,permission),
    FOREIGN KEY (holding_id,session_id)
        REFERENCES erpx_support_write.session_authority(holding_id,session_id)
        ON DELETE RESTRICT
);

CREATE TABLE erpx_support_write.capability_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    capability_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version IN (1,2)),
    scope_key TEXT NOT NULL,
    company_id UUID NOT NULL,
    branch_id UUID,
    session_id UUID NOT NULL,
    ticket_id UUID NOT NULL,
    customer_id UUID NOT NULL,
    customer_account_id UUID NOT NULL,
    agent_principal_id UUID NOT NULL,
    device_ref TEXT NOT NULL CHECK (octet_length(device_ref) BETWEEN 1 AND 256),
    support_mode TEXT NOT NULL CHECK (support_mode IN (
        'diagnostic_read','holding_mutation','impersonation'
    )),
    module TEXT NOT NULL CHECK (octet_length(module) BETWEEN 1 AND 128),
    secret_hash_sha256 BYTEA NOT NULL CHECK (octet_length(secret_hash_sha256)=32),
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revocation_reason TEXT CHECK (revocation_reason IN (
        'customer_requested','agent_disabled','device_revoked','session_revoked',
        'ticket_closed','administrative'
    )),
    revoked_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id,capability_id),
    UNIQUE (holding_id,capability_id,aggregate_version),
    FOREIGN KEY (holding_id,session_id)
        REFERENCES erpx_support_write.session_authority(holding_id,session_id)
        ON DELETE RESTRICT,
    FOREIGN KEY (holding_id,ticket_id)
        REFERENCES erpx_support_write.ticket_authority(holding_id,ticket_id)
        ON DELETE RESTRICT,
    CHECK (expires_at > issued_at),
    CHECK ((revocation_reason IS NULL) = (revoked_at IS NULL)),
    CHECK (
        (aggregate_version=1 AND revoked_at IS NULL)
        OR (aggregate_version=2 AND revoked_at IS NOT NULL)
    )
);

CREATE TABLE erpx_support_write.capability_permission_authority (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    capability_id UUID NOT NULL,
    permission_ordinal INTEGER NOT NULL CHECK (permission_ordinal >= 0),
    permission TEXT NOT NULL CHECK (octet_length(permission) BETWEEN 1 AND 256),
    capability_version BIGINT NOT NULL CHECK (capability_version IN (1,2)),
    PRIMARY KEY (holding_id,capability_id,permission_ordinal),
    UNIQUE (holding_id,capability_id,permission),
    FOREIGN KEY (holding_id,capability_id)
        REFERENCES erpx_support_write.capability_authority(holding_id,capability_id)
        ON DELETE RESTRICT
);

CREATE INDEX support_ticket_authority_scope
    ON erpx_support_write.ticket_authority
        (holding_id,company_id,branch_id,customer_id,ticket_status,updated_at DESC);
CREATE INDEX support_incident_authority_scope
    ON erpx_support_write.incident_authority
        (holding_id,company_id,branch_id,incident_status,updated_at DESC);
CREATE INDEX support_session_authority_scope
    ON erpx_support_write.session_authority
        (holding_id,company_id,branch_id,ticket_id,session_status,expires_at DESC);
CREATE INDEX support_capability_authority_scope
    ON erpx_support_write.capability_authority
        (holding_id,company_id,branch_id,session_id,expires_at DESC);

DO $$
DECLARE table_name TEXT;
BEGIN
    FOREACH table_name IN ARRAY ARRAY[
        'ticket_authority','ticket_message_authority','incident_authority',
        'incident_service_authority','incident_customer_account_authority',
        'incident_timeline_authority','incident_broadcast_authority',
        'session_authority','session_permission_authority','capability_authority',
        'capability_permission_authority'
    ] LOOP
        EXECUTE format('ALTER TABLE erpx_support_write.%I ENABLE ROW LEVEL SECURITY',table_name);
        EXECUTE format('ALTER TABLE erpx_support_write.%I FORCE ROW LEVEL SECURITY',table_name);
        EXECUTE format(
            'CREATE POLICY support_%I_holding_isolation ON erpx_support_write.%I USING (holding_id=erpx_cqrs_write.current_holding_id()) WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())',
            table_name,table_name
        );
    END LOOP;
END $$;
-- END SUPPORT_NORMALIZED_AUTHORITY


-- Hard cutover: aggregate state is owned by context-specific typed relational
-- tables. Remove the obsolete shared JSON snapshot authority for databases
-- that applied an earlier current foundation revision.
DROP TABLE IF EXISTS erpx_cqrs_write.aggregate_snapshots;
DROP TABLE IF EXISTS erpx_holding.aggregate_snapshots;

-- OPERATIONS_ORGANIZATION_LEGACY_AUTHORITY_RETIREMENT
-- Operations commands, source ingestion, receipts, audit and outbox now use
-- erpx_operations_write typed authority and erpx_cqrs_write envelopes.
-- Organization commands use erpx_organization_write typed authority. The
-- generation-one monolithic event/snapshot/SLA path is intentionally removed
-- rather than retained as a second write authority.
DROP TABLE IF EXISTS erpx_holding.operations_source_references;
DROP TABLE IF EXISTS erpx_holding.operations_sla_notifications;
DROP TABLE IF EXISTS erpx_holding.operations_events;
DROP TABLE IF EXISTS erpx_holding.operations_aggregate_snapshots;

-- Final holding_write / holding-write generation fence. The authoritative
-- erpx_meta.schema_manifest must still identify schema_revision 2.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='holding_write' AND migration_set='holding-write'
    ) THEN
        RAISE EXCEPTION 'holding-write current manifest is not current';
    END IF;
END;
$$;


-- Durable authority for the Sales-to-Finance vertical slice. Each bounded
-- context owns its rows; cross-context links are immutable identifiers and
-- versions delivered through the shared CQRS outbox.

CREATE TABLE erpx_agent_write.suggestion_envelopes (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    suggestion_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    owner_context TEXT NOT NULL CHECK (owner_context IN ('sales', 'finance')),
    capability TEXT NOT NULL CHECK (capability IN (
        'sales.quote.compose.v1', 'sales.revenue-arrangement.v1',
        'finance.accounting-plan.v1', 'finance.posting-review.v1'
    )),
    source_record_kind TEXT NOT NULL,
    source_record_id UUID NOT NULL,
    source_record_version BIGINT NOT NULL CHECK (source_record_version > 0),
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    payload_digest TEXT NOT NULL CHECK (payload_digest ~ '^[0-9A-Fa-f]{64}$'),
    output_digest TEXT CHECK (output_digest ~ '^[0-9A-Fa-f]{64}$'),
    provider_kind TEXT NOT NULL CHECK (provider_kind IN ('fixture', 'ollama', 'open_ai_compatible')),
    provider_ref TEXT NOT NULL,
    model_ref TEXT NOT NULL,
    prompt_digest TEXT NOT NULL CHECK (prompt_digest ~ '^[0-9A-Fa-f]{64}$'),
    invocation_evidence_ref TEXT NOT NULL,
    suggestion_status TEXT NOT NULL CHECK (suggestion_status IN (
        'queued', 'running', 'ready', 'applied', 'rejected', 'superseded', 'failed'
    )),
    requested_by TEXT NOT NULL,
    applied_by TEXT,
    failure_code TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, suggestion_id),
    CHECK (updated_at >= created_at),
    CHECK (suggestion_status NOT IN ('ready', 'applied', 'rejected') OR output_digest IS NOT NULL),
    CHECK (suggestion_status NOT IN ('queued', 'running', 'failed') OR output_digest IS NULL)
);

CREATE INDEX agent_suggestion_source_version
    ON erpx_agent_write.suggestion_envelopes
        (holding_id, owner_context, source_record_id, source_record_version, updated_at DESC);

CREATE TABLE erpx_operations_write.approval_sessions (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    approval_session_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    scope_key TEXT NOT NULL,
    subject_context TEXT NOT NULL CHECK (subject_context IN ('sales', 'finance')),
    subject_kind TEXT NOT NULL,
    subject_id UUID NOT NULL,
    subject_version BIGINT NOT NULL CHECK (subject_version > 0),
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    requested_by TEXT NOT NULL,
    session_status TEXT NOT NULL CHECK (session_status IN (
        'pending', 'approved', 'rejected', 'superseded', 'cancelled'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, approval_session_id),
    UNIQUE (holding_id, subject_context, subject_kind, subject_id, subject_version, policy_version),
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_operations_write.approval_session_gates (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    approval_session_id UUID NOT NULL,
    gate_kind TEXT NOT NULL CHECK (gate_kind IN (
        'sales_commercial', 'finance_accounting_plan', 'finance_posting'
    )),
    assigned_to TEXT,
    gate_status TEXT NOT NULL CHECK (gate_status IN ('pending', 'approved', 'rejected', 'cancelled')),
    decided_by TEXT,
    reason_code TEXT,
    decided_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, approval_session_id, gate_kind),
    FOREIGN KEY (holding_id, approval_session_id) REFERENCES
        erpx_operations_write.approval_sessions (holding_id, approval_session_id) ON DELETE RESTRICT,
    CHECK (
        (gate_status = 'pending' AND decided_by IS NULL AND reason_code IS NULL AND decided_at IS NULL)
        OR (gate_status IN ('approved', 'rejected') AND decided_by IS NOT NULL AND reason_code IS NOT NULL AND decided_at IS NOT NULL)
        OR gate_status = 'cancelled'
    )
);

CREATE INDEX operations_my_approval_work
    ON erpx_operations_write.approval_session_gates
        (holding_id, assigned_to, gate_status, approval_session_id)
    WHERE gate_status = 'pending';

CREATE TABLE erpx_sales_write.quote_workflows (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    quote_id UUID NOT NULL,
    workflow_version BIGINT NOT NULL CHECK (workflow_version > 0),
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    customer_id UUID NOT NULL,
    customer_version BIGINT NOT NULL CHECK (customer_version > 0),
    quote_code TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    payment_terms_days INTEGER NOT NULL CHECK (payment_terms_days BETWEEN 0 AND 365),
    valid_until TIMESTAMPTZ NOT NULL,
    document_language TEXT NOT NULL CHECK (document_language IN ('thai', 'english')),
    notes TEXT NOT NULL,
    maker TEXT NOT NULL,
    workflow_status TEXT NOT NULL CHECK (workflow_status IN (
        'draft', 'pending_approval', 'approval_rejected', 'approved', 'pdf_generated',
        'customer_accepted', 'customer_rejected', 'superseded'
    )),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, quote_id),
    UNIQUE (holding_id, quote_code),
    FOREIGN KEY (quote_id) REFERENCES erpx_sales_write.quote_guards (quote_id) ON DELETE RESTRICT,
    CHECK (updated_at >= created_at)
);

CREATE TABLE erpx_sales_write.quote_workflow_lines (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    line_id UUID NOT NULL,
    line_source TEXT NOT NULL CHECK (line_source IN ('catalog', 'custom')),
    item_id UUID,
    item_version BIGINT CHECK (item_version > 0),
    custom_reason TEXT,
    item_code TEXT NOT NULL,
    description TEXT NOT NULL,
    quantity BIGINT NOT NULL CHECK (quantity > 0),
    unit_price_minor BIGINT NOT NULL CHECK (unit_price_minor >= 0),
    discount_basis_points INTEGER NOT NULL CHECK (discount_basis_points BETWEEN 0 AND 10000),
    tax_profile_id UUID NOT NULL,
    tax_profile_version BIGINT NOT NULL CHECK (tax_profile_version > 0),
    tax_classification TEXT NOT NULL CHECK (tax_classification IN ('standard_vat', 'zero_rated', 'exempt')),
    tax_rate_basis_points INTEGER NOT NULL CHECK (tax_rate_basis_points BETWEEN 0 AND 10000),
    tax_authority_ref TEXT NOT NULL,
    PRIMARY KEY (holding_id, quote_id, quote_version, line_id),
    FOREIGN KEY (holding_id, quote_id) REFERENCES
        erpx_sales_write.quote_workflows (holding_id, quote_id) ON DELETE RESTRICT,
    CHECK (
        (line_source = 'catalog' AND item_id IS NOT NULL AND item_version IS NOT NULL AND custom_reason IS NULL)
        OR (line_source = 'custom' AND item_id IS NULL AND item_version IS NULL AND custom_reason IS NOT NULL)
    )
);

CREATE TABLE erpx_sales_write.quote_applied_suggestions (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    suggestion_id UUID NOT NULL,
    source_quote_version BIGINT NOT NULL CHECK (source_quote_version > 0),
    payload_digest TEXT NOT NULL CHECK (payload_digest ~ '^[0-9A-Fa-f]{64}$'),
    output_digest TEXT NOT NULL CHECK (output_digest ~ '^[0-9A-Fa-f]{64}$'),
    provider_label TEXT NOT NULL,
    applied_by TEXT NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL,
    superseded_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, quote_id, quote_version),
    FOREIGN KEY (holding_id, quote_id) REFERENCES
        erpx_sales_write.quote_workflows (holding_id, quote_id) ON DELETE RESTRICT,
    CHECK (source_quote_version < quote_version),
    CHECK (superseded_at IS NULL OR superseded_at >= applied_at)
);

CREATE TABLE erpx_sales_write.quote_approval_links (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    approval_session_id UUID NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    sales_approver TEXT NOT NULL,
    finance_approver TEXT NOT NULL,
    approved_at TIMESTAMPTZ NOT NULL,
    superseded_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, quote_id, quote_version),
    FOREIGN KEY (holding_id, quote_id) REFERENCES
        erpx_sales_write.quote_workflows (holding_id, quote_id) ON DELETE RESTRICT,
    CHECK (sales_approver <> finance_approver),
    CHECK (superseded_at IS NULL OR superseded_at >= approved_at)
);

CREATE TABLE erpx_sales_write.quote_approval_rejections (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    approval_session_id UUID NOT NULL,
    policy_version BIGINT NOT NULL CHECK (policy_version > 0),
    rejected_gate TEXT NOT NULL,
    rejected_by TEXT NOT NULL,
    reason_code TEXT NOT NULL,
    rejected_at TIMESTAMPTZ NOT NULL,
    superseded_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, quote_id, quote_version),
    FOREIGN KEY (holding_id, quote_id) REFERENCES
        erpx_sales_write.quote_workflows (holding_id, quote_id) ON DELETE RESTRICT,
    CHECK (superseded_at IS NULL OR superseded_at >= rejected_at)
);

CREATE TABLE erpx_sales_write.quote_accounting_plan_links (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    accounting_plan_id UUID NOT NULL,
    accounting_plan_version BIGINT NOT NULL CHECK (accounting_plan_version > 0),
    approved_by TEXT NOT NULL,
    approved_at TIMESTAMPTZ NOT NULL,
    superseded_at TIMESTAMPTZ,
    PRIMARY KEY (holding_id, quote_id, quote_version),
    FOREIGN KEY (holding_id, quote_id) REFERENCES
        erpx_sales_write.quote_workflows (holding_id, quote_id) ON DELETE RESTRICT,
    CHECK (superseded_at IS NULL OR superseded_at >= approved_at)
);

CREATE TABLE erpx_sales_write.quote_pdf_artifacts (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    artifact_id UUID NOT NULL,
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    document_language TEXT NOT NULL CHECK (document_language IN ('thai', 'english')),
    object_key TEXT NOT NULL,
    sha256_digest TEXT NOT NULL CHECK (sha256_digest ~ '^[0-9A-Fa-f]{64}$'),
    byte_length BIGINT NOT NULL CHECK (byte_length > 0),
    generated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, artifact_id),
    UNIQUE (holding_id, quote_id, quote_version),
    FOREIGN KEY (holding_id, quote_id) REFERENCES
        erpx_sales_write.quote_workflows (holding_id, quote_id) ON DELETE RESTRICT,
    CHECK (object_key !~ '(^/|(^|/)\.\.(/|$)|//)')
);

CREATE TABLE erpx_sales_write.quote_external_decisions (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    artifact_id UUID NOT NULL,
    decision TEXT NOT NULL CHECK (decision IN ('accepted', 'rejected')),
    evidence_reference TEXT NOT NULL,
    attachment_object_key TEXT,
    recorded_by TEXT NOT NULL,
    decided_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, quote_id, quote_version),
    FOREIGN KEY (holding_id, artifact_id) REFERENCES
        erpx_sales_write.quote_pdf_artifacts (holding_id, artifact_id) ON DELETE RESTRICT,
    CHECK (attachment_object_key IS NULL OR attachment_object_key !~ '(^/|(^|/)\.\.(/|$)|//)')
);

CREATE OR REPLACE FUNCTION erpx_sales_write.reject_immutable_quote_artifact_change()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'quote PDF and external-decision evidence is immutable';
END
$$;

CREATE TRIGGER quote_pdf_artifacts_immutable
    BEFORE UPDATE OR DELETE ON erpx_sales_write.quote_pdf_artifacts
    FOR EACH ROW EXECUTE FUNCTION erpx_sales_write.reject_immutable_quote_artifact_change();

CREATE TRIGGER quote_external_decisions_immutable
    BEFORE UPDATE OR DELETE ON erpx_sales_write.quote_external_decisions
    FOR EACH ROW EXECUTE FUNCTION erpx_sales_write.reject_immutable_quote_artifact_change();

CREATE TABLE erpx_finance_write.accounting_plans (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    accounting_plan_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    quote_id UUID NOT NULL,
    quote_version BIGINT NOT NULL CHECK (quote_version > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    quote_net_minor BIGINT NOT NULL CHECK (quote_net_minor > 0),
    quote_tax_minor BIGINT NOT NULL CHECK (quote_tax_minor >= 0),
    dimensions JSONB NOT NULL,
    tax_profile_id UUID NOT NULL,
    tax_profile_version BIGINT NOT NULL CHECK (tax_profile_version > 0),
    tax_classification TEXT NOT NULL CHECK (tax_classification IN ('standard_vat', 'zero_rated', 'exempt')),
    tax_rate_basis_points INTEGER NOT NULL CHECK (tax_rate_basis_points BETWEEN 0 AND 10000),
    tax_authority_ref TEXT NOT NULL,
    accounts_receivable_id UUID NOT NULL,
    output_vat_id UUID NOT NULL,
    contract_liability_id UUID NOT NULL,
    revenue_id UUID NOT NULL,
    preparer TEXT NOT NULL,
    sales_approver TEXT NOT NULL,
    finance_approver TEXT,
    approval_session_id UUID,
    suggestion_id UUID,
    suggestion_payload_digest TEXT CHECK (suggestion_payload_digest ~ '^[0-9A-Fa-f]{64}$'),
    suggestion_output_digest TEXT CHECK (suggestion_output_digest ~ '^[0-9A-Fa-f]{64}$'),
    suggestion_provider_label TEXT,
    plan_status TEXT NOT NULL CHECK (plan_status IN ('draft', 'submitted', 'approved', 'rejected', 'superseded')),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, accounting_plan_id),
    UNIQUE (holding_id, quote_id, quote_version),
    CHECK (updated_at >= created_at),
    CHECK (preparer <> sales_approver),
    CHECK (finance_approver IS NULL OR (finance_approver <> preparer AND finance_approver <> sales_approver)),
    CHECK (
        (suggestion_id IS NULL AND suggestion_payload_digest IS NULL AND suggestion_output_digest IS NULL AND suggestion_provider_label IS NULL)
        OR (suggestion_id IS NOT NULL AND suggestion_payload_digest IS NOT NULL AND suggestion_output_digest IS NOT NULL AND suggestion_provider_label IS NOT NULL)
    )
);

CREATE TABLE erpx_finance_write.accounting_plan_obligations (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    accounting_plan_id UUID NOT NULL,
    obligation_id UUID NOT NULL,
    allocated_minor BIGINT NOT NULL CHECK (allocated_minor > 0),
    template_code TEXT NOT NULL,
    PRIMARY KEY (holding_id, accounting_plan_id, obligation_id),
    FOREIGN KEY (holding_id, accounting_plan_id) REFERENCES
        erpx_finance_write.accounting_plans (holding_id, accounting_plan_id) ON DELETE RESTRICT
);

CREATE TABLE erpx_finance_write.posting_packets (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    posting_packet_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    accounting_plan_id UUID NOT NULL,
    accounting_plan_version BIGINT NOT NULL CHECK (accounting_plan_version > 0),
    source_event_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    billing_event_id UUID NOT NULL,
    fulfillment_id UUID NOT NULL,
    obligation_id UUID NOT NULL,
    entry_date DATE NOT NULL,
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    net_minor BIGINT NOT NULL CHECK (net_minor > 0),
    tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
    fulfilled_minor BIGINT NOT NULL CHECK (fulfilled_minor > 0),
    tax_profile_id UUID NOT NULL,
    tax_profile_version BIGINT NOT NULL CHECK (tax_profile_version > 0),
    tax_classification TEXT NOT NULL CHECK (tax_classification IN ('standard_vat', 'zero_rated', 'exempt')),
    tax_rate_basis_points INTEGER NOT NULL CHECK (tax_rate_basis_points BETWEEN 0 AND 10000),
    tax_authority_ref TEXT NOT NULL,
    preparer TEXT NOT NULL,
    sales_approver TEXT NOT NULL,
    final_finance_approver TEXT,
    approval_session_id UUID,
    packet_status TEXT NOT NULL CHECK (packet_status IN (
        'draft', 'submitted', 'approved', 'rejected', 'posted', 'superseded'
    )),
    invoice_id UUID,
    receivable_id UUID,
    billing_journal_id UUID,
    fulfillment_journal_id UUID,
    revenue_schedule_id UUID,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, posting_packet_id),
    UNIQUE (holding_id, source_event_id),
    FOREIGN KEY (holding_id, accounting_plan_id) REFERENCES
        erpx_finance_write.accounting_plans (holding_id, accounting_plan_id) ON DELETE RESTRICT,
    CHECK (fulfilled_minor <= net_minor),
    CHECK (final_finance_approver IS NULL OR (final_finance_approver <> preparer AND final_finance_approver <> sales_approver)),
    CHECK (
        packet_status <> 'posted'
        OR (invoice_id IS NOT NULL AND receivable_id IS NOT NULL AND billing_journal_id IS NOT NULL
            AND fulfillment_journal_id IS NOT NULL AND revenue_schedule_id IS NOT NULL)
    )
);

CREATE TABLE erpx_finance_write.sales_fact_inbox (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    consumer_name TEXT NOT NULL,
    source_event_id UUID NOT NULL,
    source_event_type TEXT NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    global_sequence BIGINT NOT NULL CHECK (global_sequence > 0),
    handoff_id UUID NOT NULL,
    handoff_payload JSONB NOT NULL,
    inbox_status TEXT NOT NULL CHECK (inbox_status IN ('staged', 'applied', 'retryable_failure', 'permanent_failure')),
    failure_code TEXT,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, consumer_name, source_event_id),
    UNIQUE (holding_id, consumer_name, global_sequence),
    UNIQUE (holding_id, handoff_id)
);

CREATE TABLE erpx_finance_write.sales_fact_checkpoints (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    consumer_name TEXT NOT NULL,
    last_global_sequence BIGINT NOT NULL CHECK (last_global_sequence > 0),
    last_source_event_id UUID NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_id, consumer_name)
);

-- New tables fail closed when no exact holding has been selected.
DO $$
DECLARE target REGCLASS;
BEGIN
    FOREACH target IN ARRAY ARRAY[
        'erpx_agent_write.suggestion_envelopes'::regclass,
        'erpx_operations_write.approval_sessions'::regclass,
        'erpx_operations_write.approval_session_gates'::regclass,
        'erpx_sales_write.quote_workflows'::regclass,
        'erpx_sales_write.quote_workflow_lines'::regclass,
        'erpx_sales_write.quote_applied_suggestions'::regclass,
        'erpx_sales_write.quote_approval_links'::regclass,
        'erpx_sales_write.quote_approval_rejections'::regclass,
        'erpx_sales_write.quote_accounting_plan_links'::regclass,
        'erpx_sales_write.quote_pdf_artifacts'::regclass,
        'erpx_sales_write.quote_external_decisions'::regclass,
        'erpx_finance_write.accounting_plans'::regclass,
        'erpx_finance_write.accounting_plan_obligations'::regclass,
        'erpx_finance_write.posting_packets'::regclass,
        'erpx_finance_write.sales_fact_inbox'::regclass,
        'erpx_finance_write.sales_fact_checkpoints'::regclass
    ]
    LOOP
        EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', target);
        EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', target);
        EXECUTE format(
            'CREATE POLICY holding_isolation ON %s USING (holding_id = erpx_cqrs_write.current_holding_id()) WITH CHECK (holding_id = erpx_cqrs_write.current_holding_id())',
            target
        );
    END LOOP;
END
$$;


-- Hard cutover fence repeated after the Sales/Finance vertical-slice schema.
-- A migration set must always end with the generation marker so a newly
-- appended feature migration cannot accidentally become its terminal fence.
DROP TABLE IF EXISTS erpx_cqrs_write.aggregate_snapshots;
DROP TABLE IF EXISTS erpx_holding.aggregate_snapshots;

-- OPERATIONS_ORGANIZATION_LEGACY_AUTHORITY_RETIREMENT
DROP TABLE IF EXISTS erpx_holding.operations_source_references;
DROP TABLE IF EXISTS erpx_holding.operations_sla_notifications;
DROP TABLE IF EXISTS erpx_holding.operations_events;
DROP TABLE IF EXISTS erpx_holding.operations_aggregate_snapshots;

-- Final holding_write / holding-write generation fence. The authoritative
-- erpx_meta.schema_manifest must still identify schema_revision 2.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='holding_write' AND migration_set='holding-write'
    ) THEN
        RAISE EXCEPTION 'holding-write current manifest is not current';
    END IF;
END;
$$;

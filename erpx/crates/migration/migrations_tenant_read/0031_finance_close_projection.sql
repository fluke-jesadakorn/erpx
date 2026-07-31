-- Disposable Finance close-review read model for one routed holding.
--
-- The close-review aggregate and control evidence remain authoritative on the
-- tenant write database. These tables contain only bounded, typed projection
-- values so the tenant read database can be rebuilt without exposing the
-- aggregate payload or command context.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_close_review_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, period_id),
    UNIQUE (holding_ref, idempotency_key),
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

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_close_account_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, period_id, account_id),
    CHECK (net_minor = debit_minor - credit_minor)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.finance_close_projection_checkpoints (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    holding_id UUID NOT NULL,
    company_id UUID NOT NULL,
    period_id UUID NOT NULL,
    last_event_id UUID NOT NULL,
    source_version BIGINT NOT NULL CHECK (source_version > 0),
    projected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, period_id)
);

CREATE INDEX IF NOT EXISTS finance_close_review_read_scope_idx
    ON erpx_tenant.finance_close_review_read_rows
       (holding_ref, holding_id, company_id, status, projected_at DESC, period_id);

CREATE INDEX IF NOT EXISTS finance_close_account_read_scope_idx
    ON erpx_tenant.finance_close_account_read_rows
       (holding_ref, holding_id, company_id, period_id, account_id);

CREATE INDEX IF NOT EXISTS finance_close_projection_checkpoints_scope_idx
    ON erpx_tenant.finance_close_projection_checkpoints
       (holding_ref, holding_id, company_id, source_version, period_id);

ALTER TABLE erpx_tenant.finance_close_review_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_close_review_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_close_account_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_close_account_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_close_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.finance_close_projection_checkpoints FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_finance_close_review_read_rows
    ON erpx_tenant.finance_close_review_read_rows;
CREATE POLICY tenant_scope_finance_close_review_read_rows
    ON erpx_tenant.finance_close_review_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_finance_close_account_read_rows
    ON erpx_tenant.finance_close_account_read_rows;
CREATE POLICY tenant_scope_finance_close_account_read_rows
    ON erpx_tenant.finance_close_account_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_finance_close_projection_checkpoints
    ON erpx_tenant.finance_close_projection_checkpoints;
CREATE POLICY tenant_scope_finance_close_projection_checkpoints
    ON erpx_tenant.finance_close_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

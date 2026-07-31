-- Rebuildable HR and payroll read models for one routed holding.
--
-- The tenant-write HR aggregates remain authoritative. These tables contain
-- bounded typed values and source event metadata only; workers may truncate and
-- rebuild them from the tenant-write outbox without copying aggregate JSON.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.hr_employee_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, employee_id),
    UNIQUE (holding_ref, employee_ref),
    CHECK (status <> 'terminated' OR terminated_on IS NOT NULL),
    CHECK (status = 'terminated' OR terminated_on IS NULL)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.hr_leave_request_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, leave_id),
    CHECK (ends_on >= starts_on),
    CHECK (status <> 'approved' OR approved_by IS NOT NULL),
    CHECK (status NOT IN ('draft', 'submitted') OR approved_by IS NULL)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.hr_expense_claim_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, expense_id),
    CHECK (status NOT IN ('approved', 'reimbursed') OR approved_by IS NOT NULL),
    CHECK (status IN ('draft', 'submitted', 'rejected') OR amount_minor >= 0)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.hr_payroll_run_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, payroll_id),
    CHECK (ends_on >= starts_on),
    CHECK (total_gross_minor - total_deductions_minor = total_net_minor),
    CHECK (status <> 'posted' OR approved_by IS NOT NULL),
    CHECK (status NOT IN ('review', 'approved', 'posted') OR reviewed_by IS NOT NULL)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.hr_payroll_line_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, payroll_id, line_no)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.hr_expired_role_reassignment_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, workflow_ref),
    CHECK (expired_principal_ref <> replacement_principal_ref)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.hr_projection_checkpoints (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, projection_name, aggregate_type, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.hr_projection_event_receipts (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, projection_name, event_id)
);

CREATE INDEX IF NOT EXISTS hr_employee_read_scope_idx
    ON erpx_tenant.hr_employee_read_rows
       (holding_ref, holding_id, company_id, branch_id, status, projected_at DESC);
CREATE INDEX IF NOT EXISTS hr_leave_request_read_scope_idx
    ON erpx_tenant.hr_leave_request_read_rows
       (holding_ref, holding_id, company_id, branch_id, status, starts_on);
CREATE INDEX IF NOT EXISTS hr_expense_claim_read_scope_idx
    ON erpx_tenant.hr_expense_claim_read_rows
       (holding_ref, holding_id, company_id, branch_id, status, projected_at DESC);
CREATE INDEX IF NOT EXISTS hr_payroll_run_read_scope_idx
    ON erpx_tenant.hr_payroll_run_read_rows
       (holding_ref, holding_id, company_id, branch_id, status, starts_on);
CREATE INDEX IF NOT EXISTS hr_payroll_line_read_employee_idx
    ON erpx_tenant.hr_payroll_line_read_rows
       (holding_ref, holding_id, company_id, branch_id, employee_id, payroll_id, line_no);
CREATE INDEX IF NOT EXISTS hr_expired_role_reassignment_read_scope_idx
    ON erpx_tenant.hr_expired_role_reassignment_read_rows
       (holding_ref, holding_id, company_id, branch_id, reassigned_at DESC);
CREATE INDEX IF NOT EXISTS hr_projection_checkpoints_scope_idx
    ON erpx_tenant.hr_projection_checkpoints
       (holding_ref, projection_name, aggregate_type, source_version, projected_at DESC);
CREATE INDEX IF NOT EXISTS hr_projection_event_receipts_aggregate_idx
    ON erpx_tenant.hr_projection_event_receipts
       (holding_ref, projection_name, aggregate_type, aggregate_id, aggregate_version);

ALTER TABLE erpx_tenant.hr_employee_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_employee_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_leave_request_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_leave_request_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_expense_claim_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_expense_claim_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_payroll_run_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_payroll_run_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_payroll_line_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_payroll_line_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_expired_role_reassignment_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_expired_role_reassignment_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_projection_event_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_projection_event_receipts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_hr_employee_read_rows
    ON erpx_tenant.hr_employee_read_rows;
CREATE POLICY tenant_scope_hr_employee_read_rows
    ON erpx_tenant.hr_employee_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_hr_leave_request_read_rows
    ON erpx_tenant.hr_leave_request_read_rows;
CREATE POLICY tenant_scope_hr_leave_request_read_rows
    ON erpx_tenant.hr_leave_request_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_hr_expense_claim_read_rows
    ON erpx_tenant.hr_expense_claim_read_rows;
CREATE POLICY tenant_scope_hr_expense_claim_read_rows
    ON erpx_tenant.hr_expense_claim_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_hr_payroll_run_read_rows
    ON erpx_tenant.hr_payroll_run_read_rows;
CREATE POLICY tenant_scope_hr_payroll_run_read_rows
    ON erpx_tenant.hr_payroll_run_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_hr_payroll_line_read_rows
    ON erpx_tenant.hr_payroll_line_read_rows;
CREATE POLICY tenant_scope_hr_payroll_line_read_rows
    ON erpx_tenant.hr_payroll_line_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_hr_expired_role_reassignment_read_rows
    ON erpx_tenant.hr_expired_role_reassignment_read_rows;
CREATE POLICY tenant_scope_hr_expired_role_reassignment_read_rows
    ON erpx_tenant.hr_expired_role_reassignment_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_hr_projection_checkpoints
    ON erpx_tenant.hr_projection_checkpoints;
CREATE POLICY tenant_scope_hr_projection_checkpoints
    ON erpx_tenant.hr_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

DROP POLICY IF EXISTS tenant_scope_hr_projection_event_receipts
    ON erpx_tenant.hr_projection_event_receipts;
CREATE POLICY tenant_scope_hr_projection_event_receipts
    ON erpx_tenant.hr_projection_event_receipts
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

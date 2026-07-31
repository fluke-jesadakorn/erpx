-- Typed tenant-write source state for HR, leave, expense, payroll, and
-- expired-role workflow reassignment.  Read projections are intentionally not
-- created here; these tables belong only to the holding write database.
CREATE TABLE IF NOT EXISTS erpx_tenant.hr_command_idempotency (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> ''),
    request_hash TEXT NOT NULL CHECK (btrim(request_hash) <> ''),
    aggregate_type TEXT NOT NULL CHECK (btrim(aggregate_type) <> ''),
    aggregate_id UUID NOT NULL,
    version BIGINT NOT NULL CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.hr_employee_records (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, employee_id),
    UNIQUE (holding_ref, employee_ref)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.hr_leave_requests (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, leave_id),
    CHECK (ends_on >= starts_on),
    UNIQUE (holding_ref, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.hr_expense_claims (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, expense_id),
    UNIQUE (holding_ref, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.hr_payroll_runs (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, payroll_id),
    CHECK (ends_on >= starts_on),
    CHECK (total_gross_minor - total_deductions_minor = total_net_minor),
    UNIQUE (holding_ref, idempotency_key)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.hr_payroll_lines (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    payroll_id UUID NOT NULL,
    line_no INTEGER NOT NULL CHECK (line_no >= 0),
    employee_id UUID NOT NULL,
    gross_minor BIGINT NOT NULL CHECK (gross_minor >= 0),
    deductions_minor BIGINT NOT NULL CHECK (deductions_minor >= 0 AND deductions_minor <= gross_minor),
    net_minor BIGINT NOT NULL CHECK (net_minor = gross_minor - deductions_minor),
    PRIMARY KEY (holding_ref, payroll_id, line_no)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.hr_expired_role_reassignments (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, workflow_ref),
    UNIQUE (holding_ref, idempotency_key),
    CHECK (expired_principal_ref <> replacement_principal_ref)
);

CREATE INDEX IF NOT EXISTS hr_employee_scope_idx
    ON erpx_tenant.hr_employee_records (holding_ref, holding_id, company_id, branch_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS hr_leave_scope_idx
    ON erpx_tenant.hr_leave_requests (holding_ref, holding_id, company_id, branch_id, status, starts_on);
CREATE INDEX IF NOT EXISTS hr_expense_scope_idx
    ON erpx_tenant.hr_expense_claims (holding_ref, holding_id, company_id, branch_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS hr_payroll_scope_idx
    ON erpx_tenant.hr_payroll_runs (holding_ref, holding_id, company_id, branch_id, status, starts_on);

ALTER TABLE erpx_tenant.hr_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_command_idempotency FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_employee_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_employee_records FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_leave_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_leave_requests FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_expense_claims ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_expense_claims FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_payroll_runs ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_payroll_runs FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_payroll_lines ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_payroll_lines FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_expired_role_reassignments ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.hr_expired_role_reassignments FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_hr_command_idempotency ON erpx_tenant.hr_command_idempotency;
CREATE POLICY tenant_scope_hr_command_idempotency ON erpx_tenant.hr_command_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_hr_employee_records ON erpx_tenant.hr_employee_records;
CREATE POLICY tenant_scope_hr_employee_records ON erpx_tenant.hr_employee_records
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_hr_leave_requests ON erpx_tenant.hr_leave_requests;
CREATE POLICY tenant_scope_hr_leave_requests ON erpx_tenant.hr_leave_requests
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_hr_expense_claims ON erpx_tenant.hr_expense_claims;
CREATE POLICY tenant_scope_hr_expense_claims ON erpx_tenant.hr_expense_claims
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_hr_payroll_runs ON erpx_tenant.hr_payroll_runs;
CREATE POLICY tenant_scope_hr_payroll_runs ON erpx_tenant.hr_payroll_runs
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_hr_payroll_lines ON erpx_tenant.hr_payroll_lines;
CREATE POLICY tenant_scope_hr_payroll_lines ON erpx_tenant.hr_payroll_lines
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_hr_expired_role_reassignments ON erpx_tenant.hr_expired_role_reassignments;
CREATE POLICY tenant_scope_hr_expired_role_reassignments ON erpx_tenant.hr_expired_role_reassignments
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

-- Terminal lifecycle states cannot be bypassed by writing a deserialized
-- aggregate directly.  Non-terminal transitions remain domain-owned.
CREATE OR REPLACE FUNCTION erpx_tenant.reject_terminal_hr_mutation()
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
    ON erpx_tenant.hr_employee_records;
CREATE TRIGGER hr_employee_terminal_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.hr_employee_records
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_terminal_hr_mutation();
DROP TRIGGER IF EXISTS hr_leave_terminal_immutable
    ON erpx_tenant.hr_leave_requests;
CREATE TRIGGER hr_leave_terminal_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.hr_leave_requests
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_terminal_hr_mutation();
DROP TRIGGER IF EXISTS hr_expense_terminal_immutable
    ON erpx_tenant.hr_expense_claims;
CREATE TRIGGER hr_expense_terminal_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.hr_expense_claims
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_terminal_hr_mutation();
DROP TRIGGER IF EXISTS hr_payroll_terminal_immutable
    ON erpx_tenant.hr_payroll_runs;
CREATE TRIGGER hr_payroll_terminal_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.hr_payroll_runs
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_terminal_hr_mutation();

CREATE OR REPLACE FUNCTION erpx_tenant.reject_hr_reassignment_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'expired-role reassignment metadata is immutable';
END;
$$;

DROP TRIGGER IF EXISTS hr_expired_role_reassignment_immutable
    ON erpx_tenant.hr_expired_role_reassignments;
CREATE TRIGGER hr_expired_role_reassignment_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.hr_expired_role_reassignments
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_hr_reassignment_mutation();

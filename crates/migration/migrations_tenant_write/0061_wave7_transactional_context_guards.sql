-- Wave 7 transactional-context guards. Aggregate snapshots, idempotency
-- receipts, audit records, and typed outbox events remain atomic in the shared
-- erpx_cqrs_write unit of work; these tables enforce context invariants and
-- searchable uniqueness on the command side.

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

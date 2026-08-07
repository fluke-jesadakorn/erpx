-- Disposable Wave 7 transactional projections. Every row is fenced by a
-- projection generation so shadow rebuilds can be activated atomically while
-- command-side writes and outbox publication continue.

CREATE SCHEMA IF NOT EXISTS erpx_sales_read;
CREATE TABLE erpx_sales_read.quote_projection (
    projection_name TEXT NOT NULL DEFAULT 'sales.quotes'
        CHECK (projection_name = 'sales.quotes'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    quote_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
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
    PRIMARY KEY (projection_name, generation, partition_key, quote_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, quote_code)
);
CREATE TABLE erpx_sales_read.sales_order_projection (
    projection_name TEXT NOT NULL DEFAULT 'sales.orders'
        CHECK (projection_name = 'sales.orders'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    sales_order_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
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
    PRIMARY KEY (projection_name, generation, partition_key, sales_order_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, quote_id),
    UNIQUE (generation, scope_key, order_code)
);
CREATE INDEX sales_read_quote_scope_customer_status
    ON erpx_sales_read.quote_projection
        (generation, scope_key, customer_id, quote_status,
         aggregate_updated_at DESC, quote_id DESC);
CREATE INDEX sales_read_order_scope_customer_status
    ON erpx_sales_read.sales_order_projection
        (generation, scope_key, customer_id, order_status,
         aggregate_updated_at DESC, sales_order_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_people_payroll_read;
CREATE TABLE erpx_people_payroll_read.employee_projection (
    projection_name TEXT NOT NULL DEFAULT 'people_payroll.employees'
        CHECK (projection_name = 'people_payroll.employees'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    employee_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
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
    PRIMARY KEY (projection_name, generation, partition_key, employee_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, employee_code)
);
CREATE TABLE erpx_people_payroll_read.leave_request_projection (
    projection_name TEXT NOT NULL DEFAULT 'people_payroll.leave_requests'
        CHECK (projection_name = 'people_payroll.leave_requests'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    leave_request_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
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
    PRIMARY KEY (projection_name, generation, partition_key, leave_request_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);
CREATE TABLE erpx_people_payroll_read.expense_claim_projection (
    projection_name TEXT NOT NULL DEFAULT 'people_payroll.expense_claims'
        CHECK (projection_name = 'people_payroll.expense_claims'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    expense_claim_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
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
    PRIMARY KEY (projection_name, generation, partition_key, expense_claim_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);
CREATE TABLE erpx_people_payroll_read.payroll_run_projection (
    projection_name TEXT NOT NULL DEFAULT 'people_payroll.payroll_runs'
        CHECK (projection_name = 'people_payroll.payroll_runs'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    payroll_run_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
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
    PRIMARY KEY (projection_name, generation, partition_key, payroll_run_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, period_code)
);
CREATE INDEX people_payroll_read_employee_scope_status
    ON erpx_people_payroll_read.employee_projection
        (generation, scope_key, employee_status,
         aggregate_updated_at DESC, employee_id DESC);
CREATE INDEX people_payroll_read_leave_scope_employee_status
    ON erpx_people_payroll_read.leave_request_projection
        (generation, scope_key, employee_id, leave_status,
         aggregate_updated_at DESC, leave_request_id DESC);
CREATE INDEX people_payroll_read_expense_scope_employee_status
    ON erpx_people_payroll_read.expense_claim_projection
        (generation, scope_key, employee_id, expense_status,
         aggregate_updated_at DESC, expense_claim_id DESC);
CREATE INDEX people_payroll_read_run_scope_status_period
    ON erpx_people_payroll_read.payroll_run_projection
        (generation, scope_key, payroll_status, ends_on DESC, payroll_run_id DESC);

CREATE SCHEMA IF NOT EXISTS erpx_pos_read;
CREATE TABLE erpx_pos_read.sale_projection (
    projection_name TEXT NOT NULL DEFAULT 'pos.sales'
        CHECK (projection_name = 'pos.sales'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    sale_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
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
    PRIMARY KEY (projection_name, generation, partition_key, sale_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE,
    UNIQUE (generation, scope_key, receipt_code)
);
CREATE TABLE erpx_pos_read.refund_projection (
    projection_name TEXT NOT NULL DEFAULT 'pos.refunds'
        CHECK (projection_name = 'pos.refunds'),
    generation UUID NOT NULL,
    partition_key TEXT NOT NULL,
    refund_id UUID NOT NULL,
    source_aggregate_version BIGINT NOT NULL CHECK (source_aggregate_version > 0),
    source_global_sequence BIGINT NOT NULL CHECK (source_global_sequence > 0),
    tenant_scope JSONB NOT NULL,
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
    PRIMARY KEY (projection_name, generation, partition_key, refund_id),
    FOREIGN KEY (projection_name, generation)
        REFERENCES erpx_cqrs_read.projection_generations (projection_name, generation)
        ON DELETE CASCADE
);
CREATE INDEX pos_read_sale_scope_register_status
    ON erpx_pos_read.sale_projection
        (generation, scope_key, register_id, sale_status,
         aggregate_updated_at DESC, sale_id DESC);
CREATE INDEX pos_read_refund_scope_sale_status
    ON erpx_pos_read.refund_projection
        (generation, scope_key, sale_id, refund_status,
         aggregate_updated_at DESC, refund_id DESC);

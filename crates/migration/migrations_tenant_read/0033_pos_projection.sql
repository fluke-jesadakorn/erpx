-- Rebuildable typed POS read model for one physically isolated holding.
--
-- The tenant-write POS tables and outbox remain authoritative.  This read
-- model contains only bounded, normalized facts and source-event metadata;
-- aggregate JSON is intentionally not persisted here.
CREATE SCHEMA IF NOT EXISTS erpx_tenant;

CREATE TABLE IF NOT EXISTS erpx_tenant.pos_sale_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, sale_id),
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

CREATE TABLE IF NOT EXISTS erpx_tenant.pos_sale_line_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, sale_id, line_id),
    UNIQUE (holding_ref, sale_id, line_no),
    CHECK (product_scope_kind IS NULL OR product_scope_kind IN ('holding', 'company', 'branch')),
    CHECK ((product_id IS NULL AND product_scope_kind IS NULL
            AND product_scope_holding_id IS NULL AND product_scope_company_id IS NULL
            AND product_scope_branch_id IS NULL)
        OR (product_id IS NOT NULL AND product_scope_kind IS NOT NULL
            AND product_scope_holding_id IS NOT NULL))
);

CREATE TABLE IF NOT EXISTS erpx_tenant.pos_sale_payment_read_rows (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, sale_id, payment_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.pos_projection_checkpoints (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, projection_name, aggregate_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.pos_projection_event_receipts (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, projection_name, event_id)
);

CREATE INDEX IF NOT EXISTS pos_sale_read_scope_idx
    ON erpx_tenant.pos_sale_read_rows
        (holding_ref, holding_id, company_id, branch_id, updated_at DESC, sale_id DESC);
CREATE INDEX IF NOT EXISTS pos_sale_line_read_sale_idx
    ON erpx_tenant.pos_sale_line_read_rows (holding_ref, sale_id, line_no);
CREATE INDEX IF NOT EXISTS pos_sale_payment_read_sale_idx
    ON erpx_tenant.pos_sale_payment_read_rows
        (holding_ref, sale_id, received_at, payment_id);
CREATE INDEX IF NOT EXISTS pos_projection_checkpoint_source_idx
    ON erpx_tenant.pos_projection_checkpoints
        (holding_ref, aggregate_id, source_version);
CREATE INDEX IF NOT EXISTS pos_projection_receipt_source_idx
    ON erpx_tenant.pos_projection_event_receipts
        (holding_ref, aggregate_id, aggregate_version);

ALTER TABLE erpx_tenant.pos_sale_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_sale_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_sale_line_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_sale_line_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_sale_payment_read_rows ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_sale_payment_read_rows FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_projection_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_projection_checkpoints FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_projection_event_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_projection_event_receipts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_pos_sale_read_rows ON erpx_tenant.pos_sale_read_rows;
CREATE POLICY tenant_scope_pos_sale_read_rows ON erpx_tenant.pos_sale_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_pos_sale_line_read_rows ON erpx_tenant.pos_sale_line_read_rows;
CREATE POLICY tenant_scope_pos_sale_line_read_rows ON erpx_tenant.pos_sale_line_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_pos_sale_payment_read_rows ON erpx_tenant.pos_sale_payment_read_rows;
CREATE POLICY tenant_scope_pos_sale_payment_read_rows ON erpx_tenant.pos_sale_payment_read_rows
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_pos_projection_checkpoints ON erpx_tenant.pos_projection_checkpoints;
CREATE POLICY tenant_scope_pos_projection_checkpoints ON erpx_tenant.pos_projection_checkpoints
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_pos_projection_event_receipts ON erpx_tenant.pos_projection_event_receipts;
CREATE POLICY tenant_scope_pos_projection_event_receipts ON erpx_tenant.pos_projection_event_receipts
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

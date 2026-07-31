-- Typed POS source state for the holding write database.
--
-- The normalized tables are authoritative.  The existing outbox_events table
-- remains the transport envelope used by projectors; no POS aggregate JSON is
-- stored in these source tables.
CREATE TABLE IF NOT EXISTS erpx_tenant.pos_sales (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, sale_id),
    UNIQUE (holding_ref, reference),
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

CREATE TABLE IF NOT EXISTS erpx_tenant.pos_sale_lines (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
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
    PRIMARY KEY (holding_ref, sale_id, line_id),
    UNIQUE (holding_ref, sale_id, line_no),
    CHECK ((product_id IS NULL AND product_scope_kind IS NULL
            AND product_scope_holding_id IS NULL AND product_scope_company_id IS NULL
            AND product_scope_branch_id IS NULL)
        OR (product_id IS NOT NULL AND product_scope_kind IN ('holding', 'company', 'branch')
            AND product_scope_holding_id IS NOT NULL)),
    FOREIGN KEY (holding_ref, sale_id)
        REFERENCES erpx_tenant.pos_sales (holding_ref, sale_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.pos_sale_payments (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    sale_id UUID NOT NULL,
    payment_id UUID NOT NULL,
    amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
    method TEXT NOT NULL CHECK (method IN ('cash', 'card', 'bank_transfer', 'mobile_wallet', 'voucher', 'other')),
    received_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (holding_ref, sale_id, payment_id),
    FOREIGN KEY (holding_ref, sale_id)
        REFERENCES erpx_tenant.pos_sales (holding_ref, sale_id)
);

CREATE TABLE IF NOT EXISTS erpx_tenant.pos_command_idempotency (
    holding_ref TEXT NOT NULL CHECK (btrim(holding_ref) <> ''),
    idempotency_key TEXT NOT NULL CHECK (btrim(idempotency_key) <> '' AND char_length(idempotency_key) <= 512),
    request_hash TEXT NOT NULL CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    command_id UUID NOT NULL,
    aggregate_type TEXT NOT NULL CHECK (aggregate_type = 'pos_sale'),
    aggregate_id UUID NOT NULL,
    aggregate_version BIGINT NOT NULL CHECK (aggregate_version > 0),
    event_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_ref, idempotency_key)
);

CREATE INDEX IF NOT EXISTS pos_sales_scope_idx
    ON erpx_tenant.pos_sales (holding_ref, holding_id, company_id, branch_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS pos_sale_lines_sale_idx
    ON erpx_tenant.pos_sale_lines (holding_ref, sale_id, line_no);
CREATE INDEX IF NOT EXISTS pos_sale_payments_sale_idx
    ON erpx_tenant.pos_sale_payments (holding_ref, sale_id, received_at, payment_id);
CREATE INDEX IF NOT EXISTS pos_idempotency_sale_idx
    ON erpx_tenant.pos_command_idempotency (holding_ref, aggregate_id, aggregate_version);

ALTER TABLE erpx_tenant.pos_sales ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_sales FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_sale_lines ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_sale_lines FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_sale_payments ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_sale_payments FORCE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_command_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE erpx_tenant.pos_command_idempotency FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS tenant_scope_pos_sales ON erpx_tenant.pos_sales;
CREATE POLICY tenant_scope_pos_sales ON erpx_tenant.pos_sales
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_pos_sale_lines ON erpx_tenant.pos_sale_lines;
CREATE POLICY tenant_scope_pos_sale_lines ON erpx_tenant.pos_sale_lines
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_pos_sale_payments ON erpx_tenant.pos_sale_payments;
CREATE POLICY tenant_scope_pos_sale_payments ON erpx_tenant.pos_sale_payments
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));
DROP POLICY IF EXISTS tenant_scope_pos_command_idempotency ON erpx_tenant.pos_command_idempotency;
CREATE POLICY tenant_scope_pos_command_idempotency ON erpx_tenant.pos_command_idempotency
    USING (holding_ref = current_setting('erpx.holding_ref', true))
    WITH CHECK (holding_ref = current_setting('erpx.holding_ref', true));

CREATE OR REPLACE FUNCTION erpx_tenant.reject_terminal_pos_mutation()
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

CREATE OR REPLACE FUNCTION erpx_tenant.reject_terminal_pos_child_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    sale_status TEXT;
    row_holding_ref TEXT;
    row_sale_id UUID;
BEGIN
    IF TG_OP = 'DELETE' THEN
        row_holding_ref := OLD.holding_ref;
        row_sale_id := OLD.sale_id;
    ELSE
        row_holding_ref := NEW.holding_ref;
        row_sale_id := NEW.sale_id;
    END IF;
    SELECT status INTO sale_status
      FROM erpx_tenant.pos_sales
     WHERE holding_ref = row_holding_ref AND sale_id = row_sale_id;
    IF sale_status IN ('voided', 'refunded') THEN
        RAISE EXCEPTION 'terminal POS sale children are immutable';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION erpx_tenant.validate_pos_sale_reconciliation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    sale_row RECORD;
    line_subtotal BIGINT;
    line_tax BIGINT;
    line_total BIGINT;
    payment_total BIGINT;
    row_holding_ref TEXT;
    row_sale_id UUID;
BEGIN
    IF TG_OP = 'DELETE' THEN
        row_holding_ref := OLD.holding_ref;
        row_sale_id := OLD.sale_id;
    ELSE
        row_holding_ref := NEW.holding_ref;
        row_sale_id := NEW.sale_id;
    END IF;
    SELECT * INTO sale_row
      FROM erpx_tenant.pos_sales
     WHERE holding_ref = row_holding_ref AND sale_id = row_sale_id;
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
      FROM erpx_tenant.pos_sale_lines
     WHERE holding_ref = sale_row.holding_ref AND sale_id = sale_row.sale_id;
    SELECT COALESCE(sum(amount_minor), 0)
      INTO payment_total
      FROM erpx_tenant.pos_sale_payments
     WHERE holding_ref = sale_row.holding_ref AND sale_id = sale_row.sale_id;

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

DROP TRIGGER IF EXISTS pos_sales_terminal_immutable ON erpx_tenant.pos_sales;
CREATE TRIGGER pos_sales_terminal_immutable
    BEFORE UPDATE OR DELETE ON erpx_tenant.pos_sales
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_terminal_pos_mutation();

DROP TRIGGER IF EXISTS pos_sale_lines_terminal_immutable ON erpx_tenant.pos_sale_lines;
CREATE TRIGGER pos_sale_lines_terminal_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON erpx_tenant.pos_sale_lines
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_terminal_pos_child_mutation();
DROP TRIGGER IF EXISTS pos_sale_payments_terminal_immutable ON erpx_tenant.pos_sale_payments;
CREATE TRIGGER pos_sale_payments_terminal_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON erpx_tenant.pos_sale_payments
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.reject_terminal_pos_child_mutation();

DROP TRIGGER IF EXISTS pos_sales_reconciliation ON erpx_tenant.pos_sales;
CREATE CONSTRAINT TRIGGER pos_sales_reconciliation
    AFTER INSERT OR UPDATE ON erpx_tenant.pos_sales
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.validate_pos_sale_reconciliation();
DROP TRIGGER IF EXISTS pos_sale_lines_reconciliation ON erpx_tenant.pos_sale_lines;
CREATE CONSTRAINT TRIGGER pos_sale_lines_reconciliation
    AFTER INSERT OR UPDATE OR DELETE ON erpx_tenant.pos_sale_lines
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.validate_pos_sale_reconciliation();
DROP TRIGGER IF EXISTS pos_sale_payments_reconciliation ON erpx_tenant.pos_sale_payments;
CREATE CONSTRAINT TRIGGER pos_sale_payments_reconciliation
    AFTER INSERT OR UPDATE OR DELETE ON erpx_tenant.pos_sale_payments
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION erpx_tenant.validate_pos_sale_reconciliation();

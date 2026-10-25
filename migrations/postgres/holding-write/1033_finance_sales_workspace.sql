ALTER TABLE commerce_products ADD COLUMN item_type TEXT NOT NULL DEFAULT 'stock'
  CHECK (item_type IN ('stock','service'));
ALTER TABLE commerce_products ADD CONSTRAINT service_tracking_none
  CHECK (item_type='stock' OR tracking_mode='none');
ALTER TABLE commerce_order_lines ADD COLUMN item_type TEXT NOT NULL DEFAULT 'stock'
  CHECK (item_type IN ('stock','service'));
ALTER TABLE commerce_orders ALTER COLUMN warehouse_id DROP NOT NULL;
ALTER TABLE commerce_orders ALTER COLUMN branch_id DROP NOT NULL;
ALTER TABLE commerce_fulfillments ADD COLUMN completion_kind TEXT NOT NULL DEFAULT 'goods'
  CHECK (completion_kind IN ('goods','service','mixed'));
ALTER TABLE commerce_fulfillments ADD COLUMN completion_note TEXT NOT NULL DEFAULT '';
ALTER TABLE commerce_fulfillments DROP CONSTRAINT IF EXISTS commerce_fulfillments_invoice_id_key;

CREATE FUNCTION erpx_lock_item_type() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.item_type IS DISTINCT FROM OLD.item_type AND (
    EXISTS(SELECT 1 FROM commerce_order_lines WHERE product_id=OLD.id) OR
    EXISTS(SELECT 1 FROM commerce_stock WHERE product_id=OLD.id AND (quantity_milli<>0 OR value_minor<>0)) OR
    EXISTS(SELECT 1 FROM inventory_ledger WHERE product_id=OLD.id)
  ) THEN RAISE EXCEPTION 'ITEM_TYPE_LOCKED: Referenced items retain their stock or service meaning'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER commerce_item_type_lock BEFORE UPDATE OF item_type ON commerce_products
  FOR EACH ROW EXECUTE FUNCTION erpx_lock_item_type();

CREATE TABLE finance_sales_number_sequences (
  company_id TEXT NOT NULL, prefix TEXT NOT NULL CHECK(prefix IN ('QT','INV','RC')),
  year INTEGER NOT NULL, last_number BIGINT NOT NULL CHECK(last_number>0),
  PRIMARY KEY(company_id,prefix,year)
);
-- Reserve any legacy numbers already using the new spelling without renumbering documents.
INSERT INTO finance_sales_number_sequences(company_id,prefix,year,last_number)
SELECT company_id,'QT',substring(number from 4 for 4)::integer,MAX(substring(number from 9)::bigint)
FROM commerce_orders WHERE number ~ '^QT-[0-9]{4}-[0-9]{6,}$' GROUP BY company_id,substring(number from 4 for 4)
HAVING MAX(substring(number from 9)::bigint)>0;
INSERT INTO finance_sales_number_sequences(company_id,prefix,year,last_number)
SELECT company_id,'INV',substring(invoice_number from 5 for 4)::integer,MAX(substring(invoice_number from 10)::bigint)
FROM finance_invoices WHERE invoice_number ~ '^INV-[0-9]{4}-[0-9]{6,}$' GROUP BY company_id,substring(invoice_number from 5 for 4)
HAVING MAX(substring(invoice_number from 10)::bigint)>0;
CREATE FUNCTION erpx_sales_document_number(p_company TEXT,p_prefix TEXT,p_date TEXT)
RETURNS TEXT LANGUAGE plpgsql AS $$
DECLARE n BIGINT; y INTEGER := EXTRACT(YEAR FROM p_date::date);
BEGIN
  INSERT INTO finance_sales_number_sequences(company_id,prefix,year,last_number)
  VALUES(p_company,p_prefix,y,1)
  ON CONFLICT(company_id,prefix,year) DO UPDATE
    SET last_number=finance_sales_number_sequences.last_number+1 RETURNING last_number INTO n;
  RETURN p_prefix||'-'||y||'-'||lpad(n::text,GREATEST(6,length(n::text)), '0');
END $$;

CREATE TABLE finance_sales_defaults (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, chart_id TEXT NOT NULL REFERENCES finance_charts(id),
  currency TEXT NOT NULL, receivable_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  revenue_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  tax_account_id TEXT REFERENCES finance_accounts(id),
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','superseded','rejected')),
  reason TEXT NOT NULL, created_by TEXT NOT NULL, reviewed_by TEXT,
  review_reason TEXT, version BIGINT NOT NULL DEFAULT 1, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE UNIQUE INDEX finance_sales_defaults_current ON finance_sales_defaults(company_id,currency)
  WHERE status='approved';

CREATE TABLE finance_customer_receipts (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, payment_id TEXT NOT NULL UNIQUE REFERENCES finance_payments(id),
  document_number TEXT NOT NULL, receipt_date TEXT NOT NULL, currency TEXT NOT NULL,
  amount_minor BIGINT NOT NULL CHECK(amount_minor>0), snapshot_json TEXT NOT NULL,
  export_html TEXT NOT NULL, export_sha256 TEXT NOT NULL,
  issued_by TEXT NOT NULL, issued_at BIGINT NOT NULL,
  UNIQUE(company_id,document_number)
);
CREATE INDEX finance_customer_receipt_reader ON finance_customer_receipts(company_id,receipt_date DESC,id);

CREATE FUNCTION erpx_customer_receipt_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'CUSTOMER_RECEIPT_IMMUTABLE: Issued receipt copies are retained unchanged'; END $$;
CREATE TRIGGER finance_customer_receipts_immutable BEFORE UPDATE OR DELETE ON finance_customer_receipts
  FOR EACH ROW EXECUTE FUNCTION erpx_customer_receipt_immutable();

CREATE FUNCTION erpx_stock_item_required() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF EXISTS(SELECT 1 FROM commerce_products WHERE id=NEW.product_id AND item_type='service')
  THEN RAISE EXCEPTION 'STOCK_ITEM_REQUIRED: Services cannot hold or move inventory'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER commerce_stock_item_required BEFORE INSERT OR UPDATE ON commerce_stock
  FOR EACH ROW EXECUTE FUNCTION erpx_stock_item_required();
CREATE TRIGGER commerce_stock_move_item_required BEFORE INSERT OR UPDATE ON commerce_stock_moves
  FOR EACH ROW EXECUTE FUNCTION erpx_stock_item_required();

CREATE FUNCTION erpx_order_item_type_required() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE current_type TEXT;
BEGIN
  SELECT item_type INTO current_type FROM commerce_products WHERE id=NEW.product_id FOR SHARE;
  IF current_type IS DISTINCT FROM NEW.item_type
  THEN RAISE EXCEPTION 'ITEM_TYPE_CHANGED: Refresh the catalogue item before preparing the document'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER commerce_order_line_item_type BEFORE INSERT OR UPDATE OF product_id,item_type ON commerce_order_lines
  FOR EACH ROW EXECUTE FUNCTION erpx_order_item_type_required();

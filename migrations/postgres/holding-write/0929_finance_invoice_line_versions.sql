ALTER TABLE finance_invoices
  ADD COLUMN line_mode TEXT NOT NULL DEFAULT 'historical_header' CHECK(line_mode IN ('historical_header','whole_document','native_lines')),
  ADD COLUMN line_revision BIGINT NOT NULL DEFAULT 1 CHECK(line_revision>0);

CREATE TABLE finance_invoice_line_versions (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  invoice_id TEXT NOT NULL REFERENCES finance_invoices(id),
  source_version BIGINT NOT NULL CHECK(source_version>0),
  ordinal INTEGER NOT NULL CHECK(ordinal>=0),
  description TEXT NOT NULL,
  quantity_millis BIGINT CHECK(quantity_millis>0),
  unit_price_minor BIGINT CHECK(unit_price_minor>0),
  tax_bps INTEGER CHECK(tax_bps BETWEEN 0 AND 10000),
  subtotal_minor BIGINT NOT NULL CHECK(subtotal_minor>=0),
  tax_minor BIGINT NOT NULL CHECK(tax_minor>=0),
  total_minor BIGINT NOT NULL CHECK(total_minor=subtotal_minor+tax_minor),
  currency TEXT NOT NULL CHECK(length(currency)=3),
  account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  tax_account_id TEXT REFERENCES finance_accounts(id),
  source_snapshot_json TEXT NOT NULL CHECK(jsonb_typeof(source_snapshot_json::jsonb)='object'),
  origin_kind TEXT NOT NULL CHECK(origin_kind IN ('historical_header','whole_document','native_line')),
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE(invoice_id,source_version,ordinal),
  CHECK(origin_kind!='native_line' OR (quantity_millis IS NOT NULL AND unit_price_minor IS NOT NULL AND tax_bps IS NOT NULL))
);
CREATE INDEX finance_invoice_line_owner ON finance_invoice_line_versions(company_id,invoice_id,source_version);

-- Retain a historical document's whole-header meaning. Product quantities and
-- contemporaneous account/party versions are deliberately not invented.
INSERT INTO finance_invoice_line_versions(id,company_id,invoice_id,source_version,ordinal,description,subtotal_minor,tax_minor,total_minor,currency,account_id,tax_account_id,source_snapshot_json,origin_kind,created_by,created_at)
SELECT i.id||':historical:1',i.company_id,i.id,1,0,'Historical whole document '||i.invoice_number,i.subtotal_minor,i.tax_minor,i.total_minor,i.currency,
  CASE WHEN i.kind='payable' THEN i.debit_account_id ELSE i.credit_account_id END,i.tax_account_id,
  jsonb_build_object('schema','finance.invoice.historical-header.v1','invoiceId',i.id,'invoiceNumber',i.invoice_number,'invoiceDate',i.invoice_date,'sourceType',i.source_type,'sourceId',i.source_id,'historicalInvoiceVersion',i.version,'subtotalMinor',i.subtotal_minor,'taxMinor',i.tax_minor,'totalMinor',i.total_minor,'currency',i.currency,'accountVersionsUnknown',true)::text,
  'historical_header',i.created_by,i.created_at
FROM finance_invoices i;

CREATE FUNCTION erpx_finance_invoice_line_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'FINANCE_INVOICE_LINE_IMMUTABLE' USING ERRCODE='23514';
END $$;
CREATE TRIGGER finance_invoice_line_immutable BEFORE UPDATE OR DELETE ON finance_invoice_line_versions FOR EACH ROW EXECUTE FUNCTION erpx_finance_invoice_line_immutable();

CREATE FUNCTION erpx_finance_invoice_line_scope() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE invoice finance_invoices%ROWTYPE;
BEGIN
  SELECT * INTO invoice FROM finance_invoices WHERE id=NEW.invoice_id FOR SHARE;
  IF invoice.id IS NULL OR invoice.company_id!=NEW.company_id OR invoice.currency!=NEW.currency
    OR NOT EXISTS(SELECT 1 FROM finance_accounts WHERE id=NEW.account_id AND company_id=NEW.company_id)
    OR (NEW.tax_account_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM finance_accounts WHERE id=NEW.tax_account_id AND company_id=NEW.company_id))
    OR invoice.status NOT IN ('draft','in_review') OR NEW.source_version!=invoice.line_revision
  THEN RAISE EXCEPTION 'FINANCE_INVOICE_LINE_SCOPE' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER finance_invoice_line_scope BEFORE INSERT ON finance_invoice_line_versions FOR EACH ROW EXECUTE FUNCTION erpx_finance_invoice_line_scope();

CREATE FUNCTION erpx_finance_native_invoice_posting() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.source_type='standalone' AND NEW.status='posted' AND OLD.status IN ('draft','in_review') THEN
    IF NOT EXISTS(SELECT 1 FROM finance_invoice_line_versions WHERE invoice_id=NEW.id AND source_version=NEW.line_revision)
      OR NEW.subtotal_minor!=(SELECT SUM(subtotal_minor) FROM finance_invoice_line_versions WHERE invoice_id=NEW.id AND source_version=NEW.line_revision)
      OR NEW.tax_minor!=(SELECT SUM(tax_minor) FROM finance_invoice_line_versions WHERE invoice_id=NEW.id AND source_version=NEW.line_revision)
    THEN RAISE EXCEPTION 'FINANCE_INVOICE_LINES_CHANGED' USING ERRCODE='23514'; END IF;
  END IF;
  IF OLD.status NOT IN ('draft','in_review') AND (NEW.line_mode,NEW.line_revision) IS DISTINCT FROM (OLD.line_mode,OLD.line_revision)
  THEN RAISE EXCEPTION 'FINANCE_INVOICE_LINES_POSTED_IMMUTABLE' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER finance_native_invoice_posting BEFORE UPDATE ON finance_invoices FOR EACH ROW EXECUTE FUNCTION erpx_finance_native_invoice_posting();

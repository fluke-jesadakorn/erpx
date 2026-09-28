-- Cross-database delivery receipt for the isolated ERPX Holding test ledger.
-- The Holding ledger is authoritative; this table is a Billing console view.
CREATE TABLE billing_test_finance_projection (
  billing_invoice_id TEXT PRIMARY KEY REFERENCES billing_invoices(id),
  stripe_invoice_id TEXT NOT NULL UNIQUE,
  customer_holding_id TEXT NOT NULL,
  company_id TEXT NOT NULL CHECK (company_id = 'erpx_billing_sandbox'),
  invoice_satang BIGINT NOT NULL CHECK (invoice_satang >= 0),
  receivable_satang BIGINT NOT NULL CHECK (receivable_satang >= 0),
  received_satang BIGINT NOT NULL CHECK (received_satang >= 0),
  reversed_satang BIGINT NOT NULL CHECK (reversed_satang >= 0),
  credited_satang BIGINT NOT NULL CHECK (credited_satang >= 0),
  stripe_status TEXT NOT NULL,
  synced_at BIGINT NOT NULL
);

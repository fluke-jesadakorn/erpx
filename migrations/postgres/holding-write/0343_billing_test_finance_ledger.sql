-- Isolated ERPX Holding Sandbox Company ledger. These tables are deliberately
-- outside customer Finance reports and never touch a live Company chart.
CREATE TABLE billing_test_finance_documents (
  stripe_invoice_id TEXT PRIMARY KEY,
  source_billing_invoice_id TEXT NOT NULL UNIQUE,
  customer_holding_id TEXT NOT NULL,
  company_id TEXT NOT NULL DEFAULT 'erpx_billing_sandbox'
    CHECK (company_id = 'erpx_billing_sandbox'),
  currency TEXT NOT NULL CHECK (currency = 'THB'),
  total_satang BIGINT NOT NULL CHECK (total_satang >= 0),
  stripe_status TEXT NOT NULL CHECK (stripe_status IN ('draft','open','paid','void','uncollectible')),
  observed_at BIGINT NOT NULL,
  created_at BIGINT NOT NULL
);

CREATE TABLE billing_test_finance_entries (
  source_key TEXT PRIMARY KEY,
  stripe_invoice_id TEXT NOT NULL REFERENCES billing_test_finance_documents(stripe_invoice_id),
  entry_kind TEXT NOT NULL CHECK (entry_kind IN ('issued','receipt','void','credit','credit_reversal')),
  amount_satang BIGINT NOT NULL CHECK (amount_satang > 0),
  effective_at BIGINT NOT NULL,
  created_at BIGINT NOT NULL
);

CREATE TABLE billing_test_finance_lines (
  source_key TEXT NOT NULL REFERENCES billing_test_finance_entries(source_key),
  ordinal INTEGER NOT NULL CHECK (ordinal IN (0,1)),
  account_code TEXT NOT NULL CHECK (account_code IN ('accounts_receivable','revenue','sandbox_cash','customer_credit')),
  debit_satang BIGINT NOT NULL DEFAULT 0 CHECK (debit_satang >= 0),
  credit_satang BIGINT NOT NULL DEFAULT 0 CHECK (credit_satang >= 0),
  PRIMARY KEY (source_key,ordinal),
  CHECK ((debit_satang > 0 AND credit_satang = 0) OR (credit_satang > 0 AND debit_satang = 0))
);

CREATE INDEX billing_test_finance_document_customer
  ON billing_test_finance_documents(customer_holding_id,created_at);

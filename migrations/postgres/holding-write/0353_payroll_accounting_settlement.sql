ALTER TABLE hr_payment_batches ADD COLUMN bank_account_id TEXT REFERENCES finance_accounts(id);

CREATE TABLE hr_payroll_payment_settlements (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  batch_id TEXT NOT NULL REFERENCES hr_payment_batches(id),
  batch_item_id TEXT NOT NULL UNIQUE REFERENCES hr_payment_batch_items(id),
  bank_transaction_id TEXT NOT NULL UNIQUE REFERENCES finance_bank_transactions(id),
  journal_id TEXT NOT NULL UNIQUE REFERENCES finance_journals(id),
  confirmed_by TEXT NOT NULL,
  confirmed_at BIGINT NOT NULL
);

CREATE INDEX hr_payroll_payment_settlements_batch_idx ON hr_payroll_payment_settlements(batch_id);

CREATE TABLE hr_payroll_account_mappings (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  pay_group_id TEXT NOT NULL REFERENCES hr_pay_groups(id),
  chart_id TEXT NOT NULL REFERENCES finance_charts(id),
  accounts_json TEXT NOT NULL CHECK(erpx_json_valid(accounts_json)),
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','superseded')),
  version BIGINT NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  approved_by TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);

CREATE INDEX hr_payroll_account_mappings_group_idx ON hr_payroll_account_mappings(company_id,pay_group_id,status,created_at DESC);
CREATE UNIQUE INDEX hr_payroll_account_mappings_one_approved ON hr_payroll_account_mappings(company_id,pay_group_id) WHERE status='approved';

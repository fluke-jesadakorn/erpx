-- A tax or social-security payable remains open until an exact imported bank
-- transaction is matched and a balancing settlement journal is posted.
CREATE TABLE hr_payroll_statutory_remittances (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  run_id TEXT NOT NULL REFERENCES hr_payroll_runs(id),
  kind TEXT NOT NULL CHECK (kind IN ('withholding_tax','social_security')),
  amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
  liability_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  bank_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  bank_transaction_id TEXT NOT NULL UNIQUE REFERENCES finance_bank_transactions(id),
  journal_id TEXT NOT NULL UNIQUE REFERENCES finance_journals(id),
  bank_reference TEXT NOT NULL,
  recorded_by TEXT NOT NULL,
  recorded_at BIGINT NOT NULL
);

CREATE INDEX hr_payroll_statutory_remittances_run_kind_idx
  ON hr_payroll_statutory_remittances(run_id,kind,recorded_at);

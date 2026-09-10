PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS finance_expenses (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  employee_id TEXT,
  expense_date TEXT NOT NULL,
  merchant TEXT NOT NULL,
  currency TEXT NOT NULL,
  amount_minor INTEGER NOT NULL CHECK(amount_minor > 0),
  tax_minor INTEGER NOT NULL DEFAULT 0,
  category_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','in_review','approved','rejected','posted','reimbursed')),
  journal_id TEXT REFERENCES finance_journals(id),
  evidence_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(evidence_json)),
  version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  approved_by TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS finance_accrual_schedules (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  description TEXT NOT NULL,
  debit_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  credit_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  currency TEXT NOT NULL,
  total_minor INTEGER NOT NULL CHECK(total_minor > 0),
  start_date TEXT NOT NULL,
  end_date TEXT NOT NULL,
  next_run_date TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('draft','active','completed','cancelled')),
  version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS finance_revenue_contracts (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  customer_id TEXT NOT NULL REFERENCES finance_parties(id),
  contract_number TEXT NOT NULL,
  currency TEXT NOT NULL,
  transaction_price_minor INTEGER NOT NULL CHECK(transaction_price_minor >= 0),
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','active','completed','cancelled')),
  version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(company_id, contract_number)
) STRICT;
CREATE TABLE IF NOT EXISTS finance_revenue_obligations (
  id TEXT PRIMARY KEY,
  contract_id TEXT NOT NULL REFERENCES finance_revenue_contracts(id),
  description TEXT NOT NULL,
  allocated_minor INTEGER NOT NULL CHECK(allocated_minor >= 0),
  recognition_method TEXT NOT NULL CHECK(recognition_method IN ('point_in_time','over_time','milestone')),
  start_date TEXT NOT NULL,
  end_date TEXT,
  recognized_minor INTEGER NOT NULL DEFAULT 0,
  revenue_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  deferred_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','active','complete'))
) STRICT;

CREATE TABLE IF NOT EXISTS finance_leases (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  lease_number TEXT NOT NULL,
  counterparty TEXT NOT NULL,
  currency TEXT NOT NULL,
  commencement_date TEXT NOT NULL,
  end_date TEXT NOT NULL,
  payment_frequency TEXT NOT NULL DEFAULT 'monthly',
  payment_minor INTEGER NOT NULL CHECK(payment_minor > 0),
  discount_rate_micros INTEGER NOT NULL CHECK(discount_rate_micros >= 0),
  right_of_use_minor INTEGER NOT NULL DEFAULT 0,
  liability_minor INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','active','modified','terminated','completed')),
  version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(company_id, lease_number)
) STRICT;

CREATE TABLE IF NOT EXISTS finance_loans (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  loan_number TEXT NOT NULL,
  lender TEXT NOT NULL,
  currency TEXT NOT NULL,
  principal_minor INTEGER NOT NULL CHECK(principal_minor > 0),
  annual_rate_micros INTEGER NOT NULL CHECK(annual_rate_micros >= 0),
  commencement_date TEXT NOT NULL,
  maturity_date TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','active','repaid','defaulted')),
  outstanding_minor INTEGER NOT NULL,
  version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(company_id, loan_number)
) STRICT;

CREATE TABLE IF NOT EXISTS finance_intercompany_transactions (
  id TEXT PRIMARY KEY,
  holding_id TEXT NOT NULL,
  source_company_id TEXT NOT NULL,
  target_company_id TEXT NOT NULL,
  reference TEXT NOT NULL,
  currency TEXT NOT NULL,
  amount_minor INTEGER NOT NULL CHECK(amount_minor > 0),
  source_journal_id TEXT REFERENCES finance_journals(id),
  target_journal_id TEXT REFERENCES finance_journals(id),
  status TEXT NOT NULL DEFAULT 'unmatched' CHECK(status IN ('unmatched','matched','settled','eliminated')),
  version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE(holding_id, reference)
) STRICT;

CREATE TABLE IF NOT EXISTS finance_consolidation_runs (
  id TEXT PRIMARY KEY,
  holding_id TEXT NOT NULL,
  presentation_currency TEXT NOT NULL,
  as_of_date TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','review','approved','published')),
  source_snapshot_json TEXT NOT NULL CHECK(json_valid(source_snapshot_json)),
  version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  approved_by TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS finance_consolidation_eliminations (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES finance_consolidation_runs(id),
  description TEXT NOT NULL,
  debit_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  credit_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  amount_minor INTEGER NOT NULL CHECK(amount_minor > 0),
  journal_id TEXT REFERENCES finance_journals(id),
  status TEXT NOT NULL DEFAULT 'proposed' CHECK(status IN ('proposed','approved','posted'))
) STRICT;

CREATE TABLE IF NOT EXISTS finance_tax_returns (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  tax_type TEXT NOT NULL,
  period_start TEXT NOT NULL,
  period_end TEXT NOT NULL,
  output_tax_minor INTEGER NOT NULL DEFAULT 0,
  input_tax_minor INTEGER NOT NULL DEFAULT 0,
  withholding_tax_minor INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','review','approved','settled','filed')),
  version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;

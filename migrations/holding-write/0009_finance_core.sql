PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS finance_charts (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  name TEXT NOT NULL,
  base_currency TEXT NOT NULL CHECK(length(base_currency)=3 AND base_currency=upper(base_currency)),
  accounting_basis TEXT NOT NULL DEFAULT 'IFRS',
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('draft','active','archived')),
  version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(company_id, name)
) STRICT;

CREATE TABLE IF NOT EXISTS finance_accounts (
  id TEXT PRIMARY KEY,
  chart_id TEXT NOT NULL REFERENCES finance_charts(id),
  company_id TEXT NOT NULL,
  code TEXT NOT NULL,
  name TEXT NOT NULL,
  category TEXT NOT NULL CHECK(category IN ('asset','liability','equity','revenue','expense','other_income','other_expense')),
  parent_id TEXT,
  currency TEXT,
  is_control INTEGER NOT NULL DEFAULT 0 CHECK(is_control IN (0,1)),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','inactive')),
  version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(chart_id, code),
  FOREIGN KEY(parent_id) REFERENCES finance_accounts(id)
) STRICT;
CREATE INDEX IF NOT EXISTS finance_accounts_company_idx ON finance_accounts(company_id, chart_id, status, code);

CREATE TABLE IF NOT EXISTS finance_periods (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  name TEXT NOT NULL,
  start_date TEXT NOT NULL,
  end_date TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'open' CHECK(state IN ('open','closing','closed')),
  close_version INTEGER NOT NULL DEFAULT 0,
  closed_by TEXT,
  closed_at INTEGER,
  created_at INTEGER NOT NULL,
  UNIQUE(company_id, start_date, end_date),
  CHECK(end_date >= start_date)
) STRICT;
CREATE INDEX IF NOT EXISTS finance_periods_company_date_idx ON finance_periods(company_id, start_date, end_date);

CREATE TABLE IF NOT EXISTS finance_journals (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  chart_id TEXT NOT NULL REFERENCES finance_charts(id),
  period_id TEXT NOT NULL REFERENCES finance_periods(id),
  journal_number TEXT NOT NULL,
  entry_date TEXT NOT NULL,
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  fx_rate_micros INTEGER,
  source_type TEXT NOT NULL DEFAULT 'manual',
  source_id TEXT,
  memo TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','in_review','posted','reversed')),
  is_opening INTEGER NOT NULL DEFAULT 0 CHECK(is_opening IN (0,1)),
  version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  posted_by TEXT,
  posted_at INTEGER,
  reversed_by TEXT,
  reversed_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(company_id, journal_number)
) STRICT;
CREATE INDEX IF NOT EXISTS finance_journals_company_date_idx ON finance_journals(company_id, entry_date, status);

CREATE TABLE IF NOT EXISTS finance_journal_lines (
  id TEXT PRIMARY KEY,
  journal_id TEXT NOT NULL REFERENCES finance_journals(id),
  account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  description TEXT NOT NULL DEFAULT '',
  debit_minor INTEGER NOT NULL DEFAULT 0 CHECK(debit_minor >= 0),
  credit_minor INTEGER NOT NULL DEFAULT 0 CHECK(credit_minor >= 0),
  base_debit_minor INTEGER NOT NULL DEFAULT 0 CHECK(base_debit_minor >= 0),
  base_credit_minor INTEGER NOT NULL DEFAULT 0 CHECK(base_credit_minor >= 0),
  dimension_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(dimension_json)),
  ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
  CHECK((debit_minor = 0 AND credit_minor > 0) OR (credit_minor = 0 AND debit_minor > 0))
) STRICT;
CREATE INDEX IF NOT EXISTS finance_journal_lines_account_idx ON finance_journal_lines(account_id, journal_id);

CREATE TABLE IF NOT EXISTS finance_parties (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('customer','supplier','employee')),
  code TEXT NOT NULL,
  name TEXT NOT NULL,
  currency TEXT NOT NULL,
  tax_registration TEXT,
  payment_terms_days INTEGER NOT NULL DEFAULT 30,
  credit_limit_minor INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','blocked')),
  version INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(company_id, kind, code)
) STRICT;

CREATE TABLE IF NOT EXISTS finance_invoices (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  party_id TEXT NOT NULL REFERENCES finance_parties(id),
  kind TEXT NOT NULL CHECK(kind IN ('payable','receivable')),
  invoice_number TEXT NOT NULL,
  invoice_date TEXT NOT NULL,
  due_date TEXT NOT NULL,
  currency TEXT NOT NULL,
  subtotal_minor INTEGER NOT NULL CHECK(subtotal_minor >= 0),
  tax_minor INTEGER NOT NULL DEFAULT 0 CHECK(tax_minor >= 0),
  total_minor INTEGER NOT NULL CHECK(total_minor >= 0),
  outstanding_minor INTEGER NOT NULL CHECK(outstanding_minor >= 0),
  tax_code TEXT,
  debit_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  credit_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  source_type TEXT,
  source_id TEXT,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','in_review','posted','partially_paid','paid','voided')),
  journal_id TEXT REFERENCES finance_journals(id),
  version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  approved_by TEXT,
  posted_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(company_id, kind, invoice_number)
) STRICT;
CREATE INDEX IF NOT EXISTS finance_invoices_aging_idx ON finance_invoices(company_id, kind, status, due_date);

CREATE TABLE IF NOT EXISTS finance_payments (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  party_id TEXT REFERENCES finance_parties(id),
  kind TEXT NOT NULL CHECK(kind IN ('payment','receipt','refund')),
  payment_number TEXT NOT NULL,
  payment_date TEXT NOT NULL,
  currency TEXT NOT NULL,
  amount_minor INTEGER NOT NULL CHECK(amount_minor > 0),
  outstanding_minor INTEGER NOT NULL CHECK(outstanding_minor >= 0),
  bank_account TEXT,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','in_review','approved','released','settled','voided')),
  journal_id TEXT REFERENCES finance_journals(id),
  version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  approved_by TEXT,
  released_by TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(company_id, payment_number)
) STRICT;

CREATE TABLE IF NOT EXISTS finance_allocations (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  payment_id TEXT NOT NULL REFERENCES finance_payments(id),
  invoice_id TEXT NOT NULL REFERENCES finance_invoices(id),
  amount_minor INTEGER NOT NULL CHECK(amount_minor > 0),
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE(payment_id, invoice_id)
) STRICT;

CREATE TABLE IF NOT EXISTS finance_bank_statements (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  bank_account TEXT NOT NULL,
  statement_number TEXT NOT NULL,
  statement_date TEXT NOT NULL,
  opening_minor INTEGER NOT NULL DEFAULT 0,
  closing_minor INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'imported' CHECK(status IN ('imported','matching','reconciled')),
  source_hash TEXT NOT NULL,
  version INTEGER NOT NULL DEFAULT 1,
  imported_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE(company_id, bank_account, source_hash)
) STRICT;

CREATE TABLE IF NOT EXISTS finance_bank_transactions (
  id TEXT PRIMARY KEY,
  statement_id TEXT NOT NULL REFERENCES finance_bank_statements(id),
  company_id TEXT NOT NULL,
  transaction_date TEXT NOT NULL,
  external_id TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  amount_minor INTEGER NOT NULL,
  currency TEXT NOT NULL,
  match_status TEXT NOT NULL DEFAULT 'unmatched' CHECK(match_status IN ('unmatched','proposed','matched','ignored')),
  matched_type TEXT,
  matched_id TEXT,
  version INTEGER NOT NULL DEFAULT 1,
  UNIQUE(statement_id, external_id)
) STRICT;

CREATE TABLE IF NOT EXISTS finance_tax_codes (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  code TEXT NOT NULL,
  name TEXT NOT NULL,
  rate_micros INTEGER NOT NULL CHECK(rate_micros >= 0),
  recoverable_micros INTEGER NOT NULL DEFAULT 1000000 CHECK(recoverable_micros BETWEEN 0 AND 1000000),
  effective_from TEXT NOT NULL,
  effective_to TEXT,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','inactive')),
  UNIQUE(company_id, code, effective_from)
) STRICT;

CREATE TABLE IF NOT EXISTS finance_assets (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  asset_number TEXT NOT NULL,
  name TEXT NOT NULL,
  account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  cost_minor INTEGER NOT NULL CHECK(cost_minor >= 0),
  residual_minor INTEGER NOT NULL DEFAULT 0 CHECK(residual_minor >= 0),
  useful_life_months INTEGER NOT NULL CHECK(useful_life_months > 0),
  in_service_date TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','in_service','impaired','retired')),
  accumulated_depreciation_minor INTEGER NOT NULL DEFAULT 0,
  version INTEGER NOT NULL DEFAULT 1,
  UNIQUE(company_id, asset_number)
) STRICT;

CREATE TABLE IF NOT EXISTS finance_budgets (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  name TEXT NOT NULL,
  fiscal_year INTEGER NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','submitted','approved','closed')),
  currency TEXT NOT NULL,
  version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(company_id, name, fiscal_year)
) STRICT;
CREATE TABLE IF NOT EXISTS finance_budget_lines (
  id TEXT PRIMARY KEY,
  budget_id TEXT NOT NULL REFERENCES finance_budgets(id),
  account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  period TEXT NOT NULL,
  amount_minor INTEGER NOT NULL,
  dimension_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(dimension_json)),
  UNIQUE(budget_id, account_id, period)
) STRICT;

CREATE TABLE IF NOT EXISTS finance_commands (
  command_id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  operation TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  result_json TEXT,
  created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS finance_commands_company_idx ON finance_commands(company_id, operation, created_at);

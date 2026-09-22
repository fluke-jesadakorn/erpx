CREATE TABLE finance_fx_revaluation_runs (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  chart_id TEXT NOT NULL REFERENCES finance_charts(id),
  period_id TEXT NOT NULL REFERENCES finance_periods(id),
  entry_date TEXT NOT NULL,
  base_currency TEXT NOT NULL,
  rates_json TEXT NOT NULL CHECK(erpx_json_valid(rates_json)),
  source_hash TEXT NOT NULL,
  gain_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  loss_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  status TEXT NOT NULL DEFAULT 'prepared' CHECK(status IN ('prepared','posted','rejected')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  journal_id TEXT REFERENCES finance_journals(id),
  preparation_proof TEXT NOT NULL,
  decision_proof TEXT,
  created_by TEXT NOT NULL,
  approved_by TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);

CREATE UNIQUE INDEX finance_fx_revaluation_posted_period_chart_idx
  ON finance_fx_revaluation_runs(period_id,chart_id) WHERE status='posted';

CREATE TABLE finance_fx_revaluation_lines (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES finance_fx_revaluation_runs(id) ON DELETE CASCADE,
  invoice_id TEXT NOT NULL REFERENCES finance_invoices(id),
  kind TEXT NOT NULL CHECK(kind IN ('receivable','payable')),
  currency TEXT NOT NULL,
  control_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  transaction_outstanding_minor BIGINT NOT NULL CHECK(transaction_outstanding_minor > 0),
  prior_base_minor BIGINT NOT NULL CHECK(prior_base_minor >= 0),
  revalued_base_minor BIGINT NOT NULL CHECK(revalued_base_minor >= 0),
  adjustment_minor BIGINT NOT NULL,
  rate_micros BIGINT NOT NULL CHECK(rate_micros > 0),
  UNIQUE(run_id,invoice_id)
);

CREATE INDEX finance_fx_revaluation_lines_invoice_idx
  ON finance_fx_revaluation_lines(invoice_id,run_id);

INSERT INTO work_cache_revisions(table_name) VALUES
  ('finance_fx_revaluation_runs'),('finance_fx_revaluation_lines')
ON CONFLICT DO NOTHING;

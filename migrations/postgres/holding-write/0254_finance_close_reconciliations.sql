ALTER TABLE finance_accounts DROP CONSTRAINT IF EXISTS finance_accounts_control_kind_check;
ALTER TABLE finance_accounts ADD CONSTRAINT finance_accounts_control_kind_check
  CHECK(control_kind IN ('none','receivable','payable','bank'));

ALTER TABLE finance_periods ADD COLUMN reconciliation_hash TEXT;

CREATE TABLE finance_close_reconciliations (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  period_id TEXT NOT NULL REFERENCES finance_periods(id),
  close_version BIGINT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('receivable','payable','bank')),
  account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  currency TEXT NOT NULL,
  ledger_minor BIGINT NOT NULL,
  source_minor BIGINT NOT NULL,
  difference_minor BIGINT NOT NULL,
  source_json TEXT NOT NULL CHECK(erpx_json_valid(source_json)),
  checked_at BIGINT NOT NULL,
  checked_by TEXT NOT NULL,
  UNIQUE(period_id,close_version,kind,account_id)
);

CREATE INDEX finance_close_reconciliations_period_idx
  ON finance_close_reconciliations(company_id,period_id,close_version);

UPDATE finance_accounts a SET control_kind='bank',is_control=1
WHERE a.category='asset' AND EXISTS (
  SELECT 1 FROM finance_bank_statements s
  WHERE s.company_id=a.company_id AND s.bank_account=a.id
);

INSERT INTO work_cache_revisions(table_name) VALUES
  ('finance_accounts'),('finance_periods'),('finance_close_reconciliations')
ON CONFLICT DO NOTHING;

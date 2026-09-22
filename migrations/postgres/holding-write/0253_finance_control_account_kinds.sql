ALTER TABLE finance_accounts
  ADD COLUMN control_kind TEXT NOT NULL DEFAULT 'none'
  CHECK(control_kind IN ('none','receivable','payable'));

UPDATE finance_accounts a SET control_kind='receivable',is_control=1
WHERE EXISTS (
  SELECT 1 FROM finance_invoices i
  WHERE i.company_id=a.company_id AND i.kind='receivable' AND i.debit_account_id=a.id
);

UPDATE finance_accounts a SET control_kind='payable',is_control=1
WHERE EXISTS (
  SELECT 1 FROM finance_invoices i
  WHERE i.company_id=a.company_id AND i.kind='payable' AND i.credit_account_id=a.id
);

INSERT INTO work_cache_revisions(table_name) VALUES('finance_accounts') ON CONFLICT DO NOTHING;

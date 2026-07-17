-- folio/db/migrations/2026-07-22-C-financial-views.sql
--
-- Server-owned views that back the deterministic financial-statement
-- resolvers in lib/finance/reports.ts. All views filter out draft
-- journal entries.

BEGIN;

CREATE OR REPLACE VIEW finance.v_trial_balance AS
SELECT
  c.code,
  c.name,
  c.name_th,
  c.account_type,
  COALESCE(SUM(l.debit), 0)::float  AS period_debit,
  COALESCE(SUM(l.credit), 0)::float AS period_credit,
  COALESCE(SUM(l.debit), 0)::float - COALESCE(SUM(l.credit), 0)::float AS net
FROM folio.chart_of_accounts c
LEFT JOIN folio.ledger_lines l ON l.account_code = c.code
LEFT JOIN folio.journal_entries j ON j.id = l.journal_entry_id AND j.is_draft = FALSE
GROUP BY c.code, c.name, c.name_th, c.account_type;

CREATE OR REPLACE VIEW finance.v_income_statement AS
SELECT
  c.code,
  c.name,
  c.name_th,
  c.account_type,
  CASE
    WHEN c.account_type = 'revenue'
      THEN COALESCE(SUM(l.credit), 0) - COALESCE(SUM(l.debit), 0)
    WHEN c.account_type = 'expense'
      THEN COALESCE(SUM(l.debit), 0) - COALESCE(SUM(l.credit), 0)
    ELSE 0
  END::float AS amount
FROM folio.chart_of_accounts c
LEFT JOIN folio.ledger_lines l ON l.account_code = c.code
LEFT JOIN folio.journal_entries j ON j.id = l.journal_entry_id AND j.is_draft = FALSE
WHERE c.account_type IN ('revenue', 'expense')
GROUP BY c.code, c.name, c.name_th, c.account_type;

CREATE OR REPLACE VIEW finance.v_balance_sheet AS
SELECT
  c.code,
  c.name,
  c.name_th,
  c.account_type,
  CASE
    WHEN c.account_type IN ('asset', 'expense')
      THEN COALESCE(SUM(l.debit), 0) - COALESCE(SUM(l.credit), 0)
    ELSE
      COALESCE(SUM(l.credit), 0) - COALESCE(SUM(l.debit), 0)
  END::float AS balance
FROM folio.chart_of_accounts c
LEFT JOIN folio.ledger_lines l ON l.account_code = c.code
LEFT JOIN folio.journal_entries j ON j.id = l.journal_entry_id AND j.is_draft = FALSE
WHERE c.account_type IN ('asset', 'liability', 'equity')
GROUP BY c.code, c.name, c.name_th, c.account_type;

CREATE OR REPLACE VIEW finance.v_period_summary AS
SELECT
  j.id AS journal_entry_id,
  j.entry_date,
  j.description,
  j.expense_id,
  j.pr_id,
  j.po_id,
  j.so_id,
  j.step,
  COALESCE(SUM(l.debit), 0)::float  AS total_debit,
  COALESCE(SUM(l.credit), 0)::float AS total_credit
FROM folio.journal_entries j
LEFT JOIN folio.ledger_lines l ON l.journal_entry_id = j.id
WHERE j.is_draft = FALSE
GROUP BY j.id, j.entry_date, j.description, j.expense_id, j.pr_id, j.po_id, j.so_id, j.step;

INSERT INTO perm.audit (kind, target) VALUES (
  'schema.migration',
  jsonb_build_object(
    'migration', '2026-07-22-C-financial-views',
    'changes', jsonb_build_array(
      'finance.v_trial_balance',
      'finance.v_income_statement',
      'finance.v_balance_sheet',
      'finance.v_period_summary'
    )
  )
);

COMMIT;
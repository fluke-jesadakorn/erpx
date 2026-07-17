-- folio/db/migrations/2026-07-22-A-cashflow-classification.sql
--
-- Cash-flow statement support:
--   finance.account_cf_class      accountant-maintained mapping of each COA code
--                                 to (activity, is_cash_account) with optional
--                                 effective dates. Activities: operating,
--                                 investing, financing, non_cash, unclassified.
--   finance.cashflow_period       fiscal-year/period registry with optional
--                                 opening-balance journal link and status.
--   finance.v_cashflow_classified server-owned join used by lib/finance/cashflow.

BEGIN;

CREATE SCHEMA IF NOT EXISTS finance;

CREATE TABLE IF NOT EXISTS finance.account_cf_class (
  account_code    VARCHAR(20)  PRIMARY KEY
                    REFERENCES folio.chart_of_accounts(code) ON UPDATE CASCADE,
  activity        VARCHAR(20)  NOT NULL
                    CHECK (activity IN ('operating','investing','financing','non_cash','unclassified')),
  is_cash_account BOOLEAN      NOT NULL DEFAULT FALSE,
  note            TEXT,
  effective_from  DATE         NOT NULL DEFAULT '0001-01-01',
  effective_to    DATE         NOT NULL DEFAULT '9999-12-31',
  updated_at      TIMESTAMPTZ  NOT NULL DEFAULT now(),
  updated_by      INT          REFERENCES folio.users(id)
);

CREATE INDEX IF NOT EXISTS account_cf_class_activity_idx
  ON finance.account_cf_class(activity, is_cash_account);

CREATE TABLE IF NOT EXISTS finance.cashflow_period (
  fiscal_year                INT          NOT NULL,
  period_start               DATE         NOT NULL,
  period_end                 DATE         NOT NULL,
  status                     VARCHAR(16)  NOT NULL DEFAULT 'open'
                               CHECK (status IN ('open','closed','locked')),
  opening_balance_journal_id INT          REFERENCES folio.journal_entries(id),
  opened_by                  INT          REFERENCES folio.users(id),
  opened_at                  TIMESTAMPTZ  NOT NULL DEFAULT now(),
  closed_at                  TIMESTAMPTZ,
  closed_by                  INT          REFERENCES folio.users(id),
  notes                      TEXT,
  PRIMARY KEY (fiscal_year)
);

INSERT INTO finance.account_cf_class (account_code, activity, is_cash_account, note)
VALUES
  ('110100','operating',  TRUE,  'Cash on Hand'),
  ('110200','operating',  TRUE,  'Cash at Bank - Savings'),
  ('110300','operating',  TRUE,  'Cash at Bank - Current'),
  ('110400','operating',  FALSE, 'Accounts Receivable'),
  ('110500','operating',  FALSE, 'Input VAT'),
  ('110600','operating',  FALSE, 'Prepaid Expenses'),
  ('210100','operating',  FALSE, 'Accounts Payable'),
  ('210200','operating',  FALSE, 'Accrued Expenses'),
  ('210300','operating',  FALSE, 'Accrued Output VAT'),
  ('210400','operating',  FALSE, 'Withholding Tax Payable'),
  ('210500','operating',  FALSE, 'Employee Reimbursement Payable'),
  ('310100','financing',  FALSE, 'Share Capital'),
  ('310200','financing',  FALSE, 'Retained Earnings'),
  ('410100','operating',  FALSE, 'Sales Revenue'),
  ('410200','operating',  FALSE, 'Service Revenue'),
  ('410300','operating',  FALSE, 'Other Income'),
  ('510100','operating',  FALSE, 'Salaries & Wages'),
  ('510200','operating',  FALSE, 'Travel & Transportation'),
  ('510300','operating',  FALSE, 'Office Supplies & Stationery'),
  ('510400','operating',  FALSE, 'Entertainment & Client Meal'),
  ('510500','operating',  FALSE, 'Internet & Utilities'),
  ('510600','operating',  FALSE, 'Post & Delivery'),
  ('510700','operating',  FALSE, 'Software & Subscriptions'),
  ('510800','operating',  FALSE, 'Maintenance & Repairs'),
  ('510900','operating',  FALSE, 'Training & Seminar'),
  ('520100','investing',  FALSE, 'Office Rental'),
  ('520200','operating',  FALSE, 'Marketing & Advertising'),
  ('520300','operating',  FALSE, 'Professional & Consulting Fees'),
  ('520400','operating',  FALSE, 'Bank Charges'),
  ('520500','investing',  FALSE, 'Insurance'),
  ('520600','non_cash',   FALSE, 'Depreciation (non-cash)'),
  ('520700','operating',  FALSE, 'Taxes & Licenses')
ON CONFLICT (account_code) DO UPDATE
  SET activity        = EXCLUDED.activity,
      is_cash_account = EXCLUDED.is_cash_account,
      note            = EXCLUDED.note,
      updated_at      = now();

CREATE OR REPLACE VIEW finance.v_cashflow_classified AS
SELECT
  l.id,
  j.id            AS journal_entry_id,
  j.entry_date,
  j.is_draft,
  j.finalized_at,
  l.account_code,
  c.name          AS account_name,
  c.account_type  AS coa_account_type,
  COALESCE(m.activity, 'unclassified') AS activity,
  COALESCE(m.is_cash_account, FALSE)   AS is_cash_account,
  l.debit,
  l.credit,
  l.description
FROM folio.ledger_lines l
JOIN folio.journal_entries j ON j.id = l.journal_entry_id
JOIN folio.chart_of_accounts c ON c.code = l.account_code
LEFT JOIN finance.account_cf_class m
       ON m.account_code = l.account_code
      AND j.entry_date BETWEEN m.effective_from AND m.effective_to
WHERE j.is_draft = FALSE;

INSERT INTO perm.audit (kind, target) VALUES (
  'schema.migration',
  jsonb_build_object(
    'migration', '2026-07-22-A-cashflow-classification',
    'changes', jsonb_build_array(
      'finance.account_cf_class',
      'finance.cashflow_period',
      'finance.v_cashflow_classified'
    )
  )
);

COMMIT;
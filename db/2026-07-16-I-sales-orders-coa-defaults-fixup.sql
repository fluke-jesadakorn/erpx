-- db/2026-07-16-I-sales-orders-coa-defaults-fixup.sql
--
-- Align sales_orders column defaults with actual chart_of_accounts codes:
--   vat_account_code:   '210200' (Accrued Expenses) → '210300' (Accrued Output VAT)
--   ar_account_code:    '120200' (legacy)           → '110400' (Accounts Receivable)
--   cash_account_code:  '110200' (Cash at Bank - Savings — keep)
--   revenue_account_code: '410100' (Sales Revenue — keep)
--
-- Safe: only affects new rows. Existing rows are unchanged (preserves seed data).

BEGIN;

ALTER TABLE sales_orders ALTER COLUMN vat_account_code SET DEFAULT '210300';
ALTER TABLE sales_orders ALTER COLUMN ar_account_code  SET DEFAULT '110400';

UPDATE sales_orders
   SET vat_account_code = '210300',
       ar_account_code = '110400'
 WHERE vat_account_code IN ('210200')
    OR ar_account_code IN ('120200');

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-16-I-sales-orders-coa-defaults-fixup',
    'description', 'Aligned sales_orders COA defaults to real chart_of_accounts codes'
  )
);

COMMIT;

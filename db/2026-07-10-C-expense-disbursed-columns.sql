-- folio/db/2026-07-10-C-expense-disbursed-columns.sql
--
-- Add disbursed_at + disbursed_by to expenses.
-- attachPaymentSlipAction (waybill/[id]/_actions.ts) marks the expense
-- as disbursed and stamps who/when; both columns are nullable so
-- historical rows remain valid.

BEGIN;

ALTER TABLE expenses
  ADD COLUMN IF NOT EXISTS disbursed_at timestamp without time zone,
  ADD COLUMN IF NOT EXISTS disbursed_by integer REFERENCES users(id);

INSERT INTO perm.audit (kind, target)
VALUES (
  'schema.migration',
  jsonb_build_object(
    'migration', '2026-07-10-C-expense-disbursed-columns',
    'columns_added', jsonb_build_array('expenses.disbursed_at', 'expenses.disbursed_by')
  )
);

COMMIT;

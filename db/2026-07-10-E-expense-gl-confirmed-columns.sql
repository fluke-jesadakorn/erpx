-- folio/db/2026-07-10-E-expense-gl-confirmed-columns.sql
--
-- Add gl_confirmed_at + gl_confirmed_by to expenses.
-- A separate accounting/finance user clicks "Confirm GL recorded"
-- on the waybill timeline after the disbursement + GL post completes;
-- the action stamps these columns and appends a kind='gl-confirmed'
-- waybill_event so the audit chain shows who signed off.

BEGIN;

ALTER TABLE expenses
  ADD COLUMN IF NOT EXISTS gl_confirmed_at timestamp without time zone,
  ADD COLUMN IF NOT EXISTS gl_confirmed_by integer REFERENCES users(id);

INSERT INTO perm.audit (kind, target)
VALUES (
  'schema.migration',
  jsonb_build_object(
    'migration', '2026-07-10-E-expense-gl-confirmed-columns',
    'columns_added', jsonb_build_array('expenses.gl_confirmed_at', 'expenses.gl_confirmed_by')
  )
);

COMMIT;

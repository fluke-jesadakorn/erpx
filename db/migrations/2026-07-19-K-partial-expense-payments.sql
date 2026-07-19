BEGIN;

ALTER TABLE folio.expense_payments
  DROP CONSTRAINT IF EXISTS expense_payments_expense_id_key,
  DROP CONSTRAINT IF EXISTS expense_payments_waybill_id_key;

ALTER TABLE folio.expense_payments
  ADD COLUMN IF NOT EXISTS journal_id bigint REFERENCES finance.journals(id),
  ADD COLUMN IF NOT EXISTS allocation_id bigint REFERENCES finance.ap_allocations(id),
  ADD COLUMN IF NOT EXISTS wht_amount_thb numeric(18,2) NOT NULL DEFAULT 0 CHECK (wht_amount_thb >= 0);

CREATE INDEX IF NOT EXISTS expense_payments_expense_idx
  ON folio.expense_payments(expense_id, payment_date, id);

CREATE INDEX IF NOT EXISTS expense_payments_waybill_idx
  ON folio.expense_payments(waybill_id, payment_date, id);

COMMIT;

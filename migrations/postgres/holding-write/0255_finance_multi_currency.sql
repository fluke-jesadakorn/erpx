ALTER TABLE finance_invoices
  ADD COLUMN fx_rate_micros BIGINT NOT NULL DEFAULT 1000000 CHECK(fx_rate_micros > 0),
  ADD COLUMN base_total_minor BIGINT NOT NULL DEFAULT 0 CHECK(base_total_minor >= 0),
  ADD COLUMN base_outstanding_minor BIGINT NOT NULL DEFAULT 0 CHECK(base_outstanding_minor >= 0);

UPDATE finance_invoices
SET base_total_minor = total_minor,
    base_outstanding_minor = outstanding_minor;

ALTER TABLE finance_payments
  ADD COLUMN fx_rate_micros BIGINT,
  ADD COLUMN base_amount_minor BIGINT CHECK(base_amount_minor >= 0),
  ADD COLUMN fx_journal_id TEXT REFERENCES finance_journals(id);

ALTER TABLE finance_allocations
  ADD COLUMN base_invoice_minor BIGINT NOT NULL DEFAULT 0 CHECK(base_invoice_minor >= 0),
  ADD COLUMN base_payment_minor BIGINT NOT NULL DEFAULT 0 CHECK(base_payment_minor >= 0),
  ADD COLUMN realized_fx_minor BIGINT NOT NULL DEFAULT 0;

UPDATE finance_allocations
SET base_invoice_minor = amount_minor,
    base_payment_minor = amount_minor;

ALTER TABLE finance_bank_statements
  ADD COLUMN closing_fx_rate_micros BIGINT NOT NULL DEFAULT 1000000 CHECK(closing_fx_rate_micros > 0),
  ADD COLUMN base_closing_minor BIGINT NOT NULL DEFAULT 0;

UPDATE finance_bank_statements SET base_closing_minor = closing_minor;

ALTER TABLE finance_payment_reversals
  ADD COLUMN fx_reversal_journal_id TEXT REFERENCES finance_journals(id);

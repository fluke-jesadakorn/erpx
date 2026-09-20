CREATE TABLE finance_payment_reversals (
  id TEXT PRIMARY KEY,
  payment_id TEXT NOT NULL UNIQUE REFERENCES finance_payments(id),
  reversal_journal_id TEXT NOT NULL UNIQUE REFERENCES finance_journals(id),
  reason TEXT NOT NULL,
  reversed_by TEXT NOT NULL,
  reversed_at BIGINT NOT NULL
);

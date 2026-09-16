CREATE TABLE spending_approval_templates (
  company_id TEXT PRIMARY KEY,
  version INTEGER NOT NULL CHECK (version > 0),
  exceptions_json TEXT NOT NULL DEFAULT '[]',
  reason TEXT NOT NULL,
  updated_by TEXT NOT NULL,
  updated_at BIGINT NOT NULL
);
ALTER TABLE finance_expenses ADD COLUMN request_kind TEXT NOT NULL DEFAULT 'expense' CHECK (request_kind IN ('expense','payment'));
ALTER TABLE finance_payments ADD COLUMN payment_slip_id TEXT REFERENCES attachments(id);

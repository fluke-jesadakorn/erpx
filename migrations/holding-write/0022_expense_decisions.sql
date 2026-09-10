CREATE TABLE expense_decisions (
  id TEXT PRIMARY KEY,
  expense_id TEXT NOT NULL REFERENCES finance_expenses(id),
  command_id TEXT NOT NULL UNIQUE REFERENCES command_receipts(command_id),
  action TEXT NOT NULL CHECK(action IN ('approve','reject','cancel')),
  actor_id TEXT NOT NULL,
  reviewed_version INTEGER NOT NULL,
  reason TEXT NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX expense_decisions_claim_idx ON expense_decisions(expense_id,created_at);

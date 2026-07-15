-- Rejection reason capture
-- Adds three columns to expenses and three to purchase_requisitions,
-- plus a partial index for fast "recently rejected" queries,
-- plus a pr_approval_logs table mirroring approval_logs.

ALTER TABLE expenses
  ADD COLUMN IF NOT EXISTS rejection_reason TEXT,
  ADD COLUMN IF NOT EXISTS rejection_actor_id INT REFERENCES users(id),
  ADD COLUMN IF NOT EXISTS rejected_at TIMESTAMP;
CREATE INDEX IF NOT EXISTS idx_expenses_rejected
  ON expenses(rejected_at) WHERE status = 'rejected';

ALTER TABLE purchase_requisitions
  ADD COLUMN IF NOT EXISTS rejection_reason TEXT,
  ADD COLUMN IF NOT EXISTS rejection_actor_id INT REFERENCES users(id),
  ADD COLUMN IF NOT EXISTS rejected_at TIMESTAMP;
CREATE INDEX IF NOT EXISTS idx_prs_rejected
  ON purchase_requisitions(rejected_at) WHERE status = 'rejected';

CREATE TABLE IF NOT EXISTS pr_approval_logs (
    id SERIAL PRIMARY KEY,
    pr_id INT REFERENCES purchase_requisitions(id) ON DELETE CASCADE,
    actor_id INT REFERENCES users(id),
    previous_status VARCHAR(50),
    new_status VARCHAR(50),
    comments TEXT,
    stage VARCHAR(50),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_pr_approval_logs_pr ON pr_approval_logs(pr_id, created_at DESC);

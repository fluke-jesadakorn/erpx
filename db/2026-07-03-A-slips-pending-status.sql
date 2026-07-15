-- 2026-07-03-A: slip status + user-controlled confirm/remove flow.
--
-- Refactor: replace the "auto-save on upload" flow with a four-step user-driven
-- flow:
--   1. upload   — file lands in MinIO, OCR runs, slip row inserted in 'pending'
--   2. review   — user can edit OCR fields, remove (discard) if wrong, or confirm
--   3. confirm  — draft expense is created, slip row linked and flipped to 'confirmed'
--   4. approve  — approval flow runs on the expense; once approved/rejected, the
--                 slip becomes hard-locked and the user can no longer remove it.
--
-- Trigger relaxation:
--   The existing `slips_check_exactly_one_parent()` trigger rejects any slip
--   that has zero or multiple parents. Pending slips have zero parents by
--   design (the parent is decided at confirm-time). We relax the trigger to
--   allow zero parents ONLY when status='pending'. Confirmed slips must
--   still have exactly one parent (the expense / pr / po they were attached to).
--
-- Lock semantics:
--   - status='pending'  → removable (uploader can discard)
--   - status='confirmed' but linked expense has no 'approved'/'rejected'
--     transition → removable (uploader can still pull a wrong upload out
--     before the approver sees it)
--   - linked expense has any 'approved' or 'rejected' transition → HARD
--     LOCKED; the user can no longer remove the slip.

-- Tx 1: column adds + backfill ------------------------------------------------
BEGIN;

ALTER TABLE slips
  ADD COLUMN IF NOT EXISTS status         VARCHAR(20) NOT NULL DEFAULT 'pending',
  ADD COLUMN IF NOT EXISTS confirmed_at   TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS discarded_at   TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS discarded_by   INT REFERENCES users(id);

UPDATE slips
   SET status = CASE
     WHEN expense_id IS NOT NULL OR pr_id IS NOT NULL OR po_id IS NOT NULL
       THEN 'confirmed'
     ELSE 'pending'
   END,
       confirmed_at = CASE
     WHEN expense_id IS NOT NULL OR pr_id IS NOT NULL OR po_id IS NOT NULL
       THEN COALESCE(confirmed_at, uploaded_at)
     ELSE NULL
   END
 WHERE status = 'pending';

COMMIT;

-- Tx 2: indexes (separate tx because the deferred trigger on slips blocks
-- CREATE INDEX inside the same tx as DML that fires the trigger). ----------
CREATE INDEX IF NOT EXISTS idx_slips_status ON slips(status);
CREATE INDEX IF NOT EXISTS idx_slips_uploader_status ON slips(uploaded_by, status);

-- Tx 3: replace the trigger function with the pending-aware version -------------
BEGIN;

CREATE OR REPLACE FUNCTION slips_check_exactly_one_parent() RETURNS TRIGGER AS $$
DECLARE
  current_row   slips%ROWTYPE;
  parent_count  INT;
  bad_cols      TEXT;
BEGIN
  SELECT * INTO current_row FROM slips WHERE id = NEW.id;

  -- pending slips may have zero parents — parent is assigned at confirm-time
  IF current_row.status = 'pending' THEN
    RETURN NEW;
  END IF;

  parent_count := (CASE WHEN current_row.expense_id IS NOT NULL THEN 1 ELSE 0 END) +
                  (CASE WHEN current_row.pr_id      IS NOT NULL THEN 1 ELSE 0 END) +
                  (CASE WHEN current_row.po_id      IS NOT NULL THEN 1 ELSE 0 END);
  IF parent_count <> 1 THEN
    bad_cols := concat_ws(',',
      CASE WHEN current_row.expense_id IS NOT NULL THEN 'expense_id' END,
      CASE WHEN current_row.pr_id      IS NOT NULL THEN 'pr_id'      END,
      CASE WHEN current_row.po_id      IS NOT NULL THEN 'po_id'      END);
    RAISE EXCEPTION 'slips_exactly_one_parent: expected exactly one parent, got % (set: %)',
      parent_count, COALESCE(bad_cols, '(none)')
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

COMMENT ON COLUMN slips.status IS
  'pending = uploaded + OCR done, no parent, uploader can still discard. '
  'confirmed = linked to expense/pr/po, in approval queue. '
  'Removal of a confirmed slip is blocked once the linked parent has any '
  'approved/rejected transition.';
COMMENT ON COLUMN slips.confirmed_at IS
  'Timestamp the slip was linked to its parent expense/pr/po.';
COMMENT ON COLUMN slips.discarded_at IS
  'Timestamp the slip was removed by the uploader (null = not discarded).';
COMMENT ON COLUMN slips.discarded_by IS
  'User who discarded the slip (null = not discarded).';

COMMIT;

-- Verify -----------------------------------------------------------------------
SELECT status, COUNT(*) FROM slips GROUP BY status ORDER BY status;

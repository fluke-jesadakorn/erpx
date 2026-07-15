-- 2026-07-02-C: slips exclusivity (exactly-one parent).
--
-- Refactor: tier 1 step C. Replaces the unstated "at-most-one-or-none" with
-- the strict "exactly-one" invariant the user requested.
--
-- Implementation notes:
--   * PostgreSQL CHECK constraints are NOT DEFERRABLE. To get deferred
--     enforcement (only fire at COMMIT, not at statement end) we use a
--     CONSTRAINT TRIGGER — the same pattern Postgres docs recommend for
--     deferred cross-row or cross-statement constraints.
--   * Pre-existing 11 orphan slips (no expense/pr/po) are deleted as
--     pre-cleanup; they were never linked to a parent and are unrecoverable.
--     Files in MinIO are untouched (orphan object cleanup is a separate task).
--   * DEFERRABLE INITIALLY DEFERRED so the orphan-then-link pattern still
--     works WITHIN a single transaction. The trigger only fires at COMMIT,
--     at which point exactly-one must be satisfied. Any commit with an
--     orphan slip fails.
--   * /api/upload must wrap INSERT + UPDATE in BEGIN/COMMIT.
--
-- The deferred-trigger behaviour is the documented contract going forward:
--   "Every slip that successfully commits has exactly one parent. Transient
--    orphans within a transaction are permitted; permanent orphans are not."

BEGIN;

-- C1. Pre-cleanup: delete orphan slips (no expense/pr/po) --------------------
DO $$
DECLARE orphan_count INT;
BEGIN
  SELECT COUNT(*) INTO orphan_count FROM slips
   WHERE expense_id IS NULL AND pr_id IS NULL AND po_id IS NULL;

  IF orphan_count > 0 THEN
    RAISE NOTICE 'slips: deleting % pre-existing orphan slip(s) (no parent)', orphan_count;
    DELETE FROM slips
     WHERE expense_id IS NULL AND pr_id IS NULL AND po_id IS NULL;
  END IF;
END$$;

-- C2. Trigger function --------------------------------------------------------
-- Reads CURRENT row state (not just NEW) so deferred trigger + UPDATE in same
-- tx sees the final state. CONSTRAINT TRIGGER is deferred to COMMIT; for each
-- event the trigger function SELECTs the row to read whatever the latest state
-- is after any subsequent modifications within the transaction.
CREATE OR REPLACE FUNCTION slips_check_exactly_one_parent() RETURNS TRIGGER AS $$
DECLARE
  current_row   slips%ROWTYPE;
  parent_count  INT;
  bad_cols      TEXT;
BEGIN
  SELECT * INTO current_row FROM slips WHERE id = NEW.id;
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

-- C3. Drop any old trigger from a previous failed run --------------------------
DROP TRIGGER IF EXISTS slips_exactly_one_parent_trg ON slips;

-- C4. Constraint trigger — DEFERRABLE so it fires at COMMIT -------------------
CREATE CONSTRAINT TRIGGER slips_exactly_one_parent_trg
  AFTER INSERT OR UPDATE ON slips
  DEFERRABLE INITIALLY DEFERRED
  FOR EACH ROW EXECUTE FUNCTION slips_check_exactly_one_parent();

COMMENT ON TRIGGER slips_exactly_one_parent_trg ON slips IS
  'Enforces exactly-one of (expense_id, pr_id, po_id). '
  'DEFERRABLE so orphan-then-link patterns within a single transaction '
  'still commit successfully; permanent orphans (slips committed with '
  'zero parents, or with multiple parents) are rejected.';

COMMIT;

-- Verify -----------------------------------------------------------------------
SELECT id, expense_id, pr_id, po_id FROM slips ORDER BY id;
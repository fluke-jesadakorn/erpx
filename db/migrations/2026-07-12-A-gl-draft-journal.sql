-- folio/db/migrations/2026-07-12-A-gl-draft-journal.sql
--
-- journal_entries staging draft: one draft row per expense created at
-- submission, recomputed on every expense_items / slips change, finalized
-- (is_draft=false, finalized_at=now()) at final approval. Lets the waybill
-- page show GL preview lines from submission onwards.

BEGIN;

ALTER TABLE journal_entries
  ADD COLUMN IF NOT EXISTS is_draft       BOOLEAN     NOT NULL DEFAULT FALSE,
  ADD COLUMN IF NOT EXISTS finalized_at   TIMESTAMP,
  ADD COLUMN IF NOT EXISTS finalized_by   INT         REFERENCES users(id),
  ADD COLUMN IF NOT EXISTS draft_source   TEXT        NOT NULL DEFAULT 'expense';

CREATE UNIQUE INDEX IF NOT EXISTS idx_journal_entries_one_draft_per_expense
  ON journal_entries(expense_id)
  WHERE is_draft = TRUE AND expense_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_journal_entries_active
  ON journal_entries(expense_id)
  WHERE is_draft = FALSE;

INSERT INTO perm.audit (kind, target) VALUES (
  'schema.migration',
  jsonb_build_object(
    'migration', '2026-07-12-A-gl-draft-journal',
    'changes', jsonb_build_array(
      'journal_entries.is_draft',
      'journal_entries.finalized_at',
      'journal_entries.finalized_by',
      'journal_entries.draft_source'
    )
  )
);

COMMIT;
-- db/2026-07-16-H-journal-entries-so-id-fixup.sql
--
-- Add so_id column to journal_entries (was missed in 2026-07-16-A).
-- Also add the partial-unique-index that mirrors expense/pr/po pattern.
--
-- Run with:
--   PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db \
--     -v ON_ERROR_STOP=1 -f folio/db/2026-07-16-H-journal-entries-so-id-fixup.sql

BEGIN;

ALTER TABLE journal_entries ADD COLUMN IF NOT EXISTS so_id integer;
ALTER TABLE journal_entries ADD CONSTRAINT journal_entries_so_id_fkey
  FOREIGN KEY (so_id) REFERENCES sales_orders(id) ON DELETE RESTRICT;

CREATE INDEX IF NOT EXISTS idx_journal_entries_so_id
  ON journal_entries (so_id);

CREATE INDEX IF NOT EXISTS idx_journal_entries_one_draft_per_so
  ON journal_entries (so_id, step)
  WHERE is_draft = true AND so_id IS NOT NULL;

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-16-H-journal-entries-so-id-fixup',
    'description', 'Added so_id column + partial unique index for sales journals'
  )
);

COMMIT;

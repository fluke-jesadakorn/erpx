-- db/2026-07-04-B-drop-reports-to-column.sql
-- Drop users.reports_to_user_id. The org tree is now derived from
-- perm.roles (level, sort_order) + users.dept_group_id + users.staff_level.
-- The column has been HR-managed historically; no production app writes to it
-- anymore. The org chart in SignInPanel reads derived_manager_id from
-- /api/actor/users (computed in app/src/lib/orgTree.ts).

BEGIN;

\echo '--- pre: column + index + constraint status ---'
SELECT column_name FROM information_schema.columns
 WHERE table_schema='public' AND table_name='users' AND column_name='reports_to_user_id';
SELECT indexname FROM pg_indexes
 WHERE schemaname='public' AND tablename='users' AND indexname='idx_users_reports_to';
SELECT conname FROM pg_constraint
 WHERE conname='users_no_self_report';

\echo '--- dropping CHECK constraint, index, column ---'
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_no_self_report;
DROP INDEX IF EXISTS idx_users_reports_to;
ALTER TABLE users DROP COLUMN IF EXISTS reports_to_user_id;

\echo '--- post: column + index + constraint status (expect nothing) ---'
SELECT column_name FROM information_schema.columns
 WHERE table_schema='public' AND table_name='users' AND column_name='reports_to_user_id';
SELECT indexname FROM pg_indexes
 WHERE schemaname='public' AND tablename='users' AND indexname='idx_users_reports_to';
SELECT conname FROM pg_constraint
 WHERE conname='users_no_self_report';

\echo '--- audit row: column dropped ---'
INSERT INTO perm.audit (kind, actor, target)
  VALUES (
    'schema.reports_to.dropped',
    'migration-2026-07-04',
    jsonb_build_object(
      'note', 'dropped users.reports_to_user_id + users_no_self_report + idx_users_reports_to; org tree now derived from role+permission+department',
      'wiped_at', now()
    )
  );

COMMIT;

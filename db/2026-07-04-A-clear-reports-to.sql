-- db/2026-07-04-A-clear-reports-to.sql
-- Wipe system-seeded reports-to values. From now on, this column is HR-managed:
--   - via the existing UserEditModal 'Manager (Reports to)' dropdown, or
--   - via direct SQL (the CHECK constraint `users_no_self_report` still blocks self-cycles).
-- The column itself + CHECK constraint + index stay; this migration only clears the data.

BEGIN;

\echo '--- pre: count of users with reports_to_user_id set ---'
SELECT COUNT(*) AS users_with_manager FROM users WHERE reports_to_user_id IS NOT NULL;

\echo '--- pre: distinct manager chain depths ---'
WITH RECURSIVE chain AS (
  SELECT id, fullname, employee_code, reports_to_user_id, 0 AS depth
    FROM users WHERE reports_to_user_id IS NULL AND is_active = TRUE
  UNION ALL
  SELECT u.id, u.fullname, u.employee_code, u.reports_to_user_id, c.depth + 1
    FROM users u JOIN chain c ON u.reports_to_user_id = c.id
   WHERE c.depth < 20
)
SELECT MAX(depth) AS max_chain_depth, COUNT(*) AS users_in_chains FROM chain;

\echo '--- clearing reports_to_user_id on all users ---'
UPDATE users SET reports_to_user_id = NULL;

\echo '--- post: count of users with reports_to_user_id set (expect 0) ---'
SELECT COUNT(*) AS users_with_manager FROM users WHERE reports_to_user_id IS NOT NULL;

\echo '--- post: count of root users (expect = active users) ---'
SELECT COUNT(*) AS root_users FROM users WHERE reports_to_user_id IS NULL AND is_active = TRUE;

\echo '--- audit row: kind=reports_to.clear, actor=migration-2026-07-04 ---'
INSERT INTO perm.audit (kind, actor, target)
  VALUES (
    'reports_to.clear',
    'migration-2026-07-04',
    jsonb_build_object(
      'note', 'cleared all users.reports_to_user_id; HR-managed from now on via /api/users PATCH (audit kind=user.reports_to.set) or direct SQL',
      'wiped_at', now()
    )
  );

COMMIT;

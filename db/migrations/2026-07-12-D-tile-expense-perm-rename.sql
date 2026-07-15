-- folio/db/migrations/2026-07-12-D-tile-expense-perm-rename.sql
--
-- Follow-up to 2026-07-09-B-tile-rename.sql. That migration renamed
-- perm.tiles.id (expense-claim → expense) and request_target
-- (→ tile:expense:view), but never renamed the matching rows in
-- perm.permissions. So users still carry legacy grants
--   tile:submit_expense:view:all
--   tile:approve_expense:view:all
--   tile:expense_claim:view:all
-- while (protected)/expense/page.tsx gates on `tile:expense:view`.
-- hasPermission() returns false → NoPermissionView renders → users
-- report being "kicked back to /".
--
-- This collapses all three legacy subjects into the canonical `expense`
-- subject, re-points role_permissions, and drops the legacy rows.

BEGIN;

-- 1. Ensure the canonical permission row exists.
INSERT INTO perm.permissions (domain, subject, verb, scope, description)
SELECT 'tile', 'expense', 'view', 'all', 'Open Expense tile'
WHERE NOT EXISTS (
  SELECT 1 FROM perm.permissions
   WHERE domain = 'tile' AND subject = 'expense' AND verb = 'view' AND scope = 'all'
);

-- 2. Re-grant to every role that currently holds any legacy expense perm.
--    INSERT ... ON CONFLICT DO NOTHING collapses 3 legacy perms → 1 row.
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT rp.role_id, 'tile:expense:view:all', rp.effect, rp.granted_by
  FROM perm.role_permissions rp
 WHERE rp.permission_id IN (
         'tile:submit_expense:view:all',
         'tile:approve_expense:view:all',
         'tile:expense_claim:view:all'
       )
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- 3. Drop legacy grants + the legacy permission rows (cascades clean any
--    straggler references).
DELETE FROM perm.role_permissions
 WHERE permission_id IN (
         'tile:submit_expense:view:all',
         'tile:approve_expense:view:all',
         'tile:expense_claim:view:all'
       );

DELETE FROM perm.permissions
 WHERE id IN (
         'tile:submit_expense:view:all',
         'tile:approve_expense:view:all',
         'tile:expense_claim:view:all'
       );

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-12-D-tile-expense-perm-rename',
    'renames', jsonb_build_object(
      'tile:submit_expense:view:all',  'tile:expense:view:all',
      'tile:approve_expense:view:all', 'tile:expense:view:all',
      'tile:expense_claim:view:all',   'tile:expense:view:all'
    ),
    'note', 'Collapses 3 legacy tile perms into canonical tile:expense:view:all to match 2026-07-09-B tile rename.'
  )
);

COMMIT;

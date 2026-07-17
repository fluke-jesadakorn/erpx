-- 2026-07-25-A-policy-hard-delete.sql
-- Allow hard-delete of non-seed roles and departments on /policy.
-- Enforce exactly one department per user.

BEGIN;

-- 1. user_roles.role_id: RESTRICT → CASCADE so role hard-delete can clean up members.
ALTER TABLE perm.user_roles
  DROP CONSTRAINT IF EXISTS user_roles_role_id_fkey;
ALTER TABLE perm.user_roles
  ADD CONSTRAINT user_roles_role_id_fkey
  FOREIGN KEY (role_id) REFERENCES perm.roles(id) ON DELETE CASCADE;

-- 2. Pre-clean the 5 live multi-dept users (950, 956, 960, 961, 963).
--    Keep the alphabetically first user:dept:*::allow per user, revoke the rest.
--    Must run BEFORE creating the unique index below.
UPDATE perm.user_permissions
   SET revoked_at = now(),
       revoked_by = 'system.policy_hard_delete'
 WHERE id IN (
   SELECT id FROM (
     SELECT id, ROW_NUMBER() OVER (
       PARTITION BY user_id
       ORDER BY permission_id
     ) AS rn
     FROM perm.user_permissions
     WHERE permission_id LIKE 'user:dept:%::allow'
       AND revoked_at IS NULL
   ) t
   WHERE rn > 1
 );

-- 3. Single-department rule. A user can hold at most one active dept membership.
CREATE UNIQUE INDEX IF NOT EXISTS perm_one_dept_per_user_idx
  ON perm.user_permissions (user_id)
  WHERE permission_id LIKE 'user:dept:%::allow'
    AND revoked_at IS NULL;

COMMIT;

-- Folio — drop legacy level columns.
--
-- The single source of truth for a user's level is the perm family
-- rbac:level:grant:min:N:all granted through perm.role_permissions.
-- Both perm.roles.level and users.staff_level are now redundant.
--
-- A new view perm.user_effective_level(user_id, effective_level) derives
-- the level on demand: MIN(N) over all level-grant perms the user holds,
-- or 10 (lowest authority) if none.

BEGIN;

-- 1. Create the derivation view BEFORE dropping columns so no caller is left dangling.
CREATE OR REPLACE VIEW perm.user_effective_level AS
SELECT u.id AS user_id,
       COALESCE(
         (SELECT MIN(split_part(rp.permission_id, ':', 5)::int)
            FROM perm.user_roles ur
            JOIN perm.role_permissions rp ON rp.role_id = ur.role_id
           WHERE ur.user_id = u.id
             AND rp.effect = 'allow'
             AND rp.permission_id ~ '^rbac:level:grant:min:\d+:all$'),
         10
       ) AS effective_level
  FROM users u;

-- 2. Drop the now-redundant columns.
ALTER TABLE perm.roles DROP COLUMN IF EXISTS level;
ALTER TABLE users   DROP COLUMN IF EXISTS staff_level;

-- 3. Drop the now-broken rbac.effective_staff_level function if it survived.
DROP FUNCTION IF EXISTS rbac.effective_staff_level(smallint, smallint);

COMMENT ON VIEW perm.user_effective_level IS
  'Per-user effective authority level (1 = highest, 10 = lowest). Derived from '
  'rbac:level:grant:min:N:all perms held via perm.user_roles. Replaces the '
  'legacy perm.roles.level and users.staff_level columns.';

COMMIT;
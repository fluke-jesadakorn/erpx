-- folio/db/2026-07-17-A-role-effective-level-view.sql
--
-- 2026-07-11-B dropped perm.roles.level and created perm.user_effective_level
-- (per-user), but never created the per-role counterpart. Many SQL queries
-- across lib/perm + web-admin still referenced the dropped pr.level column,
-- which made hydratePermSession() throw → null session → every hasPermission()
-- returned false ("all permissions fail").
--
-- This adds the missing per-role derivation view, mirroring user_effective_level
-- but keyed on role_id and scanning perm.role_permissions directly.

BEGIN;

CREATE OR REPLACE VIEW perm.role_effective_level AS
SELECT r.id AS role_id,
       COALESCE(
         (SELECT MIN(split_part(rp.permission_id, ':', 5)::int)
            FROM perm.role_permissions rp
           WHERE rp.role_id = r.id
             AND rp.effect = 'allow'
             AND rp.permission_id ~ '^rbac:level:grant:min:\d+:all$'),
         10
       ) AS effective_level
  FROM perm.roles r;

COMMENT ON VIEW perm.role_effective_level IS
  'Per-role effective authority level (1 = highest, 10 = lowest). Derived '
  'from rbac:level:grant:min:N:all perms granted to the role. Mirrors '
  'perm.user_effective_level. Replaces the dropped perm.roles.level column.';

INSERT INTO perm.audit (kind, target)
VALUES (
  'schema.migration',
  jsonb_build_object(
    'migration', '2026-07-17-A-role-effective-level-view',
    'description', 'Add perm.role_effective_level view (per-role counterpart to user_effective_level) to fix queries still referencing the dropped perm.roles.level column'
  )
);

COMMIT;

-- Level as range perm migration.
--
-- Concept:
--   Authority level is encoded as `rbac:level:grant:min:N:all` perms.
--   Lower N = higher authority (1 = CEO, 10 = read-only).
--   A user's effective level = MIN(N) of all `:min:N` perms they hold.
--
-- This migration is additive:
--   1. Seeds 10 level perms
--   2. Grants the appropriate level perm via each role's role_permissions
--   3. perm.roles.level column is kept for now (drop in cleanup step)
--   4. lib/perm/level.ts updated to derive from perms; column kept as fallback

BEGIN;

-- 1. Seed 10 level range perms
INSERT INTO perm.permissions (domain, subject, verb, scope, description)
SELECT 'rbac', 'level', 'grant:min:' || n, 'all',
       'Authority level ' || n || ' (lower N = higher authority; 1=CEO, 10=read-only)'
FROM generate_series(1, 10) AS n
ON CONFLICT DO NOTHING;

-- 2. Grant each level perm via the corresponding role's role_permissions
--    For every role with a non-null level, insert (role, level_perm, allow)
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT pr.id, 'rbac:level:grant:min:' || pr.level || ':all', 'allow', 'migration:0023'
  FROM perm.roles pr
 WHERE pr.level IS NOT NULL
   AND pr.level BETWEEN 1 AND 10
ON CONFLICT (role_id, permission_id) DO NOTHING;

COMMIT;
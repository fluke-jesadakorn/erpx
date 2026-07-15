-- folio/db/2026-07-11-C-drop-broken-rp-sync-trigger.sql
--
-- 2026-07-11-B dropped perm.roles.level but left the AFTER trigger
-- `perm_rp_sync_level` on perm.role_permissions, which still references
-- the removed column. Any INSERT/UPDATE/DELETE on perm.role_permissions
-- now raises:  "column 'level' does not exist" inside sync_role_level_from_perms().
--
-- The trigger is dead code (level is derived on demand from
-- perm.user_effective_level). Drop the trigger and its function.

BEGIN;

DROP TRIGGER IF EXISTS perm_rp_sync_level ON perm.role_permissions;
DROP FUNCTION IF EXISTS perm.sync_role_level_from_perms();

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-11-C-drop-broken-rp-sync-trigger',
    'trigger_dropped', 'perm_rp_sync_level',
    'function_dropped', 'perm.sync_role_level_from_perms',
    'reason', 'referenced perm.roles.level column that no longer exists'
  )
);

COMMIT;

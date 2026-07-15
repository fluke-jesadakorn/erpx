-- 2026-07-10-G: drop workbench tile, perm, and all role grants.
--
-- The /workbench URL is served by the generic [slug] route from the
-- perm.tiles row 'workbench'. The Waybill system already replaces the
-- legacy expense/PR/PO review queues, so the IT read-only overview view
-- is no longer useful. After this migration the tile row is gone and the
-- 'tile:workbench:view:all' permission is gone (no grants left for any role).
--
-- Idempotent: every DELETE uses unqualified WHERE clauses on stable ids;
-- running twice is a no-op.
--
-- perm_rp_sync_level (trigger on perm.role_permissions) is disabled for
-- the duration of this migration because it references a `level` column
-- that does not exist in this schema. The trigger remains present and is
-- re-enabled at the end; fixing the broken trigger body is out of scope
-- for this workbench-removal migration.

BEGIN;

ALTER TABLE perm.role_permissions DISABLE TRIGGER perm_rp_sync_level;

DELETE FROM perm.role_permissions
 WHERE permission_id IN (
  'tile:workbench:view',
  'tile:workbench:view:all'
);

DELETE FROM perm.permissions WHERE id IN (
  'tile:workbench:view',
  'tile:workbench:view:all'
);

DELETE FROM perm.tiles WHERE id = 'workbench';

ALTER TABLE perm.role_permissions ENABLE TRIGGER perm_rp_sync_level;

COMMIT;

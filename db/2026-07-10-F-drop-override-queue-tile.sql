-- folio/db/2026-07-10-F-drop-override-queue-tile.sql
--
-- Remove the /override-queue tile.
-- User only wants expense / pr / gl (ledger) tiles; the CEO override UI is out of scope.
-- The cockpit tile (which also exposes a per-row override button) is kept —
-- it still serves the cross-slice executive view, just without a dedicated queue page.
--
-- Cleans up:
--   * perm.tiles row                   (id = 'override-queue')
--   * perm.permissions view perm       (id = 'tile:override_queue:view:all')
--   * perm.role_permissions grants     (none exist, but defensive)
--   * perm.user_permissions grants     (none exist, but defensive)
--
-- Re-add by running the inverse:
--   INSERT INTO perm.tiles (id, display_name, subtitle, icon, accent,
--                           group_name, sub_view, href, request_target, sort_order)
--   VALUES ('override-queue', 'Approval Override', 'Force approve / reject',
--           '⚡', 'rose', 'cockpit', 'override', '/override-queue',
--           'tile:override-queue:view', 220);
--   INSERT INTO perm.permissions (id, domain, subject, verb, scope, description)
--   VALUES ('tile:override_queue:view:all', 'tile', 'override_queue', 'view', 'all',
--           'View access for the Approval Override tile');

BEGIN;

DELETE FROM perm.user_permissions
 WHERE permission_id = 'tile:override_queue:view:all';

DELETE FROM perm.role_permissions
 WHERE permission_id = 'tile:override_queue:view:all';

DELETE FROM perm.permissions
 WHERE id = 'tile:override_queue:view:all';

DELETE FROM perm.tiles
 WHERE id = 'override-queue';

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-10-F-drop-override-queue-tile',
    'deleted', jsonb_build_object(
      'tiles',          'override-queue',
      'permissions',    'tile:override_queue:view:all'
    )
  )
);

COMMIT;

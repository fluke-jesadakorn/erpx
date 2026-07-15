-- folio/db/migrations/2026-07-12-C-inbox-tile.sql
--
-- Inbox tile: personal hub for "waiting on me" + "watching" + "all" scopes,
-- backed by /inbox. Replaces the previous /my-waybills redirect-to-/expense
-- once the new page renders.
--
-- Default-open: required_level = NULL, required_dept_id = NULL so any
-- authenticated user (no level/dept gate) sees the tile. The /inbox page
-- itself is open to all signed-in users; per-stage mutation perms still
-- gate approval actions on /waybill/{id}.

BEGIN;

INSERT INTO perm.tiles
  (id, display_name, subtitle, icon, accent, group_name, sub_view, href,
   request_target, sort_order, is_system)
VALUES
  ('inbox', 'Inbox', 'Approvals · watches · notifications',
   '📥', 'cyan', 'workflow', 'inbox', '/inbox',
   'tile:inbox:view', 10, true)
ON CONFLICT (id) DO UPDATE SET
  display_name   = EXCLUDED.display_name,
  subtitle       = EXCLUDED.subtitle,
  icon           = EXCLUDED.icon,
  accent         = EXCLUDED.accent,
  group_name     = EXCLUDED.group_name,
  sub_view       = EXCLUDED.sub_view,
  href           = EXCLUDED.href,
  request_target = EXCLUDED.request_target,
  sort_order     = EXCLUDED.sort_order;

INSERT INTO perm.permissions (domain, subject, verb, scope, description)
SELECT 'tile', 'inbox', 'view', 'all', 'Open Inbox tile'
WHERE NOT EXISTS (
  SELECT 1 FROM perm.permissions
   WHERE domain = 'tile' AND subject = 'inbox' AND verb = 'view'
);

INSERT INTO perm.audit (kind, actor, target)
VALUES
  ('tile.create', 'migration-2026-07-12',
   jsonb_build_object(
     'id', 'inbox',
     'href', '/inbox',
     'request_target', 'tile:inbox:view',
     'group', 'workflow',
     'sort_order', 10,
     'note', 'default-open (required_level NULL, required_dept_id NULL)'
   ));

COMMIT;

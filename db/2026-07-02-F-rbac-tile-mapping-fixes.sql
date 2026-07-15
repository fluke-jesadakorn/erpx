-- 2026-07-02-F: rbac tile mapping fixes (audit 2026-07-02)
-- Pure data fix; no schema changes. Idempotent.
--
-- Findings from deep audit (see plan in chat):
--   B1: 4 tile-modules had no module_group entry → unreachable for persona
--       roles. Fix: add the missing rows so group cascade can resolve.
--   B2: 'permissions' tile pointed at module_id=tile-org-chart (same as
--       'org-chart' tile). Fix: re-route to the actual matrix editor.
--   B3: L4 blanket-allow let 'it' and 'finance' inherit tiles they should
--       not see (cockpit, summary, override-queue). Fix: explicit deny
--       rows override the inherited allow.

BEGIN;

-- B1: 4 orphaned tile-modules gain module_group membership.
INSERT INTO rbac.module_groups (module_id, group_id)
VALUES
  ('tile-summary',         'grp-cockpit'),
  ('rbac-visibility',      'grp-it'),
  ('view-hook-events',     'grp-it'),
  ('manage-hook-events',   'grp-it')
ON CONFLICT DO NOTHING;

-- B2: 'permissions' tile re-routed from the org-chart module to the
--     actual RBAC matrix editor module.
UPDATE rbac.tiles
   SET module_id = 'rbac-edit-matrix'
 WHERE id = 'permissions';

-- B3: explicit DENY rows so L4 children (it, finance) do not see
--     the tiles they shouldn't inherit. deny wins over inherited allow.
INSERT INTO rbac.permissions (role_id, module_id, action, state, updated_by)
VALUES
  ('it',      'tile-cockpit',        'read', 'deny', 'fix-rbac-mapping-2026-07-02'),
  ('it',      'tile-summary',        'read', 'deny', 'fix-rbac-mapping-2026-07-02'),
  ('it',      'tile-override-queue', 'read', 'deny', 'fix-rbac-mapping-2026-07-02'),
  ('finance', 'tile-summary',        'read', 'deny', 'fix-rbac-mapping-2026-07-02'),
  ('finance', 'tile-override-queue', 'read', 'deny', 'fix-rbac-mapping-2026-07-02')
ON CONFLICT DO NOTHING;

COMMIT;

-- Verify
SELECT 'B1 module_groups' AS check, COUNT(*)::text AS n
  FROM rbac.module_groups
 WHERE module_id IN ('tile-summary','rbac-visibility','view-hook-events','manage-hook-events')
UNION ALL
SELECT 'B2 permissions tile module', module_id
  FROM rbac.tiles WHERE id = 'permissions'
UNION ALL
SELECT 'B3 deny rows', COUNT(*)::text
  FROM rbac.permissions
 WHERE updated_by = 'fix-rbac-mapping-2026-07-02';
-- Folio — relax over-strict tile-gate defaults + surface the new /tiles
-- admin tool as a Hub tile.
--
-- Background: 0031_tile_access_gates.sql backfilled required_level from
-- `MIN(users.staff_level) + 1` of role-holders, which produced required_level=2
-- for nearly every non-workflow tile. That's too strict — managers (L3) couldn't
-- see ledger, cockpit, policy, audit, etc. This migration:
--   1. Bumps non-workflow L2 tiles to L3 (manager+) so the day-to-day UX works.
--   2. Workflow tiles (submit, my-prs, approve, search-coa) become fully public
--      (NULL/NULL) — any staff in any dept can submit. HR can tighten per-tile
--      via /tiles if they want dept-scoped workflows.
--   3. Adds a `tile-gates` Hub tile under the HR group so admins can find /tiles
--      without typing the URL. Gate = L3, any dept.
--
-- Reversible: revert perm.tiles to prior values; the tile-gates row
-- can be removed with: DELETE FROM perm.tiles WHERE id = 'tile-gates';

BEGIN;

-- 1a. Workflow tiles → fully public (any level, any dept)
UPDATE perm.tiles
   SET required_level    = NULL,
       required_dept_id  = NULL
 WHERE group_name IN ('workflow','workflow-approval','workflow-procurement');

-- 1b. Non-workflow L2 tiles → L3 (manager and above)
UPDATE perm.tiles
   SET required_level = 3
 WHERE required_level = 2
   AND group_name NOT IN ('workflow','workflow-approval','workflow-procurement');

-- 2. Surface the /tiles admin tool as an HR-group Hub tile
INSERT INTO perm.tiles (
  id, display_name, subtitle, icon, accent, group_name,
  sub_view, href, request_target, sort_order, is_system,
  required_permission, required_level, required_dept_id
) VALUES (
  'tile-gates',
  'Tile Gates',
  'Per-tile (level + dept) requirement editor',
  '🛡',
  'indigo',
  'hr',
  NULL,
  '/tiles',
  'hr_manager',
  292,
  true,
  'tile:audit:view:all',
  3,
  NULL
) ON CONFLICT (id) DO UPDATE SET
  display_name        = EXCLUDED.display_name,
  subtitle            = EXCLUDED.subtitle,
  icon                = EXCLUDED.icon,
  accent              = EXCLUDED.accent,
  group_name          = EXCLUDED.group_name,
  href                = EXCLUDED.href,
  request_target      = EXCLUDED.request_target,
  sort_order          = EXCLUDED.sort_order,
  required_permission = EXCLUDED.required_permission,
  required_level      = EXCLUDED.required_level,
  required_dept_id    = EXCLUDED.required_dept_id,
  updated_at          = now();

COMMIT;

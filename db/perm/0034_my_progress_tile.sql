-- Folio — register the "My Progress" tile.
--
-- Required level 2 so all supervisors/managers/officers can see it.
-- No required_dept_id — anyone can review their own career path.
-- Links to /my-progress (a dedicated server-rendered route, not the
-- generic [slug] tile page).

BEGIN;

INSERT INTO perm.tiles (
  id, display_name, subtitle, icon, accent, group_name, sub_view, href,
  required_level, required_dept_id,
  request_target, sort_order, is_system
)
VALUES (
  'my-progress',
  'My Progress',
  'Promotion checklist — level, dept, tenure, training, sign-off.',
  '📈',
  'emerald',
  'policy',
  NULL,
  '/my-progress',
  NULL,
  NULL,
  'hr_manager',
  95,
  true
)
ON CONFLICT (id) DO UPDATE SET
  display_name = EXCLUDED.display_name,
  subtitle     = EXCLUDED.subtitle,
  icon         = EXCLUDED.icon,
  accent       = EXCLUDED.accent,
  group_name   = EXCLUDED.group_name,
  href         = EXCLUDED.href,
  required_level = EXCLUDED.required_level,
  request_target = EXCLUDED.request_target,
  sort_order   = EXCLUDED.sort_order;

COMMIT;
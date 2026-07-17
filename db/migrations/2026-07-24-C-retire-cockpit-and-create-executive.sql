-- folio/db/migrations/2026-07-24-C-retire-cockpit-and-create-executive.sql
--
-- Retire the cockpit tile and route, introduce the Executive tile/route.
-- The cockpit tile row is kept (id='cockpit') so existing references survive;
-- only its href, perm, display_name and subtitle are repointed to /executive.

BEGIN;

-- (1) Add the new executive view perm.
INSERT INTO perm.permissions (id, description)
VALUES ('tile:executive:view::allow', 'Open the Executive overview page')
ON CONFLICT (id) DO NOTHING;

-- (2) Grant it to ceo/cfo/admin (already hold finance:report:executive::allow
--     per 9001-seed-new-grammar, but tile:executive:view is its own gate).
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'migration-2026-07-24-C'
  FROM perm.roles r
 CROSS JOIN perm.permissions p
 WHERE split_part(r.id, '::', 1) IN ('ceo','cfo','admin')
   AND p.id = 'tile:executive:view::allow'
ON CONFLICT DO NOTHING;

-- (3) Create the executive tile row.
INSERT INTO perm.tiles (id, display_name, subtitle, icon, accent, group_name,
                        href, sort_order, is_system, view_perm_id)
VALUES ('executive', 'Executive', 'Org-wide pipeline · KPIs · budget',
        '⭐', 'amber', 'exec', '/executive', 5, true,
        'tile:executive:view::allow')
ON CONFLICT (id) DO UPDATE SET
  display_name = EXCLUDED.display_name,
  subtitle     = EXCLUDED.subtitle,
  icon         = EXCLUDED.icon,
  accent       = EXCLUDED.accent,
  group_name   = EXCLUDED.group_name,
  href         = EXCLUDED.href,
  sort_order   = EXCLUDED.sort_order,
  view_perm_id = EXCLUDED.view_perm_id;

-- (4) Repoint the existing cockpit tile to executive so any legacy
--     row references keep resolving.
UPDATE perm.tiles
   SET display_name = 'Executive',
       subtitle     = 'Org-wide pipeline · KPIs · budget',
       href         = '/executive',
       view_perm_id = 'tile:executive:view::allow'
 WHERE id = 'cockpit';

-- (5) Audit trail.
INSERT INTO perm.audit (kind, actor, target) VALUES
  ('tile.migrate', 'migration-2026-07-24-C',
   jsonb_build_object('from','cockpit','to','executive',
     'href','/executive','view_perm_id','tile:executive:view::allow',
     'note','cockpit tile row preserved for backward compatibility'));

COMMIT;
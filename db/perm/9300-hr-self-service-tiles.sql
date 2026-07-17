-- 9300-hr-self-service-tiles.sql
-- Self-service perms/tile for every folio user (their own leave).
-- HR review/authorization stages match the new hr_leave waybill pipeline.

BEGIN;

INSERT INTO perm.permissions (id, description) VALUES
  ('hr:leave:submit::allow',           'Submit your own leave request'),
  ('tile:me_leave:view::allow',        'View your own leave tile'),
  ('stage:hr_review:act::allow',       'Act on hr_leave waybills at hr_review stage'),
  ('stage:hr_authorization:act::allow','Act on hr_leave waybills at hr_authorization stage'),
  ('stage:hr_disbursed:act::allow',    'Act on hr_leave waybills at hr_disbursed stage')
ON CONFLICT (id) DO NOTHING;

-- Self-service tile visible to every authenticated folio user.
INSERT INTO perm.tiles (id, display_name, subtitle, icon, accent, group_name,
                        href, sort_order, is_system, view_perm_id) VALUES
  ('me_leave', 'My Leave', 'Quota · history · request', '📅', 'rose', 'self',
   '/me/leave', 50, true, 'tile:me_leave:view::allow')
ON CONFLICT (id) DO UPDATE SET
  display_name = EXCLUDED.display_name,
  subtitle     = EXCLUDED.subtitle,
  href         = EXCLUDED.href,
  view_perm_id = EXCLUDED.view_perm_id;

-- Grant self-service perms to every existing role.
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'seed'
  FROM perm.roles r
  JOIN (VALUES
    ('hr:leave:submit::allow'),
    ('tile:me_leave:view::allow')
  ) AS p(id) ON true
ON CONFLICT DO NOTHING;

-- HR review/authorization grants.
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'seed'
  FROM perm.roles r
  JOIN (VALUES
    ('hr::5',         'stage:hr_review:act::allow'),
    ('hr::5',         'stage:hr_disbursed:act::allow'),
    ('hr_manager::3', 'stage:hr_review:act::allow'),
    ('hr_manager::3', 'stage:hr_authorization:act::allow'),
    ('hr_manager::3', 'stage:hr_disbursed:act::allow')
  ) AS m(role_id, permission_id) ON m.role_id = r.id
  JOIN perm.permissions p ON p.id = m.permission_id
ON CONFLICT DO NOTHING;

COMMIT;

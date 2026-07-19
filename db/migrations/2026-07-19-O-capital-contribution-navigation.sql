BEGIN;

INSERT INTO perm.permissions (id, description)
VALUES ('tile:capital:view::allow', 'Open the Capital Contributions workspace')
ON CONFLICT (id) DO UPDATE
SET description = excluded.description;

INSERT INTO perm.tiles (
  id, display_name, subtitle, icon, accent, group_name, sub_view, href,
  request_target, sort_order, is_system, owner_group_id, view_perm_id
)
VALUES (
  'capital',
  'Capital Contributions',
  'CEO funding · Finance verification',
  '🏦',
  'emerald',
  'finance',
  NULL,
  '/capital',
  NULL,
  115,
  TRUE,
  NULL,
  'tile:capital:view::allow'
)
ON CONFLICT (id) DO UPDATE
SET display_name = excluded.display_name,
    subtitle = excluded.subtitle,
    icon = excluded.icon,
    accent = excluded.accent,
    group_name = excluded.group_name,
    href = excluded.href,
    sort_order = excluded.sort_order,
    is_system = excluded.is_system,
    view_perm_id = excluded.view_perm_id,
    updated_at = now();

INSERT INTO perm.role_permissions (role_id, role_kind, permission_id, granted_by, significance)
SELECT r.id, r.kind, 'tile:capital:view::allow', 'capital-navigation', FALSE
  FROM perm.roles r
 WHERE r.id IN (
   'ceo', 'cfo',
   'accounting_manager', 'accounting_supervisor', 'accounting_officer',
   'finance_manager', 'finance_supervisor', 'finance_officer'
 )
ON CONFLICT (role_id, permission_id) DO NOTHING;

COMMIT;

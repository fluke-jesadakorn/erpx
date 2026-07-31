BEGIN;

INSERT INTO perm.permissions (id, description)
VALUES (
  'tile:operations:view::allow',
  'Open the cross-functional Operations command center'
)
ON CONFLICT (id) DO UPDATE
SET description = excluded.description;

INSERT INTO perm.tiles (
  id, display_name, subtitle, icon, accent, group_name, sub_view, href,
  request_target, sort_order, is_system, owner_group_id, view_perm_id
)
VALUES (
  'operations',
  'Operations',
  'Sales · purchasing · inventory · fulfillment',
  '🚚',
  'cyan',
  'work',
  NULL,
  '/operations',
  NULL,
  45,
  TRUE,
  NULL,
  'tile:operations:view::allow'
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

INSERT INTO perm.role_permissions (
  role_id, role_kind, permission_id, granted_by, significance
)
SELECT
  r.id,
  r.kind,
  'tile:operations:view::allow',
  'operations-command-center',
  FALSE
FROM perm.roles r
WHERE r.id IN (
  'ceo',
  'cfo',
  'accounting_manager',
  'accounting_supervisor',
  'finance_manager',
  'finance_supervisor',
  'it_manager',
  'it_supervisor'
)
ON CONFLICT (role_id, permission_id) DO NOTHING;

COMMIT;

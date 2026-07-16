INSERT INTO perm.permissions (id, description) VALUES
  ('tile:chat:view::allow', 'Open the full AI Chat page'),
  ('ai:chat:full::allow',  'Use the full AI Chat (HTML + SQL)')
ON CONFLICT (id) DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'migration-9008-baseline'
  FROM perm.roles r
  CROSS JOIN perm.permissions p
  WHERE p.id IN ('tile:chat:view::allow', 'ai:chat:full::allow')
ON CONFLICT DO NOTHING;

INSERT INTO perm.tiles (id, display_name, subtitle, icon, accent, group_name, href, sort_order, view_perm_id)
VALUES
  ('chat',      'AI Chat',     'Full AI assistant (SQL + HTML + charts)', '✨',     'indigo', 'work', '/chat',     25, 'tile:chat:view::allow'),
  ('sql-quick', 'Quick SQL',   'Hardcoded read-only SQL on expense',     '🔍', 'cyan',   'work', '/expense',  26, 'tile:expense:view::allow')
ON CONFLICT (id) DO UPDATE SET
  display_name = EXCLUDED.display_name,
  subtitle     = EXCLUDED.subtitle,
  icon         = EXCLUDED.icon,
  accent       = EXCLUDED.accent,
  group_name   = EXCLUDED.group_name,
  href         = EXCLUDED.href,
  sort_order   = EXCLUDED.sort_order,
  view_perm_id = EXCLUDED.view_perm_id;
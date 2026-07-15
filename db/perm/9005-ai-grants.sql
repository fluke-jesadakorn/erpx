-- 9005-ai-grants.sql
-- Seed granular ai:* grants to it::2 role + baseline ai:chat:use::allow to all personas.

INSERT INTO perm.permissions (id, description) VALUES
  ('ai:chat:use::allow',         'Use AI chat (per-tile)'),
  ('finance:sales:settle::allow','Settle sales receipt (attach AR)')
ON CONFLICT (id) DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'migration-9005'
  FROM perm.roles r
  CROSS JOIN perm.permissions p
 WHERE p.id LIKE 'ai:%::allow'
   AND split_part(r.id, '::', 1) = 'it'
ON CONFLICT DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, 'ai:chat:use::allow', 'migration-9005-baseline'
  FROM perm.roles r
ON CONFLICT DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, 'finance:sales:settle::allow', 'migration-9005'
  FROM perm.roles r
 WHERE split_part(r.id, '::', 1) IN ('sales_rep','sales_supervisor','sales_manager','admin')
ON CONFLICT DO NOTHING;
-- Open sales orders to every role.
-- Grants tile:sales:view + the three finance:sales:* action perms + stage:so_sales_review:act
-- to every role that doesn't already have them.

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'system:open-sales'
  FROM perm.roles r
  CROSS JOIN perm.permissions p
 WHERE p.id IN (
   'tile:sales:view::allow',
   'finance:sales:submit::allow',
   'finance:sales:settle::allow',
   'finance:sales:invoice::allow',
   'stage:so_sales_review:act::allow'
 )
ON CONFLICT (role_id, permission_id) DO NOTHING;
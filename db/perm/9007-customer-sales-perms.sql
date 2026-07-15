-- 9007-customer-sales-perms.sql
INSERT INTO perm.permissions (id, description) VALUES
  ('customer:manage::allow',         'Create/update/blacklist customers'),
  ('finance:gl:view::allow',         'View GL lines (read-only)'),
  ('finance:gl:confirm::allow',      'Confirm GL posting'),
  ('finance:gl:post::allow',         'Post GL accrual/settlement'),
  ('finance:sales:submit::allow',    'Submit sales order'),
  ('finance:sales:invoice::allow',   'Issue sales invoice')
ON CONFLICT (id) DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, 'customer:manage::allow', 'migration-9007'
  FROM perm.roles r
 WHERE split_part(r.id, '::', 1) IN ('sales_rep', 'sales_supervisor', 'sales_manager', 'admin')
ON CONFLICT DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'migration-9007'
  FROM perm.roles r
  CROSS JOIN perm.permissions p
 WHERE p.id IN ('finance:gl:view::allow', 'finance:gl:confirm::allow', 'finance:gl:post::allow')
   AND split_part(r.id, '::', 1) IN ('finance', 'account_officer', 'account_supervisor', 'accounting_manager', 'cfo', 'ceo', 'admin')
ON CONFLICT DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, 'finance:sales:submit::allow', 'migration-9007'
  FROM perm.roles r
 WHERE split_part(r.id, '::', 1) IN ('sales_rep', 'sales_supervisor', 'sales_manager', 'finance', 'admin')
ON CONFLICT DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, 'finance:sales:invoice::allow', 'migration-9007'
  FROM perm.roles r
 WHERE split_part(r.id, '::', 1) IN ('sales_supervisor', 'sales_manager', 'finance', 'admin')
ON CONFLICT DO NOTHING;
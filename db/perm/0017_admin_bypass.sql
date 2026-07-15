-- Seed the admin:system:bypass perm and grant it to the admin role.
-- Replaces hardcoded `role === 'admin'` checks throughout the perm layer.

INSERT INTO perm.permissions (id, domain, subject, verb, description)
VALUES ('admin:system:bypass', 'admin', 'system', 'bypass', 'System bypass for admin role (CASL manage all, tile universal access, stage override)')
ON CONFLICT (id) DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'admin', 'admin:system:bypass', 'allow', 'migration:0017_admin_bypass'
  WHERE NOT EXISTS (
    SELECT 1 FROM perm.role_permissions
     WHERE role_id = 'admin' AND permission_id = 'admin:system:bypass'
  );

-- 9400-hr-manager-rbac-matrix-view.sql
-- Grant hr_manager::3 the rbac:matrix:view::allow permission so HR Manager
-- can open the Policy tile, the /roles page, and any other surface gated by
-- the permission matrix viewer.

BEGIN;

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'seed-9400'
  FROM perm.roles r
  JOIN perm.permissions p ON p.id = 'rbac:matrix:view::allow'
 WHERE r.id = 'hr_manager::3'
   AND NOT EXISTS (
     SELECT 1 FROM perm.role_permissions rp
      WHERE rp.role_id = 'hr_manager::3'
        AND rp.permission_id = 'rbac:matrix:view::allow'
   );

COMMIT;
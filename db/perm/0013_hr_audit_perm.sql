-- Folio — grant rbac:audit:view to hr_manager (so the HR Console Audit tab works).

BEGIN;

INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT 'hr_manager', 'rbac:audit:view', 'allow', 'seed-0013'
 WHERE NOT EXISTS (
   SELECT 1 FROM perm.role_permissions
    WHERE role_id = 'hr_manager' AND permission_id = 'rbac:audit:view'
 );

COMMIT;
-- Folio — seed perm.user_roles from users.rbac_role_id.
-- Each user gets one role from their current rbac_role_id.
-- If a user has roles at different legacy levels (e.g. an admin who is
-- also an IT), we expand into multiple perm.user_roles rows so the new
-- system supports multi-role per user natively.

BEGIN;

INSERT INTO perm.user_roles (user_id, role_id, granted_by)
SELECT id, rbac_role_id, 'seed-0005'
FROM users
WHERE rbac_role_id IS NOT NULL
ON CONFLICT DO NOTHING;

COMMIT;

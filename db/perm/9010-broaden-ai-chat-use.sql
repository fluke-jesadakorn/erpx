-- 9010-broaden-ai-chat-use.sql
-- Close the gap so every role (including counsel, law, and dept-* zone markers)
-- has ai:chat:use::allow and the baseline system:authenticated:view marker.
-- Idempotent.

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, 'ai:chat:use::allow', 'migration-9010-baseline'
  FROM perm.roles r
  WHERE split_part(r.id, '::', 1) IN ('counsel', 'law')
     OR r.id LIKE 'dept-%'
ON CONFLICT DO NOTHING;

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, 'system:authenticated:view::allow', 'migration-9010-baseline'
  FROM perm.roles r
  WHERE split_part(r.id, '::', 1) IN ('counsel', 'law')
     OR r.id LIKE 'dept-%'
ON CONFLICT DO NOTHING;
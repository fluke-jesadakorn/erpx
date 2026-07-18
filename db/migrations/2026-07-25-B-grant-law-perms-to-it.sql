-- 2026-07-25-B-grant-law-perms-to-it.sql
-- Grant IT officers (`it::2`) the full Law module permission set so they can
-- operate /law, /law/upload, /law/admin alongside the existing counsel role.
-- Idempotent.
BEGIN;

INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'seed.it_law_grants'
  FROM perm.roles r
  JOIN (VALUES
    ('tile:law:view::allow'),
    ('tile:law_admin:view::allow'),
    ('tile:law_upload:view::allow'),
    ('law:contract:read::allow'),
    ('law:contract:list::allow'),
    ('law:contract:upload::allow'),
    ('law:contract:update::allow'),
    ('law:contract:delete::allow'),
    ('law:preview:read::allow'),
    ('law:chunk:read::allow'),
    ('law:rag:query::allow'),
    ('law:admin:stats::allow'),
    ('law:admin:reindex::allow'),
    ('law:admin:retention_set::allow')
  ) AS p(id) ON true
 WHERE r.id = 'it::2'
ON CONFLICT DO NOTHING;

COMMIT;

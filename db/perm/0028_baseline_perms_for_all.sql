-- Folio — baseline "submit-own" permission bundle for every system role.
-- Grants the obvious user-defaults (expense create/view_own, PR create, staff tiles,
-- org chart) to any role that does not yet have them. Tagged granted_by='baseline-0028'
-- so the batch is reversible with a single DELETE.
--
-- Why this exists: folio/db/perm/0004_seed_role_permissions.sql used unsuffixed
-- permission IDs (e.g. finance:expense:create) but the live perm.permissions table
-- stores scope-suffixed IDs (finance:expense:create:all). The seed's WHERE p.id IN (...)
-- matched zero rows, leaving manager / supervisor / accountant / hr / etc. without
-- the ability to submit expenses — runtime guard at lib/server/requireActionFor.ts:59
-- threw "access matrix disallows finance:expense:create".
--
-- Idempotent: re-running this file is a no-op once every role has the bundle.
-- Reversible:
--   DELETE FROM perm.role_permissions WHERE granted_by = 'baseline-0028';

BEGIN;

INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT r.id, p.id, 'allow', 'baseline-0028'
FROM perm.roles r
CROSS JOIN (VALUES
  ('finance:expense:create:all'),
  ('finance:expense:view_own:all'),
  ('finance:pr:create:all'),
  ('tile:submit_expense:view:all'),
  ('tile:my_history:view:all'),
  ('tile:my_prs:view:all'),
  ('tile:dash_staff:view:all'),
  ('tile:search_coa:view:all'),
  ('tile:org_chart:view:all')
) AS p(id)
LEFT JOIN perm.role_permissions rp
  ON rp.role_id = r.id AND rp.permission_id = p.id
WHERE rp.permission_id IS NULL
  AND r.is_system = true
ON CONFLICT (role_id, permission_id) DO NOTHING;

COMMIT;
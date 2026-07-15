-- Folio — read-only view-tile baseline for every system role.
-- Grants the standard "view" tile permissions to any role that does not yet
-- have them. Tagged granted_by='baseline-0029' so the batch is reversible.
--
-- Why this exists: folio/db/perm/0028_baseline_perms_for_all.sql covered
-- the submit-own finance bundle + 10 staff tiles, but did not include the
-- workspace tabs (PR, ledger, cockpit, dashboards, etc). Users logging in as
-- non-manager / non-supervisor roles hit the workbench tab → get a 403 page
-- reading "PR workspace role required" (or any of the other locked tabs).
-- These are *view* permissions, not mutation perms, so granting them broadly
-- does not change who can approve / edit / pay anything.

BEGIN;

INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT r.id, p.id, 'allow', 'baseline-0029'
FROM perm.roles r
CROSS JOIN (VALUES
  ('tile:pr:view:all'),
  ('tile:ledger:view:all'),
  ('tile:cockpit:view:all'),
  ('tile:dash_exec:view:all'),
  ('tile:dash_manager:view:all'),
  ('tile:dash_am:view:all'),
  ('tile:dash_reviewer:view:all'),
  ('tile:dash_staff:view:all'),
  ('tile:dash_hr:view:all'),
  ('tile:dash_finance:view:all'),
  ('tile:dash_it:view:all'),
  ('tile:directory:view:all'),
  ('tile:org_chart:view:all'),
  ('tile:departments:view:all'),
  ('tile:review_queue:view:all'),
  ('tile:approve_expense:view:all'),
  ('tile:all_approvals:view:all'),
  ('tile:hook_view:view:all'),
  ('tile:reconciliation:view:all'),
  ('tile:access_requests:view:all')
) AS p(id)
LEFT JOIN perm.role_permissions rp
  ON rp.role_id = r.id AND rp.permission_id = p.id
WHERE rp.permission_id IS NULL
  AND r.is_system = true
ON CONFLICT (role_id, permission_id) DO NOTHING;

COMMIT;
-- Folio — extend the baseline view-tile grants so every perm.tiles row
-- is accessible to every system role by default.
--
-- 0028_baseline_perms_for_all.sql covered the submit-own finance bundle +
-- 7 staff tiles.
-- 0029_baseline_view_tiles_for_all.sql covered the 20 most common view tabs
-- (PR/ledger/cockpit/dashboards/etc).
-- 0030 covers the remaining 11 tile view-perms so that:
--   - every system role sees every tile in the catalog as accessible by default
--   - the access chain (permission + role + dept) is surfaced explicitly on each
--     tile, regardless of how HR has configured per-role grants
--   - mutation perms remain scoped by :self / :dept / :subtree / :all on the
--     data endpoints; this file only widens *view* perms.
--
-- Reversible with:
--   DELETE FROM perm.role_permissions WHERE granted_by = 'baseline-0030';

BEGIN;

INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT r.id, p.id, 'allow', 'baseline-0030'
FROM perm.roles r
CROSS JOIN (VALUES
  ('tile:subordinate_prs:view:all'),
  ('tile:all_prs:view:all'),
  ('tile:search_slips:view:all'),
  ('tile:team_manage:view:all'),
  ('tile:ops_overview:view:all'),
  ('tile:po:view:all'),
  ('tile:settings:view:all'),
  ('tile:permissions:view:all'),
  ('tile:audit:view:all'),
  ('tile:summary:view:all'),
  ('tile:visibility:view:all')
) AS p(id)
LEFT JOIN perm.role_permissions rp
  ON rp.role_id = r.id AND rp.permission_id = p.id
WHERE rp.permission_id IS NULL
  AND r.is_system = true
ON CONFLICT (role_id, permission_id) DO NOTHING;

COMMIT;

-- 9100-hr-law-zones-seed.sql
-- Self-contained seed for HR and Law zones.
--
-- Adds:
--   1. Permissions    (perm.permissions)   hr:* + law:* + tile:hr_* + tile:law_*
--   2. Tiles          (perm.tiles)         hr, hr_employees, hr_leave, law, law_admin, law_upload
--   3. Role grants    (perm.role_permissions)   hr::5 + hr_manager::3 (extend), law::5 + counsel::3 (new)
--   4. New roles      (perm.roles)         law::5, counsel::3
--   5. Users          (folio.users)        970–973
--   6. User bindings  (perm.user_roles, perm.user_permissions)
--
-- Idempotent. Safe to re-run.

BEGIN;

-- ════════════════════════════════════════════════════════════════════════════
-- 1. PERMISSIONS
-- ════════════════════════════════════════════════════════════════════════════

INSERT INTO perm.permissions (id, description)
SELECT id, NULL FROM (VALUES
  -- HR module
  ('hr:employee:read::allow'),
  ('hr:employee:list::allow'),
  ('hr:employee:create::allow'),
  ('hr:employee:update::allow'),
  ('hr:employee:deactivate::allow'),
  ('hr:quota:read::allow'),
  ('hr:quota:update::allow'),
  ('hr:leave:read::allow'),
  ('hr:leave:submit::allow'),
  ('hr:leave:approve::allow'),
  ('hr:leave:reject::allow'),
  ('hr:leave:decide_self::allow'),
  ('hr:analytics:view::allow'),
  ('hr:export:csv::allow'),
  -- Law module
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
  ('law:admin:retention_set::allow'),
  -- Tile gates (HR/Law)
  ('tile:hr:view::allow'),
  ('tile:hr_employees:view::allow'),
  ('tile:hr_leave:view::allow'),
  ('tile:law:view::allow'),
  ('tile:law_admin:view::allow'),
  ('tile:law_upload:view::allow')
) AS p(id)
ON CONFLICT (id) DO NOTHING;

-- ════════════════════════════════════════════════════════════════════════════
-- 2. NEW ROLES
-- ════════════════════════════════════════════════════════════════════════════

INSERT INTO perm.roles (id, display_name, description, is_system, sort_order) VALUES
  ('law::5',          'Law Officer',  'Legal front-desk / paralegal',  true, 107),
  ('counsel::3',      'Counsel',      'Legal counsel / contracts lead',true, 108)
ON CONFLICT (id) DO NOTHING;

-- ════════════════════════════════════════════════════════════════════════════
-- 3. ROLE GRANTS
-- ════════════════════════════════════════════════════════════════════════════

-- hr::5 — extend (existing role). Adds HR module reads + tile gates.
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'role'
  FROM perm.roles r
  JOIN (VALUES
    ('hr::5', 'tile:hr:view::allow'),
    ('hr::5', 'tile:hr_employees:view::allow'),
    ('hr::5', 'tile:hr_leave:view::allow'),
    ('hr::5', 'hr:employee:read::allow'),
    ('hr::5', 'hr:employee:list::allow'),
    ('hr::5', 'hr:quota:read::allow'),
    ('hr::5', 'hr:leave:read::allow'),
    ('hr::5', 'hr:leave:decide_self::allow')
  ) AS m(role, perm_id) ON m.role = r.id
  JOIN perm.permissions p ON p.id = m.perm_id
ON CONFLICT DO NOTHING;

-- hr_manager::3 — extend. All hr::5 grants + write perms + analytics + export.
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'role'
  FROM perm.roles r
  JOIN (VALUES
    ('hr_manager::3', 'tile:hr:view::allow'),
    ('hr_manager::3', 'tile:hr_employees:view::allow'),
    ('hr_manager::3', 'tile:hr_leave:view::allow'),
    ('hr_manager::3', 'hr:employee:read::allow'),
    ('hr_manager::3', 'hr:employee:list::allow'),
    ('hr_manager::3', 'hr:quota:read::allow'),
    ('hr_manager::3', 'hr:leave:read::allow'),
    ('hr_manager::3', 'hr:leave:decide_self::allow'),
    ('hr_manager::3', 'hr:employee:create::allow'),
    ('hr_manager::3', 'hr:employee:update::allow'),
    ('hr_manager::3', 'hr:employee:deactivate::allow'),
    ('hr_manager::3', 'hr:quota:update::allow'),
    ('hr_manager::3', 'hr:leave:approve::allow'),
    ('hr_manager::3', 'hr:leave:reject::allow'),
    ('hr_manager::3', 'hr:analytics:view::allow'),
    ('hr_manager::3', 'hr:export:csv::allow')
  ) AS m(role, perm_id) ON m.role = r.id
  JOIN perm.permissions p ON p.id = m.perm_id
ON CONFLICT DO NOTHING;

-- law::5 — paralegal/officer. Read + upload + query.
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'role'
  FROM perm.roles r
  JOIN (VALUES
    ('law::5', 'tile:law:view::allow'),
    ('law::5', 'law:contract:read::allow'),
    ('law::5', 'law:contract:list::allow'),
    ('law::5', 'law:contract:upload::allow'),
    ('law::5', 'law:preview:read::allow'),
    ('law::5', 'law:rag:query::allow')
  ) AS m(role, perm_id) ON m.role = r.id
  JOIN perm.permissions p ON p.id = m.perm_id
ON CONFLICT DO NOTHING;

-- counsel::3 — full law CRUD + admin + upload rights.
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'role'
  FROM perm.roles r
  JOIN (VALUES
    ('counsel::3', 'tile:law:view::allow'),
    ('counsel::3', 'tile:law_admin:view::allow'),
    ('counsel::3', 'tile:law_upload:view::allow'),
    ('counsel::3', 'law:contract:read::allow'),
    ('counsel::3', 'law:contract:list::allow'),
    ('counsel::3', 'law:contract:upload::allow'),
    ('counsel::3', 'law:contract:update::allow'),
    ('counsel::3', 'law:contract:delete::allow'),
    ('counsel::3', 'law:preview:read::allow'),
    ('counsel::3', 'law:chunk:read::allow'),
    ('counsel::3', 'law:rag:query::allow'),
    ('counsel::3', 'law:admin:stats::allow'),
    ('counsel::3', 'law:admin:reindex::allow')
  ) AS m(role, perm_id) ON m.role = r.id
  JOIN perm.permissions p ON p.id = m.perm_id
ON CONFLICT DO NOTHING;

-- ════════════════════════════════════════════════════════════════════════════
-- 4. TILES
-- ════════════════════════════════════════════════════════════════════════════

INSERT INTO perm.tiles (id, display_name, subtitle, icon, accent, group_name, href, sort_order, is_system, view_perm_id) VALUES
  ('hr',          'HR Dashboard',  'Employees · Leave · Analytics', '👥', 'rose',   'people', '/hr',                   150, true, 'tile:hr:view::allow'),
  ('hr_employees','HR Employees', 'Directory + quotas',             '👤', 'rose',   'people', '/hr',                   151, true, 'tile:hr_employees:view::allow'),
  ('hr_leave',    'HR Leave',      'Leave requests + calendar',      '📅', 'rose',   'people', '/hr',                   152, true, 'tile:hr_leave:view::allow'),
  ('law',         'Law Contracts', 'Upload · Search · Review',       '⚖️', 'indigo', 'legal',  '/law',                   160, true, 'tile:law:view::allow'),
  ('law_admin',   'Law Admin',     'Stats · Index · Retention',      '📚', 'indigo', 'legal',  '/law/admin',             161, true, 'tile:law_admin:view::allow'),
  ('law_upload',  'Law Upload',    'Ingest new contract',            '⬆️', 'indigo', 'legal',  '/law/upload',            162, true, 'tile:law_upload:view::allow')
ON CONFLICT (id) DO UPDATE SET
  display_name = EXCLUDED.display_name,
  subtitle     = EXCLUDED.subtitle,
  icon         = EXCLUDED.icon,
  accent       = EXCLUDED.accent,
  group_name   = EXCLUDED.group_name,
  href         = EXCLUDED.href,
  sort_order   = EXCLUDED.sort_order,
  view_perm_id = EXCLUDED.view_perm_id,
  is_system    = EXCLUDED.is_system;

-- ════════════════════════════════════════════════════════════════════════════
-- 5. USERS
-- ════════════════════════════════════════════════════════════════════════════

INSERT INTO folio.users (id, employee_code, fullname, hired_at, secondary_locale, is_active) VALUES
  (970, 'HR001', 'Nattaya HR-Officer', '2024-01-15', 'th', true),
  (971, 'HR002', 'Pongsak HR-Manager', '2020-05-01', 'th', true),
  (972, 'LW001', 'Wichai Paralegal',   '2023-08-20', 'th', true),
  (973, 'LW002', 'Kornkrit Counsel',   '2019-03-10', 'th', true)
ON CONFLICT (id) DO NOTHING;

-- ════════════════════════════════════════════════════════════════════════════
-- 6. USER BINDINGS
-- ════════════════════════════════════════════════════════════════════════════

INSERT INTO perm.user_roles (user_id, role_id, granted_by)
SELECT u.id, m.role_id, 'seed'
  FROM folio.users u
  JOIN (VALUES
    (970, 'hr::5'),
    (971, 'hr_manager::3'),
    (972, 'law::5'),
    (973, 'counsel::3')
  ) AS m(uid, role_id) ON m.uid = u.id
ON CONFLICT DO NOTHING;

-- Department membership marker (consistent with 9002 pattern).
-- perm.user_permissions uses a DEFERRABLE EXCLUDE constraint, so ON CONFLICT
-- cannot be the arbiter. Use WHERE NOT EXISTS for idempotency.
INSERT INTO perm.user_permissions (user_id, permission_id, granted_by, reason)
SELECT u.id, 'user:dept:hr::allow', 1, 'seed'
  FROM folio.users u
 WHERE u.id IN (970, 971, 972, 973)
   AND NOT EXISTS (
     SELECT 1 FROM perm.user_permissions up
      WHERE up.user_id = u.id
        AND up.permission_id = 'user:dept:hr::allow'
        AND up.revoked_at IS NULL
   );

COMMIT;
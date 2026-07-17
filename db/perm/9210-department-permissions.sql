-- 9210-department-permissions.sql
-- Department-scoped permission bundles.
--
-- A department row in perm.roles (matching a `user:dept:<id>::allow` permission)
-- can own a bundle of permission ids. Every user holding the dept permission
-- inherits the union of role grants + user_permissions + dept bundle grants.
--
-- A new `significance` boolean on role_permissions marks specific cells that
-- editors consider "department-significant" so the new /policy matrix can
-- surface them and revoke at any time.

BEGIN;

CREATE TABLE perm.department_permissions (
  department_id  text    NOT NULL REFERENCES perm.roles(id) ON DELETE CASCADE,
  permission_id  text    NOT NULL REFERENCES perm.permissions(id) ON DELETE CASCADE,
  granted_at     timestamptz NOT NULL DEFAULT now(),
  granted_by     text    NOT NULL DEFAULT 'system',
  PRIMARY KEY (department_id, permission_id)
);
CREATE INDEX perm_dept_perm_idx ON perm.department_permissions(permission_id);

ALTER TABLE perm.role_permissions
  ADD COLUMN IF NOT EXISTS significance boolean NOT NULL DEFAULT false;

ALTER TABLE perm.department_permissions
  ADD COLUMN IF NOT EXISTS significance boolean NOT NULL DEFAULT true;

-- Seed dept-row scaffolding for every existing `user:dept:*::allow` permission.
-- The dept id used in the permission (between `user:dept:` and `::allow`)
-- already matches the id assigned to a perm.roles row in earlier migrations.
INSERT INTO perm.roles (id, display_name, description, is_system, sort_order)
SELECT DISTINCT split_part(id, ':', 3), initcap(split_part(id, ':', 3)), 'Department target', true, 50
  FROM perm.permissions
 WHERE id LIKE 'user:dept:%::allow'
 ON CONFLICT (id) DO NOTHING;

COMMIT;

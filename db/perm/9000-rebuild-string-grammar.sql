-- 9000-rebuild-string-grammar.sql
-- Rebuild perm.* schema to encode effect/dept/level inline in strings.
--
-- Grammar:
--   permission_id  '<d>:<s>:<v>[:<qualifier>]::<effect>'   (e.g. 'finance:expense:approve::allow')
--   role_id        '<name>::<level>'                       (e.g. 'manager::3')
--   dept membership 'user:dept:<id>::allow'                 (granted as user_permission)
--
-- Drops: perm.role_permissions.effect, perm.roles.kind, users.dept_group_id,
--        perm.user_effective_level, perm.role_effective_level, perm.effective_user_perms,
--        perm.active_user_permissions, tile required_level/required_dept_id.

BEGIN;

-- Drop depending views first.
DROP VIEW IF EXISTS perm.effective_user_perms;
DROP VIEW IF EXISTS perm.active_user_permissions;
DROP VIEW IF EXISTS perm.user_effective_level;
DROP VIEW IF EXISTS perm.role_effective_level;

-- Drop schema wholesale.
DROP SCHEMA IF EXISTS perm CASCADE;
CREATE SCHEMA perm;

-- Roles (no kind column, no level column).
CREATE TABLE perm.roles (
  id              text PRIMARY KEY,                       -- '<name>::<level>'
  display_name    text NOT NULL,
  description     text,
  is_system       boolean NOT NULL DEFAULT false,
  sort_order      int  NOT NULL DEFAULT 0,
  parent_role_id  text REFERENCES perm.roles(id) ON DELETE RESTRICT,
  display_name_th text,
  display_name_de text,
  monthly_budget  numeric(14,2) NOT NULL DEFAULT 0,
  head_user_id    int REFERENCES users(id) ON DELETE SET NULL,
  created_at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX perm_roles_parent_idx ON perm.roles(parent_role_id) WHERE parent_role_id IS NOT NULL;

-- Permissions (id is the full :: string).
CREATE TABLE perm.permissions (
  id          text PRIMARY KEY,
  description text
);
CREATE INDEX perm_perm_domain_idx ON perm.permissions(split_part(id, ':', 1));

-- Role → permission grants (no effect column; effect in permission_id).
CREATE TABLE perm.role_permissions (
  role_id       text    NOT NULL REFERENCES perm.roles(id) ON DELETE CASCADE,
  permission_id text    NOT NULL REFERENCES perm.permissions(id) ON DELETE CASCADE,
  granted_at    timestamptz NOT NULL DEFAULT now(),
  granted_by    text    NOT NULL DEFAULT 'system',
  PRIMARY KEY (role_id, permission_id)
);

-- User → role bindings.
CREATE TABLE perm.user_roles (
  user_id    int NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role_id    text NOT NULL REFERENCES perm.roles(id) ON DELETE RESTRICT,
  granted_at timestamptz NOT NULL DEFAULT now(),
  granted_by text,
  PRIMARY KEY (user_id, role_id)
);

-- Per-user permission overrides (effect encoded in permission_id).
CREATE TABLE perm.user_permissions (
  id            bigserial PRIMARY KEY,
  user_id       int NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  permission_id text NOT NULL REFERENCES perm.permissions(id) ON DELETE CASCADE,
  granted_by    text NOT NULL,
  reason        text,
  granted_at    timestamptz NOT NULL DEFAULT now(),
  revoked_at    timestamptz,
  revoked_by    text,
  starts_at     timestamptz NOT NULL DEFAULT now(),
  ends_at       timestamptz,
  CONSTRAINT perm_user_perm_end_after_start CHECK (ends_at IS NULL OR ends_at > starts_at),
  CONSTRAINT perm_user_permissions_one_alive
    EXCLUDE (user_id WITH =, permission_id WITH =) WHERE (revoked_at IS NULL) DEFERRABLE INITIALLY IMMEDIATE
);
CREATE INDEX perm_up_active_idx ON perm.user_permissions(user_id) WHERE revoked_at IS NULL;
CREATE INDEX perm_up_perm_idx ON perm.user_permissions(permission_id) WHERE revoked_at IS NULL;

-- Tiles (view_perm_id replaces required_level + required_dept_id).
CREATE TABLE perm.tiles (
  id            text PRIMARY KEY,
  display_name  text NOT NULL,
  subtitle      text NOT NULL DEFAULT '',
  icon          text NOT NULL DEFAULT '🧾',
  accent        text NOT NULL DEFAULT 'slate',
  group_name    text NOT NULL,
  sub_view      text,
  href          text NOT NULL,
  request_target text,
  sort_order    integer NOT NULL DEFAULT 0,
  is_system     boolean NOT NULL DEFAULT true,
  owner_group_id text,
  view_perm_id  text NOT NULL,
  created_at    timestamptz NOT NULL DEFAULT now(),
  updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX perm_tiles_group_idx ON perm.tiles(group_name, sort_order);
CREATE FUNCTION perm.touch_updated_at() RETURNS trigger AS $$
BEGIN NEW.updated_at = now(); RETURN NEW; END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER perm_tiles_touch BEFORE UPDATE ON perm.tiles
  FOR EACH ROW EXECUTE FUNCTION perm.touch_updated_at();

-- Audit (unchanged).
CREATE TABLE perm.audit (
  id          bigserial PRIMARY KEY,
  kind        text NOT NULL,
  actor       text NOT NULL DEFAULT 'system',
  target      jsonb NOT NULL,
  occurred_at timestamptz NOT NULL DEFAULT now()
);

-- Policies (unchanged).
CREATE TABLE perm.policies (
  id          text PRIMARY KEY,
  name        text NOT NULL UNIQUE,
  ast         jsonb NOT NULL,
  description text,
  enabled     boolean NOT NULL DEFAULT true,
  version     int NOT NULL DEFAULT 1,
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE perm.policy_decisions (
  id          bigserial PRIMARY KEY,
  actor_id    int,
  policy_id   text,
  surface     text NOT NULL,
  target      text,
  decision    text NOT NULL,
  reasons     jsonb,
  resource    jsonb,
  occurred_at timestamptz NOT NULL DEFAULT now()
);

-- Drop the cached FK on users; department is now a user permission.
ALTER TABLE users DROP COLUMN IF EXISTS dept_group_id;

COMMIT;

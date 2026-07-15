-- Roles as departments migration.
--
-- Concept:
--   Departments are roles in perm.roles with kind='department'.
--   User membership in a dept = row in perm.user_roles pointing to a dept-kind role.
--   perm.user_roles carries both persona + dept assignments.
--
-- Migration is non-destructive:
--   1. Add `kind` column to perm.roles
--   2. Migrate rbac.groups (kind='department') → perm.roles rows
--   3. Migrate users.dept_group_id → perm.user_roles entries
--   4. Add partial unique index (one dept per user)
--   5. Add trigger to maintain users.dept_group_id as cached FK (sync from perm.user_roles)
--   6. Drop rbac.groups (no longer needed; perm.roles is the source)
--
-- users.dept_group_id is kept as a cached FK so existing readers continue to work.
-- Code can gradually migrate to reading from perm.user_roles.

BEGIN;

-- 1. Add kind column + parent_role_id + display_name_th to perm.roles
ALTER TABLE perm.roles ADD COLUMN IF NOT EXISTS kind text NOT NULL DEFAULT 'persona'
  CHECK (kind IN ('persona', 'department'));

ALTER TABLE perm.roles ADD COLUMN IF NOT EXISTS parent_role_id text
  REFERENCES perm.roles(id) ON DELETE RESTRICT;

ALTER TABLE perm.roles ADD COLUMN IF NOT EXISTS display_name_th text;

CREATE INDEX IF NOT EXISTS perm_roles_kind_idx ON perm.roles (kind);
CREATE INDEX IF NOT EXISTS perm_roles_parent_idx ON perm.roles (parent_role_id) WHERE parent_role_id IS NOT NULL;

-- 2. Migrate rbac.groups (kind='department') → perm.roles
INSERT INTO perm.roles (id, display_name, description, kind, sort_order, is_system, created_at)
SELECT g.id, g.name, NULL, 'department', g.sort_order, g.is_system, g.created_at
FROM rbac.groups g
WHERE g.kind = 'department'
ON CONFLICT (id) DO NOTHING;

-- 2b. Populate parent_role_id (dept tree) from rbac.groups.parent_id
UPDATE perm.roles child
   SET parent_role_id = parent.id
  FROM rbac.groups g
  JOIN perm.roles parent ON parent.id = g.parent_id AND parent.kind = 'department'
 WHERE child.id = g.id
   AND g.kind = 'department'
   AND g.parent_id IS NOT NULL
   AND child.parent_role_id IS NULL;

-- 3. Migrate users.dept_group_id → perm.user_roles entries
INSERT INTO perm.user_roles (user_id, role_id, granted_by)
SELECT u.id, u.dept_group_id, 'migration:0025_roles_as_departments'
FROM users u
WHERE u.dept_group_id IS NOT NULL
ON CONFLICT (user_id, role_id) DO NOTHING;

-- 4. Trigger to enforce: one dept per user (replaces partial unique index that PG disallows with subquery)
CREATE OR REPLACE FUNCTION perm.enforce_one_dept_per_user() RETURNS trigger AS $$
DECLARE
  v_is_dept boolean;
  v_user_id int;
  v_role_id text;
BEGIN
  v_user_id := COALESCE(NEW.user_id, OLD.user_id);
  v_role_id := COALESCE(NEW.role_id, OLD.role_id);
  SELECT kind = 'department' INTO v_is_dept
    FROM perm.roles WHERE id = v_role_id;
  IF v_is_dept THEN
    IF EXISTS (
      SELECT 1 FROM perm.user_roles ur
        JOIN perm.roles r ON r.id = ur.role_id
       WHERE ur.user_id = v_user_id
         AND r.kind = 'department'
         AND ur.role_id <> v_role_id
         AND (TG_OP = 'INSERT' OR (ur.user_id <> NEW.user_id) OR (ur.role_id <> NEW.role_id))
    ) THEN
      RAISE EXCEPTION 'user % already holds another department role', v_user_id;
    END IF;
  END IF;
  RETURN COALESCE(NEW, OLD);
END $$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS perm_ur_one_dept ON perm.user_roles;
CREATE TRIGGER perm_ur_one_dept
  BEFORE INSERT OR UPDATE OR DELETE ON perm.user_roles
  FOR EACH ROW EXECUTE FUNCTION perm.enforce_one_dept_per_user();

-- 5. Trigger: keep users.dept_group_id in sync with perm.user_roles
--    The user's primary dept = the (only, by uniqueness) dept-role they hold.
--    Reads stay on users.dept_group_id (cached FK) for backward compat.
CREATE OR REPLACE FUNCTION perm.sync_user_dept_cache() RETURNS trigger AS $$
DECLARE
  v_user_id int;
BEGIN
  v_user_id := COALESCE(NEW.user_id, OLD.user_id);
  -- Ensure users row has the column (it may have been re-added)
  PERFORM 1 FROM information_schema.columns
    WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'dept_group_id';
  IF NOT FOUND THEN
    RETURN COALESCE(NEW, OLD);
  END IF;
  UPDATE users u
     SET dept_group_id = (
       SELECT ur.role_id FROM perm.user_roles ur
        JOIN perm.roles r ON r.id = ur.role_id
       WHERE ur.user_id = v_user_id AND r.kind = 'department'
       ORDER BY ur.granted_at ASC
       LIMIT 1
     )
   WHERE u.id = v_user_id;
  RETURN COALESCE(NEW, OLD);
END $$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS perm_ur_sync_dept_cache ON perm.user_roles;
CREATE TRIGGER perm_ur_sync_dept_cache
  AFTER INSERT OR UPDATE OR DELETE ON perm.user_roles
  FOR EACH ROW EXECUTE FUNCTION perm.sync_user_dept_cache();

-- 6. Drop rbac.groups (after data migrated)
DROP TABLE IF EXISTS rbac.groups CASCADE;

-- 7. Add FK from users.dept_group_id to perm.roles (cache stays in sync via trigger)
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.columns
             WHERE table_schema='public' AND table_name='users' AND column_name='dept_group_id') THEN
    ALTER TABLE users DROP CONSTRAINT IF EXISTS users_dept_group_id_fkey;
    ALTER TABLE users ADD CONSTRAINT users_dept_group_id_fkey
      FOREIGN KEY (dept_group_id) REFERENCES perm.roles(id) ON DELETE SET NULL;
  END IF;
END $$;

COMMIT;
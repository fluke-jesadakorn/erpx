-- Folio — perm.user_roles.role_id now cascades on role delete.
-- When HR deletes a custom role, all user assignments and permission grants
-- for that role are removed automatically.
-- The role's existence is the only thing "deleted" — the underlying
-- permission catalog rows (perm.permissions) are NEVER deleted.

BEGIN;

ALTER TABLE perm.user_roles
  DROP CONSTRAINT IF EXISTS user_roles_role_id_fkey;

ALTER TABLE perm.user_roles
  ADD CONSTRAINT user_roles_role_id_fkey
  FOREIGN KEY (role_id) REFERENCES perm.roles(id) ON DELETE CASCADE;

COMMIT;
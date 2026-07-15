-- Drop all legacy rbac.* tables now that the perm model is the single source
-- of truth. Surviving rbac.* tables:
--   - rbac.groups (departments — referenced by users.dept_group_id)
--
-- Drop order: leaf tables first, then tables with FKs to others, then rbac.roles.

-- 1. Compat views (no longer referenced now that all consumers are migrated).
DROP VIEW IF EXISTS rbac.roles_compat;
DROP VIEW IF EXISTS rbac.users_categorized;
DROP VIEW IF EXISTS rbac.tile_permissions;
DROP VIEW IF EXISTS rbac.tile_access_meta;

-- 2. Drop users.rbac_role_id (no longer written or read after the migration).
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_rbac_role_id_fkey;
DROP INDEX IF EXISTS users_rbac_role_id_idx;
ALTER TABLE users DROP COLUMN IF EXISTS rbac_role_id;

-- 3. Drop the dead rbac.* tables.
DROP TABLE IF EXISTS rbac.role_dept_assignments CASCADE;
DROP TABLE IF EXISTS rbac.role_groups           CASCADE;
DROP TABLE IF EXISTS rbac.group_permissions     CASCADE;
DROP TABLE IF EXISTS rbac.module_groups         CASCADE;
DROP TABLE IF EXISTS rbac.domain_scope          CASCADE;
DROP TABLE IF EXISTS rbac.domain_modules        CASCADE;
DROP TABLE IF EXISTS rbac.domains               CASCADE;
DROP TABLE IF EXISTS rbac.modules               CASCADE;
DROP TABLE IF EXISTS rbac.permissions           CASCADE;
DROP TABLE IF EXISTS rbac.audit                 CASCADE;
DROP TABLE IF EXISTS rbac.roles                 CASCADE;
DROP TABLE IF EXISTS rbac.tiles                 CASCADE;
DROP FUNCTION IF EXISTS rbac.is_descendant();
DROP FUNCTION IF EXISTS rbac.effective_staff_level(smallint, smallint);

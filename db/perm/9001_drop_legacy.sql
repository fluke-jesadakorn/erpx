-- Folio — drop legacy rbac matrix tables now that perm.* is the SoT.
-- Idempotent: each DROP uses IF EXISTS. Run after the new perm.* tables
-- have been seeded and verified.

BEGIN;

-- Migration safety: do not drop rbac.roles — users.rbac_role_id still
-- references it. The seed-0005 backfill used rbac_role_id strings to build
-- perm.user_roles one row per user. A future migration can fold
-- users.rbac_role_id into perm.user_roles; for now we keep rbac.roles.
DROP TABLE IF EXISTS rbac.permissions        CASCADE;
DROP TABLE IF EXISTS rbac.role_groups        CASCADE;
DROP TABLE IF EXISTS rbac.group_permissions  CASCADE;
DROP TABLE IF EXISTS rbac.module_groups      CASCADE;
DROP TABLE IF EXISTS rbac.tiles              CASCADE;
DROP TABLE IF EXISTS rbac.domains            CASCADE;
DROP TABLE IF EXISTS rbac.domain_modules     CASCADE;
DROP TABLE IF EXISTS rbac.domain_scope       CASCADE;
DROP TABLE IF EXISTS rbac.modules            CASCADE;

COMMIT;

-- Folio — unify user→perm grants into perm.user_permissions.
--
-- Goal: collapse rbac.perm_grants (temp-only) and perm.user_permissions (perm-only)
-- into a single perm.user_permissions table that supports both via:
--   - ends_at IS NULL     → permanent
--   - ends_at IS NOT NULL → time-bound (auto-revoked on expiry)
--
-- After this migration:
--   - rbac.perm_grants table is gone (data migrated first)
--   - rbac.active_temp_perms view is gone
--   - perm.effective_user_perms unions (real perms from roles) UNION (active perms
--     from perm.user_permissions WHERE revoked_at IS NULL AND (ends_at IS NULL OR now() <= ends_at))
--   - rbac schema is dropped

BEGIN;

-- 1. Add starts_at / ends_at to perm.user_permissions (if not already there).
ALTER TABLE perm.user_permissions
  ADD COLUMN IF NOT EXISTS starts_at timestamptz NOT NULL DEFAULT now(),
  ADD COLUMN IF NOT EXISTS ends_at   timestamptz;

ALTER TABLE perm.user_permissions
  DROP CONSTRAINT IF EXISTS perm_user_perm_end_after_start;
ALTER TABLE perm.user_permissions
  ADD CONSTRAINT perm_user_perm_end_after_start
    CHECK (ends_at IS NULL OR ends_at > starts_at);

-- 2. Backfill: migrate active rows from rbac.perm_grants into perm.user_permissions.
--    Inactive (revoked or expired) rows are dropped; they were audit-only.
--    Skip rows that already exist in perm.user_permissions (defensive — partial
--    unique index on (user_id, permission_id) WHERE revoked_at IS NULL guards).
INSERT INTO perm.user_permissions
  (user_id, permission_id, granted_by, reason, granted_at, revoked_at, revoked_by,
   starts_at, ends_at)
SELECT
  pg.user_id, pg.permission_id, pg.granted_by, pg.reason, pg.created_at,
  pg.revoked_at, pg.revoked_by, pg.starts_at, pg.ends_at
FROM rbac.perm_grants pg
WHERE NOT EXISTS (
  SELECT 1 FROM perm.user_permissions up
   WHERE up.user_id = pg.user_id
     AND up.permission_id = pg.permission_id
     AND up.revoked_at IS NULL
);

-- 3. Replace perm.effective_user_perms view: drop the rbac branch first (CASCADE
--    handles the dependency on perm.active_user_permissions).
DROP VIEW IF EXISTS perm.effective_user_perms CASCADE;

-- 4. Replace perm.active_user_permissions view to filter on ends_at too.
DROP VIEW IF EXISTS perm.active_user_permissions;
CREATE OR REPLACE VIEW perm.active_user_permissions AS
SELECT user_id, permission_id, granted_by, granted_at,
       starts_at, ends_at,
       CASE WHEN ends_at IS NULL THEN 'permanent'
            WHEN now() <= ends_at THEN 'active_window'
            ELSE 'expired' END AS lifetime_state
  FROM perm.user_permissions
 WHERE revoked_at IS NULL;

-- 5. Recreate perm.effective_user_perms with the new combined semantics.
CREATE OR REPLACE VIEW perm.effective_user_perms AS
SELECT ur.user_id, rp.permission_id, 'real'::text AS source
  FROM perm.user_roles ur
  JOIN perm.role_permissions rp ON rp.role_id = ur.role_id
 WHERE rp.effect = 'allow'
UNION
SELECT user_id, permission_id,
       CASE WHEN ends_at IS NULL THEN 'permanent' ELSE 'temporary' END AS source
  FROM perm.active_user_permissions
 WHERE lifetime_state IN ('permanent', 'active_window');

-- 6. Add a useful index for "perm → active user_ids" lookups (tile gates,
--    policy matrix, etc.). Mirrors the old rbac_pg_perm_idx shape.
CREATE INDEX IF NOT EXISTS perm_user_perm_active_perm_idx
  ON perm.user_permissions (permission_id)
  WHERE revoked_at IS NULL;

-- 7. Drop rbac.perm_grants and its view, then drop the empty rbac schema.
DROP VIEW IF EXISTS rbac.active_temp_perms;
DROP TABLE IF EXISTS rbac.perm_grants CASCADE;
DROP SCHEMA IF EXISTS rbac CASCADE;

COMMENT ON TABLE perm.user_permissions IS
  'Direct user → perm grants. ends_at IS NULL = permanent; ends_at IS NOT NULL = time-bound. '
  'Revoked rows are preserved for audit (revoked_at IS NOT NULL).';

COMMIT;
-- perm.user_permissions — permanent direct user → perm grants.
--
-- Difference from rbac.perm_grants:
--   perm_grants       = user → perm (TEMPORARY; ends_at NOT NULL mandatory)
--   user_permissions  = user → perm (PERMANENT;  no ends_at — survives sessions)
--
-- Lifetime semantics:
--   revoked_at IS NULL     = currently active
--   revoked_at IS NOT NULL = explicitly revoked (audit trail preserved)
--
-- Used when an admin drags a perm onto a user and chooses "permanent" in
-- ConfirmSheet. Time-bound drags go to rbac.perm_grants (see 0024).
--
-- Combined view perm.effective_user_perms unions:
--   - role perms (perm.role_permissions, effect=allow)
--   - temp perms (rbac.perm_grants WHERE active)
--   - permanent perms (perm.user_permissions WHERE not revoked)

BEGIN;

CREATE TABLE IF NOT EXISTS perm.user_permissions (
  id              bigserial PRIMARY KEY,
  user_id         integer NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  permission_id   text    NOT NULL REFERENCES perm.permissions(id) ON DELETE CASCADE,
  granted_by      text    NOT NULL,
  reason          text,
  granted_at      timestamptz NOT NULL DEFAULT now(),
  revoked_at      timestamptz,
  revoked_by      text,
  CONSTRAINT perm_user_permissions_one_alive
    EXCLUDE (user_id WITH =, permission_id WITH =)
    WHERE (revoked_at IS NULL) DEFERRABLE INITIALLY IMMEDIATE
);

CREATE INDEX IF NOT EXISTS perm_up_user_idx
  ON perm.user_permissions (user_id, permission_id);
CREATE INDEX IF NOT EXISTS perm_up_perm_idx
  ON perm.user_permissions (permission_id)
  WHERE revoked_at IS NULL;
CREATE INDEX IF NOT EXISTS perm_up_active_idx
  ON perm.user_permissions (user_id)
  WHERE revoked_at IS NULL;

CREATE OR REPLACE VIEW perm.active_user_permissions AS
SELECT user_id, permission_id, granted_by, granted_at
  FROM perm.user_permissions
 WHERE revoked_at IS NULL;

DROP VIEW IF EXISTS perm.effective_user_perms;
CREATE OR REPLACE VIEW perm.effective_user_perms AS
SELECT ur.user_id, rp.permission_id, 'real'::text AS source
  FROM perm.user_roles ur
  JOIN perm.role_permissions rp ON rp.role_id = ur.role_id
 WHERE rp.effect = 'allow'
UNION
SELECT user_id, permission_id, source
  FROM rbac.active_temp_perms
UNION
SELECT user_id, permission_id, 'permanent'::text AS source
  FROM perm.active_user_permissions;

COMMENT ON TABLE perm.user_permissions IS
  'Permanent direct user → perm grants. Time-bound acting grants live in rbac.perm_grants.';
COMMENT ON COLUMN perm.user_permissions.revoked_at IS
  'Soft-delete timestamp; row preserved for audit.';

COMMIT;

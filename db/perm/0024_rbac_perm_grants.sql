-- rbac.perm_grants — time-bound direct perm grants (acting + head + access request).
--
-- Difference from perm.role_permissions:
--   role_permissions = role → perm (permanent, declarative)
--   perm_grants      = user → perm (direct, time-bound; ends_at NOT NULL mandatory)
--
-- Lifetime semantics:
--   ends_at IS NOT NULL — temporary (acting, head rotation, access request)
--   revoked_at IS NOT NULL — explicitly revoked (vs naturally expired)
--
-- Combined view perm.effective_user_perms unions:
--   - real perms (from perm.user_roles + perm.role_permissions)
--   - temp perms (from rbac.perm_grants WHERE active)

BEGIN;

CREATE TABLE IF NOT EXISTS rbac.perm_grants (
  id              bigserial PRIMARY KEY,
  user_id         integer NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  permission_id   text NOT NULL REFERENCES perm.permissions(id) ON DELETE CASCADE,
  starts_at       timestamptz NOT NULL DEFAULT now(),
  ends_at         timestamptz NOT NULL,
  granted_by      text NOT NULL,
  reason          text,
  source          text NOT NULL DEFAULT 'manual'
                    CHECK (source IN ('manual','seed','bulk','access_request')),
  revoked_at      timestamptz,
  revoked_by      text,
  created_at      timestamptz NOT NULL DEFAULT now(),
  CHECK (ends_at > starts_at),
  CHECK (revoked_at IS NULL OR revoked_at >= starts_at)
);

CREATE INDEX IF NOT EXISTS rbac_pg_user_idx
  ON rbac.perm_grants (user_id, permission_id, starts_at, ends_at);
CREATE INDEX IF NOT EXISTS rbac_pg_perm_idx
  ON rbac.perm_grants (permission_id)
  WHERE revoked_at IS NULL;

-- Active temp perms (not revoked, within window)
CREATE OR REPLACE VIEW rbac.active_temp_perms AS
SELECT user_id, permission_id, granted_by, starts_at, ends_at, source
FROM rbac.perm_grants
WHERE revoked_at IS NULL
  AND now() BETWEEN starts_at AND ends_at;

-- Combined view: real (from roles) + temp (from grants)
-- Note: UNION not UNION ALL — duplicate rows collapse to one
CREATE OR REPLACE VIEW perm.effective_user_perms AS
SELECT ur.user_id, rp.permission_id, 'real'::text AS source
  FROM perm.user_roles ur
  JOIN perm.role_permissions rp ON rp.role_id = ur.role_id
 WHERE rp.effect = 'allow'
UNION
SELECT user_id, permission_id, source
  FROM rbac.active_temp_perms;

-- Partial unique index: one active head per (user, dept)
-- Used for dept head enforcement
CREATE UNIQUE INDEX IF NOT EXISTS rbac_pg_one_head_per_user_dept
  ON rbac.perm_grants (user_id, (split_part(permission_id, ':', 2)))
  WHERE permission_id ~ '^dept:.+:head:\d+:all$'
    AND revoked_at IS NULL;

-- One active grant per (user, perm) for ALL perm_grants categories
-- (multiple grants of same perm for overlapping windows collapse)
-- Skip for now to keep flexibility; add later if needed.

COMMIT;
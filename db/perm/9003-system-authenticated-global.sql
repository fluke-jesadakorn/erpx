-- 9003-system-authenticated-global.sql
-- Add a global "authenticated" perm and grant it to every persona role.
-- Updates any tile's view_perm_id that referenced the old per-tile logic so
-- the HUB (and any other tile without a specific grant) is universally open
-- to any signed-in user.

BEGIN;

INSERT INTO perm.permissions (id, description)
VALUES ('system:authenticated:view::allow', 'Global access — every signed-in user')
ON CONFLICT (id) DO NOTHING;

-- Grant the baseline bundle (incl. system:authenticated) to every persona role.
-- Idempotent: ON CONFLICT skips already-granted pairs.
INSERT INTO perm.role_permissions (role_id, permission_id, granted_by)
SELECT r.id, p.id, 'baseline-9003'
  FROM perm.roles r
 CROSS JOIN perm.permissions p
 WHERE p.id IN (
   'system:authenticated:view::allow'
 )
ON CONFLICT DO NOTHING;

COMMIT;
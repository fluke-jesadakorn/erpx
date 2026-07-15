-- Folio — give the permissions tile its own dedicated permission.
-- Previously aliased to tile:org_chart:view (HR Manager has it via the org_chart tile),
-- but tile visibility should be explicit: a tile is visible iff the actor has
-- the perm declared in rbac.tiles.required_permission.
--
-- Grant tile:permissions:view to all the personas that already had access to
-- the legacy "permissions" tile (CEO/CFO/admin/hr_manager).

BEGIN;

INSERT INTO perm.permissions (id, domain, subject, verb, description)
VALUES ('tile:permissions:view', 'tile', 'permissions', 'view', 'View access for the HR / Permissions console')
ON CONFLICT (id) DO NOTHING;

UPDATE rbac.tiles SET required_permission = 'tile:permissions:view' WHERE id = 'permissions';

INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT r.id, 'tile:permissions:view', 'allow', 'seed-0012'
  FROM perm.roles r
 WHERE r.id IN ('ceo', 'cfo', 'admin', 'hr_manager')
ON CONFLICT DO NOTHING;

COMMIT;
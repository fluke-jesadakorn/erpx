-- Folio — seed perm.roles 1:1 from rbac.roles
BEGIN;

INSERT INTO perm.roles (id, display_name, description, is_system, sort_order)
SELECT
  id,
  name,
  NULL,
  is_system,
  sort_order
FROM rbac.roles
ON CONFLICT (id) DO UPDATE SET
  display_name = EXCLUDED.display_name,
  is_system    = EXCLUDED.is_system,
  sort_order   = EXCLUDED.sort_order;

COMMIT;

-- Folio — ensure every rbac.tiles row has a matching perm.permissions row.
-- Future features ship their own permission in a migration; this seed only
-- backfills what the tile catalog needs today.
-- HR cannot create permissions (append-only catalog) — that's enforced by
-- the absence of POST /api/perm/permissions.

BEGIN;

INSERT INTO perm.permissions (id, domain, subject, verb, description)
SELECT 'tile:' || replace(id, '-', '_') || ':view',
       'tile',
       replace(id, '-', '_'),
       'view',
       'View access for the ' || display_name || ' tile'
  FROM rbac.tiles
 WHERE required_permission IS NOT NULL
   AND NOT EXISTS (
     SELECT 1 FROM perm.permissions p WHERE p.id = 'tile:' || replace(rbac.tiles.id, '-', '_') || ':view'
   )
ON CONFLICT (id) DO NOTHING;

COMMIT;
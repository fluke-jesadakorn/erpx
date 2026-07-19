INSERT INTO perm.role_permissions (role_id, role_kind, permission_id)
SELECT r.id, r.kind, 'ai:chat:full::allow'
  FROM perm.roles r
 WHERE EXISTS (SELECT 1 FROM perm.permissions p WHERE p.id = 'ai:chat:full::allow')
ON CONFLICT DO NOTHING;

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'folio_readonly_agent') THEN
    GRANT USAGE ON SCHEMA finance, folio, inventory, perm TO folio_readonly_agent;
    GRANT SELECT ON ALL TABLES IN SCHEMA finance, folio, inventory, perm TO folio_readonly_agent;
  END IF;
END $$;

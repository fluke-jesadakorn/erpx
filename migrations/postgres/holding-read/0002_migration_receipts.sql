-- Keep migration status read-only and available to the app role without
-- granting direct access to the migration receipt table.
CREATE OR REPLACE FUNCTION erpx_migration_receipts()
RETURNS TABLE(name TEXT, checksum TEXT, applied_at TIMESTAMPTZ)
LANGUAGE sql
SECURITY DEFINER
SET search_path = public
AS $$
  SELECT m.name, m.checksum, m.applied_at
  FROM public.erpx_migrations AS m
  ORDER BY m.name
$$;

GRANT EXECUTE ON FUNCTION erpx_migration_receipts() TO PUBLIC;

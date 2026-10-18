-- Match the Holding metadata reader without exposing business tables or
-- granting application roles direct access to migration receipts.
CREATE OR REPLACE FUNCTION public.erpx_migration_receipts()
RETURNS TABLE(name TEXT, checksum TEXT, applied_at TIMESTAMPTZ)
LANGUAGE sql
SECURITY DEFINER
SET search_path = pg_catalog
AS $$
  SELECT m.name, m.checksum, m.applied_at
  FROM public.erpx_migrations AS m
  ORDER BY m.name
$$;

GRANT EXECUTE ON FUNCTION public.erpx_migration_receipts() TO PUBLIC;

-- Application roles cannot read erpx_migrations directly. Expose only the
-- receipt fields required by the Platform operations screen through a
-- migration-owned, read-only function.
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

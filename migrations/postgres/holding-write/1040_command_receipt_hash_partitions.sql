-- Partition canonical receipt keys without changing the command protocol or transaction.
-- Preserve the pre-upgrade table as migration evidence; all live FKs target the new parent.
-- https://www.postgresql.org/docs/18/ddl-partitioning.html
LOCK TABLE command_receipts IN ACCESS EXCLUSIVE MODE;
DO $$
BEGIN
  -- Fail closed for security/dependency configurations this migration does not transform.
  IF EXISTS(SELECT 1 FROM pg_class WHERE oid='command_receipts'::regclass AND (relrowsecurity OR relforcerowsecurity))
     OR EXISTS(SELECT 1 FROM pg_policy WHERE polrelid='command_receipts'::regclass)
     OR EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='command_receipts'::regclass AND contype='f')
     OR EXISTS(SELECT 1 FROM pg_depend d JOIN pg_rewrite r ON r.oid=d.objid
       WHERE d.refobjid='command_receipts'::regclass AND d.classid='pg_rewrite'::regclass)
     OR EXISTS(SELECT 1 FROM pg_seclabel WHERE objoid='command_receipts'::regclass) THEN
    RAISE EXCEPTION 'Unsupported command receipt security or dependency; migration rolled back';
  END IF;
END $$;
CREATE TEMP TABLE receipt_partition_foreign_keys ON COMMIT DROP AS
 SELECT conrelid::regclass::text AS relation,conname,pg_get_constraintdef(oid) AS definition
 FROM pg_constraint WHERE confrelid='command_receipts'::regclass AND contype='f';
CREATE TEMP TABLE receipt_partition_triggers ON COMMIT DROP AS
 SELECT tgname,pg_get_triggerdef(oid) AS definition,tgenabled FROM pg_trigger
 WHERE tgrelid='command_receipts'::regclass AND NOT tgisinternal;
CREATE TEMP TABLE receipt_partition_privileges ON COMMIT DROP AS
 SELECT a.grantee,a.privilege_type,a.is_grantable
 FROM pg_class c CROSS JOIN LATERAL aclexplode(COALESCE(c.relacl,acldefault('r',c.relowner))) a
 WHERE c.oid='command_receipts'::regclass;
CREATE TEMP TABLE receipt_partition_column_privileges ON COMMIT DROP AS
 SELECT attname,a.grantee,a.privilege_type,a.is_grantable
 FROM pg_attribute c CROSS JOIN LATERAL aclexplode(c.attacl) a
 WHERE c.attrelid='command_receipts'::regclass AND c.attnum>0 AND NOT c.attisdropped;
CREATE TEMP TABLE receipt_partition_owner ON COMMIT DROP AS
 SELECT pg_get_userbyid(relowner) AS owner FROM pg_class WHERE oid='command_receipts'::regclass;
DO $$ DECLARE r record; BEGIN
 FOR r IN SELECT * FROM receipt_partition_foreign_keys LOOP
   EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I',r.relation,r.conname);
 END LOOP;
END $$;
ALTER TABLE command_receipts RENAME TO command_receipts_before_partition;
ALTER TABLE command_receipts_before_partition RENAME CONSTRAINT command_receipts_pkey TO command_receipts_before_partition_pkey;
CREATE TABLE command_receipts (LIKE command_receipts_before_partition INCLUDING ALL) PARTITION BY HASH(command_id);
DO $$ DECLARE i integer; BEGIN
 FOR i IN 0..31 LOOP
   EXECUTE format('CREATE TABLE command_receipts_partition_%s PARTITION OF command_receipts FOR VALUES WITH (MODULUS 32, REMAINDER %s)',i,i);
 END LOOP;
END $$;
INSERT INTO command_receipts SELECT * FROM command_receipts_before_partition;
DO $$ DECLARE r record; role_name text; BEGIN
 FOR r IN SELECT * FROM receipt_partition_foreign_keys LOOP
   EXECUTE format('ALTER TABLE %s ADD CONSTRAINT %I %s',r.relation,r.conname,r.definition);
 END LOOP;
 FOR r IN SELECT * FROM receipt_partition_triggers LOOP
   EXECUTE r.definition;
   -- Replicate the source trigger enablement rather than silently enabling a disabled one.
   IF r.tgenabled<>'O' THEN
     EXECUTE format('ALTER TABLE command_receipts %s TRIGGER %I',
       CASE r.tgenabled WHEN 'D' THEN 'DISABLE' WHEN 'R' THEN 'ENABLE REPLICA' ELSE 'ENABLE ALWAYS' END,
       r.tgname);
   END IF;
 END LOOP;
 -- Remove newly inherited default grants before restoring the original ACL.
 FOR r IN SELECT DISTINCT a.grantee FROM pg_class c CROSS JOIN LATERAL aclexplode(c.relacl) a
   WHERE c.oid='command_receipts'::regclass LOOP
   EXECUTE format('REVOKE ALL ON TABLE command_receipts FROM %s',CASE WHEN r.grantee=0 THEN 'PUBLIC' ELSE quote_ident(pg_get_userbyid(r.grantee)) END);
 END LOOP;
 FOR r IN SELECT * FROM receipt_partition_column_privileges LOOP
   EXECUTE format('GRANT %s (%I) ON TABLE command_receipts TO %s%s',r.privilege_type,r.attname,
     CASE WHEN r.grantee=0 THEN 'PUBLIC' ELSE quote_ident(pg_get_userbyid(r.grantee)) END,
     CASE WHEN r.is_grantable THEN ' WITH GRANT OPTION' ELSE '' END);
 END LOOP;
 FOR r IN SELECT * FROM receipt_partition_privileges LOOP
   role_name:=CASE WHEN r.grantee=0 THEN 'PUBLIC' ELSE quote_ident(pg_get_userbyid(r.grantee)) END;
   EXECUTE format('GRANT %s ON TABLE command_receipts TO %s%s',r.privilege_type,role_name,
     CASE WHEN r.is_grantable THEN ' WITH GRANT OPTION' ELSE '' END);
 END LOOP;
 EXECUTE format('ALTER TABLE command_receipts OWNER TO %I',(SELECT owner FROM receipt_partition_owner));
 FOR r IN SELECT inhrelid::regclass::text AS relation FROM pg_inherits WHERE inhparent='command_receipts'::regclass LOOP
   EXECUTE format('ALTER TABLE %s OWNER TO %I',r.relation,(SELECT owner FROM receipt_partition_owner));
 END LOOP;
END $$;

CREATE OR REPLACE FUNCTION work_cache_tracks(name text) RETURNS boolean LANGUAGE sql IMMUTABLE AS $$
 SELECT name NOT IN ('work_cache_revisions','work_cache_revision_baselines','work_cache_revision_scopes',
 'erpx_migrations','outbox_events','command_assertions','command_assertion_baselines','command_receipts','command_receipts_before_partition','audit_events','runtime_health',
 'iam_audit','iam_receipts','iam_sync_jobs','um_history','seed_repairs',
 'home_read_cursors','active_work_contexts','work_role_preferences',
 'work_card_preferences','navigation_preferences','pos_audit','pms_jobs',
 'legal_access_events','legal_document_inspections','legal_ai_proposals')
 AND name NOT LIKE '%\_receipts' ESCAPE '\'
 AND name NOT LIKE 'command\_receipts\_partition\_%' ESCAPE '\'
$$;
SELECT ensure_work_cache_coverage();

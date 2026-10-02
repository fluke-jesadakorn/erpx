CREATE TABLE pms_import_batches (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL REFERENCES organization_nodes(id),
 property_id TEXT NOT NULL REFERENCES pms_properties(id),cutover_date TEXT NOT NULL,
 property_version INTEGER NOT NULL,definition_json TEXT NOT NULL CHECK(erpx_json_valid(definition_json)),
 report_json TEXT NOT NULL CHECK(erpx_json_valid(report_json)),
 state TEXT NOT NULL DEFAULT 'preview' CHECK(state IN ('preview','approved','applied')),
 created_by TEXT NOT NULL,approved_by TEXT,opening_journal_id TEXT UNIQUE REFERENCES finance_journals(id),
 version INTEGER NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(approved_by IS NULL OR approved_by<>created_by)
);
CREATE TABLE pms_import_balances (
 batch_id TEXT NOT NULL REFERENCES pms_import_batches(id),source_key TEXT NOT NULL,
 property_id TEXT NOT NULL REFERENCES pms_properties(id),folio_id TEXT NOT NULL,
 entry_id TEXT NOT NULL UNIQUE REFERENCES pms_records(id),pocket TEXT NOT NULL CHECK(pocket IN ('due','advance','deposit')),
 amount_minor BIGINT NOT NULL CHECK(amount_minor>0),finance_payment_id TEXT REFERENCES finance_payments(id),
 PRIMARY KEY(batch_id,source_key,pocket)
);
ALTER TABLE pms_documents DROP CONSTRAINT pms_documents_kind_check;
ALTER TABLE pms_documents ADD CONSTRAINT pms_documents_kind_check CHECK(kind IN ('invoice','receipt','refund','credit_note','opening'));
CREATE INDEX pms_import_batches_property ON pms_import_batches(company_id,property_id,created_at DESC);
DO $$ DECLARE role_name text; BEGIN
 FOREACH role_name IN ARRAY ARRAY[current_database()||'_app',current_database()||'_jobs'] LOOP
  IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=role_name) THEN
   EXECUTE format('GRANT SELECT,INSERT,UPDATE ON pms_import_batches,pms_import_balances TO %I',role_name);
  END IF;
 END LOOP;
END $$;

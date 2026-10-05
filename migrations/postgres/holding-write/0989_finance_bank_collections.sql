CREATE TABLE finance_bank_csv_mappings (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, name TEXT NOT NULL,
 columns_json TEXT NOT NULL CHECK(erpx_json_valid(columns_json)), created_by TEXT NOT NULL, created_at BIGINT NOT NULL,
 UNIQUE(company_id,name)
);
CREATE TABLE finance_bank_import_files (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, bank_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 mapping_id TEXT NOT NULL REFERENCES finance_bank_csv_mappings(id), file_hash TEXT NOT NULL,
 statement_id TEXT NOT NULL REFERENCES finance_bank_statements(id), created_at BIGINT NOT NULL,
 UNIQUE(company_id,bank_account_id,file_hash)
);
CREATE TABLE finance_bank_match_targets (
 journal_line_id TEXT PRIMARY KEY REFERENCES finance_journal_lines(id),
 group_id TEXT NOT NULL REFERENCES finance_bank_match_groups(id)
);
CREATE TABLE finance_collection_cases (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, invoice_id TEXT NOT NULL REFERENCES finance_invoices(id),
 invoice_version BIGINT NOT NULL, as_of_date TEXT NOT NULL, stage TEXT NOT NULL,
 amount_minor BIGINT NOT NULL CHECK(amount_minor>0), currency TEXT NOT NULL,
 message TEXT NOT NULL, source_hash TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','test_delivered','cancelled')),
 version BIGINT NOT NULL DEFAULT 1, prepared_by TEXT NOT NULL, reviewed_by TEXT, review_reason TEXT,
 created_at BIGINT NOT NULL, reviewed_at BIGINT, CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by),
 UNIQUE(invoice_id,invoice_version,stage)
);
CREATE TABLE finance_collection_deliveries (
 case_id TEXT PRIMARY KEY REFERENCES finance_collection_cases(id), company_id TEXT NOT NULL,
 adapter TEXT NOT NULL CHECK(adapter='test'), delivered_by TEXT NOT NULL, payload_hash TEXT NOT NULL, delivered_at BIGINT NOT NULL
);
CREATE TRIGGER finance_bank_mapping_immutable BEFORE UPDATE OR DELETE ON finance_bank_csv_mappings FOR EACH ROW EXECUTE FUNCTION protect_financial_event();
CREATE TRIGGER finance_bank_file_immutable BEFORE UPDATE OR DELETE ON finance_bank_import_files FOR EACH ROW EXECUTE FUNCTION protect_financial_event();
CREATE TRIGGER finance_bank_match_allocation_immutable BEFORE UPDATE OR DELETE ON finance_bank_match_allocations FOR EACH ROW EXECUTE FUNCTION protect_financial_event();
CREATE FUNCTION protect_bank_match_group() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR (NEW.company_id,NEW.currency,NEW.source_hash,NEW.explanation_json,NEW.proposed_by)
 IS DISTINCT FROM (OLD.company_id,OLD.currency,OLD.source_hash,OLD.explanation_json,OLD.proposed_by) THEN
 RAISE EXCEPTION 'BANK_MATCH_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END IF; RETURN NEW; END; $$;
CREATE TRIGGER finance_bank_group_evidence_immutable BEFORE UPDATE OR DELETE ON finance_bank_match_groups FOR EACH ROW EXECUTE FUNCTION protect_bank_match_group();
CREATE TRIGGER finance_collection_delivery_immutable BEFORE UPDATE OR DELETE ON finance_collection_deliveries FOR EACH ROW EXECUTE FUNCTION protect_financial_event();
CREATE FUNCTION protect_collection_case() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR (NEW.company_id,NEW.invoice_id,NEW.invoice_version,NEW.as_of_date,NEW.stage,NEW.amount_minor,NEW.currency,NEW.message,NEW.source_hash,NEW.prepared_by)
 IS DISTINCT FROM (OLD.company_id,OLD.invoice_id,OLD.invoice_version,OLD.as_of_date,OLD.stage,OLD.amount_minor,OLD.currency,OLD.message,OLD.source_hash,OLD.prepared_by) THEN
 RAISE EXCEPTION 'COLLECTION_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF; RETURN NEW; END; $$;
CREATE TRIGGER finance_collection_source_immutable BEFORE UPDATE OR DELETE ON finance_collection_cases FOR EACH ROW EXECUTE FUNCTION protect_collection_case();
DO $$ DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app'); BEGIN
 IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
  EXECUTE format('GRANT SELECT,INSERT ON finance_bank_csv_mappings,finance_bank_import_files,finance_collection_deliveries TO %I',app_role);
  EXECUTE format('GRANT SELECT,INSERT,DELETE ON finance_bank_match_targets TO %I',app_role);
  EXECUTE format('GRANT SELECT,INSERT,UPDATE ON finance_collection_cases TO %I',app_role);
 END IF; END; $$;
INSERT INTO work_cache_revisions(table_name) VALUES('finance_bank_csv_mappings'),('finance_bank_import_files'),('finance_bank_match_targets'),('finance_collection_cases'),('finance_collection_deliveries') ON CONFLICT DO NOTHING;

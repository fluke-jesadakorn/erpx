CREATE TABLE finance_expansion_review_files (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,target_type TEXT NOT NULL,target_id TEXT NOT NULL,
 target_version BIGINT NOT NULL,command_id TEXT NOT NULL UNIQUE,manifest_json TEXT NOT NULL CHECK(erpx_json_valid(manifest_json)),manifest_sha256 TEXT NOT NULL,reviewed_by TEXT NOT NULL,created_at BIGINT NOT NULL
);
CREATE TRIGGER finance_expansion_review_files_immutable BEFORE UPDATE OR DELETE ON finance_expansion_review_files FOR EACH ROW EXECUTE FUNCTION protect_financial_event();
INSERT INTO work_cache_revisions(table_name) VALUES('finance_expansion_review_files') ON CONFLICT DO NOTHING;
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON finance_expansion_review_files FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON finance_expansion_review_files FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
DO $$ DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app');BEGIN
 IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN EXECUTE format('GRANT SELECT,INSERT ON finance_expansion_review_files TO %I',app_role);END IF;
END;$$;

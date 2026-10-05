CREATE TABLE finance_manual_draft_editions (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL,
 journal_id TEXT NOT NULL REFERENCES finance_journals(id), journal_version BIGINT NOT NULL CHECK(journal_version>0),
 snapshot_json TEXT NOT NULL CHECK(erpx_json_valid(snapshot_json)), snapshot_sha256 TEXT NOT NULL,
 reason TEXT NOT NULL, prepared_by TEXT NOT NULL, created_at BIGINT NOT NULL,
 UNIQUE(journal_id,journal_version)
);
CREATE TRIGGER finance_manual_draft_immutable BEFORE UPDATE OR DELETE ON finance_manual_draft_editions FOR EACH ROW EXECUTE FUNCTION protect_financial_event();
DO $$ DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app');BEGIN IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN EXECUTE format('GRANT SELECT,INSERT ON finance_manual_draft_editions TO %I',app_role);END IF;END;$$;

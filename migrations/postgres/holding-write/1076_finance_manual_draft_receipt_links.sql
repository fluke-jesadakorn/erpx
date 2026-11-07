CREATE TABLE finance_manual_draft_receipt_links (
 journal_id TEXT NOT NULL,
 journal_version BIGINT NOT NULL CHECK (journal_version > 0),
 company_id TEXT NOT NULL,
 command_key TEXT NOT NULL UNIQUE,
 actor_id TEXT NOT NULL,
 snapshot_sha256 TEXT NOT NULL,
 previous_version BIGINT,
 previous_sha256 TEXT,
 receipt_sha256 TEXT NOT NULL,
 PRIMARY KEY (journal_id, journal_version),
 FOREIGN KEY (journal_id, journal_version) REFERENCES finance_manual_draft_editions(journal_id, journal_version),
 CHECK ((journal_version = 1 AND previous_version IS NULL AND previous_sha256 IS NULL)
     OR (journal_version > 1 AND previous_version IS NOT NULL AND previous_version = journal_version - 1 AND previous_sha256 IS NOT NULL))
);
CREATE TRIGGER finance_manual_draft_receipt_links_immutable BEFORE UPDATE OR DELETE ON finance_manual_draft_receipt_links FOR EACH ROW EXECUTE FUNCTION protect_financial_event();
DO $$ DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app'); BEGIN IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN EXECUTE format('GRANT SELECT,INSERT ON finance_manual_draft_receipt_links TO %I',app_role); END IF; END; $$;

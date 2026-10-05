CREATE TABLE finance_company_tools (
 company_id TEXT NOT NULL, tool TEXT NOT NULL CHECK(tool IN ('planning','bank_automation','collections')),
 enabled BIGINT NOT NULL CHECK(enabled IN (0,1)), version BIGINT NOT NULL DEFAULT 1,
 changed_by TEXT NOT NULL, reason TEXT NOT NULL, updated_at BIGINT NOT NULL,
 PRIMARY KEY(company_id,tool)
);
DO $$ DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app'); BEGIN
 IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
  EXECUTE format('GRANT SELECT,INSERT,UPDATE ON finance_company_tools TO %I',app_role);
 END IF; END; $$;
INSERT INTO work_cache_revisions(table_name) VALUES('finance_company_tools') ON CONFLICT DO NOTHING;

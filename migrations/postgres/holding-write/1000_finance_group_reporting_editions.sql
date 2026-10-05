ALTER TABLE finance_company_tools DROP CONSTRAINT finance_company_tools_tool_check;
ALTER TABLE finance_company_tools ADD CONSTRAINT finance_company_tools_tool_check CHECK(tool IN ('planning','bank_automation','collections','business_contracts','instruments','asset_policies','group_reporting'));
CREATE TABLE finance_group_report_editions (
 id TEXT PRIMARY KEY,holding_id TEXT NOT NULL,company_id TEXT NOT NULL,parent_id TEXT REFERENCES finance_group_report_editions(id),root_id TEXT NOT NULL,edition BIGINT NOT NULL,
 start_date TEXT NOT NULL,end_date TEXT NOT NULL,currency TEXT NOT NULL,version BIGINT NOT NULL DEFAULT 1,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','rejected','superseded')),
 content_json TEXT NOT NULL CHECK(erpx_json_valid(content_json)),source_json TEXT NOT NULL CHECK(erpx_json_valid(source_json)),result_json TEXT NOT NULL CHECK(erpx_json_valid(result_json)),source_hash TEXT NOT NULL,
 export_content TEXT NOT NULL,export_sha256 TEXT NOT NULL,prepared_by TEXT NOT NULL,reviewed_by TEXT,reason TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(end_date>=start_date),CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by)
);
CREATE UNIQUE INDEX finance_group_report_amendment ON finance_group_report_editions(parent_id) WHERE parent_id IS NOT NULL AND status<>'rejected';
CREATE FUNCTION protect_finance_group_report() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR (NEW.id,NEW.holding_id,NEW.company_id,NEW.parent_id,NEW.root_id,NEW.edition,NEW.start_date,NEW.end_date,NEW.currency,NEW.content_json,NEW.source_json,NEW.result_json,NEW.source_hash,NEW.export_content,NEW.export_sha256,NEW.prepared_by,NEW.reason,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.holding_id,OLD.company_id,OLD.parent_id,OLD.root_id,OLD.edition,OLD.start_date,OLD.end_date,OLD.currency,OLD.content_json,OLD.source_json,OLD.result_json,OLD.source_hash,OLD.export_content,OLD.export_sha256,OLD.prepared_by,OLD.reason,OLD.created_at) THEN RAISE EXCEPTION 'GROUP_REPORT_EDITION_IMMUTABLE' USING ERRCODE='23514';END IF;RETURN NEW;END;$$;
CREATE TRIGGER finance_group_report_immutable BEFORE UPDATE OR DELETE ON finance_group_report_editions FOR EACH ROW EXECUTE FUNCTION protect_finance_group_report();
INSERT INTO work_cache_revisions(table_name) VALUES('finance_group_report_editions') ON CONFLICT DO NOTHING;
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON finance_group_report_editions FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON finance_group_report_editions FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
DO $$ DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app');BEGIN
 IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN EXECUTE format('GRANT SELECT,INSERT,UPDATE ON finance_group_report_editions TO %I',app_role);END IF;
END;$$;

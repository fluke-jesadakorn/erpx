CREATE TABLE finance_plan_editions (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, chart_id TEXT NOT NULL REFERENCES finance_charts(id),
 kind TEXT NOT NULL CHECK(kind IN ('budget','forecast','report')), name TEXT NOT NULL,
 parent_id TEXT REFERENCES finance_plan_editions(id), edition BIGINT NOT NULL CHECK(edition>0),
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','rejected')),
 version BIGINT NOT NULL DEFAULT 1, content_json TEXT NOT NULL CHECK(erpx_json_valid(content_json)),
 content_hash TEXT NOT NULL, prepared_by TEXT NOT NULL, reviewed_by TEXT,
 review_reason TEXT, created_at BIGINT NOT NULL, reviewed_at BIGINT,
 CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by)
);
CREATE UNIQUE INDEX finance_plan_first_edition ON finance_plan_editions(company_id,chart_id,kind,name) WHERE parent_id IS NULL AND status<>'rejected';
CREATE UNIQUE INDEX finance_plan_amendment ON finance_plan_editions(parent_id) WHERE parent_id IS NOT NULL AND status<>'rejected';
CREATE TABLE finance_report_snapshots (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, definition_id TEXT NOT NULL REFERENCES finance_plan_editions(id),
 start_date TEXT NOT NULL, end_date TEXT NOT NULL, result_json TEXT NOT NULL CHECK(erpx_json_valid(result_json)),
 source_hash TEXT NOT NULL, created_by TEXT NOT NULL, created_at BIGINT NOT NULL,
 CHECK(end_date>=start_date)
);
CREATE FUNCTION protect_finance_plan() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR OLD.status<>'draft' OR
   (NEW.id,NEW.company_id,NEW.chart_id,NEW.kind,NEW.name,NEW.parent_id,NEW.edition,NEW.content_json,NEW.content_hash,NEW.prepared_by)
   IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.chart_id,OLD.kind,OLD.name,OLD.parent_id,OLD.edition,OLD.content_json,OLD.content_hash,OLD.prepared_by) THEN
  RAISE EXCEPTION 'FINANCE_PLAN_EDITION_IMMUTABLE' USING ERRCODE='23514';
 END IF; RETURN NEW;
END; $$;
CREATE TRIGGER finance_plan_immutable BEFORE UPDATE OR DELETE ON finance_plan_editions FOR EACH ROW EXECUTE FUNCTION protect_finance_plan();
CREATE TRIGGER finance_report_snapshot_immutable BEFORE UPDATE OR DELETE ON finance_report_snapshots FOR EACH ROW EXECUTE FUNCTION protect_financial_event();
DO $$ DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app'); BEGIN
 IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
  EXECUTE format('GRANT SELECT,INSERT,UPDATE ON finance_plan_editions TO %I',app_role);
  EXECUTE format('GRANT SELECT,INSERT ON finance_report_snapshots TO %I',app_role);
 END IF; END; $$;
INSERT INTO work_cache_revisions(table_name) VALUES('finance_plan_editions'),('finance_report_snapshots') ON CONFLICT DO NOTHING;

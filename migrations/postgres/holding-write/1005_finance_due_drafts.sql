CREATE TABLE finance_due_drafts (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,holding_id TEXT NOT NULL,kind TEXT NOT NULL CHECK(kind IN ('billing','recognition','asset_depreciation','instrument_payment')),
 source_id TEXT NOT NULL,run_on TEXT NOT NULL,source_version BIGINT NOT NULL,input_json TEXT NOT NULL CHECK(erpx_json_valid(input_json)),input_hash TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'scheduled' CHECK(status IN ('scheduled','blocked','prepared','cancelled')),version BIGINT NOT NULL DEFAULT 1,
 delegated_by TEXT NOT NULL,assignment_id TEXT NOT NULL,assignment_version BIGINT NOT NULL,role TEXT NOT NULL,
 reason TEXT NOT NULL,result_json TEXT,last_code TEXT,last_message TEXT,attempts BIGINT NOT NULL DEFAULT 0,next_attempt_at BIGINT NOT NULL DEFAULT 0,
 created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL
);
CREATE INDEX finance_due_drafts_pending ON finance_due_drafts(run_on,next_attempt_at,id) WHERE status IN ('scheduled','blocked');
CREATE FUNCTION protect_finance_due_draft() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR (NEW.id,NEW.company_id,NEW.holding_id,NEW.kind,NEW.source_id,NEW.run_on,NEW.source_version,NEW.input_json,NEW.input_hash,NEW.delegated_by,NEW.assignment_id,NEW.assignment_version,NEW.role,NEW.reason,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.holding_id,OLD.kind,OLD.source_id,OLD.run_on,OLD.source_version,OLD.input_json,OLD.input_hash,OLD.delegated_by,OLD.assignment_id,OLD.assignment_version,OLD.role,OLD.reason,OLD.created_at)
 OR (OLD.status IN ('prepared','cancelled') AND NEW IS DISTINCT FROM OLD) THEN RAISE EXCEPTION 'FINANCE_SCHEDULE_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';END IF;RETURN NEW;END;$$;
CREATE TRIGGER finance_due_draft_immutable BEFORE UPDATE OR DELETE ON finance_due_drafts FOR EACH ROW EXECUTE FUNCTION protect_finance_due_draft();
INSERT INTO work_cache_revisions(table_name) VALUES('finance_due_drafts') ON CONFLICT DO NOTHING;
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON finance_due_drafts FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON finance_due_drafts FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
DO $$ DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app');BEGIN
 IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN EXECUTE format('GRANT SELECT,INSERT,UPDATE ON finance_due_drafts TO %I',app_role);END IF;END;$$;

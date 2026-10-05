CREATE TABLE finance_asset_policy_editions (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,asset_id TEXT NOT NULL REFERENCES finance_assets(id),parent_id TEXT REFERENCES finance_asset_policy_editions(id),
 version BIGINT NOT NULL DEFAULT 1,status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','active','superseded','rejected')),
 content_json TEXT NOT NULL CHECK(erpx_json_valid(content_json)),source_hash TEXT NOT NULL,prepared_by TEXT NOT NULL,reviewed_by TEXT,reason TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by)
);
CREATE UNIQUE INDEX finance_asset_policy_active ON finance_asset_policy_editions(asset_id) WHERE status='active';
CREATE UNIQUE INDEX finance_asset_policy_draft ON finance_asset_policy_editions(asset_id) WHERE status='draft';
CREATE TABLE finance_asset_policy_runs (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,asset_id TEXT NOT NULL REFERENCES finance_assets(id),policy_id TEXT NOT NULL REFERENCES finance_asset_policy_editions(id),
 ordinal BIGINT NOT NULL,entry_date TEXT NOT NULL,version BIGINT NOT NULL DEFAULT 1,status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','posted','rejected','reversed')),
 input_json TEXT NOT NULL CHECK(erpx_json_valid(input_json)),result_json TEXT NOT NULL CHECK(erpx_json_valid(result_json)),source_hash TEXT NOT NULL,
 journal_ids_json TEXT NOT NULL DEFAULT '[]' CHECK(erpx_json_valid(journal_ids_json)),reversal_ids_json TEXT NOT NULL DEFAULT '[]' CHECK(erpx_json_valid(reversal_ids_json)),
 prepared_by TEXT NOT NULL,reviewed_by TEXT,reason TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by)
);
CREATE UNIQUE INDEX finance_asset_policy_run_current ON finance_asset_policy_runs(asset_id,ordinal) WHERE status IN ('draft','posted');
CREATE FUNCTION protect_finance_asset_policy() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR (NEW.id,NEW.company_id,NEW.asset_id,NEW.parent_id,NEW.content_json,NEW.source_hash,NEW.prepared_by,NEW.reason,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.asset_id,OLD.parent_id,OLD.content_json,OLD.source_hash,OLD.prepared_by,OLD.reason,OLD.created_at) THEN RAISE EXCEPTION 'ASSET_POLICY_EDITION_IMMUTABLE' USING ERRCODE='23514';END IF;RETURN NEW;END;$$;
CREATE TRIGGER finance_asset_policy_immutable BEFORE UPDATE OR DELETE ON finance_asset_policy_editions FOR EACH ROW EXECUTE FUNCTION protect_finance_asset_policy();
CREATE FUNCTION protect_finance_asset_policy_run() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR OLD.status IN ('rejected','reversed') OR (NEW.id,NEW.company_id,NEW.asset_id,NEW.policy_id,NEW.ordinal,NEW.entry_date,NEW.input_json,NEW.result_json,NEW.source_hash,NEW.prepared_by,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.asset_id,OLD.policy_id,OLD.ordinal,OLD.entry_date,OLD.input_json,OLD.result_json,OLD.source_hash,OLD.prepared_by,OLD.created_at) THEN RAISE EXCEPTION 'ASSET_POLICY_RUN_IMMUTABLE' USING ERRCODE='23514';END IF;RETURN NEW;END;$$;
CREATE TRIGGER finance_asset_policy_run_immutable BEFORE UPDATE OR DELETE ON finance_asset_policy_runs FOR EACH ROW EXECUTE FUNCTION protect_finance_asset_policy_run();
DO $$ DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app');BEGIN
 IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN EXECUTE format('GRANT SELECT,INSERT,UPDATE ON finance_asset_policy_editions,finance_asset_policy_runs TO %I',app_role);END IF;
END;$$;
INSERT INTO work_cache_revisions(table_name) VALUES('finance_asset_policy_editions'),('finance_asset_policy_runs') ON CONFLICT DO NOTHING;

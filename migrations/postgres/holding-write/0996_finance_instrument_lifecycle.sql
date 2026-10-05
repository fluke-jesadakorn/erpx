ALTER TABLE finance_company_tools DROP CONSTRAINT finance_company_tools_tool_check;
ALTER TABLE finance_company_tools ADD CONSTRAINT finance_company_tools_tool_check CHECK(tool IN ('planning','bank_automation','collections','business_contracts','instruments','asset_policies'));
CREATE TABLE finance_instrument_editions (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL,root_id TEXT NOT NULL,parent_id TEXT REFERENCES finance_instrument_editions(id),
 kind TEXT NOT NULL CHECK(kind IN ('lease','loan')),number TEXT NOT NULL,name TEXT NOT NULL,chart_id TEXT NOT NULL REFERENCES finance_charts(id),
 edition BIGINT NOT NULL,version BIGINT NOT NULL DEFAULT 1,status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','active','superseded','rejected','closed')),
 content_json TEXT NOT NULL CHECK(erpx_json_valid(content_json)),schedule_json TEXT NOT NULL CHECK(erpx_json_valid(schedule_json)),source_hash TEXT NOT NULL,
 prepared_by TEXT NOT NULL,reviewed_by TEXT,activation_journal_id TEXT REFERENCES finance_journals(id),reason TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by)
);
CREATE UNIQUE INDEX finance_instrument_number ON finance_instrument_editions(company_id,number) WHERE parent_id IS NULL AND status<>'rejected';
CREATE UNIQUE INDEX finance_instrument_active ON finance_instrument_editions(root_id) WHERE status='active';
CREATE UNIQUE INDEX finance_instrument_amendment ON finance_instrument_editions(parent_id) WHERE parent_id IS NOT NULL AND status<>'rejected';
CREATE TABLE finance_instrument_runs (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,instrument_id TEXT NOT NULL REFERENCES finance_instrument_editions(id),kind TEXT NOT NULL CHECK(kind IN ('payment','payoff')),
 ordinal BIGINT NOT NULL,version BIGINT NOT NULL DEFAULT 1,status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','posted','rejected','reversed')),
 input_json TEXT NOT NULL CHECK(erpx_json_valid(input_json)),result_json TEXT NOT NULL CHECK(erpx_json_valid(result_json)),source_hash TEXT NOT NULL,
 bank_transaction_id TEXT NOT NULL REFERENCES finance_bank_transactions(id),journal_id TEXT REFERENCES finance_journals(id),reversal_journal_id TEXT REFERENCES finance_journals(id),
 prepared_by TEXT NOT NULL,reviewed_by TEXT,reason TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by)
);
CREATE UNIQUE INDEX finance_instrument_run_current ON finance_instrument_runs(instrument_id,ordinal) WHERE status IN ('draft','posted');
CREATE FUNCTION protect_finance_instrument_edition() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR (NEW.id,NEW.company_id,NEW.root_id,NEW.parent_id,NEW.kind,NEW.number,NEW.name,NEW.chart_id,NEW.edition,NEW.content_json,NEW.schedule_json,NEW.source_hash,NEW.prepared_by,NEW.reason,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.root_id,OLD.parent_id,OLD.kind,OLD.number,OLD.name,OLD.chart_id,OLD.edition,OLD.content_json,OLD.schedule_json,OLD.source_hash,OLD.prepared_by,OLD.reason,OLD.created_at) THEN RAISE EXCEPTION 'INSTRUMENT_EDITION_IMMUTABLE' USING ERRCODE='23514';END IF;RETURN NEW;
END;$$;
CREATE TRIGGER finance_instrument_edition_immutable BEFORE UPDATE OR DELETE ON finance_instrument_editions FOR EACH ROW EXECUTE FUNCTION protect_finance_instrument_edition();
CREATE FUNCTION protect_finance_instrument_run() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR OLD.status IN ('rejected','reversed') OR (NEW.id,NEW.company_id,NEW.instrument_id,NEW.kind,NEW.ordinal,NEW.input_json,NEW.result_json,NEW.source_hash,NEW.bank_transaction_id,NEW.prepared_by,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.instrument_id,OLD.kind,OLD.ordinal,OLD.input_json,OLD.result_json,OLD.source_hash,OLD.bank_transaction_id,OLD.prepared_by,OLD.created_at) THEN RAISE EXCEPTION 'INSTRUMENT_RUN_IMMUTABLE' USING ERRCODE='23514';END IF;RETURN NEW;
END;$$;
CREATE TRIGGER finance_instrument_run_immutable BEFORE UPDATE OR DELETE ON finance_instrument_runs FOR EACH ROW EXECUTE FUNCTION protect_finance_instrument_run();
DO $$ DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app');BEGIN
 IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN EXECUTE format('GRANT SELECT,INSERT,UPDATE ON finance_instrument_editions,finance_instrument_runs TO %I',app_role);END IF;
END;$$;
INSERT INTO work_cache_revisions(table_name) VALUES('finance_instrument_editions'),('finance_instrument_runs') ON CONFLICT DO NOTHING;

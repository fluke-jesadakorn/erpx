ALTER TABLE finance_company_tools DROP CONSTRAINT finance_company_tools_tool_check;
ALTER TABLE finance_company_tools ADD CONSTRAINT finance_company_tools_tool_check CHECK(tool IN ('planning','bank_automation','collections','business_contracts'));
CREATE TABLE finance_business_contracts (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, root_id TEXT NOT NULL, parent_id TEXT REFERENCES finance_business_contracts(id),
 number TEXT NOT NULL, name TEXT NOT NULL, chart_id TEXT NOT NULL REFERENCES finance_charts(id), party_id TEXT NOT NULL REFERENCES finance_parties(id),
 start_date TEXT NOT NULL,end_date TEXT NOT NULL, edition BIGINT NOT NULL, version BIGINT NOT NULL DEFAULT 1,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','active','rejected','cancelled','superseded')),
 content_json TEXT NOT NULL CHECK(erpx_json_valid(content_json)), source_hash TEXT NOT NULL,
 prepared_by TEXT NOT NULL, reviewed_by TEXT, reason TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(end_date>=start_date),CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by)
);
CREATE UNIQUE INDEX finance_business_contract_number ON finance_business_contracts(company_id,number) WHERE parent_id IS NULL AND status<>'rejected';
CREATE UNIQUE INDEX finance_business_contract_amendment ON finance_business_contracts(parent_id) WHERE parent_id IS NOT NULL AND status<>'rejected';
CREATE UNIQUE INDEX finance_business_contract_active ON finance_business_contracts(root_id) WHERE status='active';
CREATE TABLE finance_business_billing_runs (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, contract_id TEXT NOT NULL REFERENCES finance_business_contracts(id),root_id TEXT NOT NULL,
 contract_version BIGINT NOT NULL,billing_key TEXT NOT NULL,service_from TEXT NOT NULL,service_to TEXT NOT NULL,
 input_json TEXT NOT NULL CHECK(erpx_json_valid(input_json)),result_json TEXT NOT NULL CHECK(erpx_json_valid(result_json)),
 source_hash TEXT NOT NULL,invoice_id TEXT NOT NULL REFERENCES finance_invoices(id),invoice_version BIGINT NOT NULL,
 created_by TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(root_id,billing_key)
);
CREATE TABLE finance_recognition_runs (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL,billing_run_id TEXT NOT NULL REFERENCES finance_business_billing_runs(id),item_key TEXT NOT NULL,
 due_date TEXT NOT NULL,entry_date TEXT NOT NULL,period_id TEXT NOT NULL REFERENCES finance_periods(id),
 amount_minor BIGINT NOT NULL CHECK(amount_minor>0),account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','posted','rejected','reversed')),version BIGINT NOT NULL DEFAULT 1,
 source_hash TEXT NOT NULL,journal_id TEXT REFERENCES finance_journals(id),reversal_journal_id TEXT REFERENCES finance_journals(id),
 prepared_by TEXT NOT NULL,reviewed_by TEXT,reason TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by)
);
CREATE UNIQUE INDEX finance_recognition_current ON finance_recognition_runs(billing_run_id,item_key) WHERE status IN ('draft','posted');
CREATE FUNCTION protect_finance_business_contract() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR (NEW.id,NEW.company_id,NEW.root_id,NEW.parent_id,NEW.number,NEW.name,NEW.chart_id,NEW.party_id,NEW.start_date,NEW.end_date,NEW.edition,NEW.content_json,NEW.source_hash,NEW.prepared_by,NEW.reason,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.root_id,OLD.parent_id,OLD.number,OLD.name,OLD.chart_id,OLD.party_id,OLD.start_date,OLD.end_date,OLD.edition,OLD.content_json,OLD.source_hash,OLD.prepared_by,OLD.reason,OLD.created_at) THEN RAISE EXCEPTION 'BUSINESS_CONTRACT_EDITION_IMMUTABLE' USING ERRCODE='23514';END IF;RETURN NEW;
END;$$;
CREATE TRIGGER finance_business_contract_immutable BEFORE UPDATE OR DELETE ON finance_business_contracts FOR EACH ROW EXECUTE FUNCTION protect_finance_business_contract();
CREATE TRIGGER finance_billing_run_immutable BEFORE UPDATE OR DELETE ON finance_business_billing_runs FOR EACH ROW EXECUTE FUNCTION protect_financial_event();
CREATE FUNCTION protect_finance_recognition_run() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR OLD.status IN ('rejected','reversed') OR (NEW.id,NEW.company_id,NEW.billing_run_id,NEW.item_key,NEW.due_date,NEW.entry_date,NEW.period_id,NEW.amount_minor,NEW.account_id,NEW.source_hash,NEW.prepared_by,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.billing_run_id,OLD.item_key,OLD.due_date,OLD.entry_date,OLD.period_id,OLD.amount_minor,OLD.account_id,OLD.source_hash,OLD.prepared_by,OLD.created_at) THEN RAISE EXCEPTION 'RECOGNITION_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';END IF;RETURN NEW;
END;$$;
CREATE TRIGGER finance_recognition_immutable BEFORE UPDATE OR DELETE ON finance_recognition_runs FOR EACH ROW EXECUTE FUNCTION protect_finance_recognition_run();
DO $$ DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app'); BEGIN
 IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
 EXECUTE format('GRANT SELECT,INSERT,UPDATE ON finance_business_contracts,finance_recognition_runs TO %I',app_role);
 EXECUTE format('GRANT SELECT,INSERT ON finance_business_billing_runs TO %I',app_role);
 END IF; END;$$;
INSERT INTO work_cache_revisions(table_name) VALUES('finance_business_contracts'),('finance_business_billing_runs'),('finance_recognition_runs') ON CONFLICT DO NOTHING;

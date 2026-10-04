CREATE TABLE finance_reporting_revisions (
 chart_id TEXT PRIMARY KEY REFERENCES finance_charts(id),
 revision BIGINT NOT NULL DEFAULT 0 CHECK(revision>=0)
);
INSERT INTO finance_reporting_revisions(chart_id) SELECT id FROM finance_charts;
CREATE FUNCTION finance_reporting_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE ledger TEXT;
BEGIN
 IF TG_TABLE_NAME='finance_charts' THEN ledger:=COALESCE(NEW.id,OLD.id);
 ELSIF TG_TABLE_NAME='finance_accounts' THEN ledger:=COALESCE(NEW.chart_id,OLD.chart_id);
 ELSIF TG_TABLE_NAME='finance_journals' THEN ledger:=COALESCE(NEW.chart_id,OLD.chart_id);
 ELSE SELECT chart_id INTO ledger FROM finance_journals WHERE id=COALESCE(NEW.journal_id,OLD.journal_id);
 END IF;
 IF ledger IS NOT NULL THEN INSERT INTO finance_reporting_revisions(chart_id,revision) VALUES(ledger,1) ON CONFLICT(chart_id) DO UPDATE SET revision=finance_reporting_revisions.revision+1; END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER finance_reporting_change AFTER INSERT OR UPDATE ON finance_charts FOR EACH ROW EXECUTE FUNCTION finance_reporting_changed();
CREATE TRIGGER finance_reporting_change AFTER INSERT OR UPDATE OR DELETE ON finance_accounts FOR EACH ROW EXECUTE FUNCTION finance_reporting_changed();
CREATE TRIGGER finance_reporting_change AFTER INSERT OR UPDATE OR DELETE ON finance_journals FOR EACH ROW EXECUTE FUNCTION finance_reporting_changed();
CREATE TRIGGER finance_reporting_change AFTER INSERT OR UPDATE OR DELETE ON finance_journal_lines FOR EACH ROW EXECUTE FUNCTION finance_reporting_changed();

CREATE TABLE finance_statement_packages (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,chart_id TEXT NOT NULL REFERENCES finance_charts(id),
 profile_id TEXT NOT NULL REFERENCES finance_accounting_profiles(id),profile_version BIGINT NOT NULL,
 start_date TEXT NOT NULL,end_date TEXT NOT NULL,comparative_start_date TEXT NOT NULL,comparative_end_date TEXT NOT NULL,
 previous_package_id TEXT REFERENCES finance_statement_packages(id),edition BIGINT NOT NULL CHECK(edition>0),
 source_revision BIGINT NOT NULL,source_fingerprint TEXT NOT NULL CHECK(length(source_fingerprint)=64),
 presentation_json TEXT NOT NULL,notes_json TEXT NOT NULL,snapshot_json TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','issued','rejected','cancelled')),
 reason TEXT NOT NULL,prepared_by TEXT NOT NULL,reviewed_by TEXT,review_reason TEXT,reviewed_at BIGINT,
 export_html TEXT,export_sha256 TEXT,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 UNIQUE(chart_id,start_date,end_date,edition),
 CHECK(start_date<=end_date AND comparative_start_date<=comparative_end_date AND comparative_end_date<start_date),
 CHECK(status!='issued' OR (reviewed_by IS NOT NULL AND reviewed_by<>prepared_by AND export_html IS NOT NULL AND export_sha256 IS NOT NULL))
);
CREATE TABLE finance_statement_events (
 id TEXT PRIMARY KEY,package_id TEXT NOT NULL REFERENCES finance_statement_packages(id),company_id TEXT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('prepared','issued','rejected','cancelled')),
 actor_id TEXT NOT NULL,package_version BIGINT NOT NULL,reason TEXT NOT NULL,command_id TEXT NOT NULL UNIQUE,created_at BIGINT NOT NULL
);
CREATE TABLE finance_year_closings (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,chart_id TEXT NOT NULL REFERENCES finance_charts(id),
 profile_id TEXT NOT NULL REFERENCES finance_accounting_profiles(id),profile_version BIGINT NOT NULL,
 start_date TEXT NOT NULL,end_date TEXT NOT NULL,period_id TEXT NOT NULL REFERENCES finance_periods(id),
 retained_earnings_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 source_revision BIGINT NOT NULL,source_fingerprint TEXT NOT NULL CHECK(length(source_fingerprint)=64),
 snapshot_json TEXT NOT NULL,profit_minor BIGINT NOT NULL,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','posted','rejected','cancelled')),
 journal_id TEXT UNIQUE REFERENCES finance_journals(id),reason TEXT NOT NULL,prepared_by TEXT NOT NULL,
 reviewed_by TEXT,review_reason TEXT,reviewed_at BIGINT,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(start_date<end_date),CHECK(status!='posted' OR (reviewed_by IS NOT NULL AND reviewed_by<>prepared_by))
);
CREATE UNIQUE INDEX finance_year_closing_once ON finance_year_closings(chart_id,start_date,end_date) WHERE status IN ('draft','posted');
CREATE FUNCTION finance_fiscal_posting_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='INSERT' OR (NEW.status IN ('posted','reversed') AND OLD.status NOT IN ('posted','reversed')) THEN
  PERFORM chart_id FROM finance_reporting_revisions WHERE chart_id=NEW.chart_id FOR UPDATE;
  IF EXISTS(SELECT 1 FROM finance_year_closings WHERE company_id=NEW.company_id AND chart_id=NEW.chart_id AND status='posted' AND NEW.entry_date BETWEEN start_date AND end_date) THEN
   RAISE EXCEPTION 'FISCAL_YEAR_ALREADY_CLOSED' USING ERRCODE='23514';
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER finance_fiscal_posting_guard BEFORE INSERT OR UPDATE ON finance_journals FOR EACH ROW EXECUTE FUNCTION finance_fiscal_posting_guard();
CREATE TABLE finance_year_closing_events (
 id TEXT PRIMARY KEY,closing_id TEXT NOT NULL REFERENCES finance_year_closings(id),company_id TEXT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('prepared','posted','rejected','cancelled')),
 actor_id TEXT NOT NULL,closing_version BIGINT NOT NULL,reason TEXT NOT NULL,command_id TEXT NOT NULL UNIQUE,created_at BIGINT NOT NULL
);
CREATE FUNCTION finance_reporting_evidence_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR TG_TABLE_NAME IN ('finance_statement_events','finance_year_closing_events') THEN RAISE EXCEPTION 'FINANCE_REPORTING_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF OLD.status IN ('issued','posted','rejected','cancelled') AND NEW IS DISTINCT FROM OLD THEN RAISE EXCEPTION 'FINANCE_ISSUED_REPORT_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF (to_jsonb(NEW)-ARRAY['status','reviewed_by','review_reason','reviewed_at','export_html','export_sha256','journal_id','version','updated_at']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','reviewed_by','review_reason','reviewed_at','export_html','export_sha256','journal_id','version','updated_at']) THEN RAISE EXCEPTION 'FINANCE_REPORTING_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF NEW.status IN ('issued','posted') AND NEW.reviewed_by=NEW.prepared_by THEN RAISE EXCEPTION 'FINANCE_REPORTING_INDEPENDENT_REVIEW_REQUIRED' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
DO $$ DECLARE t TEXT; owner_role TEXT; runtime_role TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['finance_statement_packages','finance_statement_events','finance_year_closings','finance_year_closing_events'] LOOP
  EXECUTE format('CREATE TRIGGER finance_reporting_guard BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION finance_reporting_evidence_guard()',t);
  INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
  EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
  EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
 owner_role:=current_user;
 IF right(owner_role,10)='_migration' THEN
  FOREACH runtime_role IN ARRAY ARRAY[left(owner_role,length(owner_role)-10)||'_app',left(owner_role,length(owner_role)-10)||'_jobs'] LOOP
   IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=runtime_role) THEN
    EXECUTE format('GRANT SELECT,INSERT,UPDATE,DELETE ON finance_reporting_revisions,finance_statement_packages,finance_statement_events,finance_year_closings,finance_year_closing_events TO %I',runtime_role);
   END IF;
  END LOOP;
 END IF;
END $$;

CREATE TABLE finance_accounting_profiles (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  statutory_chart_id TEXT NOT NULL REFERENCES finance_charts(id),
  standard TEXT NOT NULL CHECK(standard='TFRS for NPAEs'),
  fiscal_start_month BIGINT NOT NULL CHECK(fiscal_start_month BETWEEN 1 AND 12),
  effective_date TEXT NOT NULL,
  eligibility_evidence TEXT NOT NULL,
  policy_evidence TEXT NOT NULL,
  tax_basis TEXT NOT NULL CHECK(tax_basis='current_tax'),
  inventory_policy TEXT NOT NULL CHECK(inventory_policy='moving_average'),
  depreciation_policy TEXT NOT NULL CHECK(depreciation_policy='straight_line'),
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','rejected')),
  version BIGINT NOT NULL DEFAULT 1,
  prepared_by TEXT NOT NULL,
  reviewed_by TEXT,
  review_reason TEXT,
  created_at BIGINT NOT NULL,
  reviewed_at BIGINT,
  CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by)
);
CREATE UNIQUE INDEX finance_accounting_profiles_approved_company
  ON finance_accounting_profiles(company_id) WHERE status='approved';

-- Posted journal evidence is captured after all owner writes, in the same
-- transaction. Existing evidence is explicitly marked as a migration snapshot.
CREATE TABLE finance_financial_events (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  chart_id TEXT NOT NULL REFERENCES finance_charts(id),
  period_id TEXT NOT NULL REFERENCES finance_periods(id),
  journal_id TEXT NOT NULL UNIQUE REFERENCES finance_journals(id),
  source_type TEXT NOT NULL,
  source_id TEXT,
  source_version BIGINT NOT NULL,
  effective_date TEXT NOT NULL,
  currency TEXT NOT NULL,
  base_currency TEXT NOT NULL,
  event_kind TEXT NOT NULL,
  evidence_json TEXT NOT NULL CHECK(erpx_json_valid(evidence_json)),
  legacy_snapshot BIGINT NOT NULL DEFAULT 0 CHECK(legacy_snapshot IN (0,1)),
  recorded_at BIGINT NOT NULL
);
CREATE INDEX finance_financial_events_scope_date ON finance_financial_events(company_id,chart_id,effective_date,id);

CREATE FUNCTION capture_financial_event() RETURNS TRIGGER LANGUAGE plpgsql SECURITY DEFINER SET search_path=public AS $$
BEGIN
  IF NEW.status NOT IN ('posted','reversed') THEN RETURN NULL; END IF;
  IF TG_OP='UPDATE' AND OLD.status IN ('posted','reversed') THEN RETURN NULL; END IF;
  INSERT INTO finance_financial_events(id,company_id,chart_id,period_id,journal_id,source_type,source_id,source_version,effective_date,currency,base_currency,event_kind,evidence_json,recorded_at)
  SELECT 'journal-event-'||j.id,j.company_id,j.chart_id,j.period_id,j.id,j.source_type,j.source_id,j.version,j.entry_date,j.currency,c.base_currency,
    CASE WHEN j.source_type LIKE '%reversal%' OR j.source_type='reversal' THEN 'reversal'
      WHEN j.source_type IN ('credit_note','debit_note') THEN 'credit_debit_note'
      WHEN j.source_type LIKE '%fx%' THEN 'fx_adjustment'
      WHEN j.source_type LIKE '%payment%' OR j.source_type='return_refund' THEN 'settlement' ELSE 'recognition' END,
    jsonb_build_object('journal',to_jsonb(j),'lines',COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.ordinal) FROM finance_journal_lines l WHERE l.journal_id=j.id),'[]'::jsonb))::text,
    floor(extract(epoch FROM clock_timestamp())*1000)::bigint
  FROM finance_journals j JOIN finance_charts c ON c.id=j.chart_id WHERE j.id=NEW.id
  ON CONFLICT(journal_id) DO NOTHING;
  RETURN NULL;
END;
$$;
CREATE CONSTRAINT TRIGGER finance_journal_event_capture AFTER INSERT OR UPDATE ON finance_journals
  DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION capture_financial_event();

INSERT INTO finance_financial_events(id,company_id,chart_id,period_id,journal_id,source_type,source_id,source_version,effective_date,currency,base_currency,event_kind,evidence_json,legacy_snapshot,recorded_at)
SELECT 'journal-event-'||j.id,j.company_id,j.chart_id,j.period_id,j.id,j.source_type,j.source_id,j.version,j.entry_date,j.currency,c.base_currency,
  'legacy_posting',jsonb_build_object('journal',to_jsonb(j),'lines',COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.ordinal) FROM finance_journal_lines l WHERE l.journal_id=j.id),'[]'::jsonb))::text,1,
  floor(extract(epoch FROM clock_timestamp())*1000)::bigint
FROM finance_journals j JOIN finance_charts c ON c.id=j.chart_id WHERE j.status IN ('posted','reversed');

CREATE FUNCTION protect_financial_event() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'FINANCIAL_EVENT_IMMUTABLE' USING ERRCODE='23514'; END;
$$;
CREATE TRIGGER finance_financial_event_immutable BEFORE UPDATE OR DELETE ON finance_financial_events
  FOR EACH ROW EXECUTE FUNCTION protect_financial_event();

CREATE FUNCTION protect_posted_journal_evidence() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.posted_at IS NOT NULL AND (NEW.company_id,NEW.chart_id,NEW.period_id,NEW.entry_date,NEW.currency,NEW.fx_rate_micros,NEW.source_type,NEW.source_id,NEW.memo,NEW.is_opening)
    IS DISTINCT FROM (OLD.company_id,OLD.chart_id,OLD.period_id,OLD.entry_date,OLD.currency,OLD.fx_rate_micros,OLD.source_type,OLD.source_id,OLD.memo,OLD.is_opening) THEN
    RAISE EXCEPTION 'POSTED_JOURNAL_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END;
$$;
CREATE TRIGGER finance_posted_journal_evidence BEFORE UPDATE ON finance_journals
  FOR EACH ROW EXECUTE FUNCTION protect_posted_journal_evidence();

DO $$
DECLARE app_role TEXT := regexp_replace(current_user,'_migration$','_app');
BEGIN
  IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
    EXECUTE format('GRANT SELECT,INSERT,UPDATE ON finance_accounting_profiles TO %I',app_role);
    EXECUTE format('GRANT SELECT ON finance_financial_events TO %I',app_role);
  END IF;
END;
$$;

INSERT INTO work_cache_revisions(table_name) VALUES ('finance_accounting_profiles'),('finance_financial_events') ON CONFLICT DO NOTHING;

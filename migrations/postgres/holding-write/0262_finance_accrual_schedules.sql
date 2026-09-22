ALTER TABLE finance_accrual_schedules ADD COLUMN frequency TEXT NOT NULL DEFAULT 'monthly';
ALTER TABLE finance_accrual_schedules ADD COLUMN approved_by TEXT;
ALTER TABLE finance_accrual_schedules ADD CONSTRAINT finance_accrual_schedules_frequency_check CHECK(frequency IN ('monthly','quarterly','annual'));

CREATE TABLE finance_accrual_schedule_runs (
  id TEXT PRIMARY KEY,
  schedule_id TEXT NOT NULL REFERENCES finance_accrual_schedules(id),
  company_id TEXT NOT NULL,
  period_start TEXT NOT NULL,
  period_end TEXT NOT NULL,
  amount_minor BIGINT NOT NULL CHECK(amount_minor > 0),
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  journal_id TEXT NOT NULL UNIQUE REFERENCES finance_journals(id),
  schedule_version BIGINT NOT NULL,
  source_hash TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'prepared' CHECK(status IN ('prepared','posted','rejected')),
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  prepared_by TEXT NOT NULL,
  posted_by TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(schedule_id,period_start)
);
CREATE INDEX finance_accrual_schedule_runs_company ON finance_accrual_schedule_runs(company_id,status,period_start);

CREATE TABLE finance_accrual_schedule_events (
  id TEXT PRIMARY KEY,
  schedule_id TEXT NOT NULL REFERENCES finance_accrual_schedules(id),
  run_id TEXT REFERENCES finance_accrual_schedule_runs(id),
  company_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('created','approved','cancelled','prepared','posted')),
  actor_id TEXT NOT NULL,
  schedule_version BIGINT NOT NULL,
  run_version BIGINT,
  reason TEXT NOT NULL,
  evidence_json TEXT NOT NULL CHECK(erpx_json_valid(evidence_json)),
  created_at BIGINT NOT NULL
);
CREATE INDEX finance_accrual_schedule_events_history ON finance_accrual_schedule_events(schedule_id,created_at,id);
CREATE TRIGGER finance_accrual_schedule_event_immutable
  BEFORE UPDATE OR DELETE ON finance_accrual_schedule_events
  FOR EACH ROW EXECUTE FUNCTION iam_immutable();

DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['finance_accrual_schedules','finance_accrual_schedule_runs','finance_accrual_schedule_events'] LOOP INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING; IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=t::regclass AND tgname='work_cache_change') THEN EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t); END IF; IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=t::regclass AND tgname='work_cache_truncate') THEN EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t); END IF; END LOOP; END $$;

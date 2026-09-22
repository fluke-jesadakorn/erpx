CREATE TABLE finance_consolidation_journals (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL UNIQUE REFERENCES finance_consolidation_runs(id),
  holding_id TEXT NOT NULL,
  journal_number TEXT NOT NULL,
  entry_date TEXT NOT NULL,
  currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
  memo TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'posted' CHECK(status IN ('posted','reversed')),
  source_hash TEXT NOT NULL,
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  created_by TEXT NOT NULL,
  posted_by TEXT NOT NULL,
  posted_at BIGINT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(holding_id,journal_number)
);

CREATE TABLE finance_consolidation_journal_lines (
  id TEXT PRIMARY KEY,
  journal_id TEXT NOT NULL REFERENCES finance_consolidation_journals(id),
  account_id TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  debit_minor BIGINT NOT NULL DEFAULT 0 CHECK(debit_minor >= 0),
  credit_minor BIGINT NOT NULL DEFAULT 0 CHECK(credit_minor >= 0),
  ordinal BIGINT NOT NULL CHECK(ordinal >= 0),
  CHECK((debit_minor = 0 AND credit_minor > 0) OR (credit_minor = 0 AND debit_minor > 0)),
  UNIQUE(journal_id,ordinal)
);

ALTER TABLE finance_consolidation_eliminations
  ADD COLUMN version BIGINT NOT NULL DEFAULT 1,
  ADD COLUMN source_transaction_id TEXT REFERENCES finance_intercompany_transactions(id),
  ADD COLUMN consolidation_journal_id TEXT REFERENCES finance_consolidation_journals(id),
  ADD COLUMN created_by TEXT,
  ADD COLUMN created_at BIGINT NOT NULL DEFAULT 0,
  ADD COLUMN updated_at BIGINT NOT NULL DEFAULT 0,
  ADD COLUMN approved_by TEXT,
  ADD COLUMN approved_at BIGINT,
  ADD COLUMN posted_by TEXT,
  ADD COLUMN posted_at BIGINT,
  ADD COLUMN evidence_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(evidence_json));

CREATE UNIQUE INDEX finance_consolidation_eliminations_source_idx
  ON finance_consolidation_eliminations(run_id,source_transaction_id)
  WHERE source_transaction_id IS NOT NULL;
CREATE INDEX finance_consolidation_eliminations_run_idx
  ON finance_consolidation_eliminations(run_id,status,created_at,id);

CREATE TABLE finance_consolidation_elimination_events (
  id TEXT PRIMARY KEY,
  elimination_id TEXT NOT NULL REFERENCES finance_consolidation_eliminations(id),
  run_id TEXT NOT NULL REFERENCES finance_consolidation_runs(id),
  holding_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('proposed','approved','posted')),
  from_status TEXT,
  to_status TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  version BIGINT NOT NULL,
  reason TEXT NOT NULL,
  evidence_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(evidence_json)),
  created_at BIGINT NOT NULL,
  UNIQUE(elimination_id,version)
);
CREATE INDEX finance_consolidation_elimination_events_run_idx
  ON finance_consolidation_elimination_events(run_id,created_at,id);

CREATE FUNCTION finance_consolidation_elimination_events_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'FINANCE_CONSOLIDATION_ELIMINATION_EVENT_IMMUTABLE' USING ERRCODE='23514';
END $$;
CREATE TRIGGER finance_consolidation_elimination_events_immutable
  BEFORE UPDATE OR DELETE ON finance_consolidation_elimination_events
  FOR EACH ROW EXECUTE FUNCTION finance_consolidation_elimination_events_guard();
DO $$ BEGIN
  INSERT INTO work_cache_revisions(table_name) VALUES('finance_consolidation_journals') ON CONFLICT DO NOTHING;
  CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON finance_consolidation_journals FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
  CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON finance_consolidation_journals FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
  INSERT INTO work_cache_revisions(table_name) VALUES('finance_consolidation_journal_lines') ON CONFLICT DO NOTHING;
  CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON finance_consolidation_journal_lines FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
  CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON finance_consolidation_journal_lines FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
  INSERT INTO work_cache_revisions(table_name) VALUES('finance_consolidation_eliminations') ON CONFLICT DO NOTHING;
  CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON finance_consolidation_eliminations FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
  CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON finance_consolidation_eliminations FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
  INSERT INTO work_cache_revisions(table_name) VALUES('finance_consolidation_elimination_events') ON CONFLICT DO NOTHING;
  CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON finance_consolidation_elimination_events FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
  CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON finance_consolidation_elimination_events FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

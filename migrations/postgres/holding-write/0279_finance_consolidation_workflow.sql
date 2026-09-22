ALTER TABLE finance_consolidation_runs ADD COLUMN source_hash TEXT NOT NULL DEFAULT '';
ALTER TABLE finance_consolidation_runs ADD COLUMN reviewed_by TEXT;
ALTER TABLE finance_consolidation_runs ADD COLUMN reviewed_at BIGINT;
ALTER TABLE finance_consolidation_runs ADD COLUMN published_by TEXT;
ALTER TABLE finance_consolidation_runs ADD COLUMN published_at BIGINT;

CREATE TABLE finance_consolidation_events (
  id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES finance_consolidation_runs(id),
  holding_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('prepared','reviewed','approved','published')),
  from_status TEXT,
  to_status TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  version BIGINT NOT NULL,
  reason TEXT NOT NULL,
  evidence_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(evidence_json)),
  created_at BIGINT NOT NULL,
  UNIQUE(run_id,version)
);
CREATE INDEX finance_consolidation_events_run_idx ON finance_consolidation_events(run_id,created_at,id);
CREATE FUNCTION finance_consolidation_events_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'FINANCE_CONSOLIDATION_EVENT_IMMUTABLE' USING ERRCODE='23514';
END $$;
CREATE TRIGGER finance_consolidation_events_immutable BEFORE UPDATE OR DELETE ON finance_consolidation_events FOR EACH ROW EXECUTE FUNCTION finance_consolidation_events_guard();
DO $$ BEGIN
  INSERT INTO work_cache_revisions(table_name) VALUES('finance_consolidation_events') ON CONFLICT DO NOTHING;
  CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON finance_consolidation_events FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
  CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON finance_consolidation_events FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

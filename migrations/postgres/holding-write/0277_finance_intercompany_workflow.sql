CREATE TABLE finance_intercompany_events (
  id TEXT PRIMARY KEY,
  transaction_id TEXT NOT NULL REFERENCES finance_intercompany_transactions(id),
  holding_id TEXT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('created','matched','settled','eliminated')),
  from_status TEXT,
  to_status TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  version BIGINT NOT NULL,
  reason TEXT NOT NULL,
  evidence_json TEXT NOT NULL DEFAULT '{}' CHECK(erpx_json_valid(evidence_json)),
  created_at BIGINT NOT NULL,
  UNIQUE(transaction_id,version)
);
CREATE INDEX finance_intercompany_events_transaction_idx ON finance_intercompany_events(transaction_id,created_at,id);
CREATE FUNCTION finance_intercompany_events_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'FINANCE_INTERCOMPANY_EVENT_IMMUTABLE' USING ERRCODE='23514';
END $$;
CREATE TRIGGER finance_intercompany_events_immutable BEFORE UPDATE OR DELETE ON finance_intercompany_events FOR EACH ROW EXECUTE FUNCTION finance_intercompany_events_guard();
DO $$ BEGIN
  INSERT INTO work_cache_revisions(table_name) VALUES('finance_intercompany_events') ON CONFLICT DO NOTHING;
  CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON finance_intercompany_events FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
  CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON finance_intercompany_events FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

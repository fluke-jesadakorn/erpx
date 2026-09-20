-- Cache revisions commit/roll back with business data, including direct SQL and jobs.
-- Per-table rows avoid a single holding-wide write lock across unrelated domains.
CREATE TABLE work_cache_revisions (
  table_name text PRIMARY KEY,
  revision bigint NOT NULL DEFAULT 0
);

CREATE FUNCTION work_cache_tracks(name text) RETURNS boolean LANGUAGE sql IMMUTABLE AS $$
  SELECT name NOT IN ('work_cache_revisions','erpx_migrations','outbox_events',
    'command_assertions','command_receipts','audit_events','runtime_health',
    'iam_audit','iam_receipts','iam_sync_jobs','um_history','seed_repairs',
    'home_read_cursors','active_work_contexts','work_role_preferences',
    'work_card_preferences','navigation_preferences')
    AND name NOT LIKE '%\_receipts' ESCAPE '\'
$$;

CREATE FUNCTION work_cache_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'UPDATE' AND
    (to_jsonb(OLD) - ARRAY['lease_token','lease_until','claim_until','next_attempt_at','attempts','heartbeat_at'])
    IS NOT DISTINCT FROM
    (to_jsonb(NEW) - ARRAY['lease_token','lease_until','claim_until','next_attempt_at','attempts','heartbeat_at'])
  THEN RETURN NULL; END IF;
  UPDATE work_cache_revisions SET revision=revision+1 WHERE table_name=TG_TABLE_NAME;
  RETURN NULL;
END $$;

DO $$ DECLARE t record;
BEGIN
  FOR t IN SELECT tablename FROM pg_tables WHERE schemaname='public' AND work_cache_tracks(tablename) LOOP
    INSERT INTO work_cache_revisions(table_name) VALUES(t.tablename);
    EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t.tablename);
    EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t.tablename);
  END LOOP;
END $$;

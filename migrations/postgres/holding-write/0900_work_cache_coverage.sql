-- Every tracked native owner must invalidate shared Work reads. Install coverage
-- after additive module migrations without changing excluded housekeeping tables.
DO $$ DECLARE t text; BEGIN
 FOR t IN SELECT tablename FROM pg_tables p WHERE schemaname='public' AND work_cache_tracks(tablename) AND NOT EXISTS(SELECT 1 FROM work_cache_revisions r WHERE r.table_name=p.tablename) LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass(t) AND tgname='work_cache_change') THEN
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 END IF;
 IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass(t) AND tgname='work_cache_truncate') THEN
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END IF;
 END LOOP;
END $$;

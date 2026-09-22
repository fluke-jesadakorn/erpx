-- Run after every holding migration batch, including future additive tables.
CREATE FUNCTION ensure_work_cache_coverage() RETURNS void LANGUAGE plpgsql AS $$
DECLARE t record;
BEGIN
  FOR t IN SELECT tablename FROM pg_tables WHERE schemaname='public' AND work_cache_tracks(tablename) ORDER BY tablename LOOP
    INSERT INTO work_cache_revisions(table_name) VALUES(t.tablename) ON CONFLICT DO NOTHING;
    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass(format('public.%I',t.tablename)) AND tgname='work_cache_change') THEN
      EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON public.%I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t.tablename);
      UPDATE work_cache_revisions SET revision=revision+1 WHERE table_name=t.tablename;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass(format('public.%I',t.tablename)) AND tgname='work_cache_truncate') THEN
      EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON public.%I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t.tablename);
      UPDATE work_cache_revisions SET revision=revision+1 WHERE table_name=t.tablename;
    END IF;
  END LOOP;
END $$;
SELECT ensure_work_cache_coverage();

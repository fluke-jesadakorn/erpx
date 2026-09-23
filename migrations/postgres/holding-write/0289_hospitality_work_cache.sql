-- Both tables affect pilot Work queues. Existing installations have already
-- applied 0900, so add their invalidation triggers explicitly.
DO $$ DECLARE name text; BEGIN
  FOREACH name IN ARRAY ARRAY['pos_room_charge_reversals','company_feature_rollouts'] LOOP
    INSERT INTO work_cache_revisions(table_name) VALUES(name) ON CONFLICT DO NOTHING;
    IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass(name) AND tgname='work_cache_change') THEN
      EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',name);
    END IF;
    IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=to_regclass(name) AND tgname='work_cache_truncate') THEN
      EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',name);
    END IF;
  END LOOP;
END $$;

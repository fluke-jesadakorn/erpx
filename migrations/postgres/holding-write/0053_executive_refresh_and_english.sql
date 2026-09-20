ALTER TABLE ai_workspace_turns ADD COLUMN english_answer_json TEXT;
-- Extend existing transactional invalidation to the newly added canvas tables.
DO $$ DECLARE name text;
BEGIN
 FOREACH name IN ARRAY ARRAY['executive_reports','executive_report_revisions','executive_report_shares','executive_generation_jobs','ai_workspace_turns'] LOOP
  IF NOT EXISTS(SELECT 1 FROM work_cache_revisions WHERE table_name=name) THEN
   INSERT INTO work_cache_revisions(table_name) VALUES(name);
   EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',name);
   EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',name);
  END IF;
 END LOOP;
END $$;

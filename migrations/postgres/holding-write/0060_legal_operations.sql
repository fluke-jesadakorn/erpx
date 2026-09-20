CREATE TABLE legal_notifications(id TEXT PRIMARY KEY,item_id TEXT NOT NULL REFERENCES legal_items(id),user_id TEXT NOT NULL,kind TEXT NOT NULL,created_at BIGINT NOT NULL,acknowledged_at BIGINT);
CREATE TABLE legal_external_uploads(id TEXT PRIMARY KEY,share_id TEXT NOT NULL REFERENCES legal_shares(id),object_key TEXT NOT NULL,checksum TEXT NOT NULL,name TEXT NOT NULL,media_type TEXT NOT NULL,bytes BIGINT NOT NULL,created_by TEXT NOT NULL,created_at BIGINT NOT NULL);
-- New source tables participate in the existing fail-closed cache coverage fence.
DO $$ DECLARE t record; BEGIN
 FOR t IN SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename LIKE 'legal_%' AND work_cache_tracks(tablename) LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t.tablename) ON CONFLICT DO NOTHING;
 IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=t.tablename::regclass AND tgname='work_cache_change') THEN
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t.tablename);
 END IF;
 END LOOP;
END $$;

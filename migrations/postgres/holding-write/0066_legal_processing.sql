CREATE TABLE legal_trust_material(id text PRIMARY KEY,company_id text NOT NULL,kind text NOT NULL CHECK(kind IN ('root','intermediate','crl','ocsp')),name text NOT NULL,source text NOT NULL,content text NOT NULL,checksum text NOT NULL,state text NOT NULL CHECK(state IN ('proposed','approved','retired')),created_by text NOT NULL,approved_by text,created_at bigint NOT NULL,version integer NOT NULL DEFAULT 1);
CREATE TABLE legal_processing_jobs(id text PRIMARY KEY,company_id text NOT NULL,item_id text NOT NULL REFERENCES legal_items(id),document_id text NOT NULL REFERENCES legal_documents(id),checksum text NOT NULL,kind text NOT NULL CHECK(kind IN ('ocr','verify')),state text NOT NULL CHECK(state IN ('queued','running','completed','failed','cancelled')),attempts integer NOT NULL DEFAULT 0,lease_token text,lease_until bigint,actor_json text NOT NULL,created_by text NOT NULL,created_at bigint NOT NULL,result_json text,error text);
CREATE TABLE legal_invitation_jobs(id text PRIMARY KEY,share_id text NOT NULL REFERENCES legal_shares(id),holding_slug text NOT NULL,state text NOT NULL CHECK(state IN ('queued','running','sent','failed','cancelled')),attempts integer NOT NULL DEFAULT 0,lease_token text,lease_until bigint,next_attempt_at bigint NOT NULL DEFAULT 0,error text,created_at bigint NOT NULL,sent_at bigint);
DO $$ DECLARE t text; BEGIN
 FOREACH t IN ARRAY ARRAY['legal_trust_material','legal_processing_jobs','legal_invitation_jobs'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t);
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;

CREATE TABLE legal_retention_policies(id text PRIMARY KEY,company_id text NOT NULL,name text NOT NULL,days integer NOT NULL CHECK(days>=1),basis text NOT NULL,state text NOT NULL CHECK(state IN ('draft','approved','retired')),created_by text NOT NULL,approved_by text,version integer NOT NULL DEFAULT 1,created_at bigint NOT NULL);
CREATE TABLE legal_retention_assignments(item_id text PRIMARY KEY REFERENCES legal_items(id),policy_id text NOT NULL REFERENCES legal_retention_policies(id),policy_version integer NOT NULL,eligible_at bigint NOT NULL,assigned_by text NOT NULL,created_at bigint NOT NULL);
CREATE TABLE legal_destructions(id text PRIMARY KEY,item_id text NOT NULL REFERENCES legal_items(id),policy_id text NOT NULL,policy_version integer NOT NULL,requested_by text NOT NULL,approved_by text,state text NOT NULL CHECK(state IN ('requested','approved','executing','completed','blocked','cancelled','failed')),reason text NOT NULL,manifest jsonb NOT NULL,created_at bigint NOT NULL,approved_at bigint,completed_at bigint,error text,version integer NOT NULL DEFAULT 1);
CREATE UNIQUE INDEX legal_one_destruction ON legal_destructions(item_id) WHERE state NOT IN ('completed','cancelled');
CREATE TABLE legal_disposals(document_id text PRIMARY KEY REFERENCES legal_documents(id),destruction_id text NOT NULL REFERENCES legal_destructions(id),checksum text NOT NULL,disposed_at bigint NOT NULL);
CREATE TABLE legal_file_preservations(object_key text NOT NULL,item_id text NOT NULL REFERENCES legal_items(id),source_type text NOT NULL,source_id text NOT NULL,checksum text NOT NULL,created_by text NOT NULL,created_at bigint NOT NULL,PRIMARY KEY(object_key,item_id));
-- A controlled worker may remove content only while the matching approved
-- destruction is executing and no hold protects the record. Metadata survives.
CREATE OR REPLACE FUNCTION legal_document_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'LEGAL_DOCUMENT_PRESERVATION_REQUIRED'; END IF;
 IF NEW.body='' AND NEW.object_key IS NULL AND (to_jsonb(NEW)-'body'-'object_key')=(to_jsonb(OLD)-'body'-'object_key') AND EXISTS(SELECT 1 FROM legal_destructions x WHERE x.item_id=OLD.item_id AND x.state='executing') AND NOT EXISTS(SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=OLD.item_id AND h.state!='released') THEN RETURN NEW; END IF;
 IF (to_jsonb(NEW)-'locked') IS DISTINCT FROM (to_jsonb(OLD)-'locked') OR (OLD.locked AND NOT NEW.locked) THEN RAISE EXCEPTION 'LEGAL_DOCUMENT_IMMUTABLE'; END IF;
 RETURN NEW; END $$;
DO $$ DECLARE t text; BEGIN
 FOREACH t IN ARRAY ARRAY['legal_retention_policies','legal_retention_assignments','legal_destructions','legal_disposals','legal_file_preservations'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t);
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
CREATE FUNCTION legal_attachment_preserved() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE source text; BEGIN
 PERFORM pg_advisory_xact_lock(hashtextextended('legal-files',0));
 source := CASE WHEN TG_TABLE_NAME='attachments' THEN OLD.id ELSE OLD.attachment_id END;
 IF EXISTS(SELECT 1 FROM legal_file_preservations p JOIN legal_hold_targets t ON t.item_id=p.item_id JOIN legal_holds h ON h.id=t.hold_id WHERE p.source_type='attachment' AND p.source_id=source AND h.state!='released') THEN
  IF TG_OP='DELETE' OR NEW.object_key IS DISTINCT FROM OLD.object_key THEN RAISE EXCEPTION 'LEGAL_HOLD_BLOCKS_FILE_MUTATION'; END IF;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE TRIGGER legal_attachment_preserved BEFORE UPDATE OR DELETE ON attachments FOR EACH ROW EXECUTE FUNCTION legal_attachment_preserved();
CREATE TRIGGER legal_derivative_preserved BEFORE UPDATE OR DELETE ON attachment_derivatives FOR EACH ROW EXECUTE FUNCTION legal_attachment_preserved();
INSERT INTO iam_permissions(id,definition) VALUES('legal.destroy','{"id":"legal.destroy","label":"Authorize Legal destruction","module":"legal","namespace":"customer","availability":"live","risk":"privileged","roles":["holding_owner","legal_manager"],"conditions":["Independent approval; no active hold; approved retention policy"]}');
INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||'["legal.destroy"]'::jsonb,v.grantable_json||'["legal.destroy"]'::jsonb,v.scopes_json,'system:legal',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id='*' AND r.id IN ('holding_owner','legal_manager');
UPDATE iam_roles SET published_revision=published_revision+1 WHERE company_id='*' AND id IN ('holding_owner','legal_manager');

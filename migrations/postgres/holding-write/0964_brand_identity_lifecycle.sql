CREATE TABLE brand_identity_drafts (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,based_revision BIGINT NOT NULL DEFAULT 0,
 identity_json TEXT NOT NULL CHECK(erpx_json_valid(identity_json)),
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','published','cancelled')),
 version BIGINT NOT NULL DEFAULT 1,created_by TEXT NOT NULL,updated_by TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL
);
CREATE TABLE brand_identity_versions (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,revision BIGINT NOT NULL,identity_json TEXT NOT NULL CHECK(erpx_json_valid(identity_json)),
 draft_id TEXT NOT NULL REFERENCES brand_identity_drafts(id),draft_version BIGINT NOT NULL,created_by TEXT NOT NULL,published_by TEXT NOT NULL,published_at BIGINT NOT NULL,
 UNIQUE(company_id,revision),CHECK(created_by<>published_by)
);
CREATE TABLE brand_identity_active (
 company_id TEXT PRIMARY KEY,identity_version_id TEXT NOT NULL REFERENCES brand_identity_versions(id),version BIGINT NOT NULL DEFAULT 1,
 updated_by TEXT NOT NULL,updated_at BIGINT NOT NULL
);
CREATE TABLE brand_category_locks (
 company_id TEXT NOT NULL,category TEXT NOT NULL CHECK(category IN ('identity','visual')),locked BOOLEAN NOT NULL,
 approved_snapshot_json TEXT NOT NULL CHECK(erpx_json_valid(approved_snapshot_json)),version BIGINT NOT NULL,
 updated_by TEXT NOT NULL,reason TEXT NOT NULL,updated_at BIGINT NOT NULL,PRIMARY KEY(company_id,category)
);
CREATE TABLE brand_identity_events (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,record_id TEXT NOT NULL,action TEXT NOT NULL,version BIGINT NOT NULL,
 actor_id TEXT NOT NULL,reason TEXT NOT NULL,details_json TEXT NOT NULL CHECK(erpx_json_valid(details_json)),created_at BIGINT NOT NULL
);
CREATE INDEX brand_draft_scope ON brand_identity_drafts(company_id,status,updated_at);
CREATE INDEX brand_history_scope ON brand_identity_events(company_id,created_at,id);
CREATE FUNCTION brand_identity_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR TG_TABLE_NAME IN ('brand_identity_versions','brand_identity_events') THEN RAISE EXCEPTION 'BRAND_IDENTITY_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF TG_TABLE_NAME='brand_identity_drafts' THEN
  IF (NEW.id,NEW.company_id,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.created_by,OLD.created_at)
   OR OLD.status<>'draft' AND NEW IS DISTINCT FROM OLD
  THEN RAISE EXCEPTION 'BRAND_DRAFT_IMMUTABLE' USING ERRCODE='23514'; END IF;
 END IF;
 RETURN NEW;
END $$;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['brand_identity_drafts','brand_identity_versions','brand_identity_active','brand_category_locks','brand_identity_events'] LOOP
  EXECUTE format('CREATE TRIGGER brand_identity_guard BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION brand_identity_guard()',t);
  INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
  EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
  EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
DO $$ DECLARE app_role TEXT; jobs_role TEXT; runtime_role TEXT; BEGIN
 IF current_user !~ '_migration$' THEN RETURN; END IF;
 app_role:=regexp_replace(current_user,'_migration$','_app');jobs_role:=regexp_replace(current_user,'_migration$','_jobs');
 FOREACH runtime_role IN ARRAY ARRAY[app_role,jobs_role] LOOP
  IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=runtime_role) THEN EXECUTE format('GRANT SELECT ON brand_identity_drafts,brand_identity_versions,brand_identity_active,brand_category_locks,brand_identity_events TO %I',runtime_role);END IF;
 END LOOP;
 IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
  EXECUTE format('GRANT INSERT,UPDATE ON brand_identity_drafts,brand_identity_active,brand_category_locks TO %I',app_role);
  EXECUTE format('GRANT INSERT ON brand_identity_versions,brand_identity_events TO %I',app_role);
 END IF;
END $$;

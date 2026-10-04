-- Approved policy routing and private-session retention share immutable evidence.
ALTER TABLE agent_pool_members ADD COLUMN revoked_at BIGINT;
ALTER TABLE agent_pool_members ADD COLUMN version BIGINT NOT NULL DEFAULT 1 CHECK(version>0);
ALTER TABLE ai_private_sessions ADD COLUMN generation BIGINT NOT NULL DEFAULT 1 CHECK(generation>0);
DO $$ DECLARE item RECORD; BEGIN
  FOR item IN SELECT conname FROM pg_constraint WHERE conrelid='ai_private_sessions'::regclass AND contype='u' LOOP
    EXECUTE format('ALTER TABLE ai_private_sessions DROP CONSTRAINT %I',item.conname);
  END LOOP;
END $$;
ALTER TABLE ai_private_sessions ADD UNIQUE(company_id,user_id,assignment_id,scope_key,conversation_key,generation);
CREATE TABLE ai_private_session_controls (
  session_id TEXT PRIMARY KEY REFERENCES ai_private_sessions(id),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','expired')),
  expires_at BIGINT NOT NULL,
  legal_hold BOOLEAN NOT NULL DEFAULT FALSE,
  hold_reason TEXT,
  version BIGINT NOT NULL DEFAULT 1 CHECK(version>0),
  last_used_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  CHECK((legal_hold AND hold_reason IS NOT NULL) OR (NOT legal_hold AND hold_reason IS NULL))
);
INSERT INTO ai_private_session_controls(session_id,expires_at,last_used_at,updated_at)
 SELECT id,last_used_at+2592000000,last_used_at,last_used_at FROM ai_private_sessions;
CREATE TABLE ai_private_session_events (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES ai_private_sessions(id),
  version BIGINT NOT NULL,
  action TEXT NOT NULL CHECK(action IN ('hold','release-hold','expire')),
  actor_id TEXT NOT NULL,
  reason TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE(session_id,version)
);
CREATE TABLE agent_pool_revisions (
  id TEXT PRIMARY KEY,
  pool_id TEXT NOT NULL REFERENCES agent_pools(id),
  version BIGINT NOT NULL,
  members_json TEXT NOT NULL,
  reason TEXT NOT NULL,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE(pool_id,version)
);
INSERT INTO agent_pool_revisions
 SELECT 'agent-pool-initial-'||p.id,p.id,p.version,COALESCE((SELECT json_agg(m.user_id ORDER BY m.user_id)::text FROM agent_pool_members m WHERE m.pool_id=p.id),'[]'),
 'Existing pool members retained during runtime upgrade',p.created_by,p.created_at FROM agent_pools p;
ALTER TABLE ai_workspace_turns ADD COLUMN private_session_id TEXT REFERENCES ai_private_sessions(id);
UPDATE ai_workspace_turns t SET private_session_id=s.id FROM ai_private_sessions s
 WHERE s.company_id=t.company_id AND s.user_id=t.user_id AND s.assignment_id=t.assignment_id
 AND s.scope_key=t.scope_key AND s.conversation_key=t.conversation_key AND s.generation=1;
ALTER TABLE ai_invocation_evidence ADD COLUMN governance_snapshot_json TEXT;
CREATE INDEX ai_private_session_scope ON ai_private_sessions(company_id,user_id,assignment_id,scope_key,conversation_key,generation DESC);
CREATE INDEX ai_workspace_session_history ON ai_workspace_turns(private_session_id,created_at DESC);
CREATE OR REPLACE FUNCTION agent_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR TG_TABLE_NAME IN ('ai_invocation_evidence','ai_private_sessions','ai_private_session_events','agent_pool_revisions') THEN
   RAISE EXCEPTION 'AGENT_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';
 END IF;
 IF TG_TABLE_NAME='agent_pool_members' THEN
   IF (NEW.id,NEW.pool_id,NEW.user_id,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.pool_id,OLD.user_id,OLD.created_by,OLD.created_at)
      OR NEW.version<>OLD.version+1 THEN RAISE EXCEPTION 'AGENT_MEMBERSHIP_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='agent_catalog' THEN
   IF (NEW.id,NEW.company_id,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'AGENT_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='agent_versions' THEN
   IF (NEW.id,NEW.agent_id,NEW.version_number,NEW.prompt,NEW.allowed_sources_json,NEW.prohibited_actions_json,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.agent_id,OLD.version_number,OLD.prompt,OLD.allowed_sources_json,OLD.prohibited_actions_json,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'AGENT_VERSION_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='agent_assignments' THEN
   IF (NEW.id,NEW.company_id,NEW.agent_version_id,NEW.pool_id,NEW.user_id,NEW.scope_path,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.agent_version_id,OLD.pool_id,OLD.user_id,OLD.scope_path,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'AGENT_ASSIGNMENT_IMMUTABLE' USING ERRCODE='23514'; END IF;
 END IF; RETURN NEW;
END $$;
CREATE TRIGGER agent_guard BEFORE UPDATE OR DELETE ON agent_pool_revisions FOR EACH ROW EXECUTE FUNCTION agent_guard();
CREATE TRIGGER agent_guard BEFORE UPDATE OR DELETE ON ai_private_session_events FOR EACH ROW EXECUTE FUNCTION agent_guard();
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['agent_pool_members','agent_pool_revisions','ai_private_session_controls','ai_private_session_events'] LOOP
  INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
  IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=t::regclass AND tgname='work_cache_change') THEN
   EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
  END IF;
  IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid=t::regclass AND tgname='work_cache_truncate') THEN
   EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
  END IF;
 END LOOP;
END $$;
DO $$ DECLARE app_role TEXT; jobs_role TEXT; runtime_role TEXT; BEGIN
 IF current_user !~ '_migration$' THEN RETURN; END IF;
 app_role:=regexp_replace(current_user,'_migration$','_app'); jobs_role:=regexp_replace(current_user,'_migration$','_jobs');
 FOREACH runtime_role IN ARRAY ARRAY[app_role,jobs_role] LOOP
  IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=runtime_role) THEN
   EXECUTE format('GRANT SELECT ON agent_pool_revisions,ai_private_session_controls,ai_private_session_events TO %I',runtime_role);
  END IF;
 END LOOP;
 IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
  EXECUTE format('GRANT INSERT ON agent_pool_revisions,ai_private_session_events TO %I',app_role);
  EXECUTE format('GRANT INSERT,UPDATE ON ai_private_session_controls TO %I',app_role);
 END IF;
END $$;

CREATE TABLE support_sessions (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL,department_id TEXT,
 ticket_id TEXT NOT NULL REFERENCES support_tickets(id),ticket_version BIGINT NOT NULL,requested_for TEXT NOT NULL,
 reason TEXT NOT NULL,expires_at BIGINT NOT NULL,status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected','revoked')),
 created_by TEXT NOT NULL,reviewed_by TEXT,reviewed_at BIGINT,revoked_by TEXT,revoked_at BIGINT,
 version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(expires_at>created_at),CHECK(status='pending' OR reviewed_by IS NOT NULL),CHECK(status<>'revoked' OR revoked_at IS NOT NULL)
);
CREATE INDEX support_session_queue ON support_sessions(company_id,branch_id,department_id,status,expires_at);
CREATE TABLE support_session_events (
 id TEXT PRIMARY KEY,session_id TEXT NOT NULL REFERENCES support_sessions(id),version BIGINT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('request','approve','reject','revoke')),actor_id TEXT NOT NULL,reason TEXT NOT NULL,
 status TEXT NOT NULL,expires_at BIGINT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(session_id,version)
);
CREATE TABLE support_capabilities (
 id TEXT PRIMARY KEY,session_id TEXT NOT NULL REFERENCES support_sessions(id),code TEXT NOT NULL CHECK(code IN ('customer_order_context','ticket_evidence_metadata')),
 status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','revoked')),reason TEXT NOT NULL,issued_by TEXT NOT NULL,
 revoked_by TEXT,revoked_at BIGINT,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 UNIQUE(session_id,code),CHECK(status<>'revoked' OR revoked_at IS NOT NULL)
);
CREATE TABLE support_capability_events (
 id TEXT PRIMARY KEY,capability_id TEXT NOT NULL REFERENCES support_capabilities(id),version BIGINT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('issue','reissue','revoke','session-revoke')),actor_id TEXT NOT NULL,reason TEXT NOT NULL,
 status TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(capability_id,version)
);
CREATE FUNCTION support_session_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_TABLE_NAME IN ('support_session_events','support_capability_events') OR TG_OP='DELETE' THEN RAISE EXCEPTION 'SUPPORT_SESSION_HISTORY_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF TG_TABLE_NAME='support_sessions' THEN
  IF (NEW.id,NEW.company_id,NEW.branch_id,NEW.department_id,NEW.ticket_id,NEW.ticket_version,NEW.requested_for,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.branch_id,OLD.department_id,OLD.ticket_id,OLD.ticket_version,OLD.requested_for,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'SUPPORT_SESSION_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='support_capabilities' THEN
  IF (NEW.id,NEW.session_id,NEW.code,NEW.issued_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.session_id,OLD.code,OLD.issued_by,OLD.created_at) THEN RAISE EXCEPTION 'SUPPORT_CAPABILITY_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER support_session_record_guard BEFORE UPDATE OR DELETE ON support_sessions FOR EACH ROW EXECUTE FUNCTION support_session_guard();
CREATE TRIGGER support_session_event_guard BEFORE UPDATE OR DELETE ON support_session_events FOR EACH ROW EXECUTE FUNCTION support_session_guard();
CREATE TRIGGER support_capability_record_guard BEFORE UPDATE OR DELETE ON support_capabilities FOR EACH ROW EXECUTE FUNCTION support_session_guard();
CREATE TRIGGER support_capability_event_guard BEFORE UPDATE OR DELETE ON support_capability_events FOR EACH ROW EXECUTE FUNCTION support_session_guard();
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['support_sessions','support_capabilities'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
END LOOP; END $$;

CREATE TABLE support_sla_policies(
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,name TEXT NOT NULL,policy_json JSONB NOT NULL,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','retired')),
 version BIGINT NOT NULL DEFAULT 1,created_by TEXT NOT NULL,approved_by TEXT,approved_at BIGINT,
 created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(status<>'approved' OR (approved_by IS NOT NULL AND approved_by<>created_by AND approved_at IS NOT NULL))
);
CREATE TABLE support_sla_policy_events(
 id TEXT PRIMARY KEY,policy_id TEXT NOT NULL REFERENCES support_sla_policies(id),version BIGINT NOT NULL,
 action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(policy_id,version)
);
CREATE TRIGGER support_policy_event_immutable BEFORE UPDATE OR DELETE ON support_sla_policy_events FOR EACH ROW EXECUTE FUNCTION support_evidence_guard();
CREATE FUNCTION support_policy_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF NEW.policy_json IS DISTINCT FROM OLD.policy_json OR NEW.company_id IS DISTINCT FROM OLD.company_id OR NEW.name IS DISTINCT FROM OLD.name OR NEW.created_by IS DISTINCT FROM OLD.created_by THEN RAISE EXCEPTION 'SUPPORT_POLICY_IMMUTABLE' USING ERRCODE='23514'; END IF; RETURN NEW;
END $$;
CREATE TRIGGER support_policy_snapshot_guard BEFORE UPDATE ON support_sla_policies FOR EACH ROW EXECUTE FUNCTION support_policy_immutable();
ALTER TABLE support_tickets ADD COLUMN sla_policy_id TEXT REFERENCES support_sla_policies(id),ADD COLUMN sla_policy_version BIGINT,ADD COLUMN sla_clock_json JSONB;
ALTER TABLE support_tickets ADD CONSTRAINT support_sla_complete CHECK((sla_policy_id IS NULL AND sla_policy_version IS NULL AND sla_clock_json IS NULL) OR (sla_policy_id IS NOT NULL AND sla_policy_version IS NOT NULL AND sla_clock_json IS NOT NULL));
CREATE FUNCTION support_clock_snapshot_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF OLD.sla_policy_id IS NOT NULL AND (NEW.sla_policy_id IS DISTINCT FROM OLD.sla_policy_id OR NEW.sla_policy_version IS DISTINCT FROM OLD.sla_policy_version OR NEW.sla_clock_json->'snapshot' IS DISTINCT FROM OLD.sla_clock_json->'snapshot') THEN RAISE EXCEPTION 'SUPPORT_CLOCK_SNAPSHOT_IMMUTABLE' USING ERRCODE='23514'; END IF;RETURN NEW;
END $$;
CREATE TRIGGER support_clock_snapshot_immutable BEFORE UPDATE ON support_tickets FOR EACH ROW EXECUTE FUNCTION support_clock_snapshot_guard();
CREATE INDEX support_sla_due ON support_tickets(company_id,((sla_clock_json->>'dueAt')::bigint)) WHERE sla_clock_json->>'state'='running';
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['support_sla_policies','support_sla_policy_events'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
END LOOP; END $$;

CREATE TABLE support_incidents (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL,department_id TEXT,
 title TEXT NOT NULL,impact TEXT NOT NULL,severity TEXT NOT NULL CHECK(severity IN ('critical','high','normal')),
 owner_id TEXT NOT NULL,due_date TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','investigating','mitigated','closed','cancelled')),
 root_cause TEXT,mitigation TEXT,mitigated_by TEXT,
 version BIGINT NOT NULL DEFAULT 1,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(status NOT IN ('mitigated','closed') OR (root_cause IS NOT NULL AND mitigation IS NOT NULL AND mitigated_by IS NOT NULL))
);
CREATE TABLE support_incident_tickets (
 incident_id TEXT NOT NULL REFERENCES support_incidents(id),ticket_id TEXT NOT NULL REFERENCES support_tickets(id),
 linked_version BIGINT NOT NULL,linked_by TEXT NOT NULL,linked_at BIGINT NOT NULL,active BOOLEAN NOT NULL DEFAULT true,
 PRIMARY KEY(incident_id,ticket_id)
);
CREATE TABLE support_incident_events (
 id TEXT PRIMARY KEY,incident_id TEXT NOT NULL REFERENCES support_incidents(id),version BIGINT NOT NULL,
 action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,created_at BIGINT NOT NULL,evidence_json JSONB NOT NULL,
 UNIQUE(incident_id,version)
);
CREATE TRIGGER support_incident_evidence BEFORE UPDATE OR DELETE ON support_incident_events FOR EACH ROW EXECUTE FUNCTION support_evidence_guard();
CREATE FUNCTION support_incident_link_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'SUPPORT_LINK_HISTORY_REQUIRED' USING ERRCODE='23514'; END IF;
 IF NEW.incident_id<>OLD.incident_id OR NEW.ticket_id<>OLD.ticket_id THEN RAISE EXCEPTION 'SUPPORT_LINK_IDENTITY_IMMUTABLE' USING ERRCODE='23514'; END IF;RETURN NEW;
END $$;
CREATE TRIGGER support_incident_link_evidence BEFORE UPDATE OR DELETE ON support_incident_tickets FOR EACH ROW EXECUTE FUNCTION support_incident_link_guard();
CREATE INDEX support_incident_queue ON support_incidents(company_id,branch_id,department_id,status,due_date);
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['support_incidents','support_incident_tickets','support_incident_events'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
END LOOP; END $$;

-- Lead and opportunity activities remain owned by CRM, with immutable decisions
-- and a durable Work handoff for the assigned salesperson.
CREATE TABLE crm_lead_activities (
 id TEXT PRIMARY KEY,
 company_id TEXT NOT NULL,
 lead_id TEXT NOT NULL REFERENCES crm_leads(id),
 activity_type TEXT NOT NULL CHECK(activity_type IN ('call','email','meeting','task')),
 subject TEXT NOT NULL,
 details TEXT NOT NULL,
 owner_id TEXT NOT NULL,
 due_date TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','completed','cancelled')),
 outcome TEXT,
 version BIGINT NOT NULL DEFAULT 1,
 created_by TEXT NOT NULL,
 created_at BIGINT NOT NULL,
 updated_at BIGINT NOT NULL
);
CREATE TABLE crm_lead_activity_events (
 id TEXT PRIMARY KEY,
 activity_id TEXT NOT NULL REFERENCES crm_lead_activities(id),
 version BIGINT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('create','reassign','complete','cancel')),
 actor_id TEXT NOT NULL,
 owner_id TEXT NOT NULL,
 due_date TEXT NOT NULL,
 note TEXT NOT NULL,
 snapshot_json JSONB NOT NULL,
 created_at BIGINT NOT NULL,
 UNIQUE(activity_id,version)
);
CREATE INDEX crm_lead_activities_queue ON crm_lead_activities(company_id,owner_id,status,due_date);
CREATE INDEX crm_lead_activities_lead ON crm_lead_activities(company_id,lead_id,created_at DESC,id DESC);
CREATE FUNCTION crm_lead_activity_history_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='crm_lead_activity_events' OR TG_OP='DELETE' THEN RAISE EXCEPTION 'CRM_ACTIVITY_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF (NEW.id,NEW.company_id,NEW.lead_id,NEW.activity_type,NEW.subject,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.lead_id,OLD.activity_type,OLD.subject,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'CRM_ACTIVITY_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER crm_lead_activity_guard BEFORE UPDATE OR DELETE ON crm_lead_activities FOR EACH ROW EXECUTE FUNCTION crm_lead_activity_history_guard();
CREATE TRIGGER crm_lead_activity_history_guard BEFORE UPDATE OR DELETE ON crm_lead_activity_events FOR EACH ROW EXECUTE FUNCTION crm_lead_activity_history_guard();
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['crm_lead_activities','crm_lead_activity_events'] LOOP
  INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
  EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
  EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;

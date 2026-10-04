CREATE TABLE maintenance_preventive_plans (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL,department_id TEXT,asset_id TEXT NOT NULL REFERENCES operational_assets(id),
 title TEXT NOT NULL,priority TEXT NOT NULL CHECK(priority IN ('normal','high','critical')),owner_id TEXT NOT NULL,
 interval_days INTEGER NOT NULL CHECK(interval_days BETWEEN 1 AND 3650),time_zone TEXT NOT NULL,
 first_due_local TEXT NOT NULL,next_due_local TEXT NOT NULL,next_due_at BIGINT NOT NULL,lead_days INTEGER NOT NULL DEFAULT 7 CHECK(lead_days BETWEEN 0 AND 60),
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','active','paused','cancelled')),version BIGINT NOT NULL DEFAULT 1,
 created_by TEXT NOT NULL,edited_by TEXT NOT NULL,approved_by TEXT,approved_at BIGINT,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL
);
CREATE INDEX maintenance_preventive_due ON maintenance_preventive_plans(company_id,status,next_due_at);
CREATE TABLE maintenance_preventive_events (
 id TEXT PRIMARY KEY,plan_id TEXT NOT NULL REFERENCES maintenance_preventive_plans(id),version BIGINT NOT NULL,action TEXT NOT NULL,
 actor_id TEXT NOT NULL,reason TEXT NOT NULL,snapshot_json JSONB NOT NULL,created_at BIGINT NOT NULL,UNIQUE(plan_id,version)
);
CREATE TABLE maintenance_preventive_occurrences (
 id TEXT PRIMARY KEY,plan_id TEXT NOT NULL REFERENCES maintenance_preventive_plans(id),plan_version BIGINT NOT NULL,
 work_order_id TEXT NOT NULL UNIQUE REFERENCES maintenance_work_orders(id),due_local TEXT NOT NULL,due_at BIGINT NOT NULL,
 source_asset_version BIGINT NOT NULL,definition_json JSONB NOT NULL,generated_by TEXT NOT NULL,generated_at BIGINT NOT NULL,UNIQUE(plan_id,due_local)
);
CREATE FUNCTION maintenance_preventive_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'MAINTENANCE_PREVENTIVE_HISTORY_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER maintenance_preventive_event_immutable BEFORE UPDATE OR DELETE ON maintenance_preventive_events FOR EACH ROW EXECUTE FUNCTION maintenance_preventive_immutable();
CREATE TRIGGER maintenance_preventive_occurrence_immutable BEFORE UPDATE OR DELETE ON maintenance_preventive_occurrences FOR EACH ROW EXECUTE FUNCTION maintenance_preventive_immutable();
CREATE FUNCTION maintenance_preventive_plan_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR (NEW.id,NEW.company_id,NEW.branch_id,NEW.department_id,NEW.asset_id,NEW.created_by,NEW.created_at)
    IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.branch_id,OLD.department_id,OLD.asset_id,OLD.created_by,OLD.created_at)
 THEN RAISE EXCEPTION 'MAINTENANCE_PREVENTIVE_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF OLD.status='cancelled' THEN RAISE EXCEPTION 'MAINTENANCE_PREVENTIVE_CANCELLED' USING ERRCODE='23514'; END IF;
 IF OLD.status='active' AND (NEW.title,NEW.priority,NEW.owner_id,NEW.interval_days,NEW.time_zone,NEW.first_due_local,NEW.lead_days,NEW.edited_by)
    IS DISTINCT FROM (OLD.title,OLD.priority,OLD.owner_id,OLD.interval_days,OLD.time_zone,OLD.first_due_local,OLD.lead_days,OLD.edited_by)
 THEN RAISE EXCEPTION 'MAINTENANCE_PREVENTIVE_PAUSE_BEFORE_REVISION' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER maintenance_preventive_plan_guard BEFORE UPDATE OR DELETE ON maintenance_preventive_plans FOR EACH ROW EXECUTE FUNCTION maintenance_preventive_plan_guard();
CREATE FUNCTION maintenance_preventive_evidence_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='maintenance_preventive_plans' THEN
  IF NOT EXISTS(SELECT 1 FROM maintenance_preventive_plans p JOIN maintenance_preventive_events e ON e.plan_id=p.id AND e.version=p.version
   WHERE p.id=NEW.id AND e.snapshot_json=to_jsonb(p))
  THEN RAISE EXCEPTION 'MAINTENANCE_PREVENTIVE_EVENT_REQUIRED' USING ERRCODE='23514'; END IF;
 ELSE
  IF NOT EXISTS(SELECT 1 FROM maintenance_preventive_plans p JOIN maintenance_preventive_events e ON e.plan_id=p.id AND e.version=NEW.plan_version
    JOIN maintenance_work_orders w ON w.id=NEW.work_order_id AND w.asset_id=p.asset_id AND w.company_id=p.company_id AND w.branch_id=p.branch_id AND w.department_id IS NOT DISTINCT FROM p.department_id
   WHERE p.id=NEW.plan_id AND e.snapshot_json->>'status'='active' AND e.snapshot_json->>'next_due_local'=NEW.due_local
    AND (e.snapshot_json->>'next_due_at')::bigint=NEW.due_at AND w.scheduled_at=NEW.due_at AND w.created_by=NEW.generated_by
    AND w.title=NEW.definition_json->>'title' AND w.priority=NEW.definition_json->>'priority' AND w.owner_id=NEW.definition_json->>'ownerId'
    AND NEW.definition_json->>'title'=e.snapshot_json->>'title' AND NEW.definition_json->>'ownerId'=e.snapshot_json->>'owner_id')
  THEN RAISE EXCEPTION 'MAINTENANCE_PREVENTIVE_OCCURRENCE_SOURCE_INVALID' USING ERRCODE='23514'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER maintenance_preventive_event_required AFTER INSERT OR UPDATE ON maintenance_preventive_plans DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION maintenance_preventive_evidence_guard();
CREATE CONSTRAINT TRIGGER maintenance_preventive_occurrence_source AFTER INSERT ON maintenance_preventive_occurrences DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION maintenance_preventive_evidence_guard();
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['maintenance_preventive_plans','maintenance_preventive_events','maintenance_preventive_occurrences'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
END LOOP;END $$;

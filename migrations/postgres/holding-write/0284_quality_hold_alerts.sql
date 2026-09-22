-- Quality holds need a durable reminder and escalation record so a missed
-- scheduled sweep can be recovered without losing the operational handoff.
CREATE TABLE inventory_quality_alerts (
 id TEXT PRIMARY KEY,
 hold_id TEXT NOT NULL REFERENCES inventory_quality_holds(id),
 phase TEXT NOT NULL CHECK(phase IN ('due_soon','overdue')),
 source_version BIGINT NOT NULL CHECK(source_version>0),
 due_date DATE NOT NULL,
 detected_at BIGINT NOT NULL,
 version BIGINT NOT NULL DEFAULT 1 CHECK(version>0),
 acknowledged_by TEXT,
 acknowledged_at BIGINT,
 reason TEXT,
 UNIQUE(hold_id,phase),
 CHECK((acknowledged_by IS NULL AND acknowledged_at IS NULL AND reason IS NULL AND version=1)
   OR (acknowledged_by IS NOT NULL AND acknowledged_at IS NOT NULL AND length(reason)>=3 AND version=2))
);
CREATE INDEX inventory_quality_alert_queue ON inventory_quality_alerts(hold_id,phase,acknowledged_by);
CREATE FUNCTION quality_alert_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'QUALITY_ALERT_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF OLD.acknowledged_by IS NOT NULL OR NEW.id<>OLD.id OR NEW.hold_id<>OLD.hold_id
   OR NEW.phase<>OLD.phase OR NEW.source_version<>OLD.source_version
   OR NEW.due_date<>OLD.due_date OR NEW.detected_at<>OLD.detected_at
 THEN RAISE EXCEPTION 'QUALITY_ALERT_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER quality_alert_evidence BEFORE UPDATE OR DELETE ON inventory_quality_alerts FOR EACH ROW EXECUTE FUNCTION quality_alert_guard();
INSERT INTO work_cache_revisions(table_name) VALUES('inventory_quality_alerts') ON CONFLICT DO NOTHING;
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON inventory_quality_alerts FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON inventory_quality_alerts FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();

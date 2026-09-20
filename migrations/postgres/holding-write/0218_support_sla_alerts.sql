CREATE TABLE support_sla_alerts (
 id TEXT PRIMARY KEY,
 ticket_id TEXT NOT NULL UNIQUE REFERENCES support_tickets(id),
 source_version BIGINT NOT NULL,
 breached_at BIGINT NOT NULL,
 detected_at BIGINT NOT NULL,
 version BIGINT NOT NULL DEFAULT 1,
 acknowledged_by TEXT,
 acknowledged_at BIGINT,
 reason TEXT,
 CHECK((acknowledged_by IS NULL AND acknowledged_at IS NULL AND reason IS NULL AND version=1)
   OR (acknowledged_by IS NOT NULL AND acknowledged_at IS NOT NULL AND length(reason)>=3 AND version=2))
);
CREATE FUNCTION support_alert_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'SUPPORT_ALERT_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF OLD.acknowledged_by IS NOT NULL OR NEW.id<>OLD.id OR NEW.ticket_id<>OLD.ticket_id
   OR NEW.source_version<>OLD.source_version OR NEW.breached_at<>OLD.breached_at OR NEW.detected_at<>OLD.detected_at
 THEN RAISE EXCEPTION 'SUPPORT_ALERT_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER support_alert_evidence BEFORE UPDATE OR DELETE ON support_sla_alerts FOR EACH ROW EXECUTE FUNCTION support_alert_guard();
INSERT INTO work_cache_revisions(table_name) VALUES('support_sla_alerts') ON CONFLICT DO NOTHING;
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON support_sla_alerts FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON support_sla_alerts FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();

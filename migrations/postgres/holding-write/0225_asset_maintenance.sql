CREATE TABLE operational_assets (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL,department_id TEXT,
 code TEXT NOT NULL,name TEXT NOT NULL,category TEXT NOT NULL,currency TEXT NOT NULL,cost_minor BIGINT NOT NULL DEFAULT 0,
 condition TEXT NOT NULL DEFAULT 'good' CHECK(condition IN ('good','degraded','unsafe')),
 lifecycle_status TEXT NOT NULL DEFAULT 'registered' CHECK(lifecycle_status IN ('registered','in_service','maintenance','retirement_pending','retired')),
 placement TEXT,custodian_id TEXT,acquired_date TEXT,warranty_end TEXT,retirement_reason TEXT,
 version BIGINT NOT NULL DEFAULT 1,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 UNIQUE(company_id,code),CHECK(lifecycle_status<>'in_service' OR placement IS NOT NULL),CHECK(lifecycle_status<>'retired' OR retirement_reason IS NOT NULL)
);
CREATE INDEX operational_asset_scope ON operational_assets(company_id,branch_id,department_id,lifecycle_status,code);
CREATE TABLE operational_asset_events (
 id TEXT PRIMARY KEY,asset_id TEXT NOT NULL REFERENCES operational_assets(id),version BIGINT NOT NULL,
 action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,condition TEXT NOT NULL,lifecycle_status TEXT NOT NULL,
 placement TEXT,custodian_id TEXT,created_at BIGINT NOT NULL,UNIQUE(asset_id,version)
);
CREATE TABLE maintenance_work_orders (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL,department_id TEXT,asset_id TEXT NOT NULL REFERENCES operational_assets(id),
 title TEXT NOT NULL,priority TEXT NOT NULL CHECK(priority IN ('normal','high','critical')),status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','scheduled','in_progress','blocked','completed','cancelled')),
 owner_id TEXT NOT NULL,scheduled_at BIGINT,downtime_started_at BIGINT,downtime_ended_at BIGINT,blocked_reason TEXT,completion_evidence TEXT,resulting_condition TEXT CHECK(resulting_condition IS NULL OR resulting_condition IN ('good','degraded','unsafe')),
 version BIGINT NOT NULL DEFAULT 1,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(status<>'blocked' OR blocked_reason IS NOT NULL),CHECK(status<>'completed' OR (completion_evidence IS NOT NULL AND resulting_condition IS NOT NULL))
);
CREATE INDEX maintenance_work_queue ON maintenance_work_orders(company_id,branch_id,department_id,status,scheduled_at);
CREATE TABLE maintenance_work_events (
 id TEXT PRIMARY KEY,work_order_id TEXT NOT NULL REFERENCES maintenance_work_orders(id),version BIGINT NOT NULL,
 action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,status TEXT NOT NULL,owner_id TEXT NOT NULL,
 scheduled_at BIGINT,downtime_started_at BIGINT,downtime_ended_at BIGINT,created_at BIGINT NOT NULL,UNIQUE(work_order_id,version)
);
CREATE FUNCTION asset_maintenance_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_TABLE_NAME IN ('operational_asset_events','maintenance_work_events') OR TG_OP='DELETE' THEN RAISE EXCEPTION 'ASSET_MAINTENANCE_HISTORY_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF TG_TABLE_NAME='operational_assets' THEN
  IF (NEW.id,NEW.company_id,NEW.branch_id,NEW.department_id,NEW.code,NEW.currency,NEW.cost_minor,NEW.acquired_date,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.branch_id,OLD.department_id,OLD.code,OLD.currency,OLD.cost_minor,OLD.acquired_date,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'ASSET_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='maintenance_work_orders' THEN
  IF (NEW.id,NEW.company_id,NEW.branch_id,NEW.department_id,NEW.asset_id,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.branch_id,OLD.department_id,OLD.asset_id,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'MAINTENANCE_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 END IF;RETURN NEW;
END $$;
CREATE TRIGGER operational_asset_guard BEFORE UPDATE OR DELETE ON operational_assets FOR EACH ROW EXECUTE FUNCTION asset_maintenance_guard();
CREATE TRIGGER operational_asset_event_guard BEFORE UPDATE OR DELETE ON operational_asset_events FOR EACH ROW EXECUTE FUNCTION asset_maintenance_guard();
CREATE TRIGGER maintenance_work_guard BEFORE UPDATE OR DELETE ON maintenance_work_orders FOR EACH ROW EXECUTE FUNCTION asset_maintenance_guard();
CREATE TRIGGER maintenance_work_event_guard BEFORE UPDATE OR DELETE ON maintenance_work_events FOR EACH ROW EXECUTE FUNCTION asset_maintenance_guard();
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['operational_assets','maintenance_work_orders'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
END LOOP; END $$;

-- Shared subscriptions retain their legacy cursor. Native Field delivery has
-- its own cursor so either worker can progress without skipping the other's rows.
-- Preserve ineligible observations without claiming a provider failure.
ALTER TABLE push_delivery_jobs DROP CONSTRAINT push_delivery_jobs_status_check;
ALTER TABLE push_delivery_jobs ADD CONSTRAINT push_delivery_jobs_status_check
 CHECK(status IN ('pending','sent','dead','suppressed'));
CREATE TABLE native_field_push_cursors (
 subscription_id TEXT PRIMARY KEY REFERENCES push_subscriptions(id) ON DELETE CASCADE,
 cursor_at BIGINT NOT NULL DEFAULT 0,
 cursor_id TEXT NOT NULL DEFAULT '',
 scanned_at BIGINT NOT NULL DEFAULT 0
);
CREATE FUNCTION erpx_field_push_source(source_type TEXT,source_id TEXT,company TEXT)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT coalesce($1='field_service_order'
 OR ($1='stock_count' AND EXISTS(SELECT 1 FROM inventory_service_initial_physical_sources s WHERE s.count_id=$2 AND s.company_id=$3))
 OR ($1='operational_exception' AND EXISTS(SELECT 1 FROM operations_exceptions s WHERE s.id=$2 AND s.company_id=$3 AND s.source_kind='field_service_order'))
 OR ($1='collaboration_action' AND EXISTS(SELECT 1 FROM collaboration_action_cards s WHERE s.id=$2 AND s.company_id=$3 AND s.source_type='field_service_order'))
 OR ($1='support_ticket' AND EXISTS(SELECT 1 FROM support_tickets s WHERE s.id=$2 AND s.company_id=$3 AND s.source_kind='field_service_order')),false)
$$;
CREATE FUNCTION erpx_field_push_job(job push_delivery_jobs) RETURNS boolean
LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id
 WHERE d.id=(job).inbox_delivery_id AND d.company_id=(job).company_id
 AND erpx_field_push_source(e.source_type,e.source_id,e.company_id))
$$;
CREATE FUNCTION erpx_require_native_field_push() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF current_setting('erpx.push_runtime',true) IS DISTINCT FROM 'native-rust'
 AND (erpx_field_push_job(NEW) OR (TG_OP='UPDATE' AND erpx_field_push_job(OLD))) THEN
  RAISE EXCEPTION 'Field push delivery requires the native runtime'
   USING ERRCODE='23514',CONSTRAINT='native_field_push_runtime_required';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER native_field_push_runtime_required BEFORE INSERT OR UPDATE ON push_delivery_jobs
 FOR EACH ROW EXECUTE FUNCTION erpx_require_native_field_push();
DO $$
DECLARE migration_role TEXT:=current_user; jobs_role TEXT;
BEGIN
 IF migration_role !~ '_migration$' THEN RETURN; END IF;
 jobs_role:=regexp_replace(migration_role,'_migration$','_jobs');
 EXECUTE format('GRANT SELECT,INSERT,UPDATE,DELETE ON native_field_push_cursors TO %I',jobs_role);
END $$;

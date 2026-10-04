-- Owner accounting adapters retain the approved account map and exact source
-- version used by every material, conversion-cost and output journal.
CREATE TABLE manufacturing_accounting_profiles (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,chart_id TEXT NOT NULL REFERENCES finance_charts(id),
 currency TEXT NOT NULL CHECK(currency='THB'),inventory_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 wip_account_id TEXT NOT NULL REFERENCES finance_accounts(id),labor_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 overhead_account_id TEXT NOT NULL REFERENCES finance_accounts(id),scrap_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','rejected','retired')),
 version BIGINT NOT NULL DEFAULT 1,created_by TEXT NOT NULL,reviewed_by TEXT,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(inventory_account_id<>wip_account_id),CHECK(labor_account_id<>wip_account_id),CHECK(overhead_account_id<>wip_account_id),
 CHECK(status<>'approved' OR (reviewed_by IS NOT NULL AND reviewed_by<>created_by))
);
CREATE UNIQUE INDEX manufacturing_active_accounting_profile ON manufacturing_accounting_profiles(company_id) WHERE status='approved';
CREATE TABLE manufacturing_order_postings (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,order_id TEXT NOT NULL REFERENCES manufacturing_orders(id),
 source_order_version BIGINT NOT NULL,phase TEXT NOT NULL CHECK(phase IN ('issue','report','output','scrap')),
 profile_id TEXT NOT NULL REFERENCES manufacturing_accounting_profiles(id),profile_version BIGINT NOT NULL,
 journal_id TEXT NOT NULL UNIQUE REFERENCES finance_journals(id),currency TEXT NOT NULL,
 amount_minor BIGINT NOT NULL CHECK(amount_minor>=0),account_snapshot_json TEXT NOT NULL,
 created_by TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(order_id,source_order_version)
);

CREATE TABLE production_resources (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL REFERENCES organization_nodes(id),
 code TEXT NOT NULL,name TEXT NOT NULL,daily_minutes BIGINT NOT NULL CHECK(daily_minutes BETWEEN 0 AND 1440),
 work_weekdays_json TEXT NOT NULL DEFAULT '[1,2,3,4,5]',
 status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','archived')),version BIGINT NOT NULL DEFAULT 1,
 created_by TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,UNIQUE(company_id,code)
);
CREATE TABLE production_resource_calendar (
 resource_id TEXT NOT NULL REFERENCES production_resources(id),work_date TEXT NOT NULL,
 available_minutes BIGINT NOT NULL CHECK(available_minutes BETWEEN 0 AND 1440),reason TEXT NOT NULL,
 version BIGINT NOT NULL DEFAULT 1,updated_by TEXT NOT NULL,updated_at BIGINT NOT NULL,PRIMARY KEY(resource_id,work_date)
);
CREATE TABLE manufacturing_specification_operations (
 id TEXT PRIMARY KEY,specification_id TEXT NOT NULL REFERENCES manufacturing_specifications(id),
 resource_id TEXT NOT NULL REFERENCES production_resources(id),name TEXT NOT NULL,
 setup_minutes BIGINT NOT NULL CHECK(setup_minutes>=0),run_minutes_per_batch BIGINT NOT NULL CHECK(run_minutes_per_batch>0),
 ordinal BIGINT NOT NULL,UNIQUE(specification_id,ordinal)
);
CREATE TABLE planning_resource_allocations (
 id TEXT PRIMARY KEY,scenario_id TEXT NOT NULL REFERENCES planning_scenarios(id),resource_id TEXT NOT NULL REFERENCES production_resources(id),
 work_date TEXT NOT NULL,minutes BIGINT NOT NULL CHECK(minutes>0),created_by TEXT NOT NULL,created_at BIGINT NOT NULL,released_at BIGINT,
 UNIQUE(scenario_id,resource_id,work_date)
);
CREATE INDEX planning_active_resource_allocations ON planning_resource_allocations(resource_id,work_date) WHERE released_at IS NULL;
CREATE TABLE manufacturing_operation_events (
 id TEXT PRIMARY KEY,order_id TEXT NOT NULL REFERENCES manufacturing_orders(id),operation_id TEXT NOT NULL REFERENCES manufacturing_specification_operations(id),
 action TEXT NOT NULL CHECK(action IN ('complete')),source_order_version BIGINT NOT NULL,
 labor_minutes BIGINT NOT NULL CHECK(labor_minutes>=0),machine_minutes BIGINT NOT NULL CHECK(machine_minutes>=0),
 evidence TEXT NOT NULL,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(order_id,operation_id)
);
ALTER TABLE planning_scenarios ADD COLUMN context_json TEXT NOT NULL DEFAULT '{}';
CREATE TABLE planning_scenario_events (
 id TEXT PRIMARY KEY,scenario_id TEXT NOT NULL REFERENCES planning_scenarios(id),version BIGINT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('calculate','release','cancel')),actor_id TEXT NOT NULL,reason TEXT NOT NULL,created_at BIGINT NOT NULL,
 UNIQUE(scenario_id,version)
);

CREATE FUNCTION production_snapshot_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'PRODUCTION_SNAPSHOT_IMMUTABLE' USING ERRCODE='23514'; END $$;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['manufacturing_order_postings','manufacturing_operation_events','manufacturing_cost_movements','planning_scenario_events'] LOOP
  EXECUTE format('CREATE TRIGGER production_snapshot_immutable BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION production_snapshot_immutable()',t);
 END LOOP;
END $$;
CREATE FUNCTION manufacturing_accounting_profile_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR
   (NEW.id,NEW.company_id,NEW.chart_id,NEW.currency,NEW.inventory_account_id,NEW.wip_account_id,NEW.labor_account_id,NEW.overhead_account_id,NEW.scrap_account_id,NEW.created_by,NEW.created_at)
   IS DISTINCT FROM
   (OLD.id,OLD.company_id,OLD.chart_id,OLD.currency,OLD.inventory_account_id,OLD.wip_account_id,OLD.labor_account_id,OLD.overhead_account_id,OLD.scrap_account_id,OLD.created_by,OLD.created_at)
 THEN RAISE EXCEPTION 'MANUFACTURING_ACCOUNTING_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER manufacturing_accounting_profile_guard BEFORE UPDATE OR DELETE ON manufacturing_accounting_profiles FOR EACH ROW EXECUTE FUNCTION manufacturing_accounting_profile_guard();
CREATE FUNCTION manufacturing_routing_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM manufacturing_specifications WHERE id=COALESCE(NEW.specification_id,OLD.specification_id) AND status<>'draft')
 THEN RAISE EXCEPTION 'MANUFACTURING_ROUTING_APPROVED' USING ERRCODE='23514'; END IF;
 RETURN COALESCE(NEW,OLD);
END $$;
CREATE TRIGGER manufacturing_routing_guard BEFORE INSERT OR UPDATE OR DELETE ON manufacturing_specification_operations FOR EACH ROW EXECUTE FUNCTION manufacturing_routing_guard();
CREATE FUNCTION planning_context_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.context_json IS DISTINCT FROM OLD.context_json THEN RAISE EXCEPTION 'PLANNING_CONTEXT_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER planning_context_guard BEFORE UPDATE ON planning_scenarios FOR EACH ROW EXECUTE FUNCTION planning_context_guard();
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['manufacturing_accounting_profiles','manufacturing_order_postings','production_resources','production_resource_calendar','manufacturing_specification_operations','manufacturing_operation_events','planning_resource_allocations','planning_scenario_events'] LOOP
  INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
  EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
  EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;

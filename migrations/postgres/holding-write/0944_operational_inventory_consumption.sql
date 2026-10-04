CREATE TABLE operational_inventory_consumptions (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL,department_id TEXT,
 source_kind TEXT NOT NULL CHECK(source_kind IN ('maintenance','field_service')),source_id TEXT NOT NULL,source_version BIGINT NOT NULL,
 warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),product_id TEXT NOT NULL REFERENCES commerce_products(id),
 quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),value_minor BIGINT NOT NULL CHECK(value_minor>=0),currency TEXT NOT NULL,
 movement_date TEXT NOT NULL,journal_id TEXT NOT NULL REFERENCES finance_journals(id),reason TEXT NOT NULL,created_by TEXT NOT NULL,created_at BIGINT NOT NULL
);
CREATE INDEX operational_inventory_source ON operational_inventory_consumptions(company_id,source_kind,source_id);
CREATE TABLE operational_inventory_consumption_lots (
 id TEXT PRIMARY KEY,consumption_id TEXT NOT NULL REFERENCES operational_inventory_consumptions(id),lot_id TEXT NOT NULL REFERENCES inventory_lots(id),
 quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),value_minor BIGINT NOT NULL CHECK(value_minor>=0),source_balance_version BIGINT NOT NULL,
 UNIQUE(consumption_id,lot_id)
);
CREATE FUNCTION operational_inventory_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'OPERATIONAL_INVENTORY_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER operational_inventory_evidence_immutable BEFORE UPDATE OR DELETE ON operational_inventory_consumptions FOR EACH ROW EXECUTE FUNCTION operational_inventory_evidence_immutable();
CREATE TRIGGER operational_inventory_lot_evidence_immutable BEFORE UPDATE OR DELETE ON operational_inventory_consumption_lots FOR EACH ROW EXECUTE FUNCTION operational_inventory_evidence_immutable();
CREATE FUNCTION operational_inventory_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE source_valid BOOLEAN; expected_tracking TEXT;
BEGIN
 IF NEW.source_kind='maintenance' THEN
  SELECT EXISTS(SELECT 1 FROM maintenance_work_orders w JOIN maintenance_work_events e ON e.work_order_id=w.id
   WHERE w.id=NEW.source_id AND w.company_id=NEW.company_id AND w.branch_id=NEW.branch_id AND w.department_id IS NOT DISTINCT FROM NEW.department_id
    AND w.version>=NEW.source_version AND e.version=NEW.source_version AND e.action='issue-part' AND e.actor_id=NEW.created_by) INTO source_valid;
 ELSE
  SELECT EXISTS(SELECT 1 FROM field_service_orders o JOIN field_service_order_events e ON e.order_id=o.id
   WHERE o.id=NEW.source_id AND o.company_id=NEW.company_id AND o.branch_id=NEW.branch_id AND o.department_id IS NOT DISTINCT FROM NEW.department_id
    AND o.version>=NEW.source_version AND e.version=NEW.source_version AND e.action='issue-part' AND e.actor_id=NEW.created_by
    AND e.details_json::jsonb->>'consumptionId'=NEW.id) INTO source_valid;
 END IF;
 IF NOT source_valid OR NOT EXISTS(SELECT 1 FROM commerce_warehouses w JOIN commerce_products p ON p.company_id=w.company_id
   WHERE w.id=NEW.warehouse_id AND p.id=NEW.product_id AND w.company_id=NEW.company_id AND w.branch_id=NEW.branch_id)
  OR NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=NEW.journal_id AND j.company_id=NEW.company_id AND j.currency=NEW.currency
   AND j.source_type=NEW.source_kind||'_parts' AND j.source_id=NEW.id AND j.entry_date=NEW.movement_date
   AND (SELECT SUM(l.debit_minor) FROM finance_journal_lines l WHERE l.journal_id=j.id)=NEW.value_minor
   AND (SELECT SUM(l.credit_minor) FROM finance_journal_lines l WHERE l.journal_id=j.id)=NEW.value_minor)
  OR NOT EXISTS(SELECT 1 FROM money_transaction_snapshots m WHERE m.company_id=NEW.company_id AND m.source_type='operational_inventory_consumption'
   AND m.source_id=NEW.id AND m.amount_kind='stock_cost' AND m.currency=NEW.currency AND m.transaction_date=NEW.movement_date::date AND m.amount_units=NEW.value_minor)
 THEN RAISE EXCEPTION 'OPERATIONAL_INVENTORY_SOURCE_INVALID' USING ERRCODE='23514'; END IF;
 SELECT tracking_mode INTO expected_tracking FROM commerce_products WHERE id=NEW.product_id;
 IF expected_tracking<>'none' AND NOT EXISTS(SELECT 1 FROM operational_inventory_consumption_lots l WHERE l.consumption_id=NEW.id
   GROUP BY l.consumption_id HAVING SUM(l.quantity_milli)=NEW.quantity_milli AND SUM(l.value_minor)=NEW.value_minor)
  OR expected_tracking='none' AND EXISTS(SELECT 1 FROM operational_inventory_consumption_lots WHERE consumption_id=NEW.id)
 THEN RAISE EXCEPTION 'OPERATIONAL_INVENTORY_LOT_ALLOCATION_INVALID' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER operational_inventory_source_guard AFTER INSERT ON operational_inventory_consumptions DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION operational_inventory_source_guard();
DO $$ DECLARE definition TEXT; BEGIN
 definition:=rtrim(pg_get_viewdef('inventory_ledger'::regclass,true),';'||chr(10));
 EXECUTE 'CREATE OR REPLACE VIEW inventory_ledger AS '||definition||' UNION ALL SELECT c.id,c.warehouse_id,c.product_id,-c.quantity_milli,-c.value_minor,c.created_at,c.movement_date,c.source_kind,c.source_id,c.source_id FROM operational_inventory_consumptions c';
END $$;
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['operational_inventory_consumptions','operational_inventory_consumption_lots'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
END LOOP;END $$;

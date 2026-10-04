CREATE TABLE manufacturing_material_lot_issues (
 id TEXT PRIMARY KEY,order_id TEXT NOT NULL REFERENCES manufacturing_orders(id),source_order_version BIGINT NOT NULL,
 product_id TEXT NOT NULL REFERENCES commerce_products(id),lot_id TEXT NOT NULL REFERENCES inventory_lots(id),
 quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),value_minor BIGINT NOT NULL CHECK(value_minor>=0),
 created_by TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(order_id,lot_id)
);
CREATE TABLE manufacturing_output_lots (
 id TEXT PRIMARY KEY,order_id TEXT NOT NULL REFERENCES manufacturing_orders(id),source_order_version BIGINT NOT NULL,
 lot_id TEXT NOT NULL UNIQUE REFERENCES inventory_lots(id),quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),
 value_minor BIGINT NOT NULL CHECK(value_minor>=0),created_by TEXT NOT NULL,created_at BIGINT NOT NULL
);
CREATE TABLE manufacturing_lot_genealogy (
 id TEXT PRIMARY KEY,order_id TEXT NOT NULL REFERENCES manufacturing_orders(id),
 input_issue_id TEXT NOT NULL REFERENCES manufacturing_material_lot_issues(id),output_lot_id TEXT NOT NULL REFERENCES manufacturing_output_lots(id),
 input_quantity_milli BIGINT NOT NULL CHECK(input_quantity_milli>=0),source_order_version BIGINT NOT NULL,created_at BIGINT NOT NULL,
 UNIQUE(input_issue_id,output_lot_id)
);
CREATE TABLE manufacturing_inventory_moves (
 id TEXT PRIMARY KEY,order_id TEXT NOT NULL REFERENCES manufacturing_orders(id),warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),
 product_id TEXT NOT NULL REFERENCES commerce_products(id),phase TEXT NOT NULL CHECK(phase IN ('issue','output')),
 source_order_version BIGINT NOT NULL,quantity_milli BIGINT NOT NULL,value_minor BIGINT NOT NULL,
 movement_date TEXT NOT NULL,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,
 UNIQUE(order_id,source_order_version,product_id)
);
-- Keep all existing owner ledgers; append Manufacturing for valuation and
-- backdated movement / landed-cost guards shared by the inventory owners.
DO $$ DECLARE definition TEXT; BEGIN
 SELECT regexp_replace(pg_get_viewdef('inventory_ledger'::regclass,true),';\s*$','') INTO definition;
 EXECUTE 'CREATE OR REPLACE VIEW inventory_ledger AS '||definition||$view$
 UNION ALL SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,m.movement_date,
 'manufacturing_'||m.phase,m.order_id,m.order_id FROM manufacturing_inventory_moves m$view$;
END $$;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['manufacturing_material_lot_issues','manufacturing_output_lots','manufacturing_lot_genealogy','manufacturing_inventory_moves'] LOOP
  EXECUTE format('CREATE TRIGGER production_snapshot_immutable BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION production_snapshot_immutable()',t);
  INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
  EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
  EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;

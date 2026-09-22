CREATE TABLE inventory_landed_costs (
 id TEXT PRIMARY KEY,
 company_id TEXT NOT NULL,
 branch_id TEXT NOT NULL,
 fulfillment_id TEXT NOT NULL REFERENCES commerce_fulfillments(id),
 number TEXT NOT NULL,
 cost_date TEXT NOT NULL,
 description TEXT NOT NULL,
 allocation_method TEXT NOT NULL CHECK(allocation_method IN ('quantity','receipt_value')),
 amount_minor BIGINT NOT NULL CHECK(amount_minor>0),
 currency TEXT NOT NULL,
 credit_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 status TEXT NOT NULL DEFAULT 'submitted' CHECK(status IN ('submitted','approved','rejected','posted','cancelled')),
 version BIGINT NOT NULL DEFAULT 1,
 created_by TEXT NOT NULL,
 approved_by TEXT,
 journal_id TEXT REFERENCES finance_journals(id),
 review_reason TEXT,
 created_at BIGINT NOT NULL,
 updated_at BIGINT NOT NULL,
 UNIQUE(company_id,number)
);
CREATE TABLE inventory_landed_cost_lines (
 id TEXT PRIMARY KEY,
 landed_cost_id TEXT NOT NULL REFERENCES inventory_landed_costs(id),
 fulfillment_line_id TEXT NOT NULL REFERENCES commerce_fulfillment_lines(id),
 product_id TEXT NOT NULL REFERENCES commerce_products(id),
 quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),
 receipt_value_minor BIGINT NOT NULL CHECK(receipt_value_minor>=0),
 allocated_minor BIGINT NOT NULL CHECK(allocated_minor>=0),
 stock_quantity_milli BIGINT NOT NULL CHECK(stock_quantity_milli>=0),
 stock_value_minor BIGINT NOT NULL CHECK(stock_value_minor>=0),
 stock_version BIGINT NOT NULL CHECK(stock_version>0),
 ordinal BIGINT NOT NULL,
 UNIQUE(landed_cost_id,fulfillment_line_id)
);
CREATE TABLE inventory_landed_cost_lots (
 id TEXT PRIMARY KEY,
 landed_cost_line_id TEXT NOT NULL REFERENCES inventory_landed_cost_lines(id),
 lot_id TEXT NOT NULL REFERENCES inventory_lots(id),
 quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),
 receipt_value_minor BIGINT NOT NULL CHECK(receipt_value_minor>=0),
 allocated_minor BIGINT NOT NULL CHECK(allocated_minor>=0),
 balance_quantity_milli BIGINT NOT NULL CHECK(balance_quantity_milli>=0),
 balance_value_minor BIGINT NOT NULL CHECK(balance_value_minor>=0),
 balance_version BIGINT NOT NULL CHECK(balance_version>0),
 UNIQUE(landed_cost_line_id,lot_id)
);
CREATE TABLE inventory_landed_cost_events (
 id TEXT PRIMARY KEY,
 landed_cost_id TEXT NOT NULL REFERENCES inventory_landed_costs(id),
 command_id TEXT NOT NULL,
 action TEXT NOT NULL,
 actor_id TEXT NOT NULL,
 reviewed_version BIGINT NOT NULL,
 reason TEXT NOT NULL,
 created_at BIGINT NOT NULL,
 UNIQUE(landed_cost_id,command_id)
);
CREATE TABLE inventory_landed_cost_moves (
 id TEXT PRIMARY KEY,
 landed_cost_id TEXT NOT NULL REFERENCES inventory_landed_costs(id),
 warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),
 product_id TEXT NOT NULL REFERENCES commerce_products(id),
 value_minor BIGINT NOT NULL CHECK(value_minor>0),
 movement_date TEXT NOT NULL,
 created_at BIGINT NOT NULL,
 UNIQUE(landed_cost_id,product_id)
);
CREATE FUNCTION inventory_landed_cost_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'LANDED_COST_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER inventory_landed_cost_line_immutable BEFORE UPDATE OR DELETE ON inventory_landed_cost_lines FOR EACH ROW EXECUTE FUNCTION inventory_landed_cost_evidence_immutable();
CREATE TRIGGER inventory_landed_cost_lot_immutable BEFORE UPDATE OR DELETE ON inventory_landed_cost_lots FOR EACH ROW EXECUTE FUNCTION inventory_landed_cost_evidence_immutable();
CREATE TRIGGER inventory_landed_cost_event_immutable BEFORE UPDATE OR DELETE ON inventory_landed_cost_events FOR EACH ROW EXECUTE FUNCTION inventory_landed_cost_evidence_immutable();
CREATE TRIGGER inventory_landed_cost_move_immutable BEFORE UPDATE OR DELETE ON inventory_landed_cost_moves FOR EACH ROW EXECUTE FUNCTION inventory_landed_cost_evidence_immutable();
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_viewdef('inventory_ledger'::regclass,true) INTO definition;
 definition:=rtrim(definition,E';\n ');
 EXECUTE 'CREATE OR REPLACE VIEW inventory_ledger AS '||definition||$view$
 UNION ALL SELECT m.id,m.warehouse_id,m.product_id,0::bigint,m.value_minor,m.created_at,m.movement_date,'landed_cost',m.landed_cost_id,c.number
 FROM inventory_landed_cost_moves m JOIN inventory_landed_costs c ON c.id=m.landed_cost_id$view$;
END $$;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['inventory_landed_costs','inventory_landed_cost_lines','inventory_landed_cost_lots','inventory_landed_cost_events','inventory_landed_cost_moves'] LOOP
  INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
  EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
  EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;

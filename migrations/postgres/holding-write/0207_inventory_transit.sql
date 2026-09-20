-- Preserve completed direct transfers; staged transport is opt-in for new documents.
ALTER TABLE inventory_transfers ADD COLUMN delivery_mode TEXT NOT NULL DEFAULT 'direct' CHECK(delivery_mode IN ('direct','transit'));
ALTER TABLE inventory_transfers ADD COLUMN arrival_due_date TEXT;
CREATE TABLE inventory_transit (
 id TEXT PRIMARY KEY, transfer_id TEXT NOT NULL UNIQUE REFERENCES inventory_transfers(id),
 company_id TEXT NOT NULL, source_warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),
 target_warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id), source_branch_id TEXT NOT NULL,target_branch_id TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'awaiting_dispatch' CHECK(status IN ('awaiting_dispatch','in_transit','completed','cancelled')),
 due_date TEXT NOT NULL, currency TEXT NOT NULL,chart_id TEXT NOT NULL,inventory_account_id TEXT NOT NULL,
 created_by TEXT NOT NULL,approved_by TEXT NOT NULL,dispatched_by TEXT,dispatch_date TEXT,tracking_reference TEXT,
 version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(source_warehouse_id<>target_warehouse_id)
);
CREATE TABLE inventory_transit_lines (
 id TEXT PRIMARY KEY,transit_id TEXT NOT NULL REFERENCES inventory_transit(id),source_line_id TEXT NOT NULL REFERENCES inventory_transfer_lines(id),
 product_id TEXT NOT NULL REFERENCES commerce_products(id),quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),value_minor BIGINT NOT NULL CHECK(value_minor>0),
 received_milli BIGINT NOT NULL DEFAULT 0 CHECK(received_milli>=0),lost_milli BIGINT NOT NULL DEFAULT 0 CHECK(lost_milli>=0),
 settled_value_minor BIGINT NOT NULL DEFAULT 0 CHECK(settled_value_minor>=0),
 CHECK(received_milli+lost_milli<=quantity_milli),CHECK(settled_value_minor<=value_minor),
 CHECK(received_milli+lost_milli<>quantity_milli OR settled_value_minor=value_minor),UNIQUE(transit_id,product_id)
);
CREATE TABLE inventory_transit_events (
 id TEXT PRIMARY KEY,transit_id TEXT NOT NULL REFERENCES inventory_transit(id),line_id TEXT REFERENCES inventory_transit_lines(id),
 kind TEXT NOT NULL CHECK(kind IN ('dispatch','receipt','loss_requested','loss_approved','loss_rejected','cancel')),
 quantity_milli BIGINT NOT NULL DEFAULT 0 CHECK(quantity_milli>=0),value_minor BIGINT NOT NULL DEFAULT 0 CHECK(value_minor>=0),
 movement_date TEXT NOT NULL,evidence TEXT NOT NULL,actor_id TEXT NOT NULL,reviewed_version BIGINT NOT NULL,
 proposal_id TEXT UNIQUE REFERENCES inventory_transit_events(id),journal_id TEXT REFERENCES finance_journals(id),created_at BIGINT NOT NULL
);
CREATE INDEX inventory_transit_scope ON inventory_transit(company_id,source_branch_id,target_branch_id,status,due_date);
CREATE INDEX inventory_transit_event_source ON inventory_transit_events(transit_id,line_id,kind);
CREATE FUNCTION inventory_transit_event_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'TRANSIT_EVENT_IMMUTABLE'; END $$;
CREATE TRIGGER inventory_transit_event_immutable BEFORE UPDATE OR DELETE ON inventory_transit_events FOR EACH ROW EXECUTE FUNCTION inventory_transit_event_immutable();
CREATE VIEW inventory_in_transit AS SELECT d.id transit_id,d.company_id,d.source_branch_id,d.target_branch_id,d.currency,l.product_id,
 l.quantity_milli-l.received_milli-l.lost_milli quantity_milli,l.value_minor-l.settled_value_minor value_minor
 FROM inventory_transit d JOIN inventory_transit_lines l ON l.transit_id=d.id WHERE d.status='in_transit';
-- Append the new stock movement owner without rewriting any historical owner view.
DO $$ DECLARE definition TEXT; BEGIN
 SELECT pg_get_viewdef('inventory_ledger'::regclass,true) INTO definition;
 definition:=rtrim(definition,E';\n ');
 EXECUTE 'CREATE OR REPLACE VIEW inventory_ledger AS '||definition||$view$
 UNION ALL SELECT e.id,CASE WHEN e.kind='dispatch' THEN d.source_warehouse_id ELSE d.target_warehouse_id END,l.product_id,
 CASE WHEN e.kind='dispatch' THEN -e.quantity_milli ELSE e.quantity_milli END,
 CASE WHEN e.kind='dispatch' THEN -e.value_minor ELSE e.value_minor END,e.created_at,e.movement_date,'transit',d.id,t.number
 FROM inventory_transit_events e JOIN inventory_transit d ON d.id=e.transit_id JOIN inventory_transit_lines l ON l.id=e.line_id
 JOIN inventory_transfers t ON t.id=d.transfer_id WHERE e.kind IN ('dispatch','receipt')$view$;
END $$;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['inventory_transit','inventory_transit_lines','inventory_transit_events'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;

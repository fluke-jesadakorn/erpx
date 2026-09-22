-- Preserve lot/serial identity across warehouse transfers.
CREATE TABLE inventory_transfer_lot_allocations (
 id TEXT PRIMARY KEY,
 transfer_line_id TEXT NOT NULL REFERENCES inventory_transfer_lines(id),
 lot_id TEXT NOT NULL REFERENCES inventory_lots(id),
 quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),
 value_minor BIGINT NOT NULL CHECK(value_minor>=0),
 source_balance_version BIGINT NOT NULL CHECK(source_balance_version>0),
 UNIQUE(transfer_line_id,lot_id)
);
CREATE INDEX inventory_transfer_lot_trace ON inventory_transfer_lot_allocations(lot_id,transfer_line_id);

CREATE TABLE inventory_transit_lots (
 id TEXT PRIMARY KEY,
 transit_line_id TEXT NOT NULL REFERENCES inventory_transit_lines(id),
 lot_id TEXT NOT NULL REFERENCES inventory_lots(id),
 quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),
 value_minor BIGINT NOT NULL CHECK(value_minor>=0),
 received_milli BIGINT NOT NULL DEFAULT 0 CHECK(received_milli>=0),
 lost_milli BIGINT NOT NULL DEFAULT 0 CHECK(lost_milli>=0),
 settled_value_minor BIGINT NOT NULL DEFAULT 0 CHECK(settled_value_minor>=0),
 CHECK(received_milli+lost_milli<=quantity_milli),
 CHECK(settled_value_minor<=value_minor),
 CHECK(received_milli+lost_milli<>quantity_milli OR settled_value_minor=value_minor),
 UNIQUE(transit_line_id,lot_id)
);
CREATE INDEX inventory_transit_lot_trace ON inventory_transit_lots(lot_id,transit_line_id);

CREATE TABLE inventory_count_lot_lines (
 id TEXT PRIMARY KEY,
 count_line_id TEXT NOT NULL REFERENCES inventory_count_lines(id),
 source_lot_id TEXT REFERENCES inventory_lots(id),
 result_lot_id TEXT NOT NULL,
 tracking_mode TEXT NOT NULL CHECK(tracking_mode IN ('lot','serial')),
 lot_code TEXT,
 serial_number TEXT,
 manufactured_date TEXT,
 expiry_date TEXT,
 snapshot_quantity_milli BIGINT NOT NULL CHECK(snapshot_quantity_milli>=0),
 snapshot_value_minor BIGINT NOT NULL CHECK(snapshot_value_minor>=0),
 snapshot_version BIGINT NOT NULL CHECK(snapshot_version>=0),
 counted_quantity_milli BIGINT NOT NULL CHECK(counted_quantity_milli>=0),
 counted_value_minor BIGINT NOT NULL CHECK(counted_value_minor>=0),
 unit_cost_minor BIGINT,
 CHECK((tracking_mode='lot' AND lot_code IS NOT NULL AND serial_number IS NULL) OR (tracking_mode='serial' AND serial_number IS NOT NULL)),
 CHECK(tracking_mode<>'serial' OR counted_quantity_milli IN (0,1000)),
 UNIQUE(count_line_id,result_lot_id)
);
CREATE INDEX inventory_count_lot_source ON inventory_count_lot_lines(source_lot_id,count_line_id);

CREATE TABLE inventory_transit_lot_events (
 id TEXT PRIMARY KEY,
 event_id TEXT NOT NULL REFERENCES inventory_transit_events(id),
 transit_lot_id TEXT NOT NULL REFERENCES inventory_transit_lots(id),
 quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),
 value_minor BIGINT NOT NULL CHECK(value_minor>=0),
 UNIQUE(event_id,transit_lot_id)
);
CREATE FUNCTION inventory_transit_lot_event_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'TRANSIT_LOT_EVENT_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER inventory_transit_lot_event_immutable BEFORE UPDATE OR DELETE ON inventory_transit_lot_events FOR EACH ROW EXECUTE FUNCTION inventory_transit_lot_event_immutable();

CREATE FUNCTION inventory_transfer_lot_edit_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE state TEXT;
BEGIN
 SELECT t.status INTO state FROM inventory_transfer_lines l JOIN inventory_transfers t ON t.id=l.transfer_id
 WHERE l.id=COALESCE(NEW.transfer_line_id,OLD.transfer_line_id);
 IF state NOT IN ('draft','rejected') THEN
  RAISE EXCEPTION 'TRANSFER_LOT_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';
 END IF;
 RETURN COALESCE(NEW,OLD);
END $$;
CREATE TRIGGER inventory_transfer_lot_edit_guard BEFORE INSERT OR UPDATE OR DELETE ON inventory_transfer_lot_allocations FOR EACH ROW EXECUTE FUNCTION inventory_transfer_lot_edit_guard();

CREATE FUNCTION inventory_count_lot_edit_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE state TEXT;
BEGIN
 SELECT c.status INTO state FROM inventory_count_lines l JOIN inventory_counts c ON c.id=l.count_id
 WHERE l.id=COALESCE(NEW.count_line_id,OLD.count_line_id);
 IF state NOT IN ('draft','rejected') THEN
  RAISE EXCEPTION 'COUNT_LOT_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';
 END IF;
 RETURN COALESCE(NEW,OLD);
END $$;
CREATE TRIGGER inventory_count_lot_edit_guard BEFORE INSERT OR UPDATE OR DELETE ON inventory_count_lot_lines FOR EACH ROW EXECUTE FUNCTION inventory_count_lot_edit_guard();

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['inventory_transfer_lot_allocations','inventory_transit_lots','inventory_transit_lot_events','inventory_count_lot_lines'] LOOP
  INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
  EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
  EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;

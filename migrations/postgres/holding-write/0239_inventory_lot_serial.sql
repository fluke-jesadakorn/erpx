-- Traceable lot/serial stock. Aggregate stock remains the accounting control total.
ALTER TABLE commerce_products ADD COLUMN tracking_mode TEXT NOT NULL DEFAULT 'none' CHECK(tracking_mode IN ('none','lot','serial'));

CREATE TABLE inventory_lots (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, product_id TEXT NOT NULL REFERENCES commerce_products(id),
 tracking_mode TEXT NOT NULL CHECK(tracking_mode IN ('lot','serial')),
 lot_code TEXT, serial_number TEXT, manufactured_date TEXT, expiry_date TEXT,
 status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','depleted','scrapped','returned')),
 created_by TEXT NOT NULL, created_at BIGINT NOT NULL,
 UNIQUE(id,product_id),
 CHECK((tracking_mode='lot' AND lot_code IS NOT NULL AND serial_number IS NULL) OR (tracking_mode='serial' AND serial_number IS NOT NULL))
);
CREATE UNIQUE INDEX inventory_lot_code_unique ON inventory_lots(company_id,product_id,lot_code) WHERE tracking_mode='lot';
CREATE UNIQUE INDEX inventory_serial_unique ON inventory_lots(company_id,product_id,serial_number) WHERE tracking_mode='serial';

CREATE TABLE inventory_lot_balances (
 lot_id TEXT NOT NULL REFERENCES inventory_lots(id), warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),
 quantity_milli BIGINT NOT NULL DEFAULT 0 CHECK(quantity_milli>=0),
 value_minor BIGINT NOT NULL DEFAULT 0 CHECK(value_minor>=0), version BIGINT NOT NULL DEFAULT 1 CHECK(version>0),
 PRIMARY KEY(lot_id,warehouse_id)
);
CREATE INDEX inventory_lot_balance_warehouse ON inventory_lot_balances(warehouse_id,lot_id);

CREATE TABLE inventory_lot_moves (
 id TEXT PRIMARY KEY, lot_id TEXT NOT NULL REFERENCES inventory_lots(id), warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),
 source_type TEXT NOT NULL, source_id TEXT NOT NULL, quantity_milli BIGINT NOT NULL, value_minor BIGINT NOT NULL,
 movement_date TEXT NOT NULL, created_by TEXT NOT NULL, created_at BIGINT NOT NULL,
 UNIQUE(source_type,source_id,lot_id)
);
CREATE INDEX inventory_lot_moves_trace ON inventory_lot_moves(lot_id,movement_date,created_at);

CREATE TABLE inventory_fulfillment_lot_allocations (
 id TEXT PRIMARY KEY, fulfillment_line_id TEXT NOT NULL REFERENCES commerce_fulfillment_lines(id),
 lot_id TEXT NOT NULL REFERENCES inventory_lots(id), quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),
 value_minor BIGINT NOT NULL CHECK(value_minor>=0), disposition TEXT NOT NULL CHECK(disposition IN ('available','quarantine')),
 quality_hold_id TEXT, UNIQUE(fulfillment_line_id,lot_id)
);

ALTER TABLE inventory_quality_holds ADD COLUMN lot_id TEXT REFERENCES inventory_lots(id);
ALTER TABLE inventory_quality_holds ADD COLUMN held_quantity_milli BIGINT CHECK(held_quantity_milli>0);
ALTER TABLE inventory_quality_holds ADD COLUMN source_type TEXT;
ALTER TABLE inventory_quality_holds ADD COLUMN source_id TEXT;
CREATE INDEX quality_hold_lot ON inventory_quality_holds(lot_id,status) WHERE lot_id IS NOT NULL;
ALTER TABLE inventory_fulfillment_lot_allocations ADD CONSTRAINT inventory_fulfillment_quality_hold_fk FOREIGN KEY(quality_hold_id) REFERENCES inventory_quality_holds(id);

CREATE FUNCTION inventory_tracking_mode_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.tracking_mode<>OLD.tracking_mode AND (
   EXISTS(SELECT 1 FROM commerce_stock WHERE product_id=OLD.id AND quantity_milli<>0)
   OR EXISTS(SELECT 1 FROM inventory_lots WHERE product_id=OLD.id)
 ) THEN RAISE EXCEPTION 'TRACKING_MODE_LOCKED' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER inventory_tracking_mode_guard BEFORE UPDATE OF tracking_mode ON commerce_products FOR EACH ROW EXECUTE FUNCTION inventory_tracking_mode_guard();

CREATE FUNCTION inventory_lot_control_total() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE w TEXT; p TEXT; mode TEXT; stock_qty BIGINT; stock_value BIGINT; lot_qty BIGINT; lot_value BIGINT;
BEGIN
 IF TG_TABLE_NAME='commerce_stock' THEN w:=COALESCE(NEW.warehouse_id,OLD.warehouse_id);p:=COALESCE(NEW.product_id,OLD.product_id);
 ELSE
  w:=COALESCE(NEW.warehouse_id,OLD.warehouse_id);
  SELECT product_id INTO p FROM inventory_lots WHERE id=COALESCE(NEW.lot_id,OLD.lot_id);
 END IF;
 SELECT tracking_mode INTO mode FROM commerce_products WHERE id=p;
 IF mode='none' OR mode IS NULL THEN RETURN NULL; END IF;
 SELECT COALESCE(quantity_milli,0),COALESCE(value_minor,0) INTO stock_qty,stock_value FROM commerce_stock WHERE warehouse_id=w AND product_id=p;
 SELECT COALESCE(sum(b.quantity_milli),0),COALESCE(sum(b.value_minor),0) INTO lot_qty,lot_value FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.warehouse_id=w AND l.product_id=p;
 IF COALESCE(stock_qty,0)<>lot_qty OR COALESCE(stock_value,0)<>lot_value THEN RAISE EXCEPTION 'LOT_CONTROL_TOTAL_MISMATCH' USING ERRCODE='23514'; END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER inventory_lot_balance_control AFTER INSERT OR UPDATE OR DELETE ON inventory_lot_balances DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_lot_control_total();
CREATE CONSTRAINT TRIGGER inventory_stock_lot_control AFTER INSERT OR UPDATE OR DELETE ON commerce_stock DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_lot_control_total();

CREATE FUNCTION inventory_lot_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'LOT_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER inventory_lot_moves_immutable BEFORE UPDATE OR DELETE ON inventory_lot_moves FOR EACH ROW EXECUTE FUNCTION inventory_lot_evidence_immutable();
CREATE TRIGGER inventory_fulfillment_lot_immutable BEFORE UPDATE OR DELETE ON inventory_fulfillment_lot_allocations FOR EACH ROW EXECUTE FUNCTION inventory_lot_evidence_immutable();

CREATE FUNCTION quality_lot_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.lot_id IS NOT NULL AND NOT EXISTS(
   SELECT 1 FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id
   WHERE l.id=NEW.lot_id AND l.company_id=NEW.company_id AND l.product_id=NEW.product_id AND b.warehouse_id=NEW.warehouse_id AND b.quantity_milli>=NEW.held_quantity_milli
 ) THEN RAISE EXCEPTION 'QUALITY_LOT_INVALID' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER quality_lot_guard BEFORE INSERT OR UPDATE OF lot_id,held_quantity_milli ON inventory_quality_holds FOR EACH ROW EXECUTE FUNCTION quality_lot_guard();

-- Quarantined quantities remain on hand but are unavailable to reservations and issues.
CREATE FUNCTION inventory_quarantined_quantity(w TEXT,p TEXT) RETURNS BIGINT LANGUAGE sql STABLE AS $$
 SELECT CASE WHEN EXISTS(SELECT 1 FROM inventory_quality_holds h WHERE h.warehouse_id=w AND h.product_id=p AND h.lot_id IS NULL AND h.status NOT IN ('released','cancelled'))
 THEN COALESCE((SELECT quantity_milli FROM commerce_stock WHERE warehouse_id=w AND product_id=p),0)
 ELSE COALESCE((SELECT sum(b.quantity_milli) FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.warehouse_id=w AND l.product_id=p AND EXISTS(SELECT 1 FROM inventory_quality_holds h WHERE h.lot_id=l.id AND h.warehouse_id=w AND h.status NOT IN ('released','cancelled'))),0) END
$$;
CREATE OR REPLACE FUNCTION pos_guard_reserved_stock() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE quarantined BIGINT; reserved BIGINT;
BEGIN
 IF NEW.quantity_milli>=OLD.quantity_milli THEN RETURN NEW; END IF;
 IF EXISTS(SELECT 1 FROM inventory_quality_holds WHERE warehouse_id=NEW.warehouse_id AND product_id=NEW.product_id AND lot_id IS NULL AND status NOT IN ('released','cancelled')) THEN
  RAISE EXCEPTION 'QUALITY_HOLD' USING ERRCODE='23514';
 END IF;
 quarantined:=inventory_quarantined_quantity(NEW.warehouse_id,NEW.product_id);
 SELECT COALESCE(SUM(quantity_milli),0) INTO reserved FROM inventory_active_reservations WHERE warehouse_id=NEW.warehouse_id AND product_id=NEW.product_id;
 -- A newly quarantined lot may already be reserved. Those quantities overlap, so
 -- the aggregate control must retain the larger requirement rather than sum both.
 IF NEW.quantity_milli < reserved THEN
  RAISE EXCEPTION 'STOCK_RESERVED' USING ERRCODE='23514';
 END IF;
 IF NEW.quantity_milli < quarantined THEN
  RAISE EXCEPTION 'STOCK_QUARANTINED_OR_RESERVED' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE OR REPLACE FUNCTION inventory_reservation_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE w TEXT; p TEXT; increasing BOOLEAN:=FALSE; reserved BIGINT; available BIGINT;
BEGIN
 w:=COALESCE(NEW.warehouse_id,OLD.warehouse_id); p:=COALESCE(NEW.product_id,OLD.product_id);
 IF TG_OP='INSERT' THEN increasing:=TRUE;
 ELSIF TG_OP='UPDATE' THEN
  IF TG_TABLE_NAME='inventory_order_reservations' THEN increasing:=NEW.quantity_milli-NEW.consumed_milli-NEW.released_milli>OLD.quantity_milli-OLD.consumed_milli-OLD.released_milli;
  ELSE increasing:=NEW.quantity_milli>OLD.quantity_milli; END IF;
 END IF;
 IF increasing THEN
  SELECT COALESCE(quantity_milli,0)-inventory_quarantined_quantity(w,p) INTO available FROM commerce_stock WHERE warehouse_id=w AND product_id=p;
  SELECT COALESCE(sum(quantity_milli),0) INTO reserved FROM inventory_active_reservations WHERE warehouse_id=w AND product_id=p;
  IF reserved>COALESCE(available,0) THEN RAISE EXCEPTION 'QUALITY_HOLD' USING ERRCODE='23514'; END IF;
 END IF;
 UPDATE commerce_stock SET version=version+1 WHERE warehouse_id=w AND product_id=p;
 RETURN NULL;
END $$;

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['inventory_lots','inventory_lot_balances','inventory_lot_moves','inventory_fulfillment_lot_allocations'] LOOP
  INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
  EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
  EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;

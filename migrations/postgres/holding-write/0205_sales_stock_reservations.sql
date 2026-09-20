-- Keep existing POS reservation IDs and history. Sales reservations share availability.
CREATE TABLE inventory_order_reservations (
 id TEXT PRIMARY KEY,
 order_line_id TEXT NOT NULL REFERENCES commerce_order_lines(id),
 warehouse_id TEXT NOT NULL,
 product_id TEXT NOT NULL,
 quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),
 consumed_milli BIGINT NOT NULL DEFAULT 0 CHECK(consumed_milli>=0),
 released_milli BIGINT NOT NULL DEFAULT 0 CHECK(released_milli>=0),
 expires_at BIGINT NOT NULL,
 created_by TEXT NOT NULL,
 reason TEXT NOT NULL,
 release_reason TEXT,
 created_at BIGINT NOT NULL,
 CHECK(consumed_milli+released_milli<=quantity_milli),
 CHECK(expires_at>created_at),
 FOREIGN KEY(warehouse_id,product_id) REFERENCES commerce_stock(warehouse_id,product_id)
);
CREATE INDEX inventory_order_reservations_line ON inventory_order_reservations(order_line_id);
CREATE INDEX inventory_order_reservations_stock ON inventory_order_reservations(warehouse_id,product_id,expires_at);
CREATE VIEW inventory_active_reservations AS
 SELECT warehouse_id,product_id,quantity_milli,'pos'::text source_kind,fulfillment_id source_id,NULL::text order_line_id FROM pos_reservations
 UNION ALL
 SELECT warehouse_id,product_id,quantity_milli-consumed_milli-released_milli,'sales',id,order_line_id
 FROM inventory_order_reservations WHERE quantity_milli>consumed_milli+released_milli
 AND expires_at>floor(extract(epoch FROM statement_timestamp())*1000)::bigint;
-- This guard covers every existing stock writer, including counts and transfers.
CREATE OR REPLACE FUNCTION pos_guard_reserved_stock() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.quantity_milli < COALESCE((SELECT SUM(quantity_milli) FROM inventory_active_reservations WHERE warehouse_id=NEW.warehouse_id AND product_id=NEW.product_id),0) THEN
  RAISE EXCEPTION 'STOCK_RESERVED' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
-- Serialize reservation changes with stock writers and invalidate their optimistic versions.
CREATE FUNCTION inventory_reservation_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE w TEXT; p TEXT;
BEGIN
 w:=COALESCE(NEW.warehouse_id,OLD.warehouse_id); p:=COALESCE(NEW.product_id,OLD.product_id);
 UPDATE commerce_stock SET version=version+1 WHERE warehouse_id=w AND product_id=p;
 RETURN NULL;
END $$;
CREATE TRIGGER inventory_order_reservation_changed AFTER INSERT OR UPDATE OR DELETE ON inventory_order_reservations FOR EACH ROW EXECUTE FUNCTION inventory_reservation_changed();
CREATE TRIGGER inventory_pos_reservation_changed AFTER INSERT OR UPDATE OR DELETE ON pos_reservations FOR EACH ROW EXECUTE FUNCTION inventory_reservation_changed();

-- Cancellation is a reviewed correction of an uninspected hold, never a failed-inspection bypass.
ALTER TABLE inventory_quality_holds DROP CONSTRAINT inventory_quality_holds_status_check;
ALTER TABLE inventory_quality_holds ADD CONSTRAINT inventory_quality_holds_status_check CHECK(status IN ('held','awaiting_release','released','cancel_pending','cancelled'));
ALTER TABLE inventory_quality_holds ADD COLUMN cancel_requested_by TEXT;
ALTER TABLE inventory_quality_holds ADD COLUMN cancel_reason TEXT;
ALTER TABLE inventory_quality_holds ADD COLUMN cancelled_by TEXT;
ALTER TABLE inventory_quality_holds ADD COLUMN cancelled_at BIGINT;
CREATE TABLE inventory_quality_cancellations (
 id TEXT PRIMARY KEY, hold_id TEXT NOT NULL REFERENCES inventory_quality_holds(id), version BIGINT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('request','approve','reject')), reason TEXT NOT NULL, actor_id TEXT NOT NULL, created_at BIGINT NOT NULL,
 UNIQUE(hold_id,version)
);
CREATE FUNCTION quality_cancellation_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='inventory_quality_cancellations' THEN RAISE EXCEPTION 'QUALITY_CANCELLATION_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF NEW.status='cancelled' AND OLD.status<>'cancelled' THEN
  IF OLD.status<>'cancel_pending' OR NEW.cancelled_by IS NULL OR NEW.cancelled_at IS NULL OR iam_same_person(NEW.cancelled_by,NEW.created_by,NEW.company_id) OR iam_same_person(NEW.cancelled_by,NEW.cancel_requested_by,NEW.company_id) OR EXISTS(SELECT 1 FROM inventory_quality_inspections WHERE hold_id=NEW.id) THEN RAISE EXCEPTION 'QUALITY_CANCELLATION_INVALID' USING ERRCODE='23514'; END IF;
 END IF;
 IF OLD.status='cancelled' AND NEW IS DISTINCT FROM OLD THEN RAISE EXCEPTION 'QUALITY_CANCELLATION_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER quality_cancellation_evidence BEFORE UPDATE OR DELETE ON inventory_quality_cancellations FOR EACH ROW EXECUTE FUNCTION quality_cancellation_guard();
CREATE TRIGGER quality_cancel_guard BEFORE UPDATE ON inventory_quality_holds FOR EACH ROW EXECUTE FUNCTION quality_cancellation_guard();
CREATE OR REPLACE FUNCTION pos_guard_reserved_stock() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.quantity_milli<OLD.quantity_milli AND EXISTS(SELECT 1 FROM inventory_quality_holds WHERE warehouse_id=NEW.warehouse_id AND product_id=NEW.product_id AND status NOT IN ('released','cancelled')) THEN
  RAISE EXCEPTION 'QUALITY_HOLD' USING ERRCODE='23514';
 END IF;
 IF NEW.quantity_milli < COALESCE((SELECT SUM(quantity_milli) FROM inventory_active_reservations WHERE warehouse_id=NEW.warehouse_id AND product_id=NEW.product_id),0) THEN
  RAISE EXCEPTION 'STOCK_RESERVED' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE OR REPLACE FUNCTION inventory_reservation_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE w TEXT; p TEXT; increasing BOOLEAN:=FALSE;
BEGIN
 w:=COALESCE(NEW.warehouse_id,OLD.warehouse_id); p:=COALESCE(NEW.product_id,OLD.product_id);
 IF TG_OP='INSERT' THEN increasing:=TRUE;
 ELSIF TG_OP='UPDATE' THEN
  IF TG_TABLE_NAME='inventory_order_reservations' THEN increasing:=NEW.quantity_milli-NEW.consumed_milli-NEW.released_milli>OLD.quantity_milli-OLD.consumed_milli-OLD.released_milli;
  ELSE increasing:=NEW.quantity_milli>OLD.quantity_milli; END IF;
 END IF;
 IF increasing AND EXISTS(SELECT 1 FROM inventory_quality_holds WHERE warehouse_id=w AND product_id=p AND status NOT IN ('released','cancelled')) THEN RAISE EXCEPTION 'QUALITY_HOLD' USING ERRCODE='23514'; END IF;
 UPDATE commerce_stock SET version=version+1 WHERE warehouse_id=w AND product_id=p;
 RETURN NULL;
END $$;

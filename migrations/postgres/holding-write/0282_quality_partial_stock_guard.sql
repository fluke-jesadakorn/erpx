-- Partial quality holds still protect their quarantined quantity, while an
-- approved count may reduce unrelated available stock in the same location.
CREATE OR REPLACE FUNCTION pos_guard_reserved_stock() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE quarantined BIGINT; reserved BIGINT;
BEGIN
 IF NEW.quantity_milli>=OLD.quantity_milli THEN RETURN NEW; END IF;
 quarantined:=inventory_quarantined_quantity(NEW.warehouse_id,NEW.product_id);
 SELECT COALESCE(SUM(quantity_milli),0) INTO reserved
 FROM inventory_active_reservations
 WHERE warehouse_id=NEW.warehouse_id AND product_id=NEW.product_id;
 IF NEW.quantity_milli < reserved THEN
  RAISE EXCEPTION 'STOCK_RESERVED' USING ERRCODE='23514';
 END IF;
 IF NEW.quantity_milli < quarantined THEN
  RAISE EXCEPTION 'STOCK_QUARANTINED_OR_RESERVED' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;

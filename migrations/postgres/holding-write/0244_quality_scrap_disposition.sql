-- Failed quality inspections can be resolved through an independently reviewed
-- stock write-off. The stock count remains the accounting control document.
ALTER TABLE inventory_quality_holds DROP CONSTRAINT inventory_quality_holds_status_check;
ALTER TABLE inventory_quality_holds ADD CONSTRAINT inventory_quality_holds_status_check
  CHECK(status IN ('held','awaiting_release','released','cancel_pending','cancelled','disposition_pending','disposed'));
ALTER TABLE inventory_quality_holds ADD COLUMN disposition_kind TEXT CHECK(disposition_kind IN ('scrap'));
ALTER TABLE inventory_quality_holds ADD COLUMN disposition_reason TEXT;
ALTER TABLE inventory_quality_holds ADD COLUMN disposition_requested_by TEXT;
ALTER TABLE inventory_quality_holds ADD COLUMN disposition_count_id TEXT REFERENCES inventory_counts(id);
ALTER TABLE inventory_quality_holds ADD COLUMN disposed_by TEXT;
ALTER TABLE inventory_quality_holds ADD COLUMN disposed_at BIGINT;
ALTER TABLE inventory_quality_holds ADD CONSTRAINT inventory_quality_disposition_state CHECK(
  status NOT IN ('disposition_pending','disposed') OR
  (last_result='fail' AND disposition_kind='scrap' AND disposition_reason IS NOT NULL AND disposition_requested_by IS NOT NULL AND disposition_count_id IS NOT NULL)
);
ALTER TABLE inventory_quality_holds ADD CONSTRAINT inventory_quality_disposed_state CHECK(
  status<>'disposed' OR (disposed_by IS NOT NULL AND disposed_at IS NOT NULL)
);

ALTER TABLE inventory_counts ADD COLUMN quality_hold_id TEXT REFERENCES inventory_quality_holds(id);
CREATE INDEX inventory_count_quality_hold ON inventory_counts(quality_hold_id) WHERE quality_hold_id IS NOT NULL;

CREATE TABLE inventory_quality_disposition_events (
 id TEXT PRIMARY KEY,
 hold_id TEXT NOT NULL REFERENCES inventory_quality_holds(id),
 count_id TEXT NOT NULL REFERENCES inventory_counts(id),
 action TEXT NOT NULL CHECK(action IN ('request','reject','post')),
 actor_id TEXT NOT NULL,
 reviewed_version BIGINT NOT NULL CHECK(reviewed_version>0),
 reason TEXT NOT NULL,
 created_at BIGINT NOT NULL
);
CREATE INDEX inventory_quality_disposition_history ON inventory_quality_disposition_events(hold_id,created_at,id);
CREATE FUNCTION quality_disposition_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'QUALITY_DISPOSITION_IMMUTABLE' USING ERRCODE='23514'; END $$;
CREATE TRIGGER quality_disposition_evidence_immutable BEFORE UPDATE OR DELETE ON inventory_quality_disposition_events FOR EACH ROW EXECUTE FUNCTION quality_disposition_evidence_immutable();
CREATE FUNCTION quality_disposed_hold_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.status='disposed' THEN RAISE EXCEPTION 'QUALITY_DISPOSITION_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN COALESCE(NEW,OLD);
END $$;
CREATE TRIGGER quality_disposed_hold_immutable BEFORE UPDATE OR DELETE ON inventory_quality_holds FOR EACH ROW EXECUTE FUNCTION quality_disposed_hold_immutable();

-- Disposal changes quarantine availability and stock in the same transaction.
-- Keep the captured aggregate version stable until the controlled write-off.
CREATE OR REPLACE FUNCTION quality_hold_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' AND (
   (OLD.status IN ('held','disposition_pending') AND NEW.status IN ('held','disposition_pending'))
   OR (OLD.status='disposition_pending' AND NEW.status='disposed')
 ) THEN RETURN NULL; END IF;
 UPDATE commerce_stock SET version=version+1 WHERE warehouse_id=NEW.warehouse_id AND product_id=NEW.product_id;
 RETURN NULL;
END $$;

CREATE OR REPLACE FUNCTION inventory_quarantined_quantity(w TEXT,p TEXT) RETURNS BIGINT LANGUAGE sql STABLE AS $$
 SELECT CASE WHEN EXISTS(SELECT 1 FROM inventory_quality_holds h WHERE h.warehouse_id=w AND h.product_id=p AND h.lot_id IS NULL AND h.status NOT IN ('released','cancelled','disposed'))
 THEN COALESCE((SELECT quantity_milli FROM commerce_stock WHERE warehouse_id=w AND product_id=p),0)
 ELSE COALESCE((SELECT sum(b.quantity_milli) FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.warehouse_id=w AND l.product_id=p AND EXISTS(SELECT 1 FROM inventory_quality_holds h WHERE h.lot_id=l.id AND h.warehouse_id=w AND h.status NOT IN ('released','cancelled','disposed'))),0) END
$$;
CREATE OR REPLACE FUNCTION pos_guard_reserved_stock() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE quarantined BIGINT; reserved BIGINT;
BEGIN
 IF NEW.quantity_milli>=OLD.quantity_milli THEN RETURN NEW; END IF;
 IF EXISTS(SELECT 1 FROM inventory_quality_holds WHERE warehouse_id=NEW.warehouse_id AND product_id=NEW.product_id AND lot_id IS NULL AND status NOT IN ('released','cancelled','disposed')) THEN
  RAISE EXCEPTION 'QUALITY_HOLD' USING ERRCODE='23514';
 END IF;
 quarantined:=inventory_quarantined_quantity(NEW.warehouse_id,NEW.product_id);
 SELECT COALESCE(SUM(quantity_milli),0) INTO reserved FROM inventory_active_reservations WHERE warehouse_id=NEW.warehouse_id AND product_id=NEW.product_id;
 IF NEW.quantity_milli < reserved THEN RAISE EXCEPTION 'STOCK_RESERVED' USING ERRCODE='23514'; END IF;
 IF NEW.quantity_milli < quarantined THEN RAISE EXCEPTION 'STOCK_QUARANTINED_OR_RESERVED' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['inventory_quality_disposition_events'] LOOP
  INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
  EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
  EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;

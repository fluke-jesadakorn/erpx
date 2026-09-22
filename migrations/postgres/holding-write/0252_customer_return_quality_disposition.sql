-- Customer returns re-enter accounting stock under a quantity-specific Quality
-- hold. They cannot be reserved or issued until an independent inspection and
-- release, or until controlled scrap completes.
ALTER TABLE commerce_return_lines
  ADD COLUMN condition_code TEXT NOT NULL DEFAULT 'unknown'
    CHECK(condition_code IN ('unopened','damaged','unknown')),
  ADD COLUMN condition_note TEXT,
  ADD COLUMN quality_due_date TEXT;

ALTER TABLE inventory_quality_holds
  ADD COLUMN quantity_scope TEXT NOT NULL DEFAULT 'whole'
    CHECK(quantity_scope IN ('whole','partial'));

UPDATE inventory_quality_holds
SET quantity_scope='partial'
WHERE source_type='goods_receipt' OR lot_id IS NOT NULL;

CREATE UNIQUE INDEX inventory_quality_hold_source
  ON inventory_quality_holds(source_type,source_id)
  WHERE source_type IS NOT NULL AND source_id IS NOT NULL;

CREATE OR REPLACE FUNCTION inventory_quarantined_quantity(w TEXT,p TEXT) RETURNS BIGINT LANGUAGE sql STABLE AS $$
 SELECT CASE
  WHEN EXISTS(
    SELECT 1 FROM inventory_quality_holds h
    WHERE h.warehouse_id=w AND h.product_id=p AND h.quantity_scope='whole'
      AND h.status NOT IN ('released','cancelled','disposed')
  ) THEN COALESCE((SELECT quantity_milli FROM commerce_stock WHERE warehouse_id=w AND product_id=p),0)
  ELSE LEAST(
    COALESCE((SELECT quantity_milli FROM commerce_stock WHERE warehouse_id=w AND product_id=p),0),
    COALESCE((
      SELECT sum(h.held_quantity_milli)
      FROM inventory_quality_holds h
      WHERE h.warehouse_id=w AND h.product_id=p AND h.quantity_scope='partial'
        AND h.status NOT IN ('released','cancelled','disposed')
    ),0)
  )
 END
$$;

CREATE FUNCTION commerce_return_line_quality_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.condition_code<>OLD.condition_code
    OR NEW.condition_note IS DISTINCT FROM OLD.condition_note
    OR NEW.quality_due_date IS DISTINCT FROM OLD.quality_due_date THEN
  RAISE EXCEPTION 'RETURN_QUALITY_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commerce_return_line_quality_immutable
  BEFORE UPDATE ON commerce_return_lines FOR EACH ROW
  EXECUTE FUNCTION commerce_return_line_quality_immutable();

INSERT INTO work_cache_revisions(table_name) VALUES('commerce_return_lines') ON CONFLICT DO NOTHING;

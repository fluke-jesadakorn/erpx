-- A failed quality hold may be disposed in part. The remaining quantity
-- stays quarantined and is represented by a new derived hold at posting.
ALTER TABLE inventory_quality_holds
  ADD COLUMN disposition_quantity_milli BIGINT
    CHECK(disposition_quantity_milli IS NULL OR disposition_quantity_milli > 0);

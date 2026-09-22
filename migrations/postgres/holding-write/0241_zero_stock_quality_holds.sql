-- A whole-product quality hold may be created at a receiving warehouse before
-- the expected stock arrives. Lot-specific holds remain protected by the
-- quality_lot_guard and must point to a sufficient current lot balance.
ALTER TABLE inventory_quality_holds
  DROP CONSTRAINT inventory_quality_holds_held_quantity_milli_check;
ALTER TABLE inventory_quality_holds
  ADD CONSTRAINT inventory_quality_holds_held_quantity_milli_check
  CHECK(held_quantity_milli>=0);

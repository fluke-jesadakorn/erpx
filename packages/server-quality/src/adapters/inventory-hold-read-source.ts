import 'server-only';

/** Fixed Quality-owned relation for Inventory's supplied statement snapshot. */
export function inventoryQualityHoldReadSource() {
  return 'FROM (SELECT warehouse_id, product_id, lot_id, status FROM inventory_quality_holds) h';
}

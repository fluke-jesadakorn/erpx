import 'server-only';
import {qualitySourceArmHead} from './quality-source-arm-head';

/** Fixed Quality-owned relation for Inventory's supplied statement snapshot. */
export function inventoryQualityHoldReadSource() {
  return `FROM (${qualitySourceArmHead("warehouse_id, product_id, lot_id, status","inventory_quality_holds")}) h`;
}

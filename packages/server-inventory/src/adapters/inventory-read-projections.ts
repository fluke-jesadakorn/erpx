import {inventoryQualityHoldReadSource} from '@erpx/server-quality/public/inventory-hold-read-source';
/** Fixed owner facts used by native joined Inventory projections; application ports return typed rows. */
export const inventoryReadProjection={
reservedStock:"COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0)",
quarantinedStock:"inventory_quarantined_quantity(s.warehouse_id,s.product_id)",
noActiveQualityHold:`NOT EXISTS(SELECT 1 ${inventoryQualityHoldReadSource()} WHERE h.warehouse_id=s.warehouse_id AND h.product_id=s.product_id AND h.status NOT IN ('released','cancelled','disposed'))`,
landedCostNotificationSource:"FROM inventory_landed_costs c",
landedCostCreatorNotificationSource:"FROM inventory_landed_costs",
transitNotificationSource:"FROM inventory_transit d"
} as const;

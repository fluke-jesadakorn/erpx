import {inventoryQualityHoldReadSource} from '@erpx/server-quality/public/inventory-hold-read-source';
/** Canonical owner-local fixed SQL for the Inventory ledger projection. The
 * native inventory read registry states the same fixed ledger selection; every
 * caller-unit join shares this one literal, so no duplicate TS SQL remains. */
export const inventoryLedgerProjectionSql = "SELECT \"id\",\"warehouse_id\",\"product_id\",\"quantity_milli\",\"value_minor\",\"created_at\",\"movement_date\",\"source_type\",\"source_id\",\"source_number\" FROM inventory_ledger";
/** Canonical owner-local fixed SQL for raw Inventory lot moves. Every caller-unit
 * settlement source shares this one literal, so no duplicate TS SQL remains. */
export const inventoryLotMovesSql = "SELECT * FROM inventory_lot_moves";
/** Fixed owner facts used by native joined Inventory projections; application ports return typed rows. */
export const inventoryReadProjection={
reservedStock:"COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0)",
quarantinedStock:"inventory_quarantined_quantity(s.warehouse_id,s.product_id)",
noActiveQualityHold:`NOT EXISTS(SELECT 1 ${inventoryQualityHoldReadSource()} WHERE h.warehouse_id=s.warehouse_id AND h.product_id=s.product_id AND h.status NOT IN ('released','cancelled','disposed'))`,
landedCostNotificationSource:"FROM inventory_landed_costs c",
landedCostCreatorNotificationSource:"FROM inventory_landed_costs",
transitNotificationSource:"FROM inventory_transit d"
} as const;

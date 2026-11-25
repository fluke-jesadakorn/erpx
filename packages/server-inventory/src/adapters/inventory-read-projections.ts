import {inventoryQualityHoldReadSource} from '@erpx/server-quality/public/inventory-hold-read-source';
import {activeQualityHoldExistsSql} from './read-tail-sql';
/** Canonical owner-local fixed SQL for the Inventory ledger projection. The
 * native inventory read registry states the same fixed ledger selection; every
 * caller-unit join shares this one literal, so no duplicate TS SQL remains. */
export const inventoryLedgerProjectionSql = "SELECT \"id\",\"warehouse_id\",\"product_id\",\"quantity_milli\",\"value_minor\",\"created_at\",\"movement_date\",\"source_type\",\"source_id\",\"source_number\" FROM inventory_ledger";
/** Canonical owner-local fixed SQL for raw Inventory lot moves. Every caller-unit
 * settlement source shares this one literal, so no duplicate TS SQL remains. */
export const inventoryLotMovesSql = "SELECT * FROM inventory_lot_moves";
/** Stock Count status is now owned by the native read registry as
 * `inventory_rd_count_status`; callers use the SQL-free `countStatus` bridge. */
/** Both command-receipt replay probes are owned by the native read registry:
 * `inventory_rd_purchase_replay` and `inventory_rd_command_replay`. Callers use
 * the SQL-free `purchaseReplayExists` / `commandReplayExists` bridges. */
/** Fixed owner facts used by native joined Inventory projections; application ports return typed rows. */
export const inventoryReadProjection={
reservedStock:"COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0)",
quarantinedStock:"inventory_quarantined_quantity(s.warehouse_id,s.product_id)",
noActiveQualityHold:`NOT ${activeQualityHoldExistsSql(inventoryQualityHoldReadSource(), "h.warehouse_id=s.warehouse_id AND h.product_id=s.product_id AND h.status NOT IN ('released','cancelled','disposed')")}`,
landedCostNotificationSource:"FROM inventory_landed_costs c",
landedCostCreatorNotificationSource:"FROM inventory_landed_costs",
transitNotificationSource:"FROM inventory_transit d"
} as const;

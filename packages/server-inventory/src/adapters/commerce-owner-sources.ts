import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Original reservation/lot writes share Commerce's complete owner unit.
 * Movement history also constrains backdated receipts and returns. */
export function commerceOwnerInventorySourceReadLocks(database:SqlDatabase){return [
 database.prepare("LOCK TABLE inventory_order_reservations,inventory_lots,inventory_lot_balances,inventory_lot_moves,inventory_fulfillment_lot_allocations IN SHARE ROW EXCLUSIVE MODE"),
 database.prepare("LOCK TABLE inventory_counts,inventory_adjustment_moves,inventory_transfers,inventory_transfer_moves,inventory_transit,inventory_transit_lines,inventory_transit_events IN SHARE MODE")
];}

import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {
  nativeManufacturingLane28StockHistory,
  type Lane28WorkContext,
} from './native-lane-28/client';

/** Named relation leases precede the first source snapshot. These locks are
 *  unchanged: the migrated read is a fact witness, not an authority writer. */
export function manufacturingInitialPhysicalStockSourceLocks(database: SqlDatabase) {
  return [database.prepare('LOCK TABLE manufacturing_orders,manufacturing_material_reservations,manufacturing_inventory_moves IN SHARE MODE')];
}

/** Native owner read: material-reservation and inventory-move history for one
 *  Company/warehouse/product. The retired inline EXISTS guard is executed by
 *  the Rust axum owner with sqlx (lane 28); this adapter emits no SQL and
 *  fails closed. The caller's unit stays in the signature for compatibility. */
export async function manufacturingInitialPhysicalStockHistory(
  _database: SqlDatabase,
  companyId: string,
  warehouseId: string,
  productId: string,
  actor?: Lane28WorkContext,
) {
  return nativeManufacturingLane28StockHistory(companyId, warehouseId, productId, actor);
}

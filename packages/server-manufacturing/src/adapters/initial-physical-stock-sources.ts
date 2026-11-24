import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {manufacturingExistsSelect} from './manufacturing-read-sql-fragments';
/** Text-only material-reservation / inventory-move history guard shared by the
 * initial physical stock check. The two EXISTS were byte-identical apart from
 * the source table and its alias; the helper emits each one verbatim on the
 * caller's prepared statement with unchanged bind order and stays outside the
 * RD_* read registry. */
const initialStockHistoryGuard = (table: string, alias: string) =>
  `EXISTS(${manufacturingExistsSelect(`${table} ${alias} JOIN manufacturing_orders o ON o.id=${alias}.order_id`, `o.company_id=? AND ${alias}.warehouse_id=? AND ${alias}.product_id=?`)})`;
/** Material reservation and movement history remain original Manufacturing facts. */
export function manufacturingInitialPhysicalStockSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE manufacturing_orders,manufacturing_material_reservations,manufacturing_inventory_moves IN SHARE MODE')];}
export async function manufacturingInitialPhysicalStockHistory(database:SqlDatabase,companyId:string,warehouseId:string,productId:string){
 return !!await database.prepare(`SELECT ${initialStockHistoryGuard("manufacturing_material_reservations","r")} OR ${initialStockHistoryGuard("manufacturing_inventory_moves","m")} history`).bind(companyId,warehouseId,productId,companyId,warehouseId,productId).first<boolean>('history');
}

import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Material reservation and movement history remain original Manufacturing facts. */
export function manufacturingInitialPhysicalStockSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE manufacturing_orders,manufacturing_material_reservations,manufacturing_inventory_moves IN SHARE MODE')];}
export async function manufacturingInitialPhysicalStockHistory(database:SqlDatabase,companyId:string,warehouseId:string,productId:string){
 return !!await database.prepare(`SELECT EXISTS(SELECT 1 FROM manufacturing_material_reservations r JOIN manufacturing_orders o ON o.id=r.order_id WHERE o.company_id=? AND r.warehouse_id=? AND r.product_id=?) OR EXISTS(SELECT 1 FROM manufacturing_inventory_moves m JOIN manufacturing_orders o ON o.id=m.order_id WHERE o.company_id=? AND m.warehouse_id=? AND m.product_id=?) history`).bind(companyId,warehouseId,productId,companyId,warehouseId,productId).first<boolean>('history');
}

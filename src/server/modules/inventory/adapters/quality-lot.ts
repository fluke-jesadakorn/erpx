import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {WorkContext} from '../../identity_access/public/authorization';
type Row=Record<string,string|number|null>;
export function qualityLotFacts(write:SqlDatabase,actor:WorkContext,input:{lotId?:string;productId:string;warehouseId:string}){return write.prepare("SELECT l.id,b.quantity_milli,b.version FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id WHERE l.id=? AND l.company_id=? AND l.product_id=? AND b.warehouse_id=? AND b.quantity_milli>0").bind(input.lotId!,actor.companyId,input.productId,input.warehouseId).first<Row>();}

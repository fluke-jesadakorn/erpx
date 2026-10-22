import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
type Row=Record<string,string|number|null>;
export function qualityScrapLotFacts(write:SqlDatabase,actor:WorkContext,h:Row){return write.prepare('SELECT l.*,b.quantity_milli,b.value_minor,b.version balance_version FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id WHERE l.company_id=? AND l.product_id=? AND b.warehouse_id=? AND b.quantity_milli>0 ORDER BY l.id').bind(actor.companyId,h.product_id,h.warehouse_id).all<Row>();}

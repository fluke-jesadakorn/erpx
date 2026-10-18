import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** The complete Commerce chain already holds Stock/lot/reservation leases.
 * Its initial-setup proof also reads the native ledger: lease it explicitly
 * after those original writers' Stock leases and before the first snapshot. */
export function manufacturingPostingInventorySourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE inventory_counts,inventory_ledger,inventory_lot_balances IN SHARE MODE')];}
/** A posted zero-variance Count still owns its original posting decision. An
 * absent Company mapping cannot silently adopt that retained economic source.
 * Unposted Count intent is kept byte-exact without being called prior Stock. */
export async function manufacturingPostingInventoryState(database:SqlDatabase,companyId:string){
 const row=await database.prepare("SELECT EXISTS(SELECT 1 FROM inventory_counts WHERE company_id=? AND status='posted') economics,COALESCE((SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM inventory_counts c WHERE c.company_id=?),'[]'::jsonb)::text image").bind(companyId,companyId).first<{economics:boolean;image:string}>();
 if(!row)throw Error('Original Inventory posting state unavailable');return row;
}

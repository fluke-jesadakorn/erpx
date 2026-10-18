import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Fixed original quote/sale identity, never a caller-supplied source label. */
export function crmAttachmentOrderSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE commerce_orders IN SHARE ROW EXCLUSIVE MODE');}
export function crmAttachmentOrderSourceImage(database:SqlDatabase,companyId:string,id:string){return database.prepare("SELECT to_jsonb(source)::text image FROM commerce_orders source WHERE source.id=? AND source.company_id=? AND source.kind IN ('quote','sales')").bind(id,companyId).first<string>('image');}

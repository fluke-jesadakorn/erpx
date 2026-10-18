import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** The existing Helpdesk ticket's actual order FK, including its native
 * version and customer identity. This does not grant Commerce collection read. */
export function supportOrderAttachmentSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE commerce_orders IN SHARE ROW EXCLUSIVE MODE');}
export function supportOrderAttachmentSourceImage(database:SqlDatabase,companyId:string,id:string){return database.prepare('SELECT to_jsonb(source)::text image FROM commerce_orders source WHERE source.id=? AND source.company_id=?').bind(id,companyId).first<string>('image');}

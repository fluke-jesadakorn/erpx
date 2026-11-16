import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Public customer sharing uses its own verified Portal binding. This port
 * witnesses the actual existing case; it grants no Helpdesk collection read. */
export function portalPreviewSupportSourceReadLocks(database:SqlDatabase){return database.prepare('LOCK TABLE support_tickets IN SHARE ROW EXCLUSIVE MODE');}
export function portalPreviewSupportSourceImage(database:SqlDatabase,companyId:string,id:string){return database.prepare('SELECT to_jsonb(ticket)::text image FROM support_tickets ticket WHERE ticket.id=? AND ticket.company_id=?').bind(id,companyId).first<string>('image');}

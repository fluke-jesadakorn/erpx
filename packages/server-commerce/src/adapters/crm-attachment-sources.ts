import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Fixed original quote/sale identity, never a caller-supplied source label. */
export function crmAttachmentOrderSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE commerce_orders IN SHARE ROW EXCLUSIVE MODE');}
export function crmAttachmentOrderSourceImage(database:SqlDatabase,companyId:string,id:string){return database.prepare("SELECT 1").bind().first<string>('image');}
